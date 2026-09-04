use liquers_core::{
    assets::{AssetManager, AssetRef},
    command_metadata::{ArgumentInfo, CommandKey},
    context::{Context, EnvRef, Environment, ImmediateEnvironment},
    error::{Error, ErrorType},
    metadata::{LogEntryKind, Metadata, MetadataRecord, Status},
    parse::{parse_key, parse_query},
    plan::{ParameterValue, Step},
    query::{Key, Query},
    recipes::{DefaultRecipeProvider, Recipe},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};
use liquers_macro::register_command;

type CommandEnvironment = ImmediateEnvironment<Value>;

const ROOT_WARNING: &str = "Relative key/query has no CWD; using logical root '/'.";

fn identity(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(state.try_into_string()?))
}

fn use_link(_state: &State<Value>, use_link: String) -> Result<Value, Error> {
    Ok(Value::from(use_link))
}

// The directory arrives as data through a `-R-key/.` default link rather than being read out of
// `Context`. It is explicit in the plan, overridable per call, and visible to the planner.
fn cwd(dir: Key) -> Result<Value, Error> {
    Ok(Value::from(dir.encode()))
}

fn append_cwd(state: &State<Value>, dir: Key) -> Result<Value, Error> {
    Ok(Value::from(format!(
        "{}|{}",
        state.try_into_string()?,
        dir.encode()
    )))
}

async fn pass(_state: State<Value>, use_link: String) -> Result<Value, Error> {
    Ok(Value::from(use_link))
}

async fn via_evaluate(
    _state: State<Value>,
    dir: Key,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    // A command builds an absolute query from the directory it was given; a relative one is
    // refused, because it could not identify the asset it names.
    let asset = context
        .evaluate(&Query::from(dir.join("hello.txt")))
        .await?;
    Ok(Value::from(format!(
        "via_evaluate:{}",
        asset.get().await?.try_into_string()?
    )))
}

async fn via_state(
    _state: State<Value>,
    dir: Key,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    let state = context
        .get_dependency_state(&Query::from(dir.join("hello.txt")))
        .await?;
    Ok(Value::from(format!(
        "via_state:{}",
        state.try_into_string()?
    )))
}

async fn via_apply(
    _state: State<Value>,
    dir: Key,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    let asset = context
        .apply(
            &parse_query(&format!("-R-stored/{}", dir.join("identity").encode()))?,
            State::new().with_data(Value::from("ignored")),
        )
        .await?;
    Ok(Value::from(format!(
        "via_apply:{}",
        asset.get().await?.try_into_string()?
    )))
}

