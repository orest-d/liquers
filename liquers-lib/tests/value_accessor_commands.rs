//! The `acc` commands end to end through `EnvRef::evaluate`, and `ExtValue::Accessor`.
//!
//! Design: `specs/design/settable-value-accessor/` (Phase 3, lib tests).

use std::sync::Arc;
use std::time::Duration;

use liquers_core::accessor::{memory_accessor, SetOptions, ValueAccessor};
use liquers_core::assets::AssetManager;
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::{Error, ErrorType};
use liquers_core::metadata::{Metadata, MetadataRecord, Status};
use liquers_core::parse::{parse_key, parse_query};
use liquers_core::pipeline::ElementKind;
use liquers_core::query::{Key, Query};
use liquers_core::state::State;
use liquers_core::store::AsyncMemoryStore;
use liquers_core::value::ValueInterface;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::value::{ExtValue, Value};

// The `register_*_commands!` macros expand to code naming these unqualified.
#[allow(unused_imports)]
use liquers_core::{
    command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue},
    context::Context,
};

type CommandEnvironment = DefaultEnvironment<Value>;

fn from_csv(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("table"))
}
fn is_in(_state: &State<Value>, _column: String, _values: Vec<String>) -> Result<Value, Error> {
    Ok(Value::from("filtered"))
}
fn head(_state: &State<Value>, n: i64) -> Result<Value, Error> {
    Ok(Value::from(format!("head {n}")))
}

const QUERY: &str = "-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/head-10/top.csv";
const ARGUMENT_N: &str =
    "-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query/action-3/argument-n";

async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(10), future)
        .await
        .expect("did not finish within 10 s")
}

fn key(text: &str) -> Key {
    parse_key(text).unwrap()
}

fn text_record() -> MetadataRecord {
    let mut record = MetadataRecord::new();
    record.type_identifier = "Text".to_string();
    record.type_name = "text".to_string();
    record.data_format = Some("txt".to_string());
    record
}

fn register(env: &mut CommandEnvironment) -> Result<(), Error> {
    let cr = env.get_mut_command_registry();
    liquers_lib::register_core_commands!(cr)?;
    liquers_lib::register_accessor_commands!(cr)?;
    register_command!(cr, fn from_csv(state) -> result namespace: "pl")?;
    register_command!(cr, fn is_in(state, column: String, values: Vec<String> multiple) -> result namespace: "pl")?;
    register_command!(cr, fn head(state, n: i64 = 10 (gui: IntegerField)) -> result namespace: "pl")?;
    Ok(())
}

fn environment() -> EnvRef<CommandEnvironment> {
    let mut env = CommandEnvironment::new();
    register(&mut env).unwrap();
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    env.to_ref()
}

fn json(value: &Value) -> serde_json::Value {
    value.try_into_json_value().unwrap()
}

/// An environment whose store holds `reports/recipes.yaml` (a title and two recipes) and
/// `config.yaml`.
async fn fixture() -> EnvRef<CommandEnvironment> {
    let envref = environment();
    let manager = envref.get_asset_manager();
    let recipes = format!(
        "title: Reports\nrecipes:\n- query: {QUERY}\n  title: Top\n- query: -R/data/sales.csv/-/ns-pl/from_csv/head-3/few.csv\n  title: Few\n"
    );
    manager
        .set_binary(&key("reports/recipes.yaml"), recipes.as_bytes(), text_record())
        .await
        .unwrap();
    manager
        .set_binary(&key("config.yaml"), b"title: Dashboard\nrows: 3\n", text_record())
        .await
        .unwrap();
    envref
}

async fn evaluate_accessor(
    envref: &EnvRef<CommandEnvironment>,
    query: &str,
) -> Arc<dyn ValueAccessor<Value>> {
    let asset = within(envref.evaluate(&parse_query(query).unwrap()))
        .await
        .unwrap();
    within(asset.get())
        .await
        .unwrap()
        .value()
        .unwrap()
        .try_into_accessor()
        .unwrap()
}

