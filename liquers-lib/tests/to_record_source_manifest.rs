// liquers-lib/tests/to_record_source_manifest.rs
#![cfg(feature = "records")]

use liquers_core::{
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    interpreter::evaluate,
    state::State,
    value::ValueInterface,
};
use liquers_macro::register_command;
use liquers_lib::{records::{to_record_source, ToRecordOptions}, value::Value};

/// `register_command!` resolves a `context` parameter's environment through this alias.
type CommandEnvironment = SimpleEnvironment<Value>;

fn manifest_text(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("manifest: record-stream\nchunks: []\n"))
}

async fn probe_is_manifest(
    state: State<Value>,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    let options = ToRecordOptions::default();
    let source = to_record_source(state.data_unchecked(), &state.metadata, &options, &context).await?;
    Ok(Value::from(source.manifest().is_some()))
}

#[tokio::test]
async fn to_record_source_recognizes_the_manifest_discriminator() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn manifest_text(state) -> result)?;
    register_command!(cr, async fn probe_is_manifest(state, context) -> result)?;
    let envref = env.to_ref();
    let state = evaluate(envref, "manifest_text/probe_is_manifest", None).await?;
    assert!(state.value()?.try_into_bool()?);
    Ok(())
}

// `to_record_source_leaves_source_keyless_without_metadata_key` and
// `to_record_source_rejects_missing_discriminator` in §1.2 already cover the key-derivation and
// error paths through the free function directly; this file adds only the through-`evaluate` path
// a registered `ns-rec` command actually takes.
