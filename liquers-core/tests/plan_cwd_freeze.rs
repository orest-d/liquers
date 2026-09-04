//! Behaviour of a frozen plan: CWD resolution, the `-R-key/.` link, and boundary cutting.

use liquers_core::{
    context::{Context, EnvRef, Environment, ImmediateEnvironment},
    error::Error,
    parse::{parse_key, parse_query},
    plan::{Plan, Step, VolatilitySource},
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
        .evaluate(parse_query(
            "-R-cwd/proj/a/-/where_am_i-~X~-R-key/other/place~E",
        )?)
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
    assert!(
        defaulted,
        "a relative operand with no CWD used the fallback"
    );

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
    let envref = env()?;
    let cmr = envref.get_command_metadata_registry();
    let mut plan = Plan::new();
    // A real tail: a boundary covering every step would leave the parent empty, which
    // `cut_predecessor` declines. See `a_whole_plan_cut_is_declined`.
    plan.steps = vec![
        Step::GetAsset(parse_key("a/b/x.csv")?),
        Step::Info("tail".to_owned()),
    ];
    plan.predecessor = Some(parse_query("-R/a/b/x.csv")?);
    plan.predecessor_steps = 1;

    let error = plan
        .cut_predecessor(cmr)
        .expect_err("cutting an unfrozen plan is a caller error");
    assert!(error.message.contains("frozen"), "{error}");

    plan.freeze_cwd(Some(Key::new()))?;
    assert!(plan.cut_predecessor(cmr)?);
    assert!(matches!(plan.steps.first(), Some(Step::Evaluate(_))));
    Ok(())
}

/// A dependency's failure is reported with its own cause, not merely as "a dependency failed".
///
/// This matters most once an evaluation boundary sits between the caller and the command that
/// actually failed: without chaining, the diagnosis lives only in the sub-asset's log.
#[tokio::test]
async fn dependency_failure_reports_its_cause() -> Result<(), Box<dyn std::error::Error>> {
    fn always_fails(_state: &State<Value>) -> Result<Value, Error> {
        Err(Error::general_error("the real reason".to_owned()))
    }
    fn takes_a_link(_state: &State<Value>, value: String) -> Result<Value, Error> {
        Ok(Value::from(value))
    }

    let mut environment = CommandEnvironment::new();
    let registry = &mut environment.command_registry;
    register_command!(registry, fn always_fails(state) -> result)?;
    register_command!(registry, fn takes_a_link(state, value: String) -> result)?;
    let envref = environment.to_ref();

    // The immediate environment surfaces the failure from `evaluate` itself; a queued one
    // surfaces it from `get`. Accept either, since the point is the message, not the path.
    let error = match envref
        .evaluate(parse_query("takes_a_link-~X~always_fails~E")?)
        .await
    {
        Err(error) => error,
        Ok(asset) => asset.get().await.expect_err("the dependency fails"),
    };
    let rendered = error.to_string();
    assert!(
        rendered.contains("the real reason"),
        "the cause must survive the dependency boundary, got: {rendered}"
    );
    // Surfacing the cause must not re-wrap it. Rebuilding through `Error::from_error` would store
    // the cause's already-rendered form and then re-attach its command and position, giving
    // "Command 'x' failed: Command 'x' failed: ... at .. at ..".
    assert_eq!(
        rendered.matches("failed:").count(),
        1,
        "the cause is reported once, not wrapped again: {rendered}"
    );
    assert_eq!(
        rendered.matches(" at ").count(),
        1,
        "the position is attached once: {rendered}"
    );
    Ok(())
}

// ===========================================================================================
// Equivalence suite
//
// The expanded plan is the **oracle**: this suite verifies the *cut* plan against it, rather
// than asserting a contract between two shipping forms. So it compares the result — value,
// `is_volatile`, `payload_required`, and the surfaced error — and nothing else.
//
// A cut deliberately changes asset count, dependency edges and metadata: that is what a
// boundary *is*. Those are outside what the oracle claims and are not compared. If you are
// adding a shape and reaching for a metadata assertion, you have found the feature working.
//
// Every shape runs under three CWD conditions. That axis is load-bearing rather than
// decorative: the harness this replaced always built a recipe with no `cwd:` and passed
// `cwd: None`, so it could not reach the prologue defect no matter how many shapes were added.
// ===========================================================================================

