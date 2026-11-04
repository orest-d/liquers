//! Pipeline commands (`CommandDefinition::Pipeline`; `specs/design/composite-command-definition/`,
//! Phase 3 test plan).

use liquers_core::{
    command_metadata::{
        ArgumentInfo, ArgumentType, CommandDefinition, CommandKey, CommandMetadata,
        CommandMetadataRegistry, CommandParameterValue,
    },
    context::{Context, EnvRef, Environment, SimpleEnvironment},
    error::{Error, ErrorType},
    interpreter::make_plan,
    metadata::{DependencyKey, Metadata},
    parse::parse_key,
    pipeline::Pipeline,
    plan::Step,
    query::Key,
    recipes::DefaultRecipeProvider,
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};
use liquers_macro::register_command;
use serde_json::json;

type TestEnv = SimpleEnvironment<Value>;
type CommandEnvironment = TestEnv;
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn upper(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(state.try_into_string()?.to_uppercase()))
}
fn lower(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(state.try_into_string()?.to_lowercase()))
}
fn suffix(state: &State<Value>, s: String) -> Result<Value, Error> {
    Ok(Value::from(format!("{}{}", state.try_into_string()?, s)))
}
fn concat(state: &State<Value>, a: String, b: String) -> Result<Value, Error> {
    Ok(Value::from(format!("{}{}{}", state.try_into_string()?, a, b)))
}
fn join_with(state: &State<Value>, other: String) -> Result<Value, Error> {
    Ok(Value::from(format!("{}+{}", state.try_into_string()?, other)))
}
fn needs_payload(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(state.try_into_string()?))
}

async fn make_env() -> Result<TestEnv, Box<dyn std::error::Error>> {
    let mut env = TestEnv::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn upper(state) -> result)?;
    register_command!(cr, fn suffix(state, s: String) -> result)?;
    register_command!(cr, fn concat(state, a: String, b: String) -> result)?;
    register_command!(cr, fn join_with(state, other: String) -> result)?;
    register_command!(cr, fn lower(state) -> result namespace: "x")?;
    register_command!(cr, fn needs_payload(state) -> result payload: required)?;
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(&parse_key("data/x.txt")?, b"abc", &Metadata::new())
        .await?;
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    Ok(env)
}

fn pipeline(json: serde_json::Value) -> Result<Pipeline, Error> {
    Pipeline::from_json(&json)
}

fn arg(name: &str) -> ArgumentInfo {
    ArgumentInfo::any_argument(name)
}

async fn text(envref: &EnvRef<TestEnv>, query: &str) -> Result<String, Box<dyn std::error::Error>> {
    let asset = envref.evaluate(query).await?;
    Ok(asset.get().await?.try_into_string()?)
}

/// AC-1 — a call runs the body on its input, its placeholders replaced by the arguments; the
/// query continues after it.
#[tokio::test]
async fn pipeline_command_runs_its_pipeline() -> TestResult {
    let mut env = make_env().await?;
    env.command_registry.register_pipeline(
        CommandKey::new("", "", "shout"),
        pipeline(json!([{"action": "upper"}, {"action": "suffix", "parameters": [{"placeholder": "s"}]}]))?,
        vec![arg("s")],
    )?;
    let envref = env.to_ref();
    assert_eq!(text(&envref, "-R/data/x.txt/-/shout-X/suffix-Y").await?, "ABCXY");
    assert_eq!(
        text(&envref, "-R/data/x.txt/-/shout-X/suffix-Y").await?,
        text(&envref, "-R/data/x.txt/-/upper/suffix-X/suffix-Y").await?
    );
    // The call is one step; the body is inside it.
    let plan = make_plan(envref.clone(), "-R/data/x.txt/-/shout-X").await?;
    match &plan.steps[1] {
        Step::PipelineCommand { name, plan: body, .. } => {
            assert_eq!(name, "shout");
            assert_eq!(body.steps.len(), 2, "{:?}", body.steps);
        }
        other => panic!("expected a pipeline command step, got {other:?}"),
    }
    // A pipeline command's argument type is taken from the slot it fills (AC-3).
    let metadata = envref
        .get_command_metadata_registry()
        .find_command("", "", "shout")
        .ok_or("shout not registered")?;
    assert_eq!(metadata.arguments[0].argument_type, ArgumentType::String);
    Ok(())
}

