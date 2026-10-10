//! What a command receives as its input state (`specs/design/plan-step-state-metadata/`).
//!
//! The cut plan is the reference: a predecessor boundary hands on the state of its asset
//! unchanged, so a command receives what its predecessor would produce as an asset. The
//! expanded plan approximates that. `probe` reports the metadata of the state it received, so
//! each test reads exactly what a command was given.

use liquers_core::{
    assets::{AssetManager, AssetRef},
    context::{Context, EnvRef, Environment, SimpleEnvironment},
    error::Error,
    interpreter::{apply_plan, finalize_plan, make_plan},
    metadata::{LogEntryKind, Metadata, MetadataRecord, Status},
    parse::{parse_key, parse_query},
    plan::{PlanBuilder, Step},
    query::Key,
    recipes::Recipe,
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::{Value, ValueInterface},
};
use liquers_macro::register_command;
use serde_json::{json, Value as Json};

type CommandEnvironment = SimpleEnvironment<Value>;

fn a(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("A"))
}

fn b(state: &State<Value>, context: Context<CommandEnvironment>) -> Result<Value, Error> {
    context.info("b ran")?;
    Ok(Value::from(format!("{}B", state.try_into_string()?)))
}

fn suffix(state: &State<Value>, text: String) -> Result<Value, Error> {
    Ok(Value::from(format!("{}{text}", state.try_into_string()?)))
}

/// The input state's description, as JSON text.
fn probe(state: &State<Value>) -> Result<Value, Error> {
    let Metadata::MetadataRecord(record) = &*state.metadata else {
        return Ok(Value::from(json!({ "legacy": true }).to_string()));
    };
    Ok(Value::from(
        json!({
            "query": record.query.encode(),
            "key": record.key.as_ref().map(|key| key.encode()),
            "filename": record.filename,
            "data_format": record.data_format,
            "status": format!("{:?}", record.status),
            "is_applied": record.is_applied,
        })
        .to_string(),
    ))
}

async fn env() -> Result<EnvRef<CommandEnvironment>, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    let key = parse_key("data/x.txt")?;
    let mut metadata = MetadataRecord::new();
    metadata
        .with_key(key.clone())
        .with_status(Status::Source)
        .with_type_identifier("Text".to_owned());
    store
        .set(&key, b"stored", &Metadata::MetadataRecord(metadata))
        .await?;
    store
        .set(
            &parse_key("data/legacy.txt")?,
            b"legacy",
            &Metadata::LegacyMetadata(json!({ "comment": "written by another tool" })),
        )
        .await?;

    let mut environment = CommandEnvironment::new();
    let cr = &mut environment.command_registry;
    register_command!(cr, fn a(state) -> result)?;
    register_command!(cr, fn b(state, context) -> result)?;
    register_command!(cr, fn suffix(state, text: String) -> result)?;
    register_command!(cr, fn probe(state) -> result)?;
    environment.with_async_store(Box::new(store));
    environment.with_default_recipe_provider();
    Ok(environment.to_ref())
}

fn report(state: &State<Value>) -> Result<Json, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&state.try_into_string()?)?)
}

async fn evaluate(
    envref: &EnvRef<CommandEnvironment>,
    query: &str,
) -> Result<Json, Box<dyn std::error::Error>> {
    let state = envref.evaluate(parse_query(query)?).await?.get().await?;
    report(&state)
}

fn csv_state() -> State<Value> {
    let mut metadata = MetadataRecord::new();
    metadata.data_format = Some("csv".to_owned());
    State::new()
        .with_data(Value::from("a\n1"))
        .with_metadata(Metadata::MetadataRecord(metadata))
}

/// Applies a hand-built plan in a fresh temporary asset.
async fn apply_steps(
    envref: &EnvRef<CommandEnvironment>,
    leading: Vec<Step>,
    input: State<Value>,
) -> Result<Json, Box<dyn std::error::Error>> {
    let mut plan =
        PlanBuilder::new(parse_query("probe")?, envref.get_command_metadata_registry()).build()?;
    for (index, step) in leading.into_iter().enumerate() {
        plan.steps.insert(index, step);
    }
    let context = Context::new(AssetRef::new_temporary(envref.clone()), false).await;
    let value = apply_plan(plan, input, context, envref.clone()).await?;
    Ok(serde_json::from_str(&value.try_into_string()?)?)
}

/// Phase 3 test 3 (AC-1): a step that produces no value hands its input state on.
#[tokio::test]
async fn pass_through_steps_keep_the_input_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env().await?;
    for step in [
        Step::Info("info".to_owned()),
        Step::Warning("warning".to_owned()),
        Step::Error("error".to_owned()),
        Step::SetCwd(parse_key("data")?),
        Step::Filename(liquers_core::query::ResourceName::new("named.txt".to_owned())),
    ] {
        let seen = apply_steps(&envref, vec![step.clone()], csv_state()).await?;
        assert_eq!(seen["data_format"], "csv", "after {step:?}: {seen}");
    }
    Ok(())
}

