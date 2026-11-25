//! A variadic argument with a shape: tuples, a dictionary, a JSON value
//! (`specs/design/composite-variadic-arguments/`). Commands are registered with the macro and
//! evaluated from queries, so each test covers the declared type, the plan and the retrieval.

use std::collections::BTreeMap;

use liquers_core::{
    command_metadata::{ArgumentType, CommandKey, VariadicShape},
    commands::{FromParameterGroup, FromParameterValue},
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    interpreter::evaluate,
    plan::ParameterValue,
    recipes::Recipe,
    state::State,
    value::Value,
};
use liquers_macro::*;

type CommandEnvironment = SimpleEnvironment<Value>;

#[derive(Debug, PartialEq)]
struct Point {
    x: f64,
    y: f64,
}

impl FromParameterGroup for Point {
    const ARITY: usize = 2;
    fn component_types() -> Vec<ArgumentType> {
        vec![ArgumentType::Float, ArgumentType::Float]
    }
    fn from_parameter_group(group: &[ParameterValue]) -> Result<Self, Error> {
        Ok(Point {
            x: f64::from_parameter_value(&group[0])?,
            y: f64::from_parameter_value(&group[1])?,
        })
    }
}

fn more(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("b"))
}

fn weights(_state: &State<Value>, pairs: Vec<(String, i64)>) -> Result<Value, Error> {
    Ok(Value::from(format!("{pairs:?}")))
}

fn curve(_state: &State<Value>, points: Vec<Point>) -> Result<Value, Error> {
    Ok(Value::from(
        points.iter().map(|p| format!("({},{})", p.x, p.y)).collect::<Vec<_>>().join(""),
    ))
}

fn place(
    _state: &State<Value>,
    word: String,
    layout: BTreeMap<String, serde_json::Value>,
) -> Result<Value, Error> {
    Ok(Value::from(format!(
        "{word}:{}",
        serde_json::to_string(&layout).map_err(|e| Error::general_error(e.to_string()))?
    )))
}

fn sizes(_state: &State<Value>, sizes: BTreeMap<String, i64>) -> Result<Value, Error> {
    Ok(Value::from(format!("{sizes:?}")))
}

fn object(
    _state: &State<Value>,
    object: serde_json::Map<String, serde_json::Value>,
) -> Result<Value, Error> {
    Ok(Value::from(serde_json::Value::Object(object).to_string()))
}

fn set_data(_state: &State<Value>, data: serde_json::Value) -> Result<Value, Error> {
    Ok(Value::from(data.to_string()))
}

fn environment() -> Result<CommandEnvironment, Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn more(state) -> result)?;
    register_command!(cr, fn weights(state,
        pairs: Vec<(String, i64)> multiple (label: "Weights", columns: ["Name", "Weight"])
    ) -> result)?;
    register_command!(cr, fn curve(state, points: Vec<Point> multiple (columns: ["x", "y"])) -> result)?;
    register_command!(cr, fn place(state, word: String,
        layout: BTreeMap<String, serde_json::Value> multiple) -> result)?;
    register_command!(cr, fn sizes(state, sizes: BTreeMap<String, i64> multiple) -> result)?;
    register_command!(cr, fn object(state,
        object: serde_json::Map<String, serde_json::Value> multiple) -> result)?;
    register_command!(cr, fn set_data(state, data: serde_json::Value multiple) -> result)?;
    Ok(env)
}

async fn text(query: &str) -> Result<String, Error> {
    let state = evaluate(environment()?.to_ref(), query, None).await?;
    state.try_into_string()
}

async fn failure(query: &str) -> Error {
    match text(query).await {
        Ok(value) => panic!("{query} must fail, but gave {value}"),
        Err(error) => error,
    }
}