async fn value_of(accessor: &Arc<dyn ValueAccessor<Value>>) -> Value {
    within(accessor.get())
        .await
        .unwrap()
        .state
        .value()
        .unwrap()
        .as_ref()
        .clone()
}

async fn stored_text(envref: &EnvRef<CommandEnvironment>, k: &str) -> String {
    String::from_utf8(envref.get_async_store().get_bytes(&key(k)).await.unwrap()).unwrap()
}

#[tokio::test]
async fn accessor_query_projects_recipe_argument() {
    let envref = fixture().await;
    let n = evaluate_accessor(&envref, ARGUMENT_N).await;
    assert_eq!(json(&value_of(&n).await), serde_json::json!(10));
    let part = within(n.describe()).await.unwrap();
    assert_eq!(part.type_identifier, "I64");
    assert_eq!(
        part.argument.expect("an argument").argument_type,
        ArgumentType::Integer
    );
    assert_eq!(n.address().encode(), ARGUMENT_N);
    within(n.set(Value::from(20i64), SetOptions::default()))
        .await
        .unwrap();
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&stored_text(&envref, "reports/recipes.yaml").await).unwrap();
    assert_eq!(yaml["title"], serde_yaml::Value::from("Reports"));
    assert_eq!(
        yaml["recipes"][0]["query"],
        serde_yaml::Value::from(QUERY.replace("head-10", "head-20"))
    );
    // The second evaluation is the same handle, not a copy.
    let again = evaluate_accessor(&envref, ARGUMENT_N).await;
    assert!(Arc::ptr_eq(&n, &again));
    // `acc/get` reads the current value.
    let current = within(envref.evaluate(&parse_query(&format!("{ARGUMENT_N}/get")).unwrap()))
        .await
        .unwrap();
    assert_eq!(
        json(&within(current.get()).await.unwrap().value().unwrap()),
        serde_json::json!(20)
    );
}

#[tokio::test]
async fn accessor_value_is_never_stored() {
    let envref = fixture().await;
    let accessor = evaluate_accessor(&envref, "-R-key/config.yaml/-/ns-acc/accessor").await;
    let manager = envref.get_asset_manager();
    let handle = key("handle.accessor");
    manager
        .set_state(
            &handle,
            State::from_value_and_metadata(
                Value::from_accessor(accessor).unwrap(),
                Arc::new(Metadata::new()),
            ),
        )
        .await
        .unwrap();
    // The `Accessor` type has no formats: only its metadata reaches the store, never data.
    let store = envref.get_async_store();
    let bytes = store.get_bytes(&handle).await;
    assert!(bytes.map(|b| b.is_empty()).unwrap_or(true));
    assert_eq!(
        store.get_metadata(&handle).await.unwrap().type_identifier().unwrap(),
        "Accessor"
    );
    let registry = envref.get_type_registry();
    assert!(registry
        .get("Accessor")
        .expect("Accessor is registered")
        .supported_data_formats
        .is_empty());
}

#[tokio::test]
async fn accessor_address_resolves_in_a_second_environment() {
    let first = fixture().await;
    let rows = evaluate_accessor(&first, "-R-key/config.yaml/-/ns-acc/accessor/field-rows").await;
    assert_eq!(json(&value_of(&rows).await), serde_json::json!(3));
    // Another environment with the same data: the address alone rebuilds the accessor.
    let second = fixture().await;
    let same = evaluate_accessor(&second, &rows.address().encode()).await;
    assert!(!Arc::ptr_eq(&rows, &same));
    assert_eq!(json(&value_of(&same).await), serde_json::json!(3));
}