fn make_env(store: AsyncMemoryStore) -> Result<EnvRef<CommandEnvironment>, Error> {
    let mut env = CommandEnvironment::new();
    {
        let registry = &mut env.command_registry;
        register_command!(registry, fn identity(state) -> result)?;
        register_command!(registry, fn use_link(state, use_link: String) -> result)?;
        register_command!(registry, fn cwd(dir: Key = query "-R-key/.") -> result)?;
        register_command!(registry, fn append_cwd(state, dir: Key = query "-R-key/.") -> result)?;
        register_command!(registry, async fn pass(state, use_link: String) -> result)?;
        register_command!(registry, async fn via_evaluate(state, dir: Key = query "-R-key/.", context) -> result)?;
        register_command!(registry, async fn via_state(state, dir: Key = query "-R-key/.", context) -> result)?;
        register_command!(registry, async fn via_apply(state, dir: Key = query "-R-key/.", context) -> result)?;
        let metadata = registry.register_command(
            CommandKey::new_name("collect_links"),
            |_state, arguments, _context| {
                let values = match arguments.get_parameter(0, "values")? {
                    ParameterValue::MultipleParameters(_, values) => values,
                    parameter => {
                        return Err(Error::general_error(format!(
                            "expected multiple values, got {parameter}"
                        )))
                    }
                };
                let text = values
                    .iter()
                    .map(|parameter| match parameter {
                        ParameterValue::ParameterValue(_, value, position) => {
                            value.as_str().map(str::to_owned).ok_or_else(|| {
                                Error::conversion_error(value.to_string(), "string".to_owned())
                                    .with_position(position)
                            })
                        }
                        parameter => Err(Error::general_error(format!(
                            "unexpected multiple value {parameter}"
                        ))),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::from(text.join(",")))
            },
        )?;
        metadata.with_argument(ArgumentInfo::any_argument("values").set_multiple());
    }
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    Ok(env.to_ref())
}

fn source_metadata(key: &Key) -> Metadata {
    let mut record = MetadataRecord::new();
    record
        .with_key(key.clone())
        .with_type_identifier("Text".to_owned())
        .with_status(Status::Source);
    Metadata::MetadataRecord(record)
}

async fn seed_source(store: &AsyncMemoryStore, path: &str, text: &str) -> Result<(), Error> {
    let key = parse_key(path)?;
    store
        .set(&key, text.as_bytes(), &source_metadata(&key))
        .await
}

async fn seed_yaml(store: &AsyncMemoryStore, path: &str, yaml: &[u8]) -> Result<(), Error> {
    store.set(&parse_key(path)?, yaml, &Metadata::new()).await
}

fn has_dependency(metadata: &Metadata, expected: &str) -> bool {
    let resource_query = format!("-R/{expected}");
    metadata.get_dependencies().iter().any(|dependency| {
        dependency.key.as_str() == expected || dependency.key.as_str() == resource_query
    })
}

fn dependency_keys(metadata: &Metadata) -> Vec<String> {
    metadata
        .get_dependencies()
        .iter()
        .map(|dependency| dependency.key.to_string())
        .collect()
}

fn exact_warning_count(metadata: &Metadata) -> usize {
    metadata
        .metadata_record()
        .map(|record| {
            record
                .log
                .iter()
                .filter(|entry| {
                    entry.kind == LogEntryKind::Warning && entry.message == ROOT_WARNING
                })
                .count()
        })
        .unwrap_or(0)
}

fn assert_recipe_prefix(
    recipe: &Recipe,
    envref: &EnvRef<CommandEnvironment>,
    expected_cwd: &str,
) -> Result<(), Error> {
    let plan = recipe.to_plan(envref.get_command_metadata_registry())?;
    assert!(matches!(
        plan.steps.first(),
        Some(Step::SetCwd(key)) if key.encode() == expected_cwd
    ));
    assert!(matches!(
        plan.steps.get(1),
        Some(Step::GetResource(key)) if key.encode() == "./input.txt"
    ));
    assert_eq!(
        plan.steps
            .iter()
            .filter(|step| matches!(step, Step::SetCwd(_)))
            .count(),
        1
    );
    assert_eq!(
        plan.init_steps
            .iter()
            .filter(|step| matches!(step, Step::Info(message)
                if message == &format!("Recipe set CWD to '{expected_cwd}'")))
            .count(),
        1
    );
    Ok(())
}

async fn apply_text(
    envref: &EnvRef<CommandEnvironment>,
    recipe: Recipe,
) -> Result<(AssetRef<CommandEnvironment>, String), Error> {
    let asset = envref
        .get_asset_manager()
        .apply(recipe, State::new(), None)
        .await?;
    let text = asset.get().await?.try_into_string()?;
    Ok((asset, text))
}

#[tokio::test]
async fn programmatic_and_provider_cwd_select_their_own_inputs(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    let metadata = Metadata::new();
    store
        .set(
            &parse_key("programmatic/input.txt")?,
            b"programmatic",
            &metadata,
        )
        .await?;
    store
        .set(&parse_key("provider/input.txt")?, b"provider", &metadata)
        .await?;
    seed_yaml(
        &store,
        "provider/recipes.yaml",
        br#"recipes:
  - query: "-R-stored/./input.txt/-/identity/result.txt"
    title: Provider input
    description: Reads relative to this recipes.yaml
"#,
    )
    .await?;
    let envref = make_env(store)?;

    let mut programmatic = Recipe::new(
        "-R-stored/./input.txt/-/identity/result.txt".to_owned(),
        "Programmatic input".to_owned(),
        "Reads relative to a programmatic CWD".to_owned(),
    )?;
    programmatic.cwd = Some("programmatic".to_owned());
    assert_recipe_prefix(&programmatic, &envref, "programmatic")?;

    let provider_key = parse_key("provider/result.txt")?;
    let provider_recipe = envref
        .get_recipe_provider()
        .recipe_opt(&provider_key, envref.clone())
        .await?
        .expect("provider recipe");
    assert_recipe_prefix(&provider_recipe, &envref, "provider")?;

    let (_, programmatic_text) = apply_text(&envref, programmatic).await?;
    assert_eq!(programmatic_text, "programmatic");
    let keyed = envref.evaluate("-R/provider/result.txt").await?;
    assert_eq!(keyed.get().await?.try_into_string()?, "provider");
    Ok(())
}

#[tokio::test]
async fn explicit_cwd_overrides_recipe_cwd_for_later_relative_link(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    seed_source(&store, "a/c/hello.txt", "a/c/hello").await?;
    seed_source(&store, "a/b/hello.txt", "a/b/wrong").await?;
    let envref = make_env(store)?;

    let mut recipe = Recipe::new(
        "-R-cwd/../c/-/use_link-~X~-R/./hello.txt~E/result.txt".to_owned(),
        "Explicit CWD".to_owned(),
        "Explicit CWD overrides recipe CWD".to_owned(),
    )?;
    recipe.cwd = Some("a/b".to_owned());
    let plan = recipe.to_plan(envref.get_command_metadata_registry())?;
    assert!(matches!(plan.steps.first(), Some(Step::SetCwd(key)) if key.encode() == "a/b"));
    assert!(matches!(plan.steps.get(1), Some(Step::SetCwd(key)) if key.encode() == "../c"));
    let Some(Step::Action { parameters, .. }) = plan.steps.get(2) else {
        panic!("expected action after ordered CWD steps");
    };
    assert!(matches!(
        parameters.0.first(),
        Some(ParameterValue::ParameterLink(_, query, _)) if query.encode() == "-R/./hello.txt"
    ));

    let (_, text) = apply_text(&envref, recipe).await?;
    assert_eq!(text, "a/c/hello");
    Ok(())
}

#[tokio::test]
async fn resolved_dependency_identity_reuses_cached_asset() -> Result<(), Box<dyn std::error::Error>>
{
    let store = AsyncMemoryStore::new(&Key::new());
    seed_source(&store, "a/c/hello.txt", "a/c/hello").await?;
    seed_source(&store, "a/b/hello.txt", "a/b/wrong").await?;
    let envref = make_env(store)?;
    let mut recipe = Recipe::new(
        "-R-cwd/../c/-/use_link-~X~-R/./hello.txt~E/result.txt".to_owned(),
        "Cached dependency".to_owned(),
        "Equivalent dependencies share identity".to_owned(),
    )?;
    recipe.cwd = Some("a/b".to_owned());

    for _ in 0..2 {
        let (asset, text) = apply_text(&envref, recipe.clone()).await?;
        assert_eq!(text, "a/c/hello");
        let metadata = asset.get_metadata().await?;
        assert!(
            has_dependency(&metadata, "a/c/hello.txt"),
            "recorded dependencies: {:?}",
            dependency_keys(&metadata)
        );
        assert!(!has_dependency(&metadata, "a/b/hello.txt"));
    }

    let absolute_1 = envref.evaluate("-R/a/c/hello.txt").await?;
    assert_eq!(absolute_1.get().await?.try_into_string()?, "a/c/hello");
    let absolute_2 = envref.evaluate("-R/a/c/hello.txt").await?;
    assert_eq!(absolute_2.get().await?.try_into_string()?, "a/c/hello");
    assert_eq!(absolute_1.id(), absolute_2.id());

    let sibling = envref.evaluate("-R/a/b/hello.txt").await?;
    assert_eq!(sibling.get().await?.try_into_string()?, "a/b/wrong");
    assert_ne!(absolute_1.id(), sibling.id());
    Ok(())
}

#[tokio::test]
async fn context_boundary_commands_use_active_cwd() -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    seed_source(&store, "a/c/hello.txt", "a/c/hello").await?;
    seed_source(&store, "a/c/identity", "a/c/hello").await?;
    seed_source(&store, "a/b/hello.txt", "a/b/wrong").await?;
    seed_yaml(
        &store,
        "a/c/recipes.yaml",
        br#"recipes:
  - query: via_evaluate/evaluate.txt
  - query: via_state/state.txt
  - query: via_apply/apply.txt
"#,
    )
    .await?;
    let envref = make_env(store)?;

    for (key, expected, dependency_expected) in [
        ("a/c/evaluate.txt", "via_evaluate:a/c/hello", true),
        ("a/c/state.txt", "via_state:a/c/hello", true),
        ("a/c/apply.txt", "via_apply:a/c/hello", false),
    ] {
        let first = envref.evaluate(format!("-R/{key}")).await?;
        assert_eq!(first.get().await?.try_into_string()?, expected);
        let second = envref.evaluate(format!("-R/{key}")).await?;
        assert_eq!(second.get().await?.try_into_string()?, expected);
        assert_eq!(first.id(), second.id());
        let metadata = first.get_metadata().await?;
        assert_eq!(
            has_dependency(&metadata, "a/c/hello.txt"),
            dependency_expected,
            "recorded dependencies: {:?}",
            dependency_keys(&metadata)
        );
        if !dependency_expected {
            assert!(!has_dependency(&metadata, "a/c/identity"));
        }
    }

    let expected_1 = envref.evaluate("-R/a/c/hello.txt").await?;
    expected_1.get().await?;
    let expected_2 = envref.evaluate("-R/a/c/hello.txt").await?;
    expected_2.get().await?;
    let sibling = envref.evaluate("-R/a/b/hello.txt").await?;
    sibling.get().await?;
    assert_eq!(expected_1.id(), expected_2.id());
    assert_ne!(expected_1.id(), sibling.id());
    Ok(())
}

