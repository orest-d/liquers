// liquers-lib/tests/to_record_conversions.rs
#![cfg(feature = "records")]

use liquers_core::{
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    interpreter::evaluate,
    state::State,
    value::ValueInterface,
};
use liquers_macro::register_command;
use liquers_lib::{
    records::{to_record, ToRecordOptions},
    value::{ExtValueInterface, Value},
};

/// `register_command!` resolves a `context` parameter's environment through this alias.
type CommandEnvironment = SimpleEnvironment<Value>;

fn csv_bytes(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from_bytes(b"id,name\n1,Alice\n2,Bob".to_vec()))
}

async fn probe_row_count(
    state: State<Value>,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    let options = ToRecordOptions { format: Some("csv".to_string()), ..Default::default() };
    let view = to_record(state.data_unchecked(), &state.metadata, &options, &context).await?;
    Ok(Value::from(view.len() as i64))
}

#[tokio::test]
async fn to_record_accepts_csv_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn csv_bytes(state) -> result)?;
    register_command!(cr, async fn probe_row_count(state, context) -> result)?;
    let envref = env.to_ref();
    let state = evaluate(envref, "csv_bytes/probe_row_count", None).await?;
    assert_eq!(state.value()?.try_into_i64()?, 2);
    Ok(())
}

async fn probe_refuses_source(
    state: State<Value>,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    let options = ToRecordOptions::default();
    match to_record(state.data_unchecked(), &state.metadata, &options, &context).await {
        Err(e) => Ok(Value::from(format!("{e}"))), // report the message so the test can inspect it
        Ok(_) => Err(Error::general_error("expected to_record to refuse a source".to_string())),
    }
}

fn empty_manifest_source(_state: &State<Value>) -> Result<Value, Error> {
    use liquers_records::{ManifestSource, ManifestSpec};
    use std::sync::Arc;
    let spec = ManifestSpec { chunks: vec![], template: None, extension: None, stored: true, cached: true, uniform_schema: None, ..ManifestSpec::default() };
    let source: Arc<dyn liquers_records::RecordSource> = Arc::new(ManifestSource::new(spec, None)?);
    Ok(Value::from_record_source(source))
}

#[tokio::test]
async fn to_record_refuses_a_source_naming_materialize() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn empty_manifest_source(state) -> result)?;
    register_command!(cr, async fn probe_refuses_source(state, context) -> result)?;
    let envref = env.to_ref();
    let state = evaluate(envref, "empty_manifest_source/probe_refuses_source", None).await?;
    assert!(state.try_into_string()?.to_lowercase().contains("materialize"));
    Ok(())
}

fn unlabelled_text(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("id,name\n1,Alice")) // text with no data_format in its metadata
}

async fn probe_refuses_unlabelled_text(
    state: State<Value>,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    let options = ToRecordOptions::default(); // format: None — never sniffed
    match to_record(state.data_unchecked(), &state.metadata, &options, &context).await {
        Err(_) => Ok(Value::from(true)),
        Ok(_) => Err(Error::general_error("expected to_record to refuse unlabelled text".to_string())),
    }
}

#[tokio::test]
async fn to_record_refuses_unlabelled_text_rather_than_guessing() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn unlabelled_text(state) -> result)?;
    register_command!(cr, async fn probe_refuses_unlabelled_text(state, context) -> result)?;
    let envref = env.to_ref();
    let state = evaluate(envref, "unlabelled_text/probe_refuses_unlabelled_text", None).await?;
    assert!(state.value()?.try_into_bool()?);
    Ok(())
}
