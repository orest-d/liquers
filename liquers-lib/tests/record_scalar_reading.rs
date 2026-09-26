//! Phase 3 §5.5 / Phase 4 Step 5.3 — scalar reading through `ValueExtension`'s hooks.
//!
//! A view with exactly one row and one payload column reads as that cell
//! (`specs/design/record-streams/phase2-architecture.md` §"A view as a value"); anything else
//! refuses, naming the shape it actually has.

#![cfg(feature = "records")]

use std::convert::TryFrom;
use std::sync::Arc;

use liquers_core::query::Key;
use liquers_core::store::{AsyncMemoryStore, AsyncStore};
use liquers_core::value::ValueInterface;
use liquers_core::{
    context::{Context, Environment},
    error::Error,
    metadata::Metadata,
    parse::parse_key,
};
use liquers_macro::register_command;
use liquers_records::{
    Buffer, Column, FieldSchema, FieldType, FieldValue, RecordBatch, RecordSchema, RecordView,
};

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::value::{ExtValueInterface, Value};

type CommandEnvironment = DefaultEnvironment<Value>;

fn one_row_one_column_batch() -> Result<RecordBatch, Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new(
        "value",
        FieldType::Int,
    )])?);
    let column = Column::Int {
        validity: None,
        values: Buffer::from_slice(&[42i64]),
    };
    Ok(RecordBatch::new(schema, vec![column], None, None, vec![])?)
}

fn one_row_one_float_column_batch(value: f64) -> Result<RecordBatch, Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new(
        "price",
        FieldType::Float,
    )])?);
    let column = Column::Float {
        validity: None,
        values: Buffer::from_slice(&[value]),
    };
    Ok(RecordBatch::new(schema, vec![column], None, None, vec![])?)
}

fn two_row_batch() -> Result<RecordBatch, Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new(
        "value",
        FieldType::Int,
    )])?);
    let column = Column::Int {
        validity: None,
        values: Buffer::from_slice(&[1i64, 2]),
    };
    Ok(RecordBatch::new(schema, vec![column], None, None, vec![])?)
}

#[test]
fn single_cell_view_reads_as_a_scalar() -> Result<(), Box<dyn std::error::Error>> {
    let batch = one_row_one_column_batch()?;
    assert_eq!(batch.value(0, 0)?, FieldValue::Int(42));
    Ok(())
}

/// `single_cell` is also reachable through `ValueExtension`'s hooks, not just directly on the
/// `RecordBatch` — this is the path a linked command argument actually uses.
#[test]
fn single_cell_view_reads_as_a_scalar_through_the_value_hooks() -> Result<(), Box<dyn std::error::Error>>
{
    let batch = one_row_one_column_batch()?;
    let view: Arc<dyn RecordView> = Arc::new(batch);
    let value = Value::from_record_view(view);

    // The cell reads as the base value `I64(42)` would: as `i64`, and as `f64` through the base
    // value's own (lossy) widening — and not as `i32`, which the base `I64` refuses too.
    assert_eq!(value.try_into_i64()?, 42);
    assert_eq!(i64::try_from(value.clone())?, 42);
    assert_eq!(value.try_into_f64()?, 42.0);
    assert_eq!(f64::try_from(value.clone())?, 42.0);
    let base = liquers_lib::value::SimpleValue::I64 { value: 42 };
    assert_eq!(value.try_into_i32().is_err(), base.try_into_i32().is_err());
    Ok(())
}

/// A single-cell `Float` view answers `try_into_f64`, and refuses `try_into_i32`/`try_into_bool`,
/// exactly as the base value `F64(3.5)` does.
#[test]
fn single_float_cell_answers_f64_and_refuses_mismatched_hooks() -> Result<(), Box<dyn std::error::Error>>
{
    let batch = one_row_one_float_column_batch(3.5)?;
    let view: Arc<dyn RecordView> = Arc::new(batch);
    let value = Value::from_record_view(view);

    assert_eq!(value.try_into_f64()?, 3.5);
    assert!(value.try_into_i32().is_err());
    assert!(value.try_into_bool().is_err());
    Ok(())
}

