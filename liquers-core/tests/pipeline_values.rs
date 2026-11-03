//! A pipeline is a value: stored, read back, produced by a command (AC-39 of
//! `composite-command-definition`).

use liquers_core::{
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    interpreter::evaluate,
    metadata::{Metadata, MetadataRecord, Status},
    parse::{parse_key, parse_query},
    pipeline::Pipeline,
    query::Key,
    recipes::DefaultRecipeProvider,
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::{DefaultValueSerializer, Value, ValueInterface},
};
use liquers_macro::register_command;

type TestEnv = SimpleEnvironment<Value>;
type CommandEnvironment = TestEnv;
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn as_pipeline(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from_pipeline(state.value()?.try_into_pipeline()?))
}

/// AC-39 — a pipeline value is stored in each of its formats and read back as itself.
#[tokio::test]
async fn pipeline_value_stored_and_read_back() -> TestResult {
    let pipeline = Pipeline::try_from(&parse_query("-R/data/x.csv/-/ns-pl/head-5")?)?;
    let value = Value::Pipeline(pipeline.clone());
    let store = AsyncMemoryStore::new(&Key::new());
    for (name, format) in [("p.json", "json"), ("p.yaml", "yaml"), ("p.txt", "txt")] {
        let mut record = MetadataRecord::new();
        record.status = Status::Ready;
        record.with_type_identifier("Pipeline".to_owned());
        record.data_format = Some(format.to_owned());
        store
            .set(
                &parse_key(format!("stored/{name}"))?,
                &value.as_bytes(format)?,
                &Metadata::MetadataRecord(record),
            )
            .await?;
    }
    let mut env = TestEnv::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn as_pipeline(state) -> result)?;
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    let envref = env.to_ref();
    for name in ["p.json", "p.yaml", "p.txt"] {
        let state = evaluate(envref.clone(), &format!("-R/stored/{name}"), None).await?;
        assert_eq!(state.value()?.try_into_pipeline()?, pipeline, "{name}");
        assert_eq!(state.value()?.identifier(), "Pipeline", "{name}");
    }
    // A command produces a pipeline from the quoted query before it.
    let state = evaluate(envref.clone(), "-R/data/x.csv/-/ns-pl/head-5/q/as_pipeline", None).await?;
    assert_eq!(*state.value()?, Value::Pipeline(pipeline));
    Ok(())
}
