// liquers-lib/tests/record_manifest_keyed.rs
#![cfg(feature = "records")]

use std::sync::Arc;

use liquers_core::{
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    interpreter::evaluate,
    parse::parse_key,
    state::State,
};
use liquers_lib::{
    records::{to_record_source, ToRecordOptions},
    value::Value,
};
use liquers_macro::register_command;
use liquers_records::{
    ChunkList, ChunkTemplate, FieldSchema, FieldType, KeyRole, ManifestSource, ManifestSpec,
    RecordSchema, RecordSource,
};

/// `register_command!` resolves a `context` parameter's environment through this alias.
type CommandEnvironment = SimpleEnvironment<Value>;

fn template_spec(stored: bool, cached: bool, uniform_schema: Option<Arc<RecordSchema>>) -> ManifestSpec {
    ManifestSpec {
        chunks: vec![],
        template: Some(ChunkTemplate {
            query: "ns-sql/sql_query".to_string(),
            first_offset: 0,
            step: 1000,
            batch_size: 1000,
        }),
        extension: Some("csv".to_string()),
        stored,
        cached,
        uniform_schema,
        ..ManifestSpec::default()
    }
}

#[test]
fn manifest_with_key_derives_unbounded_naming() -> Result<(), Box<dyn std::error::Error>> {
    let key = parse_key("data/sales/daily.manifest.yaml")?;
    let source = ManifestSource::new(template_spec(true, true, None), None)?.with_key(key)?;
    match source.chunks() {
        ChunkList::Unbounded { computed } => assert_eq!(computed.len(), 0),
        ChunkList::Known(_) => panic!("expected Unbounded for a manifest with a template"),
    }
    Ok(())
}

#[test]
fn keyed_chunk_addressing_carries_uniform_schema() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("order_id", FieldType::Int).with_key(KeyRole::Id),
    ])?);
    let spec = template_spec(false, true, Some(schema));
    let source = ManifestSource::new(spec, None)?.with_key(parse_key("data/sales/daily.manifest.yaml")?)?;
    assert!(source.schema().is_some());
    Ok(())
}

#[test]
fn chunk_list_unbounded_with_template_has_no_computed_ids_yet() -> Result<(), Box<dyn std::error::Error>> {
    let source = ManifestSource::new(template_spec(true, true, None), None)?
        .with_key(parse_key("data/sales/daily.manifest.yaml")?)?;
    match source.chunks() {
        ChunkList::Unbounded { computed } => assert_eq!(computed.len(), 0),
        ChunkList::Known(_) => panic!("expected Unbounded"),
    }
    Ok(())
}

#[test]
fn stored_false_cached_true_are_preserved_on_spec() -> Result<(), Box<dyn std::error::Error>> {
    let source = ManifestSource::new(template_spec(false, true, None), None)?;
    assert!(!source.spec().stored);
    assert!(source.spec().cached);
    Ok(())
}

#[test]
fn manifest_schema_is_applied_to_the_whole_source() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id),
        FieldSchema::new("value", FieldType::Float),
    ])?);
    let source = ManifestSource::new(template_spec(true, true, Some(schema)), None)?;
    let source_schema = source.schema().expect("uniform_schema declared");
    assert_eq!(source_schema.fields.len(), 2);
    Ok(())
}

#[test]
fn manifest_spec_reads_its_envelope_fields() -> Result<(), Box<dyn std::error::Error>> {
    // `manifest:` and `version:` are fields of `ManifestSpec`: a whole manifest document
    // deserializes directly, and the discriminator is written back on serialization.
    let yaml = r#"
manifest: record-stream
version: 1
extension: csv
stored: false
cached: true
chunks: []
template:
  query: ns-sql/sql_query
  first_offset: 0
  step: 1000
  batch_size: 1000
"#;
    let spec: ManifestSpec = serde_yaml::from_str(yaml)?;
    assert!(!spec.stored);
    assert!(spec.cached);
    assert!(spec.template.is_some());
    Ok(())
}

// `to_record_source` needs a `Context`, whose constructor is async and takes an `AssetRef`.
// The tests therefore evaluate a query ending in a probe command, as
// `liquers-core/tests/async_hellow_world.rs` does, instead of building a `Context` by hand.

fn manifest_text(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("manifest: record-stream\nchunks: []\n"))
}

fn not_a_manifest_text(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("chunks: []\n")) // valid YAML, no `manifest:` key
}

/// Reports what `to_record_source` made of the state: `"schema:<bool>,manifest:<bool>"`, or the
/// error text prefixed with `"error:"`.
async fn probe_source(
    state: State<Value>,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    let options = ToRecordOptions::default();
    match to_record_source(state.data_unchecked(), &state.metadata, &options, &context).await {
        Ok(source) => Ok(Value::from(format!(
            "schema:{},manifest:{}",
            source.schema().is_some(),
            source.manifest().is_some()
        ))),
        Err(e) => Ok(Value::from(format!("error:{e}"))),
    }
}

async fn probe(query: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn manifest_text(state) -> result)?;
    register_command!(cr, fn not_a_manifest_text(state) -> result)?;
    register_command!(cr, async fn probe_source(state, context) -> result)?;
    let state = evaluate(env.to_ref(), query, None).await?;
    Ok(state.try_into_string()?)
}

#[tokio::test]
async fn to_record_source_recognizes_manifest_discriminator() -> Result<(), Box<dyn std::error::Error>> {
    // No `uniform_schema` declared, so the source has no schema.
    assert_eq!(probe("manifest_text/probe_source").await?, "schema:false,manifest:true");
    Ok(())
}

#[tokio::test]
async fn to_record_source_rejects_missing_discriminator() -> Result<(), Box<dyn std::error::Error>> {
    // Text without `manifest: record-stream` is never taken for a manifest; with no data format
    // to parse it as a table either, it is refused rather than sniffed.
    assert!(probe("not_a_manifest_text/probe_source").await?.starts_with("error:"));
    Ok(())
}

#[tokio::test]
async fn to_record_source_leaves_source_keyless_without_metadata_key() -> Result<(), Box<dyn std::error::Error>> {
    // A manifest built by a command (never stored) has no metadata key; its chunks stay unkeyed
    // rather than the call failing.
    assert!(probe("manifest_text/probe_source").await?.ends_with("manifest:true"));
    Ok(())
}