#[tokio::test]
async fn recursive_links_and_multiple_parameters_use_active_cwd(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    seed_source(&store, "a/c/one.txt", "one").await?;
    seed_source(&store, "a/c/two.txt", "two").await?;
    let envref = make_env(store)?;

    let mut multiple = Recipe::new(
        "collect_links-~X~-R/./one.txt~E-~X~-R/./two.txt~E/result.txt".to_owned(),
        "Multiple links".to_owned(),
        "Resolve every variadic link".to_owned(),
    )?;
    multiple.cwd = Some("a/c".to_owned());
    let (multiple_asset, text) = apply_text(&envref, multiple).await?;
    assert_eq!(text, "one,two");
    let metadata = multiple_asset.get_metadata().await?;
    assert!(
        has_dependency(&metadata, "a/c/one.txt"),
        "recorded dependencies: {:?}",
        dependency_keys(&metadata)
    );
    assert!(
        has_dependency(&metadata, "a/c/two.txt"),
        "recorded dependencies: {:?}",
        dependency_keys(&metadata)
    );

    let mut scoped = Recipe::new(
        "pass-~X~-R-cwd/./child/-/cwd~E/append_cwd/result.txt".to_owned(),
        "Scoped link".to_owned(),
        "Linked CWD changes do not leak".to_owned(),
    )?;
    scoped.cwd = Some("a/c".to_owned());
    let (_, scoped_text) = apply_text(&envref, scoped).await?;
    assert_eq!(scoped_text, "a/c/child|a/c");
    Ok(())
}

