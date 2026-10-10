//! Command aliases end to end: planning, execution, dependencies and registration.
//!
//! `first` is to `pick` what `pl/head` is to `pl/slice`: the same command with its leading
//! argument fixed. See specs/design/command-alias-contract/ (Phase 3) and
//! specs/reference/COMMAND_ALIASES.md.

use liquers_core::{
    command_metadata::{
        ArgumentInfo, CommandDefinition, CommandKey, CommandMetadata, CommandParameterValue,
    },
    context::{Context, Environment, SimpleEnvironment},
    error::{Error, ErrorType},
    interpreter::{evaluate, make_plan},
    metadata::DependencyKey,
    plan::{ActionOrigin, ParameterValue, Plan, Step},
    state::State,
    value::Value,
};
use liquers_macro::*;

type CommandEnvironment = SimpleEnvironment<Value>;

fn text(_state: &State<Value>, value: String) -> Result<Value, Error> {
    Ok(Value::from(value))
}

/// Characters `offset .. offset + length` of the input text.
fn pick(state: &State<Value>, offset: i32, length: i32) -> Result<Value, Error> {
    let input = state.try_into_string()?;
    let picked: String = input
        .chars()
        .skip(offset.max(0) as usize)
        .take(length.max(0) as usize)
        .collect();
    Ok(Value::from(picked))
}

fn tag(state: &State<Value>, prefix: String, items: Vec<String>) -> Result<Value, Error> {
    let input = state.try_into_string()?;
    Ok(Value::from(format!("{prefix}:{input}:{}", items.join(","))))
}

fn fail(_state: &State<Value>, reason: String) -> Result<Value, Error> {
    Err(Error::general_error(reason))
}

fn ticker(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("tick"))
}

fn key(name: &str) -> CommandKey {
    CommandKey::new("", "", name)
}

fn int(value: i64) -> CommandParameterValue {
    CommandParameterValue::Value(serde_json::Value::from(value))
}

fn string(value: &str) -> CommandParameterValue {
    CommandParameterValue::Value(serde_json::Value::from(value))
}

/// The environment of Phase 3: three targets and their aliases, plus a volatile `ticker`.
fn environment() -> Result<CommandEnvironment, Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn text(state, value: String) -> result)?;
    register_command!(cr, fn pick(state, offset: i32, length: i32) -> result)?;
    register_command!(cr, fn tag(state, prefix: String, items: Vec<String> multiple) -> result)?;
    register_command!(cr, fn fail(state, reason: String) -> result)?;
    register_command!(cr, fn ticker(state) -> result volatile: true)?;

    cr.register_alias(
        key("first"),
        key("pick"),
        vec![int(0)],
        vec![ArgumentInfo::integer_argument("n", false).with_default(2)],
    )?
    .with_label("First characters");
    cr.register_alias(
        key("tagged"),
        key("tag"),
        vec![string("#")],
        vec![ArgumentInfo::string_argument("items").set_multiple()],
    )?;
    cr.register_alias(key("broken"), key("fail"), vec![string("boom")], vec![])?;
    Ok(env)
}

/// The last action of a plan, with its parameters and origin.
fn last_action(plan: &Plan) -> (&str, &[ParameterValue], &ActionOrigin) {
    match plan.steps.iter().rev().find(|step| step.is_action()) {
        Some(Step::Action {
            action_name,
            parameters,
            origin,
            ..
        }) => (action_name, &parameters.0, origin),
        _ => panic!("the plan has an action"),
    }
}

/// Phase 3 test 3 (AC-1): the head fills the target's first argument and is named by it; the
/// alias's own argument takes its default.
#[tokio::test]
async fn alias_head_fills_target_leading_argument() -> Result<(), Box<dyn std::error::Error>> {
    let envref = environment()?.to_ref();
    let plan = make_plan(envref, "text-abcdef/first").await?;

    let (name, parameters, origin) = last_action(&plan);
    assert_eq!(name, "pick");
    assert_eq!(origin.alias(), Some(&key("first")));
    assert_eq!(parameters.len(), 2);
    assert!(
        matches!(&parameters[0], ParameterValue::DefaultValue(n, v) if n == "offset" && v == 0),
        "{:?}",
        parameters[0]
    );
    assert!(
        matches!(&parameters[1], ParameterValue::DefaultValue(n, v) if n == "n" && v == 2),
        "{:?}",
        parameters[1]
    );
    Ok(())
}