use liquers_core::{
    assets::AssetRef,
    command_metadata::PayloadRequirement,
    context::ImmediateEnvironmentWithPayload,
    interpreter::{apply_plan, finalize_plan, finalize_plan_expanded},
    metadata::{Metadata, MetadataRecord, Status},
    recipes::{DefaultRecipeProvider, Recipe},
    store::{AsyncMemoryStore, AsyncStore},
    value::ValueInterface,
};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Which working key the shape is evaluated under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cwd {
    /// No working key at all — the condition the previous harness was limited to.
    None,
    /// A programmatic `recipe.cwd`, which is what creates the prologue.
    Recipe,
    /// A recipe read from a provider at a key, which adds the keyed-asset path.
    Provider,
}

impl Cwd {
    fn label(self) -> &'static str {
        match self {
            Cwd::None => "no-cwd",
            Cwd::Recipe => "recipe-cwd",
            Cwd::Provider => "provider",
        }
    }
}

/// What the oracle claims, and therefore all that is compared.
#[derive(Debug, PartialEq)]
struct Outcome {
    result: Result<String, String>,
    is_volatile: bool,
    payload_required: PayloadRequirement,
}

fn describe(outcome: &Outcome) -> String {
    match &outcome.result {
        Ok(value) => format!(
            "ok:{value} volatile={} payload={}",
            outcome.is_volatile, outcome.payload_required
        ),
        Err(message) => format!(
            "err:{message} volatile={} payload={}",
            outcome.is_volatile, outcome.payload_required
        ),
    }
}

const SUITE_CWD: &str = "proj/a";

async fn suite_store() -> Result<AsyncMemoryStore, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    for (path, text) in [
        ("input.csv", "root-input"),
        ("data/big.csv", "shared"),
        ("proj/a/input.csv", "folder-input"),
        ("proj/a/x.csv", "folder-x"),
        ("proj/a/sub/input.csv", "sub-input"),
        // Root copies, so the no-CWD condition produces a value rather than agreeing only on a
        // "key not found" — an error matching both ways is a weaker check than a value doing so.
        ("x.csv", "root-x"),
        ("sub/input.csv", "root-sub-input"),
    ] {
        let key = parse_key(path)?;
        let mut record = MetadataRecord::new();
        record
            .with_key(key.clone())
            .with_type_identifier("Text".to_owned())
            .with_status(Status::Source);
        store
            .set(&key, text.as_bytes(), &Metadata::MetadataRecord(record))
            .await?;
    }
    Ok(store)
}

/// Builds the plan for `query` under one CWD condition, without evaluating it.
///
/// The conditions differ in *where the recipe's working key comes from*, which is what puts a
/// `SetCwd` prologue in front of the builder's steps — the thing the previous harness could
/// never produce. `Cwd::Provider` additionally obtains the recipe the way the asset manager
/// does, from a `recipes.yaml` at a key.
async fn suite_plan(
    envref: &EnvRef<CommandEnvironment>,
    query: &str,
    condition: Cwd,
    cut: bool,
) -> Result<(Plan, Context<CommandEnvironment>), Error> {
    let cmr = envref.get_command_metadata_registry();
    let recipe = match condition {
        Cwd::None => Recipe::new(query.to_owned(), String::new(), String::new())?,
        Cwd::Recipe => {
            let mut recipe = Recipe::new(query.to_owned(), String::new(), String::new())?;
            recipe.cwd = Some(SUITE_CWD.to_owned());
            recipe
        }
        Cwd::Provider => provider_recipe(envref, query).await?,
    };
    let mut plan = recipe.to_plan(cmr)?;
    let asset = AssetRef::new_temporary(envref.clone());
    let context = Context::new(asset, false).await;
    // The oracle is built by `finalize_plan_expanded`, not by asking `finalize_plan` not to
    // cut. An oracle derived from the cutting path cannot detect the cutting path regressing —
    // which is exactly what happened once `finalize_plan` started cutting: both sides of the
    // comparison were cut, and the suite passed while comparing a plan against itself.
    if cut {
        finalize_plan(envref.clone(), &mut plan, &context, &State::new()).await?;
    } else {
        finalize_plan_expanded(envref.clone(), &mut plan, &context).await?;
    }
    Ok((plan, context))
}