#[tokio::test]
async fn nested_keyed_recipe_cwd_is_not_bypassed() -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    seed_yaml(
        &store,
        "parent/recipes.yaml",
        br#"recipes:
  - query: "use_link-~X~-R/./child/result.txt~E/outer.txt"
"#,
    )
    .await?;
    seed_yaml(
        &store,
        "parent/child/recipes.yaml",
        br#"recipes:
  - query: "use_link-~X~-R/./input.txt~E/result.txt"
    links:
      use_link: "-R/./override.txt"
"#,
    )
    .await?;
    seed_source(&store, "parent/child/input.txt", "child-input").await?;
    seed_source(&store, "parent/child/override.txt", "child-override").await?;
    seed_source(&store, "parent/input.txt", "parent-wrong-input").await?;
    seed_source(&store, "parent/override.txt", "parent-wrong-override").await?;
    let envref = make_env(store)?;

    let outer = envref.evaluate("-R/parent/outer.txt").await?;
    assert_eq!(outer.get().await?.try_into_string()?, "child-override");
    let metadata = outer.get_metadata().await?;
    assert!(
        has_dependency(&metadata, "parent/child/result.txt"),
        "recorded dependencies: {:?}",
        dependency_keys(&metadata)
    );
    assert!(!has_dependency(&metadata, "parent/input.txt"));
    assert!(!has_dependency(&metadata, "parent/override.txt"));
    Ok(())
}

