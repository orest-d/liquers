//! Recipe pipelines (AC-15, AC-30 of `specs/design/composite-command-definition/`).

use liquers_core::{
    context::{Context, EnvRef, Environment, ImmediateEnvironment},
    error::Error,
    metadata::Metadata,
    parse::parse_key,
    pipeline::Pipeline,
    plan::Step,
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::{Value, ValueInterface},
};
use liquers_macro::register_command;
use serde_json::json;

type CommandEnvironment = ImmediateEnvironment<Value>;
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn upper(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(state.try_into_string()?.to_uppercase()))
}
fn suffix(state: &State<Value>, s: String) -> Result<Value, Error> {
    Ok(Value::from(format!("{}{}", state.try_into_string()?, s)))
}
fn keys(state: &State<Value>, prefix: String) -> Result<Value, Error> {
    let json = state.value()?.try_into_json_value()?;
    let mut names: Vec<String> = json
        .as_object()
        .map(|o| o.keys().map(|k| format!("{prefix}{k}")).collect())
        .unwrap_or_default();
    names.sort();
    Ok(Value::from(names.join(",")))
}

async fn make_env(recipes: &str) -> Result<EnvRef<CommandEnvironment>, Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("data/recipes.yaml")?,
            recipes.as_bytes(),
            &Metadata::new(),
        )
        .await?;
    store
        .set(&parse_key("data/x.txt")?, b"abc", &Metadata::new())
        .await?;
    let mut env = CommandEnvironment::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn upper(state) -> result)?;
    register_command!(cr, fn suffix(state, s: String) -> result)?;
    register_command!(cr, fn keys(state, prefix: String = "") -> result)?;
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    Ok(env.to_ref())
}

fn recipe(yaml: &str) -> Result<Recipe, Box<dyn std::error::Error>> {
    Ok(serde_yaml::from_str(yaml)?)
}

/// AC-15 — the pipeline runs first and the query continues from its state; alone, it is the
/// recipe, filename included.
#[tokio::test]
async fn recipe_pipeline_runs_before_query() -> TestResult {
    let envref = make_env(
        r#"recipes:
  - pipeline: [{value: {region: [EU], year: 2026}}]
    query: keys-k_/suffix-X/out.txt
  - pipeline:
      elements: [{value: hello}, {action: upper}]
      filename: alone.txt
  - pipeline: [{resource: data/x.txt}]
    query: upper/x.txt
"#,
    )
    .await?;
    let out = envref.evaluate("-R/data/out.txt").await?;
    assert_eq!(out.get().await?.try_into_string()?, "k_region,k_yearX");
    let alone = envref.evaluate("-R/data/alone.txt").await?;
    assert_eq!(alone.get().await?.try_into_string()?, "HELLO");
    let x = envref.evaluate("-R/data/x.txt").await?;
    // `data/x.txt` is the stored resource; the recipe named x.txt reads it and continues.
    let _ = x.get().await?;

    let r = recipe("pipeline: [{value: a}]\nquery: upper/u.txt\n")?;
    assert_eq!(r.filename()?.map(|f| f.name), Some("u.txt".to_owned()));
    assert_eq!(r.data_format()?, Some("txt".to_owned()));
    assert_eq!(r.key()?, None);
    let r = recipe("pipeline: {elements: [{value: a}], filename: p.json}\n")?;
    assert_eq!(r.filename()?.map(|f| f.name), Some("p.json".to_owned()));
    Ok(())
}

/// AC-11, AC-15 — a recipe pipeline takes literal values only.
#[tokio::test]
async fn recipe_pipeline_refuses_placeholders() -> TestResult {
    let envref = make_env("recipes: []\n").await?;
    let r = recipe("pipeline: [{value: {placeholder: n}}]\nquery: upper\n")?;
    let err = r
        .to_plan(envref.get_command_metadata_registry())
        .expect_err("placeholder");
    assert!(err.message.contains("placeholder 'n'"), "{}", err.message);
    Ok(())
}