/// Whether a plan carries a boundary, which is what "cut" means observably.
fn has_boundary(plan: &Plan) -> bool {
    plan.steps
        .iter()
        .any(|step| matches!(step, Step::Evaluate(_)))
}

/// Writes `query` into `proj/a/recipes.yaml` and reads the recipe back through the provider, so
/// the working key is provider-supplied rather than set by the test.
///
/// A recipe is addressed by its query's filename, so a shape that has none gets one appended.
async fn provider_recipe(
    envref: &EnvRef<CommandEnvironment>,
    query: &str,
) -> Result<Recipe, Error> {
    let parsed = parse_query(query)?;
    let (stored, name) = match parsed.filename() {
        Some(filename) => (query.to_owned(), filename.name.clone()),
        None => (format!("{query}/suite.txt"), "suite.txt".to_owned()),
    };
    let yaml = format!("recipes:\n  - query: \"{stored}\"\n");
    envref
        .get_async_store()
        .set(
            &parse_key("proj/a/recipes.yaml")?,
            yaml.as_bytes(),
            &Metadata::new(),
        )
        .await?;
    envref
        .get_recipe_provider()
        .recipe_opt(&parse_key(&format!("proj/a/{name}"))?, envref.clone())
        .await?
        .ok_or_else(|| Error::general_error(format!("no provider recipe for {query}")))
}

/// Evaluates one shape both ways and returns the two outcomes plus whether a cut happened.
async fn evaluate_both_ways(
    envref: &EnvRef<CommandEnvironment>,
    query: &str,
    condition: Cwd,
) -> Result<(Outcome, Outcome, bool), Error> {
    let (expanded_plan, expanded_context) = suite_plan(envref, query, condition, false).await?;
    let (cut_plan, cut_context) = suite_plan(envref, query, condition, true).await?;

    // The oracle must actually be expanded, or the comparison is between a plan and itself.
    assert!(
        !has_boundary(&expanded_plan),
        "the oracle for {query} [{}] carries a boundary; it is not an expanded plan",
        condition.label()
    );
    let was_cut = has_boundary(&cut_plan);

    let run = |plan: Plan, context: Context<CommandEnvironment>| {
        let envref = envref.clone();
        async move {
            let is_volatile = plan.is_volatile;
            let payload_required = plan.payload_required;
            let result = apply_plan(plan, State::new(), context, envref)
                .await
                .and_then(|value| (*value).try_into_string())
                .map_err(|error| error.message.clone());
            Outcome {
                result,
                is_volatile,
                payload_required,
            }
        }
    };

    let expanded = run(expanded_plan, expanded_context).await;
    let cut = run(cut_plan, cut_context).await;
    Ok((expanded, cut, was_cut))
}

/// Runs every shape under every condition and reports **all** divergences, not the first.
///
/// The four divergences this design removed were found in one forced run of the whole suite; a
/// fail-fast harness would have surfaced them one release apart.
async fn assert_equivalent(
    envref: &EnvRef<CommandEnvironment>,
    shapes: &[(&str, &str)],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut divergences = Vec::new();
    let mut cut_count = 0usize;
    for (label, query) in shapes {
        for condition in [Cwd::None, Cwd::Recipe, Cwd::Provider] {
            let (expanded, cut, was_cut) = evaluate_both_ways(envref, query, condition).await?;
            if was_cut {
                cut_count += 1;
            }
            if expanded != cut {
                divergences.push(format!(
                    "  {label} [{}] {query}\n      expanded: {}\n      cut:      {}",
                    condition.label(),
                    describe(&expanded),
                    describe(&cut)
                ));
            }
        }
    }
    assert!(
        divergences.is_empty(),
        "cut and expanded diverged on {} of {} runs:\n{}",
        divergences.len(),
        shapes.len() * 3,
        divergences.join("\n")
    );
    assert!(
        cut_count > 0,
        "no shape was cut at all — the suite would pass vacuously"
    );
    Ok(())
}

// --- suite fixtures ------------------------------------------------------------------------

static ANALYZE_CALLS: AtomicUsize = AtomicUsize::new(0);

