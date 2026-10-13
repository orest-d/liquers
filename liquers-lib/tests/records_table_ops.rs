//! `ns-rec` table operations and their `ns-pl` counterparts, end to end through
//! `EnvRef::evaluate` (`specs/design/record-filter-commands/phase3-examples.md`).
//!
//! The fixture is `orders.csv` from `schema-example.md`: A-1 Alice Smith 120.5 `["gift"]`,
//! A-2 Bob Jones 35.0 `[]`, A-3 (no customer) 99.9 `["bulk","b2b"]`. `split_rows-<tag>` turns a
//! view into a three-chunk source (one row per chunk) that counts, under `<tag>`, how many chunks
//! were streamed.
#![cfg(feature = "records")]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::metadata::{Metadata, Status};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::state::State;
use liquers_core::store::{AsyncMemoryStore, AsyncStore};
use liquers_core::value::ValueInterface;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::records::{
    FieldValue, InMemorySource, MappedSource, RecordSource, RecordView, ViewMap,
};
use liquers_lib::register_records_commands;
use liquers_lib::value::{ExtValueInterface, Value};

// The `register_*_commands!` macros expand to code naming these unqualified.
#[allow(unused_imports)]
use liquers_core::{
    command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue},
    commands::CommandRegistry,
    context::Context,
};
#[allow(unused_imports)]
use liquers_lib::value::simple::SimpleValue;

type CommandEnvironment = DefaultEnvironment<Value>;
type Res = Result<(), Box<dyn std::error::Error>>;

const ORDERS: &[u8] = b"order_id,placed_at,customer,amount,tags
A-1,2026-10-01T09:15:00Z,Alice Smith,120.5,\"[\"\"gift\"\"]\"
A-2,2026-10-02T14:00:00Z,Bob Jones,35.0,[]
A-3,2026-10-03T08:30:00Z,,99.9,\"[\"\"bulk\"\",\"\"b2b\"\"]\"
";

/// The declared schema of `schema-example.md`: an Id, roles, a full-text index (YAML tag form) and
/// a JSON list column.
const ORDERS_SCHEMA: &str = "type_identifier: shop.Order
fields:
  - name: order_id
    data_type: Text
    nullable: false
    key: Id
  - name: placed_at
    data_type: Timestamp
    nullable: false
    role: { indexed: [Range], stored: true, fast: true }
  - name: customer
    label: Customer name
    data_type: Text
    role: { indexed: [Exact, !FullText { analyzer: Simple, positions: true }], stored: true }
  - name: amount
    data_type: Float
    description: Order total in EUR
    role: { indexed: [Range] }
  - name: tags
    data_type: Text
    media_type: application/json
";

const CSV: &str = "-R/data/orders.csv/-/ns-rec/to_record-csv";
const DECLARED: &str = "-R/data/orders.csv/-/ns-rec/to_record-csv-t-~X~-R/data/orders.schema.yaml~E";

// ------------------------------------------------------------------------------------------------
// Fixture
// ------------------------------------------------------------------------------------------------

fn counts() -> &'static Mutex<HashMap<String, usize>> {
    static COUNTS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    COUNTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn count_of(tag: &str) -> usize {
    counts().lock().unwrap_or_else(|p| p.into_inner()).get(tag).copied().unwrap_or(0)
}

/// A view as a source of one-row chunks that counts the chunks it streams under `tag`.
fn split_rows(state: &State<Value>, tag: String) -> Result<Value, Error> {
    let view = state.value()?.as_record_view()?;
    let rows = (0..view.len()).map(|row| view.row(row)).collect::<Result<Vec<_>, Error>>()?;
    let inner: Arc<dyn RecordSource> = Arc::new(InMemorySource::new(rows));
    let counting: ViewMap = Arc::new(move |chunk: Arc<dyn RecordView>| {
        // MappedSource::new maps one empty batch to derive its schema; that is not a chunk.
        if chunk.is_empty() {
            return Ok(chunk);
        }
        *counts().lock().unwrap_or_else(|p| p.into_inner()).entry(tag.clone()).or_insert(0) += 1;
        Ok(chunk)
    });
    Ok(Value::from_record_source(Arc::new(MappedSource::new(inner, counting)?)))
}