/// AC-1, AC-5, AC-14 - consecutive parameters form tuples; the metadata says so.
#[tokio::test]
async fn pairs_from_flat_query() -> Result<(), Error> {
    assert_eq!(text("weights-a-1-b-2").await?, r#"[("a", 1), ("b", 2)]"#);
    assert_eq!(text("weights").await?, "[]");

    let env = environment()?;
    let metadata = env
        .command_registry
        .command_metadata_registry
        .get(CommandKey::new_name("weights"))
        .expect("weights is registered");
    let argument = &metadata.arguments[0];
    assert!(argument.multiple);
    assert_eq!(
        argument.shape,
        VariadicShape::Tuples(vec![ArgumentType::String, ArgumentType::Integer])
    );
    assert_eq!(argument.argument_type, ArgumentType::Any);
    assert_eq!(argument.columns, vec!["Name", "Weight"]);
    Ok(())
}

/// AC-2 - each position is parsed with its component type, and the error says where.
#[tokio::test]
async fn typed_component_error_is_positioned() {
    let error = failure("weights-a-1-b-x").await;
    assert_eq!(error.position.offset, "weights-a-1-b-".len(), "{error}");
}

/// AC-3 - an incomplete element is refused when the plan is built.
#[tokio::test]
async fn incomplete_element_fails_planning() {
    let error = failure("weights-a-1-b").await;
    assert_eq!(error.position.offset, "weights-a-1-".len(), "{error}");
    assert!(error.message.contains("pairs"), "names the argument: {error}");
}

/// AC-7 - a link is one parameter of an element; grouping happens once it has a value.
#[tokio::test]
async fn link_elements_group_after_materialisation() -> Result<(), Error> {
    assert_eq!(text("weights-a-1-~X~more~E-2").await?, r#"[("a", 1), ("b", 2)]"#);
    Ok(())
}

/// AC-7 - a recipe overrides a shaped argument with nested or flat JSON.
#[tokio::test]
async fn overrides_take_the_natural_json() -> Result<(), Error> {
    let env = environment()?;
    let registry = &env.command_registry.command_metadata_registry;
    let elements = |command: &str, name: &str, value: serde_json::Value| -> Result<usize, Error> {
        let recipe = Recipe::new(command.to_string(), String::new(), String::new())?
            .with_argument(name.to_string(), value);
        let plan = recipe.to_plan(registry)?;
        let Some(liquers_core::plan::Step::Action { parameters, .. }) = plan.steps.last() else {
            return Err(Error::general_error("an action step expected".to_string()));
        };
        Ok(parameters.0.last().and_then(|p| p.multiple()).map(|e| e.len()).unwrap_or(0))
    };
    assert_eq!(elements("weights", "pairs", serde_json::json!([["a", 1], ["b", 2]]))?, 4);
    assert_eq!(elements("weights", "pairs", serde_json::json!(["a", 1, "b", 2]))?, 4);
    assert_eq!(elements("place-x", "layout", serde_json::json!({"row": 0, "col": 8}))?, 4);
    assert_eq!(elements("set_data", "data", serde_json::json!([1, "a"]))?, 1);
    assert!(elements("weights", "pairs", serde_json::json!(["a", 1, "b"])).is_err());
    Ok(())
}

/// AC-9, AC-11 - key and value pairs; a JSON value is a number when written as one.
#[tokio::test]
async fn dictionary_from_pairs() -> Result<(), Error> {
    assert_eq!(
        text("place-current-row-0-col-8-align-center").await?,
        r#"current:{"align":"center","col":8,"row":0}"#
    );
    assert_eq!(text("place-current").await?, "current:{}");
    assert_eq!(text("sizes-w-3-h-4").await?, r#"{"h": 4, "w": 3}"#);
    // A typed value is parsed with its type, at its position.
    let error = failure("sizes-w-3-h-x").await;
    assert_eq!(error.position.offset, "sizes-w-3-h-".len(), "{error}");
    Ok(())
}

/// AC-10 - a key given twice is refused when the plan is built, at the second key.
#[tokio::test]
async fn duplicate_key_fails_planning() {
    let error = failure("place-current-row-0-row-1").await;
    assert_eq!(error.position.offset, "place-current-row-0-".len(), "{error}");
    let error = failure("place-current-row").await;
    assert_eq!(error.position.offset, "place-current-".len(), "{error}");
}

/// AC-12 - a type of the caller's as the element, through `FromParameterGroup`.
#[tokio::test]
async fn custom_element_type() -> Result<(), Error> {
    assert_eq!(text("curve-0-0-1-2.5").await?, "(0,0)(1,2.5)");
    let env = environment()?;
    let metadata = env
        .command_registry
        .command_metadata_registry
        .get(CommandKey::new_name("curve"))
        .expect("curve is registered");
    assert_eq!(
        metadata.arguments[0].shape,
        VariadicShape::Tuples(vec![ArgumentType::Float, ArgumentType::Float])
    );
    assert!(failure("curve-0-0-1").await.message.contains("points"));

    // The number of column names is checked against the type when the command is registered.
    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    let registered = register_command!(cr, fn curve(state,
        points: Vec<Point> multiple (columns: ["x", "y", "z"])) -> result);
    assert!(registered.is_err(), "three names for two components");
    Ok(())
}

/// AC-15 - the parameters decide the form of a JSON value.
#[tokio::test]
async fn json_value_forms() -> Result<(), Error> {
    for (query, expected) in [
        ("set_data", "null"),
        ("set_data-5", "5"),
        ("set_data-abc", r#""abc""#),
        ("set_data-true", r#""true""#),
        ("set_data-", "[]"),
        ("set_data--1-a", r#"[1,"a"]"#),
        ("set_data-1-a", r#"{"1":"a"}"#),
        ("set_data-row-0-col-8", r#"{"col":8,"row":0}"#),
    ] {
        assert_eq!(text(query).await?, expected, "{query}");
    }
    Ok(())
}

/// AC-16 - pairs of a JSON value are checked like a dictionary's.
#[tokio::test]
async fn json_value_pair_errors() {
    let error = failure("set_data-row-0-col").await;
    assert_eq!(error.position.offset, "set_data-row-0-".len(), "{error}");
    let error = failure("set_data-row-0-row-1").await;
    assert_eq!(error.position.offset, "set_data-row-0-".len(), "{error}");
}

/// AC-17 - a dictionary collected as a JSON object.
#[tokio::test]
async fn json_object_argument() -> Result<(), Error> {
    assert_eq!(text("object-title-Sales-w-3").await?, r#"{"title":"Sales","w":3}"#);
    assert_eq!(text("object").await?, "{}");
    Ok(())
}
