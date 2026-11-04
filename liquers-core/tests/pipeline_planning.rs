//! Planning and running pipelines (`specs/design/composite-command-definition/`, Phase 3).

use liquers_core::{
    command_metadata::CommandMetadataRegistry,
    context::{Context, Environment, SimpleEnvironment},
    error::{Error, ErrorType},
    interpreter::{evaluate, evaluate_pipeline},
    metadata::Metadata,
    parse::{parse_key, parse_query},
    pipeline::{Pipeline, PipelinePlanBuilder, PlanningMode},
    plan::{ParameterValue, Step},
    query::Key,
    recipes::DefaultRecipeProvider,
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::{Value, ValueInterface},
};
use liquers_macro::register_command;
use serde_json::json;

type TestEnv = SimpleEnvironment<Value>;
type CommandEnvironment = TestEnv;
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn hello() -> Result<Value, Error> {
    Ok(Value::from("hello"))
}
fn upper(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(state.try_into_string()?.to_uppercase()))
}
fn suffix(state: &State<Value>, s: String) -> Result<Value, Error> {
    Ok(Value::from(format!("{}{}", state.try_into_string()?, s)))
}
fn add(state: &State<Value>, a: i64, b: i64) -> Result<Value, Error> {
    let x = match &*state.value()? {
        Value::None => 0,
        Value::I64(x) => *x,
        Value::I32(x) => i64::from(*x),
        other => return Err(Error::conversion_error(other.identifier(), "i64")),
    };
    Ok(Value::from(x + a + b))
}
fn total(_state: &State<Value>, items: Vec<i64>) -> Result<Value, Error> {
    Ok(Value::from(items.iter().sum::<i64>()))
}
fn describe(_state: &State<Value>, config: Value) -> Result<Value, Error> {
    Ok(Value::from(config.try_into_json_value()?.to_string()))
}
fn len(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(state.try_into_string()?.len() as i64))
}
fn concat(state: &State<Value>, other: String) -> Result<Value, Error> {
    Ok(Value::from(format!("{}{}", state.try_into_string()?, other)))
}

async fn make_env() -> Result<TestEnv, Box<dyn std::error::Error>> {
    let mut env = TestEnv::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn hello() -> result)?;
    register_command!(cr, fn upper(state) -> result)?;
    register_command!(cr, fn suffix(state, s: String = "!") -> result)?;
    register_command!(cr, fn add(state, a: i64, b: i64 = 0) -> result)?;
    register_command!(cr, fn total(state, items: Vec<i64> multiple) -> result)?;
    register_command!(cr, fn describe(state, config: Value) -> result)?;
    register_command!(cr, fn len(state) -> result)?;
    register_command!(cr, fn concat(state, other: String) -> result)?;
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("data/x.txt")?,
            b"abc",
            &Metadata::new(),
        )
        .await?;
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    Ok(env)
}

fn cmr(env: &TestEnv) -> &CommandMetadataRegistry {
    &env.command_registry.command_metadata_registry
}

async fn run(env: TestEnv, pipeline: serde_json::Value) -> Result<Value, Box<dyn std::error::Error>> {
    let pipeline = Pipeline::from_json(&pipeline)?;
    let envref = env.to_ref();
    let state = evaluate_pipeline(envref, &pipeline, None).await?;
    Ok((*state.value()?).clone())
}