/// AC-15 — a query starting with a resource after a pipeline would discard its state.
#[tokio::test]
async fn recipe_pipeline_before_resource_query_is_refused() -> TestResult {
    let envref = make_env("recipes: []\n").await?;
    let r = recipe("pipeline: [{value: 1}]\nquery: -R/data/x.txt/-/upper\n")?;
    let err = r
        .to_plan(envref.get_command_metadata_registry())
        .expect_err("dead pipeline");
    assert!(
        err.message.contains("starts with a resource"),
        "{}",
        err.message
    );
    let r = recipe("pipeline: {elements: [{value: 1}], filename: a.txt}\nquery: upper/b.txt\n")?;
    let err = r
        .to_plan(envref.get_command_metadata_registry())
        .expect_err("two filenames");
    assert!(err.message.contains("filename"), "{}", err.message);
    Ok(())
}

/// AC-15 — `arguments` and `links` override the query's last action, so they need a query.
#[tokio::test]
async fn recipe_overrides_need_a_query() -> TestResult {
    let envref = make_env("recipes: []\n").await?;
    let cmr = envref.get_command_metadata_registry();
    let r = recipe("pipeline: [{value: a}]\narguments: {s: x}\n")?;
    let err = r.to_plan(cmr).expect_err("no query");
    assert!(err.message.contains("no query"), "{}", err.message);
    // With a query they override its last action, after the pipeline.
    let r = recipe("pipeline: [{value: a}]\nquery: suffix-y\narguments: {s: z}\n")?;
    let plan = r.to_plan(cmr)?;
    assert!(
        matches!(plan.steps[0], Step::UseValue(_)),
        "{:?}",
        plan.steps
    );
    let state = liquers_core::interpreter::apply_plan(
        plan,
        State::new(),
        Context::new(
            liquers_core::assets::AssetRef::new_temporary(envref.clone()),
            false,
        )
        .await,
        envref.clone(),
    )
    .await?;
    assert_eq!(state.try_into_string()?, "az");
    Ok(())
}

/// AC-15 — a recipe without a pipeline plans exactly as before.
#[tokio::test]
async fn existing_recipes_unchanged() -> TestResult {
    let envref = make_env("recipes: []\n").await?;
    let cmr = envref.get_command_metadata_registry();
    let r = recipe("query: -R/data/x.txt/-/suffix-a/upper/out.txt\narguments: {}\n")?;
    let plan = r.to_plan(cmr)?;
    let direct = liquers_core::plan::PlanBuilder::new(r.get_query()?, cmr)
        .with_placeholders_allowed()
        .build()?;
    assert_eq!(serde_json::to_value(&plan)?, serde_json::to_value(&direct)?);
    // Serialized, the recipe has no `pipeline` key.
    assert!(!serde_yaml::to_string(&r)?.contains("pipeline"));
    Ok(())
}

/// AC-30 — a recipe's pipeline plans as the recipe does.
#[tokio::test]
async fn recipe_to_pipeline_plans_as_recipe() -> TestResult {
    let envref = make_env("recipes: []\n").await?;
    let cmr = envref.get_command_metadata_registry();
    let r = recipe("pipeline: [{value: {a: 1}}]\nquery: keys/suffix-x/out.txt\n")?;
    let as_pipeline = r.to_pipeline(cmr)?;
    let expected = Pipeline::from_json(&json!({
        "elements": [{"value": {"a": 1}}, {"action": "keys"}, {"action": "suffix", "parameters": ["x"]}],
        "filename": "out.txt"
    }))?;
    assert_eq!(as_pipeline, expected);
    let steps = |p: &liquers_core::plan::Plan| -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(&p.steps)
    };
    assert_eq!(steps(&as_pipeline.to_plan(cmr)?)?, steps(&r.to_plan(cmr)?)?);
    Ok(())
}
