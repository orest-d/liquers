//! Real tests for the two `ChunkResolver` implementations' dependency-recording contract, relocated
//! from Phase 3 §5.8's `#[ignore]`d sketches by
//! `specs/design/record-streams/phase4-implementation.md` Step 5.6.
//!
//! `ContextResolver::evaluate(q)` adds `q` to the calling asset's dependencies (it evaluates through
//! `Context::get_dependency_state`, which records the dependency as any nested `-R/`/link
//! evaluation does); `EnvResolver::evaluate(q)` adds nothing (it evaluates through a bare `EnvRef`,
//! which outlives any one request and has no "calling asset" to record against — the resolver
//! `liquers-axum` uses to serve a source over HTTP). Both are driven through `evaluate` and a probe
//! command that builds the resolver from its `Context`/`EnvRef` and evaluates a small target query.
//!
//! Needs `liquers-lib`'s `Value: RecordValue`, which is why these live here rather than in
//! `liquers-records` (see `sources.rs`'s module doc: "`ContextResolver`/`EnvResolver` are
//! compile-checked here only").
#![cfg(feature = "records")]

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::interpreter::evaluate;
use liquers_core::metadata::DependencyKey;
use liquers_core::parse::parse_query;
use liquers_core::value::ValueInterface;
use liquers_macro::register_command;
use liquers_records::{
    ChunkResolver, ContextResolver, EnvResolver, FieldSchema, FieldType, FieldValue,
    RecordBatchMut, RecordSchema, RecordView, RecordViewMut,
};

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::value::{ExtValueInterface, Value};

type CommandEnvironment = DefaultEnvironment<Value>;

/// The dependency every probe command evaluates through its resolver.
///
/// A `RecordView`, not a scalar: `ChunkResolver::evaluate` classifies a resolved dependency
/// through `classify_state` (`liquers-records/src/sources.rs`), which checks
/// `value.as_record_view()` **before** ever calling `state.as_bytes()`. A scalar such as `I64`
/// takes the fallback branch instead, and that call fails with `Unsupported format bin`: an
/// ad-hoc query asset with no filename of its own (`target_value` has none) gets a *declared*
/// `data_format` of `"bin"` from `Recipe::get_asset_info`/`Recipe::data_format`'s fallback —
/// unconditionally, not only through the free `evaluate()` function
/// (`FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT`, filed by this step) — and `liquers-lib`'s
/// `SimpleValue::as_bytes` has no `"bin"` arm at all (its own generic-bytes format is spelled
/// `"b"`), so every scalar fails the same way regardless of which value is picked. A `RecordView`
/// sidesteps the whole defect rather than re-demonstrating it: these tests are about dependency
/// recording, not about the format bug.
fn target_value() -> Result<Value, Error> {
    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("value", FieldType::Int)])?);
    let mut batch = RecordBatchMut::with_capacity(schema, 1);
    batch.append_row(&[FieldValue::Int(42)])?;
    let view: Arc<dyn RecordView> = Arc::new(batch.freeze()?);
    Ok(Value::from_record_view(view))
}

/// Evaluates `target_value` through a [`ContextResolver`] built from this command's own `Context`.
async fn context_resolver_probe<E: Environment<Value = Value>>(
    context: Context<E>,
) -> Result<Value, Error> {
    let resolver = ContextResolver::new(context.clone());
    let query = parse_query("target_value")?;
    let _ = ChunkResolver::evaluate(&resolver, query).await?;
    Ok(Value::none())
}

/// Evaluates `target_value` through an [`EnvResolver`] built from this command's `EnvRef` — the
/// same query, resolved the way `liquers-axum` would for a request that must outlive itself.
async fn env_resolver_probe<E: Environment<Value = Value>>(
    context: Context<E>,
) -> Result<Value, Error> {
    let resolver = EnvResolver::new(context.get_envref());
    let query = parse_query("target_value")?;
    let _ = ChunkResolver::evaluate(&resolver, query).await?;
    Ok(Value::none())
}

fn build_env() -> Result<DefaultEnvironment<Value>, Error> {
    let mut env = DefaultEnvironment::<Value>::new();
    let cr = env.get_mut_command_registry();
    register_command!(cr, fn target_value() -> result)?;
    register_command!(cr, async fn context_resolver_probe(context) -> result)?;
    register_command!(cr, async fn env_resolver_probe(context) -> result)?;
    Ok(env)
}

/// Contract: after `ContextResolver::evaluate(q)`, the current asset's `Metadata.dependencies`
/// contains `q`.
#[tokio::test]
async fn context_resolver_records_each_evaluated_chunk_as_a_dependency(
) -> Result<(), Box<dyn std::error::Error>> {
    let env = build_env()?;
    let state = evaluate(env.to_ref(), "context_resolver_probe", None).await?;

    let target_key = DependencyKey::from(&parse_query("target_value")?);
    let deps = state.metadata.get_dependencies();
    assert!(
        deps.iter().any(|dependency| dependency.key == target_key),
        "ContextResolver::evaluate must record its query as a dependency of the calling asset; \
         got {deps:?}"
    );
    Ok(())
}

/// Contract: `EnvResolver::evaluate(q)` does not add `q` to any asset's dependencies — it is used
/// where the caller (`liquers-axum`) outlives the request.
#[tokio::test]
async fn env_resolver_records_no_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let env = build_env()?;
    let state = evaluate(env.to_ref(), "env_resolver_probe", None).await?;

    let deps = state.metadata.get_dependencies();
    assert!(
        deps.is_empty(),
        "EnvResolver::evaluate must add no dependency; got {deps:?}"
    );
    Ok(())
}