/// A multi-row view is not a scalar shape, and both ways of asking for a scalar out of it —
/// `ValueInterface::try_into_f64` and the `TryFrom` path a linked argument binds through — refuse
/// with an error naming that shape.
#[test]
fn multi_row_view_refuses_scalar_read_naming_its_shape() -> Result<(), Box<dyn std::error::Error>> {
    let batch = two_row_batch()?;
    let view: Arc<dyn RecordView> = Arc::new(batch);

    let payload_columns = view.schema().payload_fields().len();
    let is_scalar_shape = view.len() == 1 && payload_columns == 1;
    assert!(!is_scalar_shape, "a 2-row, 1-column view is not a scalar shape");

    let value = Value::from_record_view(view);

    let via_interface = value.try_into_f64();
    let message = via_interface
        .as_ref()
        .expect_err("a 2-row view has no single cell to read")
        .to_string();
    assert!(
        message.contains('2') && message.contains('1'),
        "error should name the shape: {message}"
    );

    let via_try_from = f64::try_from(value);
    let message = via_try_from
        .as_ref()
        .expect_err("f64::try_from must refuse the same shape")
        .to_string();
    assert!(
        message.contains('2') && message.contains('1'),
        "error should name the shape: {message}"
    );
    Ok(())
}

fn price_view() -> Result<Value, Error> {
    let batch = one_row_one_float_column_batch(3.5)
        .map_err(|e| Error::general_error(format!("test fixture batch: {e}")))?;
    let view: Arc<dyn RecordView> = Arc::new(batch);
    Ok(Value::from_record_view(view))
}

fn record_price(price: f64) -> Result<Value, Error> {
    Ok(Value::from(price))
}

fn make_env(store: AsyncMemoryStore) -> Result<liquers_core::context::EnvRef<CommandEnvironment>, Error>
{
    let mut env = DefaultEnvironment::<Value>::new();
    {
        let cr = env.get_mut_command_registry();
        register_command!(cr, fn price_view() -> result)?;
        register_command!(cr, fn record_price(price: f64) -> result)?;
    }
    // `DefaultEnvironment::new()` already installs a store-backed recipe provider
    // (`liquers_lib::environment::LibKind`), so no explicit `with_recipe_provider` call is
    // needed here — see `liquers-lib/tests/environment_defaults.rs`.
    env.with_async_store(Box::new(store));
    Ok(env.to_ref())
}

/// Contract (phase2-architecture.md, §"A view as a value"): once
/// `EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS` is fixed (Step 0.1) and `RecordView` answers
/// the scalar hooks (Step 5.3), a recipe's `links:` can bind a one-cell view query into an `f64`
/// command argument exactly as a plain numeric value would.
#[tokio::test]
async fn record_cell_binds_to_an_f64_command_argument_through_a_link(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("data/recipes.yaml")?,
            br#"recipes:
  - query: "record_price/result.txt"
    links:
      price: "price_view"
"#,
            &Metadata::new(),
        )
        .await?;
    let envref = make_env(store)?;

    let asset = envref.evaluate("-R/data/result.txt").await?;
    let state = asset.get().await?;
    assert_eq!(state.value()?.try_into_f64()?, 3.5);
    Ok(())
}

/// A `RecordSource` refuses every scalar hook outright, even one wrapping a single-cell view —
/// scalar reading is a `RecordView` capability only (phase2-architecture.md §"A view as a
/// value" is about views, never sources).
#[test]
fn record_source_refuses_scalar_reads() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_records::InMemorySource;

    let batch = one_row_one_column_batch()?;
    let view: Arc<dyn RecordView> = Arc::new(batch);
    let source: Arc<dyn liquers_records::RecordSource> = Arc::new(InMemorySource::new(vec![view]));
    let value = Value::from_record_source(source);

    assert!(value.try_into_i32().is_err());
    assert!(value.try_into_i64().is_err());
    assert!(value.try_into_f64().is_err());
    assert!(value.try_into_bool().is_err());
    assert!(value.try_into_string().is_err());
    Ok(())
}