/// Phase 3 test 4 (AC-2, AC-11): the cut plan hands `probe` the state of asset `a/b` as it is.
#[tokio::test]
async fn cut_boundary_hands_on_the_predecessor_state() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env().await?;
    let seen = evaluate(&envref, "a/b/probe").await?;
    let predecessor = envref.evaluate(parse_query("a/b")?).await?;
    predecessor.get().await?;
    let Metadata::MetadataRecord(record) = predecessor.get_metadata().await? else {
        panic!("structured metadata expected");
    };
    assert_eq!(seen["query"], record.query.encode());
    assert_eq!(seen["query"], "a/b");
    assert_eq!(seen["key"], Json::Null);
    assert_eq!(seen["filename"], Json::Null);
    assert_eq!(seen["data_format"], Json::Null);
    assert_eq!(seen["status"], format!("{:?}", record.status));
    assert_eq!(seen["is_applied"], false);
    Ok(())
}

/// Phase 3 test 5 (AC-3): a fetched asset arrives with its own format, filename and key.
#[tokio::test]
async fn get_asset_hands_on_the_fetched_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env().await?;
    let seen = evaluate(&envref, "-R/data/x.txt/-/probe").await?;
    assert_eq!(seen["data_format"], "txt", "{seen}");
    assert_eq!(seen["filename"], "x.txt", "{seen}");
    assert_eq!(seen["key"], "data/x.txt", "{seen}");
    Ok(())
}

/// Phase 3 test 6 (AC-3): the bytes of a fetched asset keep the format they are written in.
#[tokio::test]
async fn get_asset_binary_keeps_the_fetched_format() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env().await?;
    let seen = evaluate(&envref, "-R-bin/data/x.txt/-/probe").await?;
    assert_eq!(seen["data_format"], "txt", "{seen}");
    assert_eq!(seen["key"], "data/x.txt", "{seen}");
    Ok(())
}

/// Phase 3 test 7 (AC-4): a resource read from the store arrives with its stored metadata.
#[tokio::test]
async fn get_resource_hands_on_the_stored_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env().await?;
    let seen = apply_steps(
        &envref,
        vec![Step::GetResource(parse_key("data/x.txt")?)],
        State::new(),
    )
    .await?;
    assert_eq!(seen["data_format"], "txt", "{seen}");
    assert_eq!(seen["key"], "data/x.txt", "{seen}");
    Ok(())
}

/// Phase 3 test 8 (AC-5, AC-10): the output's filename labels the asset, never the input.
#[tokio::test]
async fn output_filename_does_not_label_the_input() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env().await?;
    let asset = envref
        .evaluate(parse_query("-R/data/x.txt/-/probe/out.csv")?)
        .await?;
    let seen = report(&asset.get().await?)?;
    assert_eq!(seen["data_format"], "txt", "{seen}");
    let Metadata::MetadataRecord(record) = asset.get_metadata().await? else {
        panic!("structured metadata expected");
    };
    assert_eq!(record.filename.as_deref(), Some("out.csv"));
    assert_eq!(record.declared_data_format(), Some("csv"));
    Ok(())
}

/// Phase 3 test 9 (AC-6, AC-11): an expanded plan describes each prefix as the cut plan does.
#[tokio::test]
async fn expanded_plan_approximates_the_cut_plan() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env().await?;
    let cut = evaluate(&envref, "a/b/probe").await?;
    let applied = envref
        .get_asset_manager()
        .apply(
            Recipe::new("a/b/probe".to_owned(), String::new(), String::new())?,
            State::new().with_data(Value::from("input")),
            None,
        )
        .await?;
    let expanded = report(&applied.get().await?)?;
    for field in ["query", "key", "filename", "data_format"] {
        assert_eq!(expanded[field], cut[field], "{field}: {expanded} vs {cut}");
    }
    let Metadata::MetadataRecord(record) = applied.get_metadata().await? else {
        panic!("structured metadata expected");
    };
    assert!(
        record
            .log
            .iter()
            .any(|entry| entry.kind == LogEntryKind::Info && entry.message == "b ran"),
        "the asset's log holds what b wrote: {:?}",
        record.log
    );
    Ok(())
}