fn analyze(state: &State<Value>) -> Result<Value, Error> {
    ANALYZE_CALLS.fetch_add(1, Ordering::SeqCst);
    Ok(Value::from(format!("A[{}]", state.try_into_string()?)))
}
fn seed(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("seed"))
}
fn upper(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(state.try_into_string()?.to_uppercase()))
}
fn boom(_state: &State<Value>) -> Result<Value, Error> {
    Err(Error::general_error("the real reason".to_owned()))
}
fn join(state: &State<Value>, other: String) -> Result<Value, Error> {
    Ok(Value::from(format!("{}+{other}", state.try_into_string()?)))
}
fn vol_counted(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("vol"))
}
fn sink(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(format!("<{}>", state.try_into_string()?)))
}

fn suite_env(store: AsyncMemoryStore) -> Result<EnvRef<CommandEnvironment>, Error> {
    let mut environment = CommandEnvironment::new();
    {
        let registry = &mut environment.command_registry;
        register_command!(registry, fn analyze(state) -> result)?;
        register_command!(registry, fn seed(state) -> result)?;
        register_command!(registry, fn upper(state) -> result)?;
        register_command!(registry, fn boom(state) -> result)?;
        register_command!(registry, fn join(state, other: String) -> result)?;
        register_command!(registry, fn sink(state) -> result)?;
        register_command!(registry, fn where_am_i(state, dir: Key = query "-R-key/.") -> result)?;
        register_command!(registry, fn vol_counted(state) -> result volatile: true)?;
    }
    environment.with_async_store(Box::new(store));
    environment.with_recipe_provider(Box::new(DefaultRecipeProvider));
    Ok(environment.to_ref())
}

/// E1-E6, E9-E12, E15, E16 — every shape expressible without a payload, under all three CWD
/// conditions. E7/E8/E13/E14 need a payload environment and run separately below.
#[tokio::test]
async fn equivalence_suite() -> Result<(), Box<dyn std::error::Error>> {
    let envref = suite_env(suite_store().await?)?;
    assert_equivalent(
        &envref,
        &[
            ("E1  transform chain", "seed/upper"),
            (
                "E2  resource then action",
                "-R-stored/./input.csv/-/analyze",
            ),
            (
                "E3  resource, action, filename",
                "-R-stored/./x.csv/-/analyze/result.txt",
            ),
            (
                "E4  cwd-setting predecessor",
                "-R-cwd/./sub/-R-stored/./input.csv/-/analyze",
            ),
            ("E5  absolute query", "/-R-stored/./input.csv/-/analyze"),
            ("E6  volatile command", "vol_counted/upper"),
            (
                "E9  explicit link parameter",
                "-R-stored/./x.csv/-/join-~X~-R-stored/data/big.csv/-/analyze~E",
            ),
            ("E11 relative default link", "where_am_i/upper"),
            (
                "E12 chain through a link",
                "seed/join-~X~-R-stored/data/big.csv/-/analyze~E/upper",
            ),
            ("E16 v mid-chain", "seed/v/upper"),
            ("E16 v at the end", "seed/upper/v"),
            ("Ex  failure surfaces its cause", "boom/upper"),
            ("Ex  longer chain", "seed/upper/sink/analyze"),
        ],
    )
    .await
}

// --- payload shapes: E7, E8, E13, E14 -------------------------------------------------------
//
// A payload is deliberately not part of a cache key, so a payload-requiring prefix can never
// become a cached boundary. These run on a payload environment and check *where the boundary
// lands*, which is the observable the placement rule decides.

type PayloadEnvironment = ImmediateEnvironmentWithPayload<Value, String>;

fn greet(state: &State<Value>, who: String) -> Result<Value, Error> {
    Ok(Value::from(format!("{}, {who}", state.try_into_string()?)))
}
fn stamp(_state: &State<Value>, mark: String) -> Result<Value, Error> {
    Ok(Value::from(format!("stamped:{mark}")))
}
fn shout(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(format!("{}!", state.try_into_string()?)))
}

fn payload_env(declared: bool) -> Result<EnvRef<PayloadEnvironment>, Error> {
    type CommandEnvironment = PayloadEnvironment;
    let mut environment = PayloadEnvironment::new();
    {
        let registry = &mut environment.command_registry;
        register_command!(registry, fn greet(state, who: String) -> result)?;
        register_command!(registry, fn shout(state) -> result)?;
        if declared {
            register_command!(registry, fn stamp(state, mark: String injected) -> result
                payload: required)?;
        } else {
            register_command!(registry, fn stamp(state, mark: String injected) -> result)?;
        }
    }
    Ok(environment.to_ref())
}