/// AC-2 — arguments ordered differently from the slots they fill, and one used twice.
#[tokio::test]
async fn arguments_in_any_order() -> TestResult {
    let mut env = make_env().await?;
    env.command_registry.register_pipeline(
        CommandKey::new("", "", "swap"),
        pipeline(json!([{"action": "concat",
            "arguments": {"a": {"placeholder": "second"}, "b": {"placeholder": "first"}}}]))?,
        vec![arg("first"), arg("second")],
    )?;
    env.command_registry.register_pipeline(
        CommandKey::new("", "", "twice"),
        pipeline(json!([
            {"action": "suffix", "parameters": [{"placeholder": "t"}]},
            {"action": "suffix", "parameters": [{"placeholder": "t"}]}
        ]))?,
        vec![arg("t")],
    )?;
    let envref = env.to_ref();
    assert_eq!(text(&envref, "-R/data/x.txt/-/swap-1-2").await?, "abc21");
    assert_eq!(text(&envref, "-R/data/x.txt/-/twice-z").await?, "abczz");
    Ok(())
}

/// AC-3 — an omitted argument takes its declared default; with none it is `ArgumentMissing`
/// naming the pipeline command's argument.
#[tokio::test]
async fn defaults_and_missing_arguments() -> TestResult {
    let mut env = make_env().await?;
    let mut with_default = arg("s");
    with_default.default = CommandParameterValue::Value(json!("-dflt"));
    env.command_registry.register_pipeline(
        CommandKey::new("", "", "tag"),
        pipeline(json!([{"action": "suffix", "parameters": [{"placeholder": "s"}]}]))?,
        vec![with_default],
    )?;
    env.command_registry.register_pipeline(
        CommandKey::new("", "", "tag_required"),
        pipeline(json!([{"action": "suffix", "parameters": [{"placeholder": "needed"}]}]))?,
        vec![arg("needed")],
    )?;
    let envref = env.to_ref();
    assert_eq!(text(&envref, "-R/data/x.txt/-/tag").await?, "abc-dflt");
    assert_eq!(text(&envref, "-R/data/x.txt/-/tag-given").await?, "abcgiven");
    let error = make_plan(envref.clone(), "-R/data/x.txt/-/tag_required")
        .await
        .expect_err("missing argument");
    assert_eq!(error.error_type, ErrorType::ArgumentMissing, "{}", error.message);
    assert!(error.message.contains("needed"), "{}", error.message);
    Ok(())
}

/// AC-4 — `{state: true}` is the pipeline command's input.
#[tokio::test]
async fn input_state_as_argument() -> TestResult {
    let mut env = make_env().await?;
    env.command_registry.register_pipeline(
        CommandKey::new("", "", "echo"),
        pipeline(json!([{"action": "upper"}, {"action": "join_with", "arguments": {"other": {"state": true}}}]))?,
        vec![],
    )?;
    let envref = env.to_ref();
    assert_eq!(text(&envref, "-R/data/x.txt/-/echo").await?, "ABC+abc");
    // Outside a pipeline command there is no input state.
    let error = liquers_core::interpreter::evaluate_pipeline(
        envref.clone(),
        &pipeline(json!([{"value": "q"}, {"action": "join_with", "arguments": {"other": {"state": true}}}]))?,
        None,
    )
    .await
    .expect_err("no input state");
    assert!(error.message.contains("input state"), "{}", error.message);
    Ok(())
}

/// AC-4 — the outer command's input reaches an argument of a nested pipeline command.
#[tokio::test]
async fn input_state_passes_through_nested_commands() -> TestResult {
    let mut env = make_env().await?;
    env.command_registry.register_pipeline(
        CommandKey::new("", "", "inner"),
        pipeline(json!([{"action": "join_with", "arguments": {"other": {"placeholder": "o"}}}]))?,
        vec![arg("o")],
    )?;
    env.command_registry.register_pipeline(
        CommandKey::new("", "", "outer"),
        pipeline(json!([{"action": "upper"}, {"action": "inner", "arguments": {"o": {"state": true}}}]))?,
        vec![],
    )?;
    let envref = env.to_ref();
    assert_eq!(text(&envref, "-R/data/x.txt/-/outer").await?, "ABC+abc");
    Ok(())
}