#[tokio::test]
async fn structural_edit_expires_action_accessor_asset() {
    let envref = fixture().await;
    let action_query = "-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query/action-3";
    let action = evaluate_accessor(&envref, action_query).await;
    within(action.get()).await.unwrap();
    let query = evaluate_accessor(
        &envref,
        "-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query",
    )
    .await;
    within(query.get()).await.unwrap();
    // A step inserted before `head`: element 3 is now another action.
    let inserted = QUERY.replace("is_in-region-EU-US/", "is_in-region-EU-US/is_in-region-CH/");
    within(query.set(
        Value::from_query(&parse_query(&inserted).unwrap()),
        SetOptions::default(),
    ))
    .await
    .unwrap();
    let error = within(action.get()).await.unwrap_err();
    assert_ne!(error.error_type, ErrorType::StatusConflict);
    let asset = within(envref.evaluate(&parse_query(action_query).unwrap()))
        .await
        .unwrap();
    // The invalidation expired the old asset, so this evaluation built a new handle.
    let fresh = within(asset.get())
        .await
        .unwrap()
        .value()
        .unwrap()
        .try_into_accessor()
        .unwrap();
    assert!(!Arc::ptr_eq(&action, &fresh));
    let element = value_of(&fresh).await.try_into_pipeline().unwrap();
    assert_eq!(element.elements.len(), 1);
    match &element.elements[0].item {
        ElementKind::Action(action) => assert_eq!(action.action, "is_in"),
        other => panic!("element 3 is not an action: {other:?}"),
    }
}

#[tokio::test]
async fn document_field_projection() {
    let envref = fixture().await;
    let title = evaluate_accessor(&envref, "-R-key/config.yaml/-/ns-acc/accessor/field-title").await;
    assert_eq!(json(&value_of(&title).await), serde_json::json!("Dashboard"));
    within(title.set(Value::from("Sales"), SetOptions::default()))
        .await
        .unwrap();
    assert_eq!(
        stored_text(&envref, "config.yaml").await,
        "title: Sales\nrows: 3\n"
    );
}

#[tokio::test]
async fn store_command_views() {
    let envref = fixture().await;
    let binary = evaluate_accessor(&envref, "-R-key/config.yaml/-/ns-acc/store-binary").await;
    assert_eq!(
        value_of(&binary).await.try_into_bytes().unwrap(),
        b"title: Dashboard\nrows: 3\n".to_vec()
    );
    let metadata = evaluate_accessor(&envref, "-R-key/config.yaml/-/ns-acc/store-metadata").await;
    assert_eq!(value_of(&metadata).await.identifier(), "Metadata");
    let value = evaluate_accessor(&envref, "-R-key/config.yaml/-/ns-acc/store").await;
    assert_eq!(value_of(&value).await.try_into_string().unwrap(), "title: Dashboard\nrows: 3\n");
    // A key that does not exist yet: status None, created by a set.
    let fresh = evaluate_accessor(&envref, "-R-key/reports/new.txt/-/ns-acc/store").await;
    assert_eq!(within(fresh.info()).await.unwrap().status, Status::None);
    within(fresh.set(Value::from("new"), SetOptions::default()))
        .await
        .unwrap();
    assert_eq!(stored_text(&envref, "reports/new.txt").await, "new");
}

#[tokio::test]
async fn ext_value_accessor_hooks_round_trip() {
    let accessor = memory_accessor::<Value>(None, Query::new());
    let value = Value::from_accessor(accessor.clone()).unwrap();
    assert!(matches!(value, Value::Extended(ExtValue::Accessor { .. })));
    assert_eq!(value.identifier(), "Accessor");
    assert!(Arc::ptr_eq(&value.try_into_accessor().unwrap(), &accessor));
    assert!(Value::from("text").try_into_accessor().is_err());
    let core = liquers_core::accessor::memory_accessor::<liquers_core::value::Value>(None, Query::new());
    assert_eq!(
        liquers_core::value::Value::from_accessor(core).unwrap_err().error_type,
        ErrorType::NotSupported
    );
}