async fn payload_boundary(
    envref: &EnvRef<PayloadEnvironment>,
    query: &str,
) -> Result<Option<String>, Error> {
    let cmr = envref.get_command_metadata_registry();
    let recipe = Recipe::new(query.to_owned(), String::new(), String::new())?;
    let mut plan = recipe.to_plan(cmr)?;
    let asset = AssetRef::new_temporary(envref.clone());
    let context = Context::new(asset, false).await;
    // `finalize_plan` already cuts; a second call would re-cut an already-cut plan.
    finalize_plan(envref.clone(), &mut plan, &context, &State::new()).await?;
    Ok(plan.steps.iter().find_map(|step| match step {
        Step::Evaluate(query) => Some(query.encode()),
        _other => None,
    }))
}

/// E7 — a declared payload requirement is visible in the candidate's plan, so the walk steps
/// back in front of it and the payload-consuming part stays inline.
#[tokio::test]
async fn e7_declared_payload_moves_the_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let envref = payload_env(true)?;
    assert_eq!(
        payload_boundary(&envref, "greet-World/stamp/shout")
            .await?
            .as_deref(),
        Some("greet-World"),
        "the boundary lands in front of the payload-requiring command"
    );
    Ok(())
}

/// E8 — the pinned **inequivalence**. Without `payload: required` the plan cannot see that the
/// command reads a payload, so the boundary is cut across it and the value is lost behind the
/// cache key. This is the "declare it, or lose it" rule, and it stays falsifiable by being
/// asserted rather than described.
#[tokio::test]
async fn e8_an_undeclared_payload_is_cut_across() -> Result<(), Box<dyn std::error::Error>> {
    let envref = payload_env(false)?;
    assert_eq!(
        payload_boundary(&envref, "greet-World/stamp/shout")
            .await?
            .as_deref(),
        Some("greet-World/stamp"),
        "undeclared, the payload command is swallowed by the boundary — a declaration defect, \
         not a cutting defect"
    );
    Ok(())
}

/// E13 — mid-chain: the walk steps back exactly one level, no further.
#[tokio::test]
async fn e13_mid_chain_payload_steps_back_once() -> Result<(), Box<dyn std::error::Error>> {
    let envref = payload_env(true)?;
    assert_eq!(
        payload_boundary(&envref, "greet-A/greet-B/stamp/shout")
            .await?
            .as_deref(),
        Some("greet-A/greet-B"),
        "the walk stops at the first cacheable candidate, it does not keep unwinding"
    );
    Ok(())
}

/// E14 — the requirement reaches the head, so no boundary at any position is safe.
#[tokio::test]
async fn e14_head_payload_cuts_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let envref = payload_env(true)?;
    assert_eq!(
        payload_boundary(&envref, "stamp/greet-World/shout").await?,
        None
    );
    Ok(())
}

// --- corner cases ---------------------------------------------------------------------------

/// E15 — a recipe-level `volatile:` is in no query, so no candidate could reveal it. Without the
/// `Declared` source the prefix would be cut into a cached boundary and run once, while the
/// volatile parent dutifully recomputed around it: volatile in name, fixed in value.
#[tokio::test]
async fn e15_a_volatile_recipe_is_not_cut() -> Result<(), Box<dyn std::error::Error>> {
    let envref = suite_env(suite_store().await?)?;
    let cmr = envref.get_command_metadata_registry();

    let mut recipe = Recipe::new(
        "seed/upper/out.txt".to_owned(),
        String::new(),
        String::new(),
    )?;
    let plain = recipe.to_plan(cmr)?;
    assert_eq!(plain.volatility_source, None);

    recipe.volatile = true;
    let mut plan = recipe.to_plan(cmr)?;
    assert!(
        plan.is_volatile,
        "the recipe's declaration reaches the plan"
    );
    assert_eq!(plan.volatility_source, Some(VolatilitySource::Declared));
    plan.freeze_cwd(None)?;
    assert!(
        !plan.cut_predecessor(cmr)?,
        "nothing in a declared-volatile plan may be cached"
    );
    Ok(())
}