/// Phase 3 test 10 (AC-7): a prefix that only reads a key is not cut into a boundary, so the
/// command gets the keyed asset's state, key included.
#[tokio::test]
async fn bare_key_read_is_not_cut() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env().await?;
    let mut plan = make_plan(envref.clone(), "-R/data/x.txt/-/probe").await?;
    let context = Context::new(AssetRef::new_temporary(envref.clone()), false).await;
    finalize_plan(envref.clone(), &mut plan, &context, &State::new()).await?;
    assert!(
        matches!(plan.steps.first(), Some(Step::GetAsset(_))),
        "{:?}",
        plan.steps
    );
    assert_eq!(evaluate(&envref, "-R/data/x.txt/-/probe").await?["key"], "data/x.txt");
    Ok(())
}

/// Phase 3 test 11 (AC-8): a query with no filename declares no format, so the value's own
/// default applies; a filename still declares one.
#[tokio::test]
async fn unnamed_query_declares_no_data_format() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env().await?;
    let asset = envref.evaluate(parse_query("a/b")?).await?;
    let state = asset.get().await?;
    assert_eq!(state.metadata.declared_data_format(), None);
    assert_eq!(state.as_bytes()?, b"AB".to_vec());

    let named = envref.evaluate(parse_query("a/b/out.csv")?).await?;
    named.get().await?;
    let Metadata::MetadataRecord(record) = named.get_metadata().await? else {
        panic!("structured metadata expected");
    };
    assert_eq!(record.declared_data_format(), Some("csv"));
    Ok(())
}

/// Phase 3 test 12 (AC-10): the asset's own record is built as before — the final query, its
/// filename and format, and the log of the commands it ran.
#[tokio::test]
async fn asset_record_is_unchanged_by_step_states() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env().await?;
    let asset = envref.evaluate(parse_query("a/b/out.txt")?).await?;
    assert_eq!(asset.get().await?.try_into_string()?, "AB");
    let Metadata::MetadataRecord(record) = asset.get_metadata().await? else {
        panic!("structured metadata expected");
    };
    assert_eq!(record.query.encode(), "a/b/out.txt");
    assert_eq!(record.filename.as_deref(), Some("out.txt"));
    assert_eq!(record.declared_data_format(), Some("txt"));
    assert_eq!(record.key, None);
    assert!(!record.is_applied);
    Ok(())
}

/// Phase 3 test 13 (AC-4): legacy stored metadata is not handed on, and is not an error; the
/// state still names the key it was read from, whose filename seeds the format.
#[tokio::test]
async fn legacy_stored_metadata_degrades_to_a_warning() -> Result<(), Box<dyn std::error::Error>>
{
    let envref = env().await?;
    let seen = apply_steps(
        &envref,
        vec![Step::GetResource(parse_key("data/legacy.txt")?)],
        State::new(),
    )
    .await?;
    assert_eq!(seen["key"], "data/legacy.txt", "{seen}");
    assert_eq!(seen["filename"], "legacy.txt", "{seen}");
    assert_eq!(seen["data_format"], "txt", "{seen}");
    assert_eq!(seen["query"], "", "nothing of the legacy record is handed on: {seen}");
    Ok(())
}

/// Phase 3 test 14 (AC-11): a recipe override patches the last action, whose query then no
/// longer describes what runs; earlier actions keep theirs.
#[tokio::test]
async fn recipe_with_overrides_records_no_prefix_query() -> Result<(), Box<dyn std::error::Error>>
{
    let envref = env().await?;
    let recipe = Recipe::new("a/suffix-x".to_owned(), String::new(), String::new())?
        .with_argument("text".to_owned(), json!("y"));
    let plan = recipe.to_plan(envref.get_command_metadata_registry())?;
    let queries: Vec<Option<String>> = plan
        .steps
        .iter()
        .filter_map(|step| {
            if let Step::Action { query, .. } = step {
                Some(query.as_ref().map(|q| q.encode()))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(queries, vec![Some("a".to_owned()), None]);
    Ok(())
}

/// Phase 3 test 25 (AC-12): an applied plan says so — on the asset, its `AssetInfo` and every
/// state its plan hands on; an evaluated query does not.
#[tokio::test]
async fn applied_plan_is_marked_applied() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env().await?;
    let applied = envref
        .get_asset_manager()
        .apply(
            Recipe::new("a/b/probe".to_owned(), String::new(), String::new())?,
            State::new().with_data(Value::from("input")),
            None,
        )
        .await?;
    assert_eq!(report(&applied.get().await?)?["is_applied"], true);
    let metadata = applied.get_metadata().await?;
    assert!(metadata.is_applied());
    let Metadata::MetadataRecord(record) = metadata else {
        panic!("structured metadata expected");
    };
    assert!(record.get_asset_info().is_applied);

    let evaluated = envref.evaluate(parse_query("a/b/probe")?).await?;
    assert_eq!(report(&evaluated.get().await?)?["is_applied"], false);
    assert!(!evaluated.get_metadata().await?.is_applied());
    Ok(())
}