async fn put(store: &AsyncMemoryStore, key: &str, data: &[u8]) -> Result<(), Error> {
    let key = parse_key(key)?;
    let mut metadata = Metadata::new();
    let filename = key
        .filename()
        .ok_or_else(|| Error::general_error(format!("key '{key}' has no filename")))?;
    metadata.set_filename(filename.encode().as_ref())?;
    metadata.set_status(Status::Source)?;
    store.set(&key, data, &metadata).await
}

fn environment() -> Result<CommandEnvironment, Error> {
    let mut env = CommandEnvironment::new();
    {
        let cr = env.get_mut_command_registry();
        register_records_commands!(cr)?;
        #[cfg(feature = "polars")]
        liquers_lib::register_polars_commands!(cr)?;
        liquers_lib::register_search_commands!(cr)?;
        register_command!(cr, fn split_rows(state, tag: String) -> result namespace: "fixture")?;
    }
    Ok(env)
}

async fn fixture() -> Result<EnvRef<CommandEnvironment>, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    put(&store, "data/orders.csv", ORDERS).await?;
    put(&store, "data/orders.schema.yaml", ORDERS_SCHEMA.as_bytes()).await?;
    let mut env = environment()?;
    env.with_async_store(Box::new(store));
    Ok(env.to_ref())
}

async fn eval(env: &EnvRef<CommandEnvironment>, query: &str) -> Result<State<Value>, Error> {
    env.evaluate(query).await?.get().await
}

/// The error a query fails with, as text.
async fn error_of(env: &EnvRef<CommandEnvironment>, query: &str) -> String {
    // A failed command yields a state whose value is the error, surfaced by `value()`.
    match eval(env, query).await.and_then(|state| state.value().map(|_| ())) {
        Ok(()) => panic!("{query} succeeded"),
        Err(error) => error.to_string(),
    }
}

/// A result as a view: records directly, a DataFrame through the bridge.
async fn view_of(env: &EnvRef<CommandEnvironment>, query: &str) -> Result<Arc<dyn RecordView>, Error> {
    let state = eval(env, query).await?;
    let value = state.value()?;
    match value.identifier().as_ref() {
        #[cfg(feature = "polars")]
        "polars.DataFrame" => {
            let df = value.as_polars_dataframe()?;
            Ok(Arc::new(liquers_lib::records::polars::dataframe_to_record_batch(&df)?))
        }
        _ => value.as_record_view(),
    }
}

async fn identifier_of(env: &EnvRef<CommandEnvironment>, query: &str) -> Result<String, Error> {
    Ok(eval(env, query).await?.value()?.identifier().to_string())
}

fn ids(view: &Arc<dyn RecordView>) -> Vec<String> {
    let index = view.schema().index_of("order_id").expect("order_id column");
    (0..view.len())
        .map(|row| match view.value(row, index).expect("cell") {
            FieldValue::Text(text) => text.to_string(),
            other => format!("{other:?}"),
        })
        .collect()
}

async fn ids_of(env: &EnvRef<CommandEnvironment>, query: &str) -> Result<Vec<String>, Error> {
    Ok(ids(&view_of(env, query).await?))
}

fn names(view: &Arc<dyn RecordView>) -> Vec<String> {
    view.schema().fields.iter().map(|field| field.name.clone()).collect()
}

fn as_i64(value: FieldValue) -> i64 {
    match value {
        FieldValue::Int(n) => n,
        FieldValue::UInt(n) => n as i64,
        other => panic!("not a count: {other:?}"),
    }
}

fn q(prefix: &str, rest: &str) -> String {
    format!("{prefix}/{rest}")
}

// ------------------------------------------------------------------------------------------------
// Filters
// ------------------------------------------------------------------------------------------------

#[tokio::test]
async fn comparison_filters_parse_to_the_column_type() -> Res {
    let env = fixture().await?;
    assert_eq!(ids_of(&env, &q(CSV, "gt-amount-50")).await?, ["A-1", "A-3"]);
    assert_eq!(ids_of(&env, &q(CSV, "eq-amount-35")).await?, ["A-2"]);
    assert_eq!(ids_of(&env, &q(CSV, "ne-amount-35")).await?, ["A-1", "A-3"]);
    assert_eq!(ids_of(&env, &q(CSV, "lt-amount-99.9")).await?, ["A-2"]);
    assert_eq!(ids_of(&env, &q(CSV, "lte-amount-99.9")).await?, ["A-2", "A-3"]);
    assert_eq!(ids_of(&env, &q(CSV, "gte-amount-99.9")).await?, ["A-1", "A-3"]);
    Ok(())
}

