//! Behaviour of a frozen plan: CWD resolution, the `-R-key/.` link, and boundary cutting.

use liquers_core::{
    context::{Context, EnvRef, Environment, ImmediateEnvironment},
    error::Error,
    parse::{parse_key, parse_query},
    plan::{Plan, Step},
    query::Key,
    state::State,
    value::Value,
};
use liquers_macro::register_command;

type CommandEnvironment = ImmediateEnvironment<Value>;

fn where_am_i(_state: &State<Value>, dir: Key) -> Result<Value, Error> {
    Ok(Value::from(format!("dir={}", dir.encode())))
}

fn env() -> Result<EnvRef<CommandEnvironment>, Error> {
    let mut environment = CommandEnvironment::new();
    let registry = &mut environment.command_registry;
    register_command!(registry, fn where_am_i(state, dir: Key = query "-R-key/.") -> result)?;
    Ok(environment.to_ref())
}

/// A `-R-key/.` default link delivers the working directory to a command as data.
///
/// This is the supported replacement for reading the working key out of `Context`: the directory
/// is explicit in the plan, overridable per call, and visible to the planner.
#[tokio::test]
async fn cwd_reaches_a_command_through_a_key_link() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env()?;
    let asset = envref
        .evaluate(parse_query("-R-cwd/proj/a/-/where_am_i")?)
        .await?;
    assert_eq!(asset.get().await?.try_into_string()?, "dir=proj/a");
    Ok(())
}

/// The same query in two directories yields two results, and the link is what distinguishes them.
#[tokio::test]
async fn key_link_resolves_per_directory() -> Result<(), Box<dyn std::error::Error>> {
    for folder in ["proj/a", "proj/b"] {
        let envref = env()?;
        let asset = envref
            .evaluate(parse_query(&format!("-R-cwd/{folder}/-/where_am_i"))?)
            .await?;
        assert_eq!(
            asset.get().await?.try_into_string()?,
            format!("dir={folder}")
        );
    }
    Ok(())
}

/// An explicit argument overrides the default link, which is the point of expressing the
/// directory as data rather than reading it from context.
#[tokio::test]
async fn key_link_default_is_overridable() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env()?;
    let asset = envref
        .evaluate(parse_query("-R-cwd/proj/a/-/where_am_i-~X~-R-key/other/place~E")?)
        .await?;
    assert_eq!(asset.get().await?.try_into_string()?, "dir=other/place");
    Ok(())
}

/// Freezing rewrites relative operands and is idempotent; re-freezing under a different CWD is a
/// caller error rather than a silent re-resolution.
#[tokio::test]
async fn freeze_is_idempotent_and_rejects_a_second_cwd() -> Result<(), Box<dyn std::error::Error>> {
    let mut plan = Plan::new();
    plan.steps = vec![
        Step::SetCwd(parse_key("a/b")?),
        Step::GetAsset(parse_key("./x.csv")?),
    ];

    let (_, defaulted) = plan.freeze_cwd(Some(parse_key("root")?))?;
    assert!(!defaulted, "an explicit entry CWD is not a root fallback");
    assert!(matches!(&plan.steps[1], Step::GetAsset(key) if key.encode() == "a/b/x.csv"));

    let before = plan.steps.clone();
    plan.freeze_cwd(Some(parse_key("root")?))?;
    assert_eq!(
        format!("{:?}", before),
        format!("{:?}", plan.steps),
        "freezing twice against the same CWD must not move anything"
    );

    let error = plan
        .freeze_cwd(Some(parse_key("elsewhere")?))
        .expect_err("re-freezing under a different CWD is a caller error");
    assert!(error.message.contains("already frozen"), "{error}");
    Ok(())
}

/// A relative operand with no entry CWD falls back to logical root and says so, so the caller can
/// warn exactly once. A plan with no relative operand reports no fallback at all.
#[tokio::test]
async fn root_fallback_is_reported_only_when_used() -> Result<(), Box<dyn std::error::Error>> {
    let mut relative = Plan::new();
    relative.steps = vec![Step::GetAsset(parse_key("./x.csv")?)];
    let (_, defaulted) = relative.freeze_cwd(None)?;
    assert!(defaulted, "a relative operand with no CWD used the fallback");

    let mut absolute = Plan::new();
    absolute.steps = vec![Step::GetAsset(parse_key("data/x.csv")?)];
    let (_, defaulted) = absolute.freeze_cwd(None)?;
    assert!(!defaulted, "an absolute operand never touches the fallback");
    Ok(())
}

/// Cutting requires a frozen plan: cutting an unfrozen one would produce a boundary query that
/// still depended on a working key, which is the defect freezing removes.
#[tokio::test]
async fn cut_requires_a_frozen_plan() -> Result<(), Box<dyn std::error::Error>> {
    let mut plan = Plan::new();
    plan.steps = vec![Step::GetAsset(parse_key("a/b/x.csv")?)];
    plan.predecessor = Some(parse_query("-R/a/b/x.csv")?);
    plan.predecessor_steps = 1;

    let error = plan
        .cut_predecessor()
        .expect_err("cutting an unfrozen plan is a caller error");
    assert!(error.message.contains("frozen"), "{error}");

    plan.freeze_cwd(Some(Key::new()))?;
    assert!(plan.cut_predecessor()?);
    assert!(matches!(plan.steps.first(), Some(Step::Evaluate(_))));
    Ok(())
}