/// AC-5 — a body whose first element sets the state is a source command.
#[tokio::test]
async fn source_command_has_no_input() -> TestResult {
    let mut env = make_env().await?;
    let metadata = env.command_registry.register_pipeline(
        CommandKey::new("", "", "hi"),
        pipeline(json!([{"value": "hi"}, {"action": "upper"}]))?,
        vec![],
    )?;
    assert!(metadata.state_argument.is_none());
    let envref = env.to_ref();
    assert_eq!(text(&envref, "hi").await?, "HI");
    Ok(())
}

/// AC-6 — registration problems are issues; an error refuses registration and changes nothing.
#[tokio::test]
async fn registration_issues_are_reported() -> TestResult {
    let mut env = make_env().await?;
    let cr = &mut env.command_registry;
    let key = CommandKey::new("", "", "bad");
    let refused = |result: Result<&mut CommandMetadata, Error>, needle: &str| match result {
        Ok(_) => panic!("registered despite: {needle}"),
        Err(e) => assert!(e.message.contains(needle), "{needle}: {}", e.message),
    };
    refused(
        cr.register_pipeline(
            key.clone(),
            pipeline(json!([{"action": "suffix", "parameters": [{"placeholder": "nope"}]}]))?,
            vec![],
        ),
        "Placeholder 'nope' is not a declared argument",
    );
    let mut injected = arg("s");
    injected.injected = true;
    refused(
        cr.register_pipeline(
            key.clone(),
            pipeline(json!([{"action": "suffix", "parameters": [{"placeholder": "s"}]}]))?,
            vec![injected],
        ),
        "injected",
    );
    refused(
        cr.register_pipeline(key.clone(), pipeline(json!([{"action": "missing"}]))?, vec![]),
        "Step command 'missing' is not registered",
    );
    refused(
        cr.register_pipeline(
            key.clone(),
            pipeline(json!([{"action": "suffix", "arguments": {"t": "x"}}]))?,
            vec![],
        ),
        "'t' is not an argument",
    );
    refused(
        cr.register_pipeline(
            key.clone(),
            pipeline(json!({"elements": [{"action": "upper"}], "filename": "a.txt"}))?,
            vec![],
        ),
        "filename",
    );
    assert!(cr.command_metadata_registry.find_command("", "", "bad").is_none());
    // An unused argument is a warning: registration succeeds and the registry reports it.
    cr.register_pipeline(key.clone(), pipeline(json!([{"action": "upper"}]))?, vec![arg("spare")])?;
    let report = cr.command_metadata_registry.check();
    assert!(!report.has_errors(), "{report}");
    assert!(report.to_string().contains("'spare' is used by no placeholder"), "{report}");
    Ok(())
}

/// AC-7 — elements resolve through their own `ns` and the default namespaces, never the
/// caller's.
#[tokio::test]
async fn elements_resolve_without_caller_ns() -> TestResult {
    let mut env = make_env().await?;
    // `x/upper` exists too; the body's `upper` has no `ns`, so it is the root one.
    env.command_registry.register_command(
        CommandKey::new("", "x", "upper"),
        |state: &State<Value>, _args, _context| lower(state),
    )?;
    env.command_registry.register_pipeline(
        CommandKey::new("", "x", "shout"),
        pipeline(json!([{"action": "upper"}]))?,
        vec![],
    )?;
    let envref = env.to_ref();
    assert_eq!(text(&envref, "-R/data/x.txt/-/ns-x/shout").await?, "ABC");
    Ok(())
}