#[tokio::test]
async fn root_fallback_is_single_warning_under_shared_context(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    seed_source(&store, "root.txt", "root").await?;
    seed_source(&store, "other.txt", "other").await?;
    let envref = make_env(store)?;

    let recipe = Recipe::new(
        "-R/./root.txt/-/use_link-~X~-R/./other.txt~E".to_owned(),
        "Root fallback".to_owned(),
        "Warn once for several relative operands".to_owned(),
    )?;
    let (asset, text) = apply_text(&envref, recipe).await?;
    assert_eq!(text, "other");
    assert_eq!(exact_warning_count(&asset.get_metadata().await?), 1);

    let missing = Recipe::new(
        "-R-stored/./missing.txt".to_owned(),
        "Missing relative resource".to_owned(),
        "Preserve the resolved store error".to_owned(),
    )?;
    let error = match envref
        .get_asset_manager()
        .apply(missing, State::new(), None)
        .await
    {
        Ok(asset) => asset.get().await.expect_err("missing resource must fail"),
        Err(error) => error,
    };
    assert_eq!(error.error_type, ErrorType::KeyNotFound);
    assert!(
        error.to_string().contains("missing.txt"),
        "resolved key missing from error: {error}"
    );
    Ok(())
}

async fn run_absolute_case(
    cwd: Option<&str>,
    query: &str,
    expected: &str,
    expected_warnings: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    seed_source(&store, "data", "outer-data").await?;
    seed_source(&store, "hello.txt", "root-hello").await?;
    seed_source(&store, "a/c/hello.txt", "cwd-hello").await?;
    let envref = make_env(store)?;
    let mut recipe = Recipe::new(
        query.to_owned(),
        "Absolute scope".to_owned(),
        "Absolute outer and independent nested link".to_owned(),
    )?;
    recipe.cwd = cwd.map(str::to_owned);
    let (asset, text) = apply_text(&envref, recipe).await?;
    assert_eq!(text, expected);
    assert_eq!(
        exact_warning_count(&asset.get_metadata().await?),
        expected_warnings
    );
    Ok(())
}

#[tokio::test]
async fn absolute_outer_query_keeps_relative_link_independent(
) -> Result<(), Box<dyn std::error::Error>> {
    run_absolute_case(
        Some("a/c"),
        "/-R/./data/-/use_link-~X~-R/./hello.txt~E",
        "cwd-hello",
        0,
    )
    .await?;
    run_absolute_case(
        None,
        "/-R/./data/-/use_link-~X~-R/./hello.txt~E",
        "root-hello",
        1,
    )
    .await?;
    run_absolute_case(
        None,
        "/-R/./data/-/use_link-~X~/-R/./hello.txt~E",
        "root-hello",
        0,
    )
    .await?;
    Ok(())
}
