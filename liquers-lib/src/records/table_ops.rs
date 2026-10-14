//! `ns-rec` table operations: filters, sort, tail, column dropping, counts and the schema — the
//! vocabulary `ns-pl` has, over records, without polars. Each filter builds one search-language
//! node and runs the unranked search path, so a view stays a lazy row selection and a source is
//! filtered chunk by chunk ([`FilteredSource`]); `drop_columns` and (in `commands.rs`)
//! `select_columns` and `head` wrap a source lazily too ([`MappedSource`]).
//!
//! Names, arguments and defaults follow `ns-pl` (polars first), per
//! `specs/design/record-filter-commands/conventions.md`; the design is
//! `specs/design/record-filter-commands/phase2-architecture.md`.

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;
use liquers_records::search::{
    search_view, FieldTest, FilteredSource, Predicate, SearchExpr, SearchOptions, StrictPredicate,
};
use liquers_records::{
    Buffer, ChunkResolver, Column, CompareOp, ContextResolver, FieldSchema, FieldType,
    MappedSource, RecordBatch, RecordSchema, RecordSource, RecordView, ViewMap,
    DEFAULT_MATERIALIZE_MAX_ROWS,
};

use super::commands::non_negative_usize;
use super::convert::ToRecordOptions;
use crate::value::simple::SimpleValue;
use crate::value::{ExtValueInterface, Value};

/// What a table operation receives: a table, or a stream of tables it can wrap lazily.
#[derive(Debug)]
pub enum TableInput {
    View(Arc<dyn RecordView>),
    Source(Arc<dyn RecordSource>),
}

/// The input of a table operation. A `RecordSource`, or anything that names a manifest, stays a
/// source; every other input `to_record_source` accepts becomes a view (materializing a one-view
/// source is a free shallow clone).
pub async fn table_input<E: Environment<Value = Value>>(
    state: &State<Value>,
    context: &Context<E>,
) -> Result<TableInput, Error> {
    let value = state.data_unchecked();
    match value.identifier().as_ref() {
        "RecordView" => return Ok(TableInput::View(value.as_record_view()?)),
        "RecordSource" => return Ok(TableInput::Source(value.as_record_source()?)),
        _ => {}
    }
    let source = super::convert::to_record_source(
        value,
        &state.metadata,
        &ToRecordOptions::default(),
        context,
    )
    .await?;
    if source.manifest().is_some() {
        return Ok(TableInput::Source(source));
    }
    Ok(TableInput::View(materialize_source(source, context).await?))
}

/// The whole input as one table — what `sort` and `tail` need.
async fn table_view<E: Environment<Value = Value>>(
    state: &State<Value>,
    context: &Context<E>,
) -> Result<Arc<dyn RecordView>, Error> {
    match table_input(state, context).await? {
        TableInput::View(view) => Ok(view),
        TableInput::Source(source) => materialize_source(source, context).await,
    }
}

async fn materialize_source<E: Environment<Value = Value>>(
    source: Arc<dyn RecordSource>,
    context: &Context<E>,
) -> Result<Arc<dyn RecordView>, Error> {
    let resolver: Arc<dyn ChunkResolver> = Arc::new(ContextResolver::new(context.clone()));
    let batch = source.materialize(resolver, DEFAULT_MATERIALIZE_MAX_ROWS).await?;
    Ok(batch as Arc<dyn RecordView>)
}