/// Phase 3 test 4 (AC-2): a written parameter goes to the alias's own argument; a second one is
/// reported against the alias, which accepts one.
#[tokio::test]
async fn alias_parameters_follow_head() -> Result<(), Box<dyn std::error::Error>> {
    let envref = environment()?.to_ref();
    let plan = make_plan(envref.clone(), "text-abcdef/first-3").await?;
    let (_, parameters, _) = last_action(&plan);
    assert!(
        matches!(&parameters[1], ParameterValue::ParameterValue(n, v, _) if n == "n" && v == 3),
        "{:?}",
        parameters[1]
    );

    let err = make_plan(envref, "text-abcdef/first-1-2")
        .await
        .expect_err("the alias accepts one parameter");
    assert_eq!(err.error_type, ErrorType::TooManyParameters);
    assert!(
        err.message.contains("command 'first': accepts 1"),
        "{}",
        err.message
    );
    Ok(())
}

/// Phase 3 test 5 (AC-9): evaluating through the alias equals writing the target out.
#[tokio::test]
async fn alias_executes_target() -> Result<(), Box<dyn std::error::Error>> {
    let envref = environment()?.to_ref();
    let via_alias = evaluate(envref.clone(), "text-abcdef/first-3", None).await?;
    let written_out = evaluate(envref.clone(), "text-abcdef/pick-0-3", None).await?;
    assert_eq!(via_alias.try_into_string()?, "abc");
    assert_eq!(written_out.try_into_string()?, "abc");

    let default = evaluate(envref, "text-abcdef/first", None).await?;
    assert_eq!(default.try_into_string()?, "ab");
    Ok(())
}

/// Phase 3 test 6 (AC-7): every written parameter goes to the variadic argument after the head.
#[tokio::test]
async fn alias_variadic_collects_after_head() -> Result<(), Box<dyn std::error::Error>> {
    let envref = environment()?.to_ref();
    let plan = make_plan(envref.clone(), "text-x/tagged-a-b-c").await?;
    let (name, parameters, _) = last_action(&plan);
    assert_eq!(name, "tag");
    assert_eq!(parameters.len(), 2);
    assert!(
        matches!(&parameters[1], ParameterValue::MultipleParameters(n, items) if n == "items" && items.len() == 3),
        "{:?}",
        parameters[1]
    );

    let state = evaluate(envref, "text-x/tagged-a-b-c", None).await?;
    assert_eq!(state.try_into_string()?, "#:x:a,b,c");
    Ok(())
}

/// Phase 3 test 7 (AC-9, AC-12): a failure of the target names the alias the user wrote.
#[tokio::test]
async fn alias_execution_error_names_alias() -> Result<(), Box<dyn std::error::Error>> {
    let envref = environment()?.to_ref();
    let err = evaluate(envref, "text-x/broken", None)
        .await
        .expect_err("fail always fails");
    assert!(err.message.contains("boom"), "{}", err.message);
    assert!(
        err.message.contains("(via alias 'broken')"),
        "{}",
        err.message
    );
    Ok(())
}

/// Phase 3 test 8 (AC-12): the plan depends on the alias's metadata and on the target's
/// metadata and implementation - not on an implementation of the alias, which has none.
#[tokio::test]
async fn alias_plan_depends_on_alias_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let envref = environment()?.to_ref();
    let plan = make_plan(envref, "text-abcdef/first-3").await?;
    let keys: Vec<&DependencyKey> = plan.dependencies.iter().map(|d| &d.key).collect();

    for expected in [
        DependencyKey::for_command_metadata(&key("first")),
        DependencyKey::for_command_metadata(&key("pick")),
        DependencyKey::for_command_implementation(&key("pick")),
    ] {
        assert!(keys.contains(&&expected), "missing {expected}: {keys:?}");
    }
    let alias_impl = DependencyKey::for_command_implementation(&key("first"));
    assert!(
        !keys.contains(&&alias_impl),
        "an alias has no implementation"
    );
    Ok(())
}