/// A finite expiration bounds the result's validity, not the purity of the computation, so it
/// must not stop the prefix being cached.
#[tokio::test]
async fn a_finite_expiration_still_cuts() -> Result<(), Box<dyn std::error::Error>> {
    let envref = suite_env(suite_store().await?)?;
    let cmr = envref.get_command_metadata_registry();
    let mut recipe = Recipe::new(
        "seed/upper/out.txt".to_owned(),
        String::new(),
        String::new(),
    )?;
    recipe.expires = "in 5 minutes".parse()?;
    let mut plan = recipe.to_plan(cmr)?;
    assert!(!plan.expires.is_never(), "the expiration reached the plan");
    plan.freeze_cwd(None)?;
    assert!(
        plan.cut_predecessor(cmr)?,
        "a finite expiration does not block a cut"
    );
    Ok(())
}

/// **The debt the recipe fold incurred.** Folding a recipe's volatility into the plan makes a
/// volatile recipe stop registering plan dependencies, exactly as a volatile plan already does.
/// Nineteen suites stayed green through that change precisely because nothing asserted it; this
/// is the assertion.
#[tokio::test]
async fn volatile_recipe_skips_dependency_registration() -> Result<(), Box<dyn std::error::Error>> {
    let envref = suite_env(suite_store().await?)?;
    let cmr = envref.get_command_metadata_registry();
    let query = "-R-stored/proj/a/input.csv/-/analyze/out.txt";

    for (volatile, expect_records) in [(false, true), (true, false)] {
        let mut recipe = Recipe::new(query.to_owned(), String::new(), String::new())?;
        recipe.volatile = volatile;
        let mut plan = recipe.to_plan(cmr)?;
        let asset = AssetRef::new_temporary(envref.clone());
        let context = Context::new(asset, false).await;
        finalize_plan(envref.clone(), &mut plan, &context, &State::new()).await?;

        let recorded = !context.take_pending_dependencies().await.is_empty();
        assert_eq!(
            recorded, expect_records,
            "volatile={volatile}: a volatile plan records no dependencies, a plain one does"
        );
    }
    Ok(())
}

/// A frozen, cut plan survives a serde round trip with its new fields intact, and a plan
/// serialized before they existed loads at the pre-change values.
#[tokio::test]
async fn cut_plan_survives_serde_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let envref = suite_env(suite_store().await?)?;
    let cmr = envref.get_command_metadata_registry();
    let mut recipe = Recipe::new(
        "seed/upper/out.txt".to_owned(),
        String::new(),
        String::new(),
    )?;
    recipe.cwd = Some(SUITE_CWD.to_owned());
    let mut plan = recipe.to_plan(cmr)?;
    plan.freeze_cwd(None)?;
    assert!(plan.cut_predecessor(cmr)?);

    let text = serde_json::to_string(&plan)?;
    let back: Plan = serde_json::from_str(&text)?;
    assert_eq!(back.prologue_steps, plan.prologue_steps);
    assert_eq!(back.volatility_source, plan.volatility_source);
    assert_eq!(back.frozen_cwd, plan.frozen_cwd);
    assert_eq!(back.steps.len(), plan.steps.len());
    assert_eq!(back.predecessor_steps, plan.predecessor_steps);
    // `check_consistent` is crate-private, so the invariants it enforces are asserted here
    // directly; the unit tests in `plan.rs` cover the function itself.
    assert!(back.prologue_steps <= back.steps.len());
    Ok(())
}

/// Two consumers sharing a prefix get one boundary query, which is what makes the intermediate
/// shareable at all: the asset manager keys a non-keyed query asset by its query.
#[tokio::test]
async fn a_shared_prefix_yields_one_boundary_query() -> Result<(), Box<dyn std::error::Error>> {
    let envref = suite_env(suite_store().await?)?;
    let cmr = envref.get_command_metadata_registry();
    let mut boundaries = Vec::new();
    for query in [
        "-R-stored/./input.csv/-/analyze/upper/report.txt",
        "-R-stored/./input.csv/-/analyze/sink/summary.txt",
    ] {
        let mut recipe = Recipe::new(query.to_owned(), String::new(), String::new())?;
        recipe.cwd = Some(SUITE_CWD.to_owned());
        let mut plan = recipe.to_plan(cmr)?;
        plan.freeze_cwd(None)?;
        assert!(plan.cut_predecessor(cmr)?);
        boundaries.push(plan.steps.iter().find_map(|step| match step {
            Step::Evaluate(query) => Some(query.encode()),
            _other => None,
        }));
    }
    assert_eq!(
        boundaries[0], boundaries[1],
        "a shared prefix must produce the same boundary query, or it cannot be shared"
    );
    assert_eq!(
        boundaries[0].as_deref(),
        Some("-R-stored/proj/a/input.csv/-/analyze"),
        "and it is absolute, so it identifies the same asset from anywhere"
    );
    Ok(())
}