/// AC-41 — a plan from a query reports the query's positions, and so does a pipeline's.
#[tokio::test]
async fn positions_reach_plan_steps() -> TestResult {
    let env = make_env().await?;
    let query = parse_query("hello/suffix-x/upper")?;
    let pipeline = Pipeline::try_from(&query)?;
    let from_query = PipelinePlanBuilder::new(cmr(&env)).plan_query(&query)?;
    let from_pipeline = pipeline.to_plan(cmr(&env))?;
    let positions = |plan: &liquers_core::plan::Plan| -> Vec<(u32, usize)> {
        plan.steps
            .iter()
            .filter_map(|s| match s {
                Step::Action { position, .. } => Some((position.line, position.column)),
                _ => None,
            })
            .collect()
    };
    assert_eq!(positions(&from_query), positions(&from_pipeline));
    assert_eq!(positions(&from_query)[1], (1, 7), "{:?}", positions(&from_query));

    // An element planned natively keeps its own position too.
    let mut native = pipeline.clone();
    native.elements.insert(0, liquers_core::pipeline::Located::new(
        liquers_core::pipeline::ElementKind::Value(liquers_core::pipeline::Operand::Literal(json!("x"))),
    ));
    let plan = native.to_plan(cmr(&env))?;
    assert!(matches!(plan.steps[0], Step::UseValue(_)), "{:?}", plan.steps);
    Ok(())
}

/// AC-5 — each state element plans as exactly one step.
#[tokio::test]
async fn value_and_resource_elements_set_state() -> TestResult {
    let env = make_env().await?;
    let cases = [
        (json!([{"value": {"a": 1}}]), "UseValue"),
        (json!([{"value": 1}, {"resource": "data/x.txt"}]), "GetAsset"),
        (json!([{"value": 1}, {"resource": "data/x.txt", "read": "b"}]), "GetAssetBinary"),
        (json!([{"value": 1}, {"resource": "data/x.txt", "read": "meta"}]), "GetAssetMetadata"),
        (json!([{"value": 1}, {"resource": "data/x.txt", "read": "r"}]), "GetAssetRecipe"),
        (json!([{"value": 1}, {"resource": "data", "read": "dir"}]), "GetAssetDirectory"),
        (json!([{"value": 1}, {"resource": "data/x.txt", "read": "sbin"}]), "GetResource"),
        (json!([{"value": 1}, {"resource": "data/x.txt", "read": "stored_meta"}]), "GetResourceMetadata"),
        (json!([{"value": 1}, {"resource": "data", "read": "sdir"}]), "GetResourceDirectory"),
        (json!([{"value": 1}, {"key_value": "a/b"}]), "UseKeyValue"),
        (json!([{"value": 1}, {"evaluate": "hello"}]), "Evaluate"),
        (json!([{"value": 1}, {"evaluate": [{"value": 2}]}]), "Plan"),
        (json!([{"value": 1}, {"query_value": "hello/upper"}]), "UseQueryValue"),
    ];
    for (p, step) in cases {
        let plan = Pipeline::from_json(&p)?.to_plan(cmr(&env))?;
        let last = plan.steps.last().expect("a step");
        let name = format!("{last:?}");
        assert!(name.starts_with(step), "{p}: {name}");
        assert_eq!(plan.steps.len(), p.as_array().map(Vec::len).unwrap_or(0), "{p}: {:?}", plan.steps);
    }
    // And they run.
    assert_eq!(run(make_env().await?, json!([{"value": "abc"}, {"action": "len"}])).await?, Value::from(3i64));
    assert_eq!(
        run(make_env().await?, json!([{"value": 1}, {"evaluate": [{"value": "xy"}, {"action": "upper"}]}]))
            .await?,
        Value::from("XY")
    );
    assert_eq!(
        run(make_env().await?, json!([{"value": 1}, {"resource": "data/x.txt"}, {"action": "upper"}])).await?,
        Value::from("ABC")
    );
    Ok(())
}

/// AC-11 — a placeholder outside a definition is an error naming it.
#[tokio::test]
async fn placeholder_outside_definition_is_error() -> TestResult {
    let env = make_env().await?;
    for p in [
        json!([{"value": {"placeholder": "n"}}]),
        json!(["hello", {"action": "add", "arguments": {"a": {"placeholder": "n"}}}]),
        json!([{"resource": {"placeholder": "n"}}]),
    ] {
        let err = Pipeline::from_json(&p)?.to_plan(cmr(&env)).expect_err("placeholder");
        assert!(err.message.contains("placeholder 'n'"), "{p}: {}", err.message);
    }
    let err = Pipeline::from_json(&json!(["hello", {"action": "describe", "arguments": {"config": {"state": true}}}]))?
        .to_plan(cmr(&env))
        .expect_err("input state");
    assert!(err.message.contains("input state"), "{}", err.message);
    Ok(())
}