#[tokio::test]
async fn timestamp_compares_with_an_iso_date() -> Res {
    let env = fixture().await?;
    assert_eq!(ids_of(&env, &q(CSV, "gte-placed_at-2026~_10~_02")).await?, ["A-2", "A-3"]);
    Ok(())
}

#[tokio::test]
async fn eq_value_with_wildcards_is_literal() -> Res {
    let env = fixture().await?;
    // `*` is spelled with the named entity `~nast~`; eq takes it literally.
    assert!(ids_of(&env, &q(CSV, "eq-customer-Bob~nast~")).await?.is_empty());
    assert_eq!(ids_of(&env, &q(CSV, "eq-customer-Bob~.Jones")).await?, ["A-2"]);
    assert!(ids_of(&env, &q(CSV, "eq-customer-bob~.jones")).await?.is_empty(), "exact, so case matters");
    Ok(())
}

#[tokio::test]
async fn text_filters_are_case_insensitive_by_default() -> Res {
    let env = fixture().await?;
    assert_eq!(ids_of(&env, &q(CSV, "contains-customer-SMITH")).await?, ["A-1"]);
    assert!(ids_of(&env, &q(CSV, "contains-customer-SMITH-t")).await?.is_empty());
    assert_eq!(ids_of(&env, &q(CSV, "starts_with-customer-bob")).await?, ["A-2"]);
    assert_eq!(ids_of(&env, &q(CSV, "ends_with-customer-JONES")).await?, ["A-2"]);
    // A raw `*` does not parse in a query; it is the named entity `~nast~`.
    let glob = "glob-customer-~nast~smith";
    assert_eq!(ids_of(&env, &q(CSV, glob)).await?, ["A-1"]);
    assert!(ids_of(&env, &q(CSV, &format!("{glob}-t"))).await?.is_empty());
    // Regex metacharacters are literal.
    assert!(ids_of(&env, &q(CSV, "contains-customer-A.ice")).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn is_in_and_null_filters() -> Res {
    let env = fixture().await?;
    assert_eq!(ids_of(&env, &q(CSV, "is_in-order_id-A~_1-A~_3")).await?, ["A-1", "A-3"]);
    assert_eq!(ids_of(&env, &q(CSV, "is_null-customer")).await?, ["A-3"]);
    assert_eq!(ids_of(&env, &q(CSV, "is_not_null-customer")).await?, ["A-1", "A-2"]);
    Ok(())
}

#[tokio::test]
async fn is_in_without_values_is_refused() -> Res {
    let env = fixture().await?;
    let message = error_of(&env, &q(CSV, "is_in-order_id")).await;
    assert!(message.contains("is_in"), "{message}");
    Ok(())
}

#[tokio::test]
async fn filter_equals_unranked_search() -> Res {
    let env = fixture().await?;
    // A free-text term, and a field test (exact, case-insensitive like search).
    for expr in ["bob", "order_id~ncolon~a~_2", "customer~ncolon~bob"] {
        let filtered = ids_of(&env, &q(CSV, &format!("filter-{expr}"))).await?;
        let searched = ids_of(&env, &q(CSV, &format!("ns-search/search-{expr}-f-f-0"))).await?;
        assert_eq!(filtered, searched, "{expr}");
    }
    assert_eq!(ids_of(&env, &q(CSV, "filter-bob")).await?, ["A-2"]);
    assert_eq!(ids_of(&env, &q(CSV, "filter-order_id~ncolon~a~_2")).await?, ["A-2"]);
    Ok(())
}

#[tokio::test]
async fn filters_over_a_source_stay_lazy() -> Res {
    let env = fixture().await?;
    let query = q(CSV, "ns-fixture/split_rows-lazy_filter/ns-rec/gt-amount-50");
    assert_eq!(identifier_of(&env, &query).await?, "RecordSource");
    assert_eq!(count_of("lazy_filter"), 0, "nothing streamed before a consumer asks");
    assert_eq!(ids_of(&env, &q(&query, "materialize")).await?, ["A-1", "A-3"]);
    assert_eq!(count_of("lazy_filter"), 3);
    Ok(())
}

#[tokio::test]
async fn unknown_column_is_refused_naming_available() -> Res {
    let env = fixture().await?;
    for query in [q(CSV, "eq-nope-1"), q(CSV, "ns-fixture/split_rows-unknown/ns-rec/eq-nope-1")] {
        let message = error_of(&env, &query).await;
        assert!(message.contains("nope") && message.contains("order_id"), "{message}");
    }
    let message = error_of(&env, &q(CSV, "sort-nope")).await;
    assert!(message.contains("nope") && message.contains("order_id"), "{message}");
    Ok(())
}

#[tokio::test]
async fn json_list_cells_match_any_element() -> Res {
    let env = fixture().await?;
    assert_eq!(ids_of(&env, &q(DECLARED, "eq-tags-b2b")).await?, ["A-3"]);
    assert_eq!(ids_of(&env, &q(DECLARED, "is_in-tags-gift-bulk")).await?, ["A-1", "A-3"]);
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Sort, tail, drop_columns
// ------------------------------------------------------------------------------------------------

#[tokio::test]
async fn sort_orders_stably_with_nulls_first_by_default() -> Res {
    let env = fixture().await?;
    assert_eq!(ids_of(&env, &q(CSV, "sort-amount")).await?, ["A-2", "A-3", "A-1"]);
    assert_eq!(ids_of(&env, &q(CSV, "sort-amount-f")).await?, ["A-1", "A-3", "A-2"]);
    assert_eq!(ids_of(&env, &q(CSV, "sort-customer")).await?, ["A-3", "A-1", "A-2"]);
    assert_eq!(ids_of(&env, &q(CSV, "sort-customer-t-t")).await?, ["A-1", "A-2", "A-3"]);
    assert_eq!(ids_of(&env, &q(CSV, "sort-customer-f-t")).await?, ["A-2", "A-1", "A-3"]);
    // Stability with ties is pinned by `Column::sort_indices`' unit test; the fixture has none.
    Ok(())
}

#[tokio::test]
async fn sort_over_a_source_materializes_first() -> Res {
    let env = fixture().await?;
    let query = q(CSV, "ns-fixture/split_rows-sort/ns-rec/sort-amount-f");
    assert_eq!(identifier_of(&env, &query).await?, "RecordView");
    assert_eq!(ids_of(&env, &query).await?, ["A-1", "A-3", "A-2"]);
    Ok(())
}

// ns-pl's lazy collect needs a multi-threaded runtime (POLARS-COLLECT-PANICS-ON-CURRENT-THREAD-RUNTIME).
#[cfg_attr(feature = "polars", tokio::test(flavor = "multi_thread", worker_threads = 2))]
#[cfg_attr(not(feature = "polars"), tokio::test)]
async fn tail_and_drop_columns_match_polars() -> Res {
    let env = fixture().await?;
    assert_eq!(ids_of(&env, &q(CSV, "tail-2")).await?, ["A-2", "A-3"]);
    let dropped = view_of(&env, &q(CSV, "drop_columns-tags-customer")).await?;
    assert_eq!(names(&dropped), ["order_id", "placed_at", "amount"]);
    #[cfg(feature = "polars")]
    {
        assert_eq!(ids_of(&env, &q(CSV, "ns-pl/tail-2")).await?, ["A-2", "A-3"]);
        let pl = view_of(&env, &q(CSV, "ns-pl/drop_columns-tags-customer")).await?;
        assert_eq!(names(&pl), names(&dropped));
    }
    Ok(())
}

#[tokio::test]
async fn drop_columns_refuses_the_id_field() -> Res {
    let env = fixture().await?;
    let message = error_of(&env, &q(DECLARED, "drop_columns-order_id")).await;
    assert!(message.contains("order_id") && message.contains("key field"), "{message}");
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Counts and the schema
// ------------------------------------------------------------------------------------------------

#[tokio::test]
async fn counts_over_a_source_are_chunkwise() -> Res {
    let env = fixture().await?;
    let source = q(CSV, "ns-fixture/split_rows-counts/ns-rec");
    let nrows = eval(&env, &q(&source, "nrows")).await?.value()?.try_into_i64()?;
    assert_eq!(nrows, 3);
    assert_eq!(count_of("counts"), 3, "each chunk streamed once");
    let ncols = eval(&env, &q(&source, "ncols")).await?.value()?.try_into_i64()?;
    assert_eq!(ncols, 5);
    let shape = eval(&env, &q(&source, "shape")).await?.value()?.try_into_json_value()?;
    assert_eq!(shape, serde_json::json!({"nrows": 3, "ncols": 5}));
    let counted = view_of(&env, &q(&source, "count")).await?;
    let counts: Vec<i64> = (0..5).map(|c| as_i64(counted.value(0, c).expect("cell"))).collect();
    assert_eq!(counts, [3, 3, 2, 3, 3]);
    Ok(())
}

#[cfg(feature = "polars")]
// ns-pl's lazy collect needs a multi-threaded runtime (POLARS-COLLECT-PANICS-ON-CURRENT-THREAD-RUNTIME).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shape_and_count_match_polars() -> Res {
    let env = fixture().await?;
    let rec = eval(&env, &q(CSV, "shape")).await?.value()?.try_into_json_value()?;
    let pl = eval(&env, &q(CSV, "ns-pl/shape")).await?.value()?.try_into_json_value()?;
    assert_eq!(rec, pl);
    let rec = view_of(&env, &q(CSV, "count")).await?;
    let pl = view_of(&env, &q(CSV, "ns-pl/count")).await?;
    assert_eq!(names(&rec), names(&pl));
    for c in 0..5 {
        assert_eq!(as_i64(rec.value(0, c)?), as_i64(pl.value(0, c)?), "column {c}");
    }
    Ok(())
}

#[tokio::test]
async fn schema_and_records_schema_agree() -> Res {
    let env = fixture().await?;
    let schema = eval(&env, &q(CSV, "schema")).await?.value()?.try_into_string()?;
    let alias = eval(&env, &q(CSV, "records_schema")).await?.value()?.try_into_string()?;
    assert_eq!(schema, alias);
    assert!(schema.contains("\"placed_at\"") && schema.contains("Timestamp"), "{schema}");
    Ok(())
}

#[tokio::test]
async fn schema_round_trips_through_the_schema_argument() -> Res {
    let env = fixture().await?;
    // A .yaml file arrives as bytes; it is linked into the schema argument as text.
    let declared = eval(&env, &q(DECLARED, "schema")).await?.value()?.try_into_string()?;
    assert!(declared.contains("FullText") && declared.contains("shop.Order"), "{declared}");
    // The JSON that schema printed is accepted back by the schema argument.
    let relinked = format!(
        "-R/data/orders.csv/-/ns-rec/to_record-csv-t-~X~{DECLARED}/schema~E/schema"
    );
    let again = eval(&env, &relinked).await?.value()?.try_into_string()?;
    assert_eq!(again, declared);
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Lazy row-local operations over sources (AC-18), and the build without polars (AC-15)
// ------------------------------------------------------------------------------------------------

#[tokio::test]
async fn select_columns_over_a_source_is_lazy() -> Res {
    let env = fixture().await?;
    let query = q(DECLARED, "ns-fixture/split_rows-select/ns-rec/select_columns-amount");
    assert_eq!(identifier_of(&env, &query).await?, "RecordSource");
    assert_eq!(count_of("select"), 0);
    let view = view_of(&env, &q(&query, "materialize")).await?;
    assert_eq!(names(&view), ["amount", "order_id"]);
    assert_eq!(view.len(), 3);
    Ok(())
}

#[tokio::test]
async fn head_over_a_source_stops_early() -> Res {
    let env = fixture().await?;
    let query = q(CSV, "ns-fixture/split_rows-head/ns-rec/head-1");
    assert_eq!(identifier_of(&env, &query).await?, "RecordSource");
    assert_eq!(ids_of(&env, &q(&query, "materialize")).await?, ["A-1"]);
    assert_eq!(count_of("head"), 1, "only the first chunk is read");
    Ok(())
}

#[tokio::test]
async fn table_ops_need_no_polars() -> Res {
    let env = fixture().await?;
    assert_eq!(ids_of(&env, &q(CSV, "gt-amount-50/sort-amount")).await?, ["A-3", "A-1"]);
    let counted = view_of(&env, &q(CSV, "count")).await?;
    assert_eq!(as_i64(counted.value(0, 2)?), 2);
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// ns-pl over records, and the shared vocabulary (AC-9, AC-10, AC-11, AC-17)
// ------------------------------------------------------------------------------------------------

#[cfg(feature = "polars")]
// ns-pl's lazy collect needs a multi-threaded runtime (POLARS-COLLECT-PANICS-ON-CURRENT-THREAD-RUNTIME).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rec_and_pl_filters_and_sort_agree() -> Res {
    let env = fixture().await?;
    for op in [
        "eq-amount-35",
        "gt-amount-50",
        "contains-customer-SMITH",
        "starts_with-customer-bob",
        "is_in-order_id-A~_1-A~_3",
        "is_null-customer",
        "is_not_null-customer",
        "sort-amount-f",
        "sort-customer",
        "sort-customer-t-t",
        "tail-2",
    ] {
        let rec = ids_of(&env, &q(CSV, op)).await?;
        let pl = ids_of(&env, &q(CSV, &format!("ns-pl/{op}"))).await?;
        assert_eq!(rec, pl, "{op}");
    }
    Ok(())
}

#[cfg(feature = "polars")]
// ns-pl's lazy collect needs a multi-threaded runtime (POLARS-COLLECT-PANICS-ON-CURRENT-THREAD-RUNTIME).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pl_commands_accept_a_record_view() -> Res {
    let env = fixture().await?;
    assert_eq!(identifier_of(&env, &q(CSV, "ns-pl/sort-amount-f")).await?, "polars.DataFrame");
    assert_eq!(ids_of(&env, &q(CSV, "ns-pl/sort-amount-f")).await?, ["A-1", "A-3", "A-2"]);
    assert_eq!(ids_of(&env, &q(CSV, "ns-pl/eq-order_id-A~_2")).await?, ["A-2"]);
    let shape = eval(&env, &q(CSV, "ns-pl/shape")).await?.value()?.try_into_json_value()?;
    assert_eq!(shape, serde_json::json!({"nrows": 3, "ncols": 5}));
    Ok(())
}

#[cfg(feature = "polars")]
// ns-pl's lazy collect needs a multi-threaded runtime (POLARS-COLLECT-PANICS-ON-CURRENT-THREAD-RUNTIME).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pl_refuses_a_source_naming_materialize() -> Res {
    let env = fixture().await?;
    let message = error_of(&env, &q(CSV, "ns-fixture/split_rows-pl/ns-pl/shape")).await;
    assert!(message.contains("ns-rec/materialize"), "{message}");
    let through = eval(&env, &q(CSV, "ns-fixture/split_rows-pl2/ns-rec/materialize/ns-pl/nrows")).await?;
    assert_eq!(through.value()?.try_into_i64()?, 3);
    Ok(())
}

#[cfg(feature = "polars")]
// ns-pl's lazy collect needs a multi-threaded runtime (POLARS-COLLECT-PANICS-ON-CURRENT-THREAD-RUNTIME).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pl_sort_accepts_old_flag_spelling() -> Res {
    let env = fixture().await?;
    assert_eq!(ids_of(&env, &q(CSV, "ns-pl/sort-amount-t")).await?, ["A-2", "A-3", "A-1"]);
    Ok(())
}

#[cfg(feature = "polars")]
// ns-pl's lazy collect needs a multi-threaded runtime (POLARS-COLLECT-PANICS-ON-CURRENT-THREAD-RUNTIME).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rec_commands_accept_a_dataframe() -> Res {
    let env = fixture().await?;
    let query = "-R/data/orders.csv/-/ns-pl/sort-amount/ns-rec/eq-order_id-A~_2";
    assert_eq!(ids_of(&env, query).await?, ["A-2"]);
    Ok(())
}

#[cfg(feature = "polars")]
#[test]
fn shared_commands_have_the_same_arguments() -> Res {
    let mut env = environment()?;
    let registry = &env.get_mut_command_registry().command_metadata_registry;
    let in_namespace = |ns: &str| {
        registry
            .commands
            .iter()
            .filter(|command| command.namespace == ns)
            .map(|command| {
                let arguments: Vec<(String, String)> = command
                    .arguments
                    .iter()
                    .map(|a| (a.name.clone(), format!("{:?}", a.default)))
                    .collect();
                (command.name.clone(), arguments)
            })
            .collect::<HashMap<_, _>>()
    };
    let rec = in_namespace("rec");
    let pl = in_namespace("pl");
    // Deliberate differences, each documented next to the command (conventions.md). None today.
    let documented: [&str; 0] = [];
    let mut shared = 0;
    for (name, rec_arguments) in &rec {
        if let Some(pl_arguments) = pl.get(name) {
            shared += 1;
            if !documented.contains(&name.as_str()) {
                assert_eq!(rec_arguments, pl_arguments, "rec/{name} and pl/{name} differ");
            }
        }
    }
    assert!(shared >= 15, "only {shared} shared commands");
    Ok(())
}