// --- input state and the boundary -----------------------------------------------------------

fn wrap(state: &State<Value>) -> Result<Value, Error> {
    let inner = match state.try_into_string() {
        Ok(text) => text,
        Err(_) => "None".to_owned(),
    };
    Ok(Value::from(format!("[{inner}]")))
}

fn wrap_env() -> Result<EnvRef<CommandEnvironment>, Error> {
    let mut environment = CommandEnvironment::new();
    {
        let registry = &mut environment.command_registry;
        register_command!(registry, fn wrap(state) -> result)?;
    }
    Ok(environment.to_ref())
}

/// A caller's input state must survive, which means the prefix that consumes it must not be
/// moved behind a boundary.
///
/// A boundary is a cache entry keyed by its query, and an input state is not part of that key —
/// the same soundness argument as a payload. A cut boundary is evaluated as its own asset,
/// starting from `State::new()`, so a cut prefix would silently receive nothing: applying
/// `wrap/wrap` to `"x"` produced `[[None]]` instead of `[[x]]`.
#[tokio::test]
async fn an_input_state_survives_finalization() -> Result<(), Box<dyn std::error::Error>> {
    let envref = wrap_env()?;
    let cmr = envref.get_command_metadata_registry();

    for (label, input, expected) in [
        ("empty state", State::new(), "[[None]]"),
        (
            "supplied state",
            State::new().with_data(Value::from("x")),
            "[[x]]",
        ),
    ] {
        let recipe = Recipe::new("wrap/wrap".to_owned(), String::new(), String::new())?;
        let mut plan = recipe.to_plan(cmr)?;
        let asset = AssetRef::new_temporary(envref.clone());
        let context = Context::new(asset, false).await;
        finalize_plan(envref.clone(), &mut plan, &context, &input).await?;

        let stateful = !input.is_none();
        assert_eq!(
            has_boundary(&plan),
            !stateful,
            "{label}: a plan fed by an input state must not be cut"
        );
        let result = apply_plan(plan, input, context, envref.clone()).await?;
        assert_eq!((*result).try_into_string()?, expected, "{label}");
    }
    Ok(())
}

/// The decline is recorded, so it is distinguishable from a plan that had no predecessor.
#[tokio::test]
async fn a_stateful_application_says_why_it_was_not_cut() -> Result<(), Box<dyn std::error::Error>>
{
    let envref = wrap_env()?;
    let cmr = envref.get_command_metadata_registry();
    let recipe = Recipe::new("wrap/wrap".to_owned(), String::new(), String::new())?;
    let mut plan = recipe.to_plan(cmr)?;
    let asset = AssetRef::new_temporary(envref.clone());
    let context = Context::new(asset, false).await;
    let input = State::new().with_data(Value::from("x"));
    finalize_plan(envref.clone(), &mut plan, &context, &input).await?;

    assert!(
        plan.init_steps
            .iter()
            .any(|step| matches!(step, Step::Info(m)
            if m.contains("input state"))),
        "the reason is recorded: {:?}",
        plan.init_steps
    );
    Ok(())
}

/// `finalize_plan_expanded` never cuts, whatever the plan — that is what makes it usable as the
/// suite's oracle and for analysis.
#[tokio::test]
async fn finalize_expanded_never_cuts() -> Result<(), Box<dyn std::error::Error>> {
    let envref = wrap_env()?;
    let cmr = envref.get_command_metadata_registry();
    let recipe = Recipe::new("wrap/wrap/wrap".to_owned(), String::new(), String::new())?;
    let mut plan = recipe.to_plan(cmr)?;
    let asset = AssetRef::new_temporary(envref.clone());
    let context = Context::new(asset, false).await;
    finalize_plan_expanded(envref.clone(), &mut plan, &context).await?;

    assert!(
        !has_boundary(&plan),
        "expanded means no boundary: {:?}",
        plan.steps
    );
    assert!(
        plan.frozen_cwd.is_some(),
        "but it is still frozen and analysed"
    );
    Ok(())
}