/// AC-16 — query text, structured elements and query text again make one plan, in order.
#[tokio::test]
async fn query_then_pipeline_then_query() -> TestResult {
    let env = make_env().await?;
    let p = json!([
        "hello/upper",
        {"action": "describe", "arguments": {"config": {"k": [1, 2]}}},
        "suffix-x"
    ]);
    let plan = Pipeline::from_json(&p)?.to_plan(cmr(&env))?;
    let names: Vec<_> = plan
        .steps
        .iter()
        .filter_map(|s| match s {
            Step::Action { action_name, query, .. } => Some((action_name.clone(), query.is_some())),
            _ => None,
        })
        .collect();
    assert_eq!(
        names,
        vec![
            ("hello".to_owned(), true),
            ("upper".to_owned(), true),
            ("describe".to_owned(), false),
            ("suffix".to_owned(), false)
        ]
    );
    assert_eq!(run(env, p).await?, Value::from(r#"{"k":[1,2]}x"#));
    Ok(())
}

/// AC-17 — positional first, named for the rest, defaults for what is left out; errors otherwise.
#[tokio::test]
async fn positional_then_named_arguments() -> TestResult {
    assert_eq!(
        run(make_env().await?, json!([{"action": "add", "parameters": [2], "arguments": {"b": 3}}])).await?,
        Value::from(5i64)
    );
    assert_eq!(run(make_env().await?, json!([{"action": "add", "arguments": {"a": 4}}])).await?, Value::from(4i64));
    assert_eq!(
        run(make_env().await?, json!([{"action": "add", "arguments": {"b": 1, "a": 2}}])).await?,
        Value::from(3i64)
    );
    let env = make_env().await?;
    for (p, needle) in [
        (json!([{"action": "add", "arguments": {"c": 1}}]), "no such argument"),
        (json!([{"action": "add", "parameters": [1], "arguments": {"a": 1}}]), "both by position and by name"),
        (json!([{"action": "add", "arguments": {"b": 1}}]), "Missing argument"),
        (json!([{"action": "add", "parameters": [1, 2, 3]}]), "parameter"),
    ] {
        let err = Pipeline::from_json(&p)?.to_plan(cmr(&env)).expect_err("refused");
        assert!(err.message.contains(needle), "{p}: {}", err.message);
        if needle != "Missing argument" && needle != "parameter" {
            assert_eq!(err.error_type, ErrorType::ParameterError, "{p}");
        }
    }
    Ok(())
}

/// AC-18 — a representable nested pipeline is a link; one that is not runs as a nested plan.
#[tokio::test]
async fn nested_pipeline_argument_representable_and_not() -> TestResult {
    let env = make_env().await?;
    let representable = json!(["hello", {"action": "concat", "arguments": {"other": {"pipeline": ["hello/upper"]}}}]);
    let plan = Pipeline::from_json(&representable)?.to_plan(cmr(&env))?;
    let Step::Action { parameters, .. } = plan.steps.last().expect("a step") else {
        panic!("expected an action")
    };
    assert!(
        matches!(&parameters.0[0], ParameterValue::ParameterLink(_, q, _) if q.encode() == "hello/upper"),
        "{:?}",
        parameters.0
    );
    assert_eq!(run(env, representable).await?, Value::from("helloHELLO"));

    let env = make_env().await?;
    let not_representable = json!([
        "hello",
        {"action": "concat", "arguments": {"other": {"pipeline": [{"value": "ab"}, {"action": "upper"}]}}}
    ]);
    let plan = Pipeline::from_json(&not_representable)?.to_plan(cmr(&env))?;
    let Step::Action { parameters, .. } = plan.steps.last().expect("a step") else {
        panic!("expected an action")
    };
    assert!(matches!(&parameters.0[0], ParameterValue::ParameterPipeline(_, _, _)), "{:?}", parameters.0);
    assert_eq!(run(env, not_representable).await?, Value::from("helloAB"));
    Ok(())
}

/// AC-19 — the variadic rules V1–V7.
#[tokio::test]
async fn variadic_rules_v1_to_v9() -> TestResult {
    for (p, expected) in [
        // V1: positional, every remaining source one element.
        (json!([{"action": "total", "parameters": [1, 2, 3]}]), 6),
        // V2: by name, as {multiple}, or as an array.
        (json!([{"action": "total", "arguments": {"items": {"multiple": [1, 2]}}}]), 3),
        (json!([{"action": "total", "arguments": {"items": [4, 5]}}]), 9),
        // V6: an empty array is no elements.
        (json!([{"action": "total", "arguments": {"items": []}}]), 0),
        // V7: a link is one element.
        (json!([{"action": "total", "arguments": {"items": {"multiple": [1, {"link": "hello/len"}]}}}]), 6),
    ] {
        assert_eq!(run(make_env().await?, p.clone()).await?, Value::from(expected as i64), "{p}");
    }
    // V4: a placeholder bound to an array is spliced; a scalar binding is one element.
    let p = Pipeline::from_json(&json!([{"action": "total", "arguments": {"items": {"multiple": [1, {"placeholder": "xs"}, {"placeholder": "y"}]}}}]))?;
    let bound = p.bind(&[("xs".to_owned(), json!([2, 3])), ("y".to_owned(), json!(4))].into_iter().collect())?;
    let state = evaluate_pipeline(make_env().await?.to_ref(), &bound, None).await?;
    assert_eq!(*state.value()?, Value::from(10i64));
    Ok(())
}

/// AC-23 — a list for a single argument: `any` takes the whole array, a scalar type refuses it.
#[tokio::test]
async fn list_for_single_argument() -> TestResult {
    assert_eq!(
        run(make_env().await?, json!([{"action": "describe", "arguments": {"config": [1, 2]}}])).await?,
        Value::from("[1,2]")
    );
    let env = make_env().await?;
    let err = Pipeline::from_json(&json!([{"action": "add", "arguments": {"a": [1, 2]}}]))?
        .to_plan(cmr(&env))
        .expect_err("not an integer");
    assert_eq!(err.error_type, ErrorType::ConversionError, "{}", err.message);
    // Not representable (S4), so it never reaches the query planner.
    assert!(
        Pipeline::from_json(&json!([{"action": "describe", "arguments": {"config": [1, 2]}}]))?
            .to_query_with(cmr(&env))
            .is_err()
    );
    Ok(())
}

/// A query planned through the pipeline planner in `Delegate` mode is the query planner's plan.
#[tokio::test]
async fn delegate_mode_query_plan_is_unchanged() -> TestResult {
    let env = make_env().await?;
    let query = parse_query("hello/suffix-x/upper")?;
    let plan = PipelinePlanBuilder::new(cmr(&env))
        .with_mode(PlanningMode::Delegate)
        .plan_query(&query)?;
    let direct = liquers_core::plan::PlanBuilder::new(query.clone(), cmr(&env)).build()?;
    assert_eq!(serde_json::to_value(&plan)?, serde_json::to_value(&direct)?);
    // And a representable pipeline plans as its query.
    let from_pipeline = Pipeline::try_from(&query)?.to_plan(cmr(&env))?;
    assert_eq!(serde_json::to_value(&from_pipeline)?, serde_json::to_value(&direct)?);
    // The evaluation path is unchanged.
    let state = evaluate(env.to_ref(), "hello/suffix-x/upper", None).await?;
    assert_eq!(*state.value()?, Value::from("HELLOX"));
    Ok(())
}