/// AC-7 — a pipeline command that reaches itself is refused when registered, and a cycle that
/// was never registered through `register_pipeline` is a planning error naming the chain.
#[tokio::test]
async fn pipeline_command_cycle_is_reported() -> TestResult {
    let mut env = make_env().await?;
    let a = CommandKey::new("", "", "a");
    let b = CommandKey::new("", "", "b");
    env.command_registry
        .register_pipeline(a.clone(), pipeline(json!([{"action": "upper"}]))?, vec![])?;
    env.command_registry
        .register_pipeline(b.clone(), pipeline(json!([{"action": "a"}]))?, vec![])?;
    let error = env
        .command_registry
        .register_pipeline(a.clone(), pipeline(json!([{"action": "b"}]))?, vec![])
        .err()
        .ok_or("cycle registered")?;
    assert!(error.message.contains("cycle"), "{}", error.message);

    // Built by hand, as a deserialized registry would be.
    let mut cmr = CommandMetadataRegistry::new();
    for (key, body) in [(a.clone(), "b"), (b.clone(), "a")] {
        let mut metadata = CommandMetadata::from_key(key);
        metadata.definition = CommandDefinition::Pipeline {
            pipeline: pipeline(json!([{"action": body}]))?,
        };
        cmr.add_command(&metadata);
    }
    assert!(cmr.check().has_errors());
    let error = liquers_core::pipeline::PipelinePlanBuilder::new(&cmr)
        .plan_query(&liquers_core::parse::parse_query("a")?)
        .expect_err("cycle");
    assert!(error.message.contains("cycle: a -> b -> a"), "{}", error.message);
    Ok(())
}

/// AC-8 — the result is cached under the calling query, and the pipeline command's metadata is
/// a dependency; an error in a step names the pipeline command.
#[tokio::test]
async fn result_cached_and_invalidated_by_metadata() -> TestResult {
    let mut env = make_env().await?;
    let key = CommandKey::new("", "", "shout");
    env.command_registry.register_pipeline(
        key.clone(),
        pipeline(json!([{"action": "upper"}, {"action": "suffix", "parameters": [{"placeholder": "s"}]}]))?,
        vec![arg("s")],
    )?;
    env.command_registry.register_pipeline(
        CommandKey::new("", "", "broken"),
        pipeline(json!([{"action": "suffix", "parameters": [{"link": "-R/data/missing.txt"}]}]))?,
        vec![],
    )?;
    let envref = env.to_ref();
    let plan = make_plan(envref.clone(), "-R/data/x.txt/-/shout-X").await?;
    let metadata_key = DependencyKey::for_command_metadata(&key);
    assert!(
        plan.dependencies.iter().any(|d| d.key == metadata_key),
        "{:?}",
        plan.dependencies
    );
    // The step commands are dependencies as well, as if they were called directly.
    let upper_key = DependencyKey::for_command_metadata(&CommandKey::new("", "", "upper"));
    assert!(plan.dependencies.iter().any(|d| d.key == upper_key));
    let error = text(&envref, "-R/data/x.txt/-/broken").await.expect_err("fails");
    assert!(error.to_string().contains("pipeline command 'broken'"), "{error}");
    Ok(())
}

/// AC-20 — a step that needs a payload makes the pipeline command need one.
#[tokio::test]
async fn injection_and_payload_forwarded() -> TestResult {
    let mut env = make_env().await?;
    let metadata = env.command_registry.register_pipeline(
        CommandKey::new("", "", "with_payload"),
        pipeline(json!([{"action": "needs_payload"}]))?,
        vec![],
    )?;
    assert!(metadata.payload_required.is_required());
    let envref = env.to_ref();
    let plan = make_plan(envref.clone(), "-R/data/x.txt/-/with_payload").await?;
    assert!(plan.payload_required.is_required());
    Ok(())
}

/// AC-21 — a pipeline command survives the registry's YAML and JSON forms unchanged.
#[tokio::test]
async fn registry_export_round_trip() -> TestResult {
    let mut env = make_env().await?;
    env.command_registry.register_pipeline(
        CommandKey::new("", "", "shout"),
        pipeline(json!([{"action": "upper"}, {"action": "suffix", "parameters": [{"placeholder": "s"}]}]))?,
        vec![arg("s")],
    )?;
    let cmr = &env.command_registry.command_metadata_registry;
    let shout = cmr.find_command("", "", "shout").ok_or("missing")?;
    // `metadata_version` is computed by the registry, not serialized.
    let read_back = |mut metadata: CommandMetadata| {
        metadata.metadata_version = shout.metadata_version.clone();
        metadata
    };
    let yaml = serde_yaml::to_string(&shout)?;
    assert_eq!(read_back(serde_yaml::from_str(&yaml)?), shout, "{yaml}");
    let json_text = serde_json::to_string(&shout)?;
    assert_eq!(read_back(serde_json::from_str(&json_text)?), shout);
    assert!(yaml.contains("Pipeline"), "{yaml}");
    Ok(())
}
