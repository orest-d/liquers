//! The generator example of `specs/guides/RECORD_STREAM_GUIDE.md` §6.4: a command returning a
//! **generator view** (a `RowFnView`, whose cells are computed only when read — nothing is
//! materialized), and a command returning a **record source** whose chunks are calls to that
//! command, through a manifest template.
//!
//! The rows sample `x + offset`, `sin(a·x + offset)`, `sin(b·x + offset)` and their sum at
//! `x = 0, 1, 2, …`. With `a = 1` and `b = √2` the two frequencies are incommensurate (their ratio
//! is irrational), so the sum never repeats.
#![cfg(feature = "records")]

use std::sync::Arc;

use liquers_core::context::{Context, EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::store::AsyncMemoryStore;
use liquers_core::query::Key;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::records::{
    ChunkTemplate, FieldSchema, FieldType, FieldValue, ManifestSource, ManifestSpec, RecordSchema,
    RecordView, RowFnView, RowId,
};
use liquers_lib::register_records_commands;
use liquers_lib::value::{ExtValueInterface, Value};

/// The four columns every chunk has.
fn waves_schema() -> Result<Arc<RecordSchema>, Error> {
    Ok(Arc::new(RecordSchema::new(vec![
        FieldSchema::new("x", FieldType::Float).with_label("x + offset").not_null(),
        FieldSchema::new("wave_a", FieldType::Float).with_label("sin(a·x + offset)").not_null(),
        FieldSchema::new("wave_b", FieldType::Float).with_label("sin(b·x + offset)").not_null(),
        FieldSchema::new("sum", FieldType::Float).with_label("wave_a + wave_b").not_null(),
    ])?))
}

/// A generator view over rows `start .. min(start + length, total)`. Returns a `RowFnView`: the
/// closure computes a cell only when it is read, so nothing is stored. `total` bounds the series,
/// so the chunk at the end comes back short — which is what ends a manifest template's walk.
fn waves(a: f64, b: f64, offset: f64, total: i64, start: i64, length: i64) -> Result<Value, Error> {
    let rows = usize::try_from(length.min(total - start).max(0))
        .map_err(|_| Error::general_error(format!("waves: bad row count for start {start}")))?;
    let view = RowFnView::new(waves_schema()?, rows, move |row, col| {
        let x = (start + row as i64) as f64;
        let wave_a = (a * x + offset).sin();
        let wave_b = (b * x + offset).sin();
        match col {
            0 => Ok(FieldValue::Float(x + offset)),
            1 => Ok(FieldValue::Float(wave_a)),
            2 => Ok(FieldValue::Float(wave_b)),
            3 => Ok(FieldValue::Float(wave_a + wave_b)),
            other => Err(Error::general_error(format!("waves: no column {other}"))),
        }
    })?;
    Ok(Value::from_record_view(Arc::new(view)))
}

/// A record source over `total` rows of `waves`, `batch` rows per chunk. A keyless manifest whose
/// template renders chunk `i` as `ns-demo/waves-<a>-<b>-<offset>-<total>-<i·batch>-<batch>`: the
/// template appends the chunk's offset and batch size to its query. Chunks are evaluated one at a
/// time, when the source is streamed.
fn waves_source(a: f64, b: f64, offset: f64, total: i64, batch: i64) -> Result<Value, Error> {
    let batch = u64::try_from(batch)
        .map_err(|_| Error::general_error("waves_source: batch must not be negative".to_string()))?;
    let spec = ManifestSpec {
        template: Some(ChunkTemplate {
            query: format!("ns-demo/waves-{a}-{b}-{offset}-{total}"),
            first_offset: 0,
            step: batch,
            batch_size: batch,
        }),
        uniform_schema: Some(waves_schema()?),
        ..ManifestSpec::default()
    };
    Ok(Value::from_record_source(Arc::new(ManifestSource::new(spec, None)?)))
}

fn build_env() -> Result<EnvRef<DefaultEnvironment<Value>>, Error> {
    type CommandEnvironment = DefaultEnvironment<Value>;

    let mut env = DefaultEnvironment::<Value>::new();
    {
        let cr = env.get_mut_command_registry();
        register_records_commands!(cr)?;
        register_command!(cr,
            fn waves(a: f64, b: f64, offset: f64, total: i64, start: i64, length: i64) -> result
            namespace: "demo"
            label: "Two sine waves"
        )?;
        register_command!(cr,
            fn waves_source(a: f64, b: f64, offset: f64, total: i64, batch: i64) -> result
            namespace: "demo"
            label: "Two sine waves, chunked"
        )?;
    }
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    Ok(env.to_ref())
}

async fn eval(
    envref: EnvRef<DefaultEnvironment<Value>>,
    query: &str,
) -> Result<liquers_core::state::State<Value>, Error> {
    envref.evaluate(query).await?.get().await
}

fn float(view: &Arc<dyn RecordView>, row: usize, col: usize) -> Result<f64, Error> {
    match view.value(row, col)? {
        FieldValue::Float(value) => Ok(value),
        other => Err(Error::general_error(format!("expected a Float, got {other:?}"))),
    }
}

/// `ns-demo/waves-…` returns a generator: a view that is not a `RecordBatch`, whose cells are the
/// formula's values at the requested rows.
#[tokio::test]
async fn waves_returns_a_generator_view() -> Result<(), Box<dyn std::error::Error>> {
    let envref = build_env()?;
    let state = eval(envref, "ns-demo/waves-1-1.4142135623730951-0.5-100-10-4").await?;
    let view = state.value()?.as_record_view()?;

    assert_eq!(view.len(), 4);
    assert!(view.as_batch().is_none(), "a generator, not a materialized batch");
    assert_eq!(float(&view, 0, 0)?, 10.5, "x + offset at x = 10");
    let x = 12.0;
    let expected = (x + 0.5_f64).sin() + (std::f64::consts::SQRT_2 * x + 0.5).sin();
    assert!((float(&view, 2, 3)? - expected).abs() < 1e-12);
    Ok(())
}

/// `ns-demo/waves_source-…` is a record source whose chunks are `waves` calls. Materializing it
/// walks the chunks — 4 + 4 + 2 rows here, the short last chunk ending the walk — and every row
/// keeps its chunk and row number.
#[tokio::test]
async fn waves_source_materializes_chunk_by_chunk() -> Result<(), Box<dyn std::error::Error>> {
    let envref = build_env()?;
    let state = eval(
        envref,
        "ns-demo/waves_source-1-1.4142135623730951-0.5-10-4/ns-rec/materialize",
    )
    .await?;
    let table = state.value()?.as_record_view()?;

    assert_eq!(table.len(), 10);
    assert_eq!(table.row_id(5)?, RowId { chunk: 1, row: 1 });
    assert_eq!(table.row_number(5)?, Some(5));
    assert_eq!(float(&table, 9, 0)?, 9.5, "the last row is x = 9");
    Ok(())
}