/// "no field named 'x' (available: …)" — the wording `RecordView::select_columns` uses.
fn field_index(schema: &RecordSchema, column: &str) -> Result<usize, Error> {
    match schema.resolve_field(column)? {
        Some(index) => Ok(index),
        None => {
            let available: Vec<&str> = schema.fields.iter().map(|f| f.name.as_str()).collect();
            Err(Error::general_error(format!(
                "no field named '{column}' (available: {})",
                available.join(", ")
            )))
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Filters
// ------------------------------------------------------------------------------------------------

/// Runs `expr` unranked over the input. `column`, when given, must exist (AC-6): on a view and on
/// a source declaring its schema the error is immediate, otherwise it surfaces as the first chunk
/// streams.
async fn run_filter<E: Environment<Value = Value>>(
    state: State<Value>,
    expr: SearchExpr,
    column: Option<&str>,
    case_sensitive: bool,
    context: Context<E>,
) -> Result<Value, Error> {
    let predicate: Arc<dyn Predicate> = match column {
        Some(column) => Arc::new(StrictPredicate::new(Arc::new(expr), vec![column.to_string()])),
        None => Arc::new(expr),
    };
    let options = SearchOptions {
        case_sensitive,
        fields: Vec::new(),
        limit: None,
    };
    match table_input(&state, &context).await? {
        TableInput::View(view) => {
            let (out, report) = search_view(&view, predicate.as_ref(), &options, false)?;
            for warning in &report.warnings {
                context.warning(warning)?;
            }
            Ok(Value::from_record_view(out))
        }
        TableInput::Source(source) => {
            if let Some(schema) = source.schema() {
                for warning in predicate.bind(&schema, &options)?.warnings {
                    context.warning(&warning)?;
                }
            }
            let filtered = FilteredSource::new(source, predicate, options);
            Ok(Value::from_record_source(Arc::new(filtered)))
        }
    }
}

async fn compare<E: Environment<Value = Value>>(
    state: State<Value>,
    column: String,
    value: String,
    op: CompareOp,
    context: Context<E>,
) -> Result<Value, Error> {
    let expr = SearchExpr::field(&column, FieldTest::Compare(op, value));
    run_filter(state, expr, Some(&column), true, context).await
}

/// Rows whose `column` equals `value` (parsed to the column's type; literal, case-sensitive).
pub async fn eq<E: Environment<Value = Value>>(
    state: State<Value>,
    column: String,
    value: String,
    context: Context<E>,
) -> Result<Value, Error> {
    compare(state, column, value, CompareOp::Eq, context).await
}

pub async fn ne<E: Environment<Value = Value>>(
    state: State<Value>,
    column: String,
    value: String,
    context: Context<E>,
) -> Result<Value, Error> {
    compare(state, column, value, CompareOp::Ne, context).await
}

pub async fn gt<E: Environment<Value = Value>>(
    state: State<Value>,
    column: String,
    value: String,
    context: Context<E>,
) -> Result<Value, Error> {
    compare(state, column, value, CompareOp::Gt, context).await
}

pub async fn gte<E: Environment<Value = Value>>(
    state: State<Value>,
    column: String,
    value: String,
    context: Context<E>,
) -> Result<Value, Error> {
    compare(state, column, value, CompareOp::Ge, context).await
}

pub async fn lt<E: Environment<Value = Value>>(
    state: State<Value>,
    column: String,
    value: String,
    context: Context<E>,
) -> Result<Value, Error> {
    compare(state, column, value, CompareOp::Lt, context).await
}

pub async fn lte<E: Environment<Value = Value>>(
    state: State<Value>,
    column: String,
    value: String,
    context: Context<E>,
) -> Result<Value, Error> {
    compare(state, column, value, CompareOp::Le, context).await
}

pub async fn starts_with<E: Environment<Value = Value>>(
    state: State<Value>,
    column: String,
    text: String,
    case_sensitive: bool,
    context: Context<E>,
) -> Result<Value, Error> {
    let expr = SearchExpr::field(&column, FieldTest::prefix(&text));
    run_filter(state, expr, Some(&column), case_sensitive, context).await
}

pub async fn ends_with<E: Environment<Value = Value>>(
    state: State<Value>,
    column: String,
    text: String,
    case_sensitive: bool,
    context: Context<E>,
) -> Result<Value, Error> {
    let expr = SearchExpr::field(&column, FieldTest::suffix(&text));
    run_filter(state, expr, Some(&column), case_sensitive, context).await
}

pub async fn contains<E: Environment<Value = Value>>(
    state: State<Value>,
    column: String,
    text: String,
    case_sensitive: bool,
    context: Context<E>,
) -> Result<Value, Error> {
    let expr = SearchExpr::field(&column, FieldTest::substring(&text));
    run_filter(state, expr, Some(&column), case_sensitive, context).await
}

/// `*` and `?` wildcards; without them, an exact match.
pub async fn glob<E: Environment<Value = Value>>(
    state: State<Value>,
    column: String,
    pattern: String,
    case_sensitive: bool,
    context: Context<E>,
) -> Result<Value, Error> {
    let expr = SearchExpr::field(&column, FieldTest::Glob(pattern));
    run_filter(state, expr, Some(&column), case_sensitive, context).await
}

pub async fn is_in<E: Environment<Value = Value>>(
    state: State<Value>,
    column: String,
    values: Vec<String>,
    context: Context<E>,
) -> Result<Value, Error> {
    if values.is_empty() {
        return Err(Error::general_error(
            "ns-rec/is_in requires at least one value".to_string(),
        ));
    }
    let items = values
        .into_iter()
        .map(|value| SearchExpr::field(&column, FieldTest::Compare(CompareOp::Eq, value)))
        .collect();
    run_filter(state, SearchExpr::any(items), Some(&column), true, context).await
}

pub async fn is_null<E: Environment<Value = Value>>(
    state: State<Value>,
    column: String,
    context: Context<E>,
) -> Result<Value, Error> {
    let expr = SearchExpr::not(SearchExpr::field(&column, FieldTest::Glob("*".to_string())));
    run_filter(state, expr, Some(&column), true, context).await
}

pub async fn is_not_null<E: Environment<Value = Value>>(
    state: State<Value>,
    column: String,
    context: Context<E>,
) -> Result<Value, Error> {
    let expr = SearchExpr::field(&column, FieldTest::Glob("*".to_string()));
    run_filter(state, expr, Some(&column), true, context).await
}

/// A search-language expression, applied as `ns-search/search` with `rank=false`, `limit=0`:
/// unknown fields warn and match nothing, an unparseable expression searches its words.
pub async fn filter<E: Environment<Value = Value>>(
    state: State<Value>,
    query: String,
    context: Context<E>,
) -> Result<Value, Error> {
    let (expr, parse_error) = SearchExpr::parse_lenient(&query);
    if let Some(error) = parse_error {
        context.warning(&format!("{error}; searching the words of the expression instead"))?;
    }
    run_filter(state, expr, None, false, context).await
}

// ------------------------------------------------------------------------------------------------
// Ordering, rows and columns
// ------------------------------------------------------------------------------------------------

/// Stable sort by one column: nulls first unless `nulls_last`, NaN above every number (polars).
/// A source is materialized first.
pub async fn sort<E: Environment<Value = Value>>(
    state: State<Value>,
    column: String,
    ascending: bool,
    nulls_last: bool,
    context: Context<E>,
) -> Result<Value, Error> {
    let view = table_view(&state, &context).await?;
    let index = field_index(view.schema(), &column)?;
    let order = view.column(index)?.sort_indices(ascending, nulls_last)?;
    Ok(Value::from_record_view(view.take(&order)?))
}

/// The last `n` rows (default 5), materialized for inspection, as `head` is. A source is
/// materialized first.
pub async fn tail<E: Environment<Value = Value>>(
    state: State<Value>,
    n: i64,
    context: Context<E>,
) -> Result<Value, Error> {
    let view = table_view(&state, &context).await?;
    let take = non_negative_usize(n, "n")?.min(view.len());
    let batch = view.slice(view.len() - take, take)?.materialize()?;
    Ok(Value::from_record_view(batch as Arc<dyn RecordView>))
}

/// The view function behind `drop_columns`: every field but `columns`, in schema order. Dropping
/// an unknown field, the Id field or the Source field is refused — `select_columns` keeps those,
/// and so must every projection.
fn drop_map(columns: Vec<String>) -> ViewMap {
    Arc::new(move |view: Arc<dyn RecordView>| {
        let schema = view.schema().clone();
        for column in &columns {
            let index = field_index(&schema, column)?;
            let key = [schema.id_field(), schema.source_field()];
            if key.contains(&Some(index)) {
                return Err(Error::general_error(format!(
                    "ns-rec/drop_columns cannot drop the key field '{column}' (an Id or Source \
                     field is kept by every projection)"
                )));
            }
        }
        let keep: Vec<&str> = schema
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .filter(|name| !columns.iter().any(|column| column == name))
            .collect();
        view.select_columns(&keep)
    })
}

pub async fn drop_columns<E: Environment<Value = Value>>(
    state: State<Value>,
    columns: Vec<String>,
    context: Context<E>,
) -> Result<Value, Error> {
    if columns.is_empty() {
        return Err(Error::general_error(
            "ns-rec/drop_columns requires at least one column name".to_string(),
        ));
    }
    let map = drop_map(columns);
    match table_input(&state, &context).await? {
        TableInput::View(view) => Ok(Value::from_record_view(map(view)?)),
        TableInput::Source(source) => {
            Ok(Value::from_record_source(Arc::new(MappedSource::new(source, map)?)))
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Counts and the schema
// ------------------------------------------------------------------------------------------------

/// Rows, the schema, and the non-null count per column — over a source summed chunk by chunk,
/// never concatenated.
struct Tally {
    rows: usize,
    schema: Option<Arc<RecordSchema>>,
    non_null: Vec<usize>,
}

/// What a count needs: `Rows` reads only lengths (so chunks may disagree on columns), `Columns`
/// also needs one schema and the per-column non-null counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TallyMode {
    Rows,
    Columns,
}

impl Tally {
    /// Adopts `schema` as the one every chunk must match, with a zero count per field — so a
    /// declared source with no chunks still counts zero for each column.
    fn adopt(&mut self, schema: Arc<RecordSchema>) {
        self.non_null = vec![0; schema.fields.len()];
        self.schema = Some(schema);
    }
}

/// Chunks of an undeclared source may differ; counting by position across them would put one
/// chunk's counts under another's names. Same names and types in order, or refused.
fn check_same_columns(expected: &RecordSchema, actual: &RecordSchema) -> Result<(), Error> {
    let differing = expected
        .fields
        .iter()
        .zip(actual.fields.iter())
        .position(|(e, a)| e.name != a.name || e.data_type != a.data_type);
    if expected.fields.len() == actual.fields.len() && differing.is_none() {
        return Ok(());
    }
    let at = differing.unwrap_or(expected.fields.len().min(actual.fields.len()));
    let name = |schema: &RecordSchema| {
        schema.fields.get(at).map(|f| f.name.clone()).unwrap_or_else(|| "(none)".to_string())
    };
    Err(Error::general_error(format!(
        "the input's chunks differ in schema at field {at} ('{}' vs '{}'); column counts need one \
         schema (nrows does not)",
        name(expected),
        name(actual)
    )))
}

fn tally_view(tally: &mut Tally, view: &Arc<dyn RecordView>, mode: TallyMode) -> Result<(), Error> {
    tally.rows += view.len();
    match mode {
        TallyMode::Rows => return Ok(()),
        TallyMode::Columns => {}
    }
    match &tally.schema {
        Some(schema) => check_same_columns(schema, view.schema())?,
        None => tally.adopt(view.schema().clone()),
    }
    for (index, count) in tally.non_null.iter_mut().enumerate() {
        let nulls = view.column(index)?.null_mask().count_ones();
        *count += view.len() - nulls;
    }
    Ok(())
}

async fn tally<E: Environment<Value = Value>>(
    state: &State<Value>,
    context: &Context<E>,
    mode: TallyMode,
) -> Result<Tally, Error> {
    let mut tally = Tally {
        rows: 0,
        schema: None,
        non_null: Vec::new(),
    };
    match table_input(state, context).await? {
        TableInput::View(view) => tally_view(&mut tally, &view, mode)?,
        TableInput::Source(source) => {
            use futures::StreamExt;
            if let Some(schema) = source.schema() {
                tally.adopt(schema);
            }
            let resolver: Arc<dyn ChunkResolver> = Arc::new(ContextResolver::new(context.clone()));
            let mut stream = source.stream(resolver).await?;
            while let Some(view) = stream.next().await {
                tally_view(&mut tally, &view?, mode)?;
            }
        }
    }
    Ok(tally)
}

fn count_i64(n: usize) -> Result<i64, Error> {
    i64::try_from(n).map_err(|_| Error::general_error(format!("count {n} exceeds i64")))
}

pub async fn nrows<E: Environment<Value = Value>>(
    state: State<Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    Ok(Value::Base(SimpleValue::from(count_i64(tally(&state, &context, TallyMode::Rows).await?.rows)?)))
}

pub async fn ncols<E: Environment<Value = Value>>(
    state: State<Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    let tally = tally(&state, &context, TallyMode::Columns).await?;
    let ncols = tally.schema.map(|schema| schema.fields.len()).unwrap_or(0);
    Ok(Value::Base(SimpleValue::from(count_i64(ncols)?)))
}

/// `{nrows, ncols}`, the form `ns-pl/shape` returns.
pub async fn shape<E: Environment<Value = Value>>(
    state: State<Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    let tally = tally(&state, &context, TallyMode::Columns).await?;
    let ncols = tally.schema.map(|schema| schema.fields.len()).unwrap_or(0);
    let mut map = std::collections::BTreeMap::new();
    map.insert("nrows".to_string(), SimpleValue::from(count_i64(tally.rows)?));
    map.insert("ncols".to_string(), SimpleValue::from(count_i64(ncols)?));
    Ok(Value::Base(SimpleValue::Object { value: map }))
}

/// One row: the non-null count of every column, as an `Int` column of the same name — the form
/// `ns-pl/count` returns.
pub async fn count<E: Environment<Value = Value>>(
    state: State<Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    let tally = tally(&state, &context, TallyMode::Columns).await?;
    let schema = tally.schema.ok_or_else(|| {
        Error::general_error("ns-rec/count: the input declares no schema and has no rows".to_string())
    })?;
    let fields = schema
        .fields
        .iter()
        .map(|field| FieldSchema::new(&field.name, FieldType::Int))
        .collect();
    let columns = tally
        .non_null
        .iter()
        .map(|&n| {
            Ok(Column::Int {
                validity: None,
                values: Buffer::from_slice(&[count_i64(n)?]),
            })
        })
        .collect::<Result<Vec<Column>, Error>>()?;
    let batch = RecordBatch::new(Arc::new(RecordSchema::new(fields)?), columns, None, None, vec![])?;
    Ok(Value::from_record_view(Arc::new(batch) as Arc<dyn RecordView>))
}

/// The schema as JSON text — what a `schema` argument and a manifest's `uniform_schema` accept,
/// linked or pasted. A source answers with its declared schema, or its first chunk's.
pub async fn schema<E: Environment<Value = Value>>(
    state: State<Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    let schema = match table_input(&state, &context).await? {
        TableInput::View(view) => view.schema().clone(),
        TableInput::Source(source) => match source.schema() {
            Some(schema) => schema,
            None => {
                use futures::StreamExt;
                let resolver: Arc<dyn ChunkResolver> =
                    Arc::new(ContextResolver::new(context.clone()));
                let mut stream = source.stream(resolver).await?;
                match stream.next().await {
                    Some(view) => view?.schema().clone(),
                    None => {
                        return Err(Error::general_error(
                            "ns-rec/schema: the source declares no schema and has no chunks"
                                .to_string(),
                        ))
                    }
                }
            }
        },
    };
    let text = serde_json::to_string_pretty(schema.as_ref())
        .map_err(|error| Error::from_error(liquers_core::error::ErrorType::SerializationError, error))?;
    Ok(Value::from_string(text))
}