/// Phase 3 test 9 (AC-10, AC-8): registration copies the target's state argument and flags and
/// records the definition; the alias of a volatile command plans volatile.
#[tokio::test]
async fn register_alias_copies_target_flags() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = environment()?;
    let cr = &mut env.command_registry;
    cr.register_alias(key("tock"), key("ticker"), vec![], vec![])?;

    let tock = cr
        .command_metadata_registry
        .get(key("tock"))
        .expect("tock is registered");
    let ticker = cr
        .command_metadata_registry
        .get(key("ticker"))
        .expect("ticker is registered");
    assert!(tock.volatile);
    assert_eq!(tock.state_argument, ticker.state_argument);
    assert_eq!(
        tock.definition,
        CommandDefinition::Alias {
            command: key("ticker"),
            head_parameters: vec![],
        }
    );

    let first = cr
        .command_metadata_registry
        .get(key("first"))
        .expect("first is registered");
    assert_eq!(first.label, "First characters");

    let plan = make_plan(env.to_ref(), "tock").await?;
    assert!(plan.is_volatile);
    Ok(())
}

/// Phase 3 test 10 (AC-10, AC-3, AC-5, AC-6): registration rejects an alias that cannot fit.
#[tokio::test]
async fn register_alias_rejects_inconsistent_shape() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = environment()?;
    let cr = &mut env.command_registry;

    let err = cr
        .register_alias(key("x"), key("missing"), vec![], vec![])
        .expect_err("target missing");
    assert_eq!(err.error_type, ErrorType::ActionNotRegistered);

    let err = cr
        .register_alias(key("pick"), key("pick"), vec![], vec![])
        .expect_err("alias of itself");
    assert_eq!(err.error_type, ErrorType::NotSupported);

    let err = cr
        .register_alias(key("x"), key("first"), vec![], vec![])
        .expect_err("alias of an alias");
    assert_eq!(err.error_type, ErrorType::NotSupported);

    let err = cr
        .register_alias(key("x"), key("pick"), vec![int(0), int(1), int(2)], vec![])
        .expect_err("head longer than the target's arguments");
    assert_eq!(err.error_type, ErrorType::ParameterError);

    let err = cr
        .register_alias(key("x"), key("pick"), vec![int(0)], vec![])
        .expect_err("the target's `length` has no counterpart");
    assert_eq!(err.error_type, ErrorType::ParameterError);

    let err = cr
        .register_alias(
            key("x"),
            key("tag"),
            vec![string("#")],
            vec![ArgumentInfo::string_argument("items")],
        )
        .expect_err("`items` must be variadic, as the target's is");
    assert_eq!(err.error_type, ErrorType::ParameterError);

    assert!(cr.command_metadata_registry.get(key("x")).is_none());
    Ok(())
}

/// Phase 3 test 12 (AC-1): an alias laid out as liquers-py lays them out - built by hand,
/// never through `register_alias` - is planned without the registration shape check.
#[tokio::test]
async fn hand_built_alias_is_planned_without_shape_check() -> Result<(), Box<dyn std::error::Error>>
{
    let mut env = environment()?;
    let mut loose = CommandMetadata::new("loose");
    loose.definition = CommandDefinition::Alias {
        command: key("pick"),
        head_parameters: vec![int(1)],
    };
    env.command_registry
        .command_metadata_registry
        .add_command(&loose);

    let plan = make_plan(env.to_ref(), "text-abcdef/loose").await?;
    let (name, parameters, origin) = last_action(&plan);
    assert_eq!(name, "pick");
    assert_eq!(origin.alias(), Some(&key("loose")));
    assert_eq!(parameters.len(), 1, "only the head: nothing lines it up");
    Ok(())
}

fn short(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(format!("old:{}", state.try_into_string()?)))
}

/// Replacing a registered command with an alias drops the command's implementation: the alias
/// carries no `impl_version`, and the query runs the target, not the old executor.
#[tokio::test]
async fn register_alias_replaces_a_registered_command() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = environment()?;
    let cr = &mut env.command_registry;
    register_command!(cr, fn short(state) -> result version: 7)?;
    assert!(!cr
        .command_metadata_registry
        .get(key("short"))
        .expect("short is registered")
        .impl_version
        .is_unknown());

    cr.register_alias(
        key("short"),
        key("pick"),
        vec![int(0)],
        vec![ArgumentInfo::integer_argument("n", false).with_default(1)],
    )?;
    assert!(
        cr.command_metadata_registry
            .get(key("short"))
            .expect("short is now an alias")
            .impl_version
            .is_unknown(),
        "an alias has no implementation version"
    );

    let state = evaluate(env.to_ref(), "text-abc/short", None).await?;
    assert_eq!(state.try_into_string()?, "a");
    Ok(())
}