#[cfg(feature = "records")]
#[tokio::test]
async fn record_row_column_and_cell_projections() {
    use liquers_lib::records::accessor::{CellStep, ColumnStep, RowStep};
    use liquers_lib::value::ExtValueInterface;
    use liquers_records::{
        Buffer, Column, FieldSchema, FieldType, RecordBatch, RecordSchema, RecordView, RowRun,
    };

    let schema = Arc::new(
        RecordSchema::new(vec![
            FieldSchema::new("n", FieldType::Int),
            FieldSchema::new("x", FieldType::Float),
        ])
        .unwrap(),
    );
    let n = Column::Int {
        validity: None,
        values: Buffer::from_slice(&[1i64, 2, 3]),
    };
    let x = Column::Float {
        validity: None,
        values: Buffer::from_slice(&[0.5f64, 1.5, 2.5]),
    };
    // Rows 10..13 of chunk 7: an edit must keep these identities.
    let runs = vec![RowRun { chunk: 7, first_row: 10, first_number: Some(100), len: 3 }];
    let batch = RecordBatch::new(schema, vec![n, x], None, Some(runs), vec![]).unwrap();
    let ids_before: Vec<_> = (0..3).map(|r| batch.row_id(r).unwrap()).collect();
    let table = memory_accessor::<Value>(
        Some(State::from_value_and_metadata(
            Value::from_record_view(Arc::new(batch)),
            Arc::new(Metadata::new()),
        )),
        Query::new(),
    );
    let row = table.project(Arc::new(RowStep(1)), Query::new()).unwrap();
    let column = table
        .project(Arc::new(ColumnStep::new("x")), Query::new())
        .unwrap();
    let cell = table
        .project(Arc::new(CellStep::new(2, "n")), Query::new())
        .unwrap();
    assert_eq!(json(&value_of(&row).await), serde_json::json!({"n": 2, "x": 1.5}));
    assert_eq!(json(&value_of(&column).await), serde_json::json!([0.5, 1.5, 2.5]));
    assert_eq!(json(&value_of(&cell).await), serde_json::json!(3));

    within(cell.set(Value::from(30i64), SetOptions::default()))
        .await
        .unwrap();
    within(row.set(
        Value::try_from_json_value(&serde_json::json!({"x": 9.0})).unwrap(),
        SetOptions::default(),
    ))
    .await
    .unwrap();
    within(column.get()).await.unwrap();
    assert_eq!(json(&value_of(&column).await), serde_json::json!([0.5, 9.0, 2.5]));

    let view = value_of(&table).await.as_record_view().unwrap();
    let cells: Vec<String> = (0..view.len())
        .flat_map(|r| (0..2).map(move |c| (r, c)))
        .map(|(r, c)| format!("{:?}", view.value(r, c).unwrap()))
        .collect();
    // Only the two edited cells changed; row 1's `n` kept its value.
    assert_eq!(
        cells,
        ["Int(1)", "Float(0.5)", "Int(2)", "Float(9.0)", "Int(30)", "Float(2.5)"]
    );
    // And every row kept its identity.
    let ids_after: Vec<_> = (0..3).map(|r| view.row_id(r).unwrap()).collect();
    assert_eq!(ids_after, ids_before);
    // A column of the wrong length is refused and writes nothing.
    let short = Value::try_from_json_value(&serde_json::json!([1.0])).unwrap();
    assert!(within(column.set(short, SetOptions::default())).await.is_err());
    assert_eq!(json(&value_of(&cell).await), serde_json::json!(30));
}

/// recipes-editor AC-6: `acc/recipe_file` reads and writes a recipe by its filename.
#[tokio::test]
async fn recipe_file_command_projects_by_filename() {
    let envref = fixture().await;
    let few = evaluate_accessor(&envref, "-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe_file-few.csv").await;
    let recipe = value_of(&few).await;
    assert_eq!(json(&recipe)["title"], serde_json::json!("Few"));
    let mut edited: liquers_core::recipes::Recipe = serde_json::from_value(json(&recipe)).unwrap();
    edited.title = "First few".to_owned();
    within(few.set(Value::from_recipe(edited), SetOptions::default()))
        .await
        .unwrap();
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&stored_text(&envref, "reports/recipes.yaml").await).unwrap();
    assert_eq!(yaml["recipes"][1]["title"], serde_yaml::Value::from("First few"));
    assert_eq!(yaml["recipes"][0]["title"], serde_yaml::Value::from("Top"));
}
