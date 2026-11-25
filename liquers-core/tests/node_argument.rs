//! Node arguments (design `recipe-tree`, AC-17, AC-20, AC-21): the `register_command!` DSL, a bare
//! node name planned as a link into the working folder, and links given directly. The recipe
//! form (`form: recipe`, design `node-argument-recipe-form`) passes the node's recipe.

use liquers_core::{
    command_metadata::{ArgumentGUIInfo, ArgumentType, CommandKey, NodeArgument, NodeLinkForm},
    context::{Context, EnvRef, Environment, ImmediateEnvironment},
    error::{Error, ErrorType},
    metadata::{Metadata, MetadataRecord, Status},
    parse::{parse_key, parse_query},
    query::Key,
    recipes::DefaultRecipeProvider,
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::{Value, ValueInterface},
};
use liquers_macro::register_command;

type CommandEnvironment = ImmediateEnvironment<Value>;

fn attach(state: &State<Value>, part: Value) -> Result<Value, Error> {
    Ok(Value::from(format!(
        "{}+{}",
        state.try_into_string()?,
        part.try_into_string()?
    )))
}

fn locate(dir: Key) -> Result<Value, Error> {
    Ok(Value::from(dir.encode()))
}

fn locate_value(dir: Value) -> Result<Value, Error> {
    Ok(Value::from(dir.try_into_string()?))
}

/// The title of the recipe it is given, or `none` when the node has no recipe.
pub(crate) fn describe(part: Value) -> Result<Value, Error> {
    match part {
        Value::Recipe(recipe) => Ok(Value::from(recipe.title.clone())),
        other if other.is_none() => Ok(Value::from("none")),
        other => Err(Error::general_error(format!(
            "not a recipe: {}",
            other.identifier()
        ))),
    }
}

pub(crate) fn make_part() -> Result<Value, Error> {
    Ok(Value::from("part"))
}

fn make_env(store: AsyncMemoryStore) -> Result<EnvRef<CommandEnvironment>, Error> {
    let mut env = CommandEnvironment::new();
    {
        let registry = &mut env.command_registry;
        register_command!(registry, fn attach(state, part: Value (node: local, ext: "glb")) -> result)?;
        register_command!(registry, fn locate(dir: Key (node: local, ext: "glb", form: key)) -> result)?;
        register_command!(registry,
            fn locate_value(dir: Value (label: "Directory", node: local, ext: "glb", form: key)) -> result
        )?;
        register_command!(registry,
            fn describe(part: Value (node: local, ext: "glb", form: recipe)) -> result
        )?;
        register_command!(registry, fn make_part() -> result)?;
    }
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    Ok(env.to_ref())
}

async fn seed(store: &AsyncMemoryStore, path: &str, text: &str) -> Result<(), Error> {
    let key = parse_key(path)?;
    let mut record = MetadataRecord::new();
    record
        .with_key(key.clone())
        .with_type_identifier("Text".to_owned())
        .with_status(Status::Source);
    // The `.glb` here is text: tell the store how to read it.
    record.data_format = Some("txt".to_owned());
    store
        .set(&key, text.as_bytes(), &Metadata::MetadataRecord(record))
        .await
}

async fn car() -> Result<EnvRef<CommandEnvironment>, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    seed(&store, "scenes/car/body.glb", "body").await?;
    seed(&store, "scenes/car/wheel.glb", "wheel").await?;
    seed(&store, "data/x.glb", "x").await?;
    make_env(store)
}

async fn text(envref: &EnvRef<CommandEnvironment>, query: &str) -> Result<String, Error> {
    let asset = envref.evaluate(parse_query(query)?).await?;
    asset.get().await?.try_into_string()
}

/// AC-17: the DSL declares the extension and the link form, and picks the node selector.
#[test]
fn register_node_argument_with_macro() -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    let envref = make_env(store)?;
    let cmr = envref.get_command_metadata_registry();
    let attach = cmr
        .get(CommandKey::new_name("attach"))
        .ok_or("attach not registered")?;
    assert_eq!(
        attach.arguments[0].argument_type,
        ArgumentType::Node(NodeArgument::new("glb"))
    );
    assert_eq!(attach.arguments[0].gui_info, ArgumentGUIInfo::NodeSelector);
    for name in ["locate", "locate_value"] {
        let command = cmr.get(CommandKey::new_name(name)).ok_or("not registered")?;
        assert_eq!(
            command.arguments[0].argument_type,
            ArgumentType::Node(NodeArgument::new("glb").with_form(NodeLinkForm::Key))
        );
    }
    Ok(())
}

/// AC-20: a bare node name means the same as the link it stands for.
#[tokio::test]
async fn node_name_resolves_to_link() -> Result<(), Box<dyn std::error::Error>> {
    let envref = car().await?;
    let by_name = text(&envref, "-R-cwd/scenes/car/-R/./body.glb/-/attach-wheel").await?;
    let by_link = text(
        &envref,
        "-R-cwd/scenes/car/-R/./body.glb/-/attach-~X~-R/./wheel.glb~E",
    )
    .await?;
    assert_eq!(by_name, "body+wheel");
    assert_eq!(by_link, by_name);
    assert_eq!(
        text(&envref, "-R-cwd/scenes/car/-/locate-wheel").await?,
        "scenes/car/wheel.glb"
    );
    Ok(())
}

/// AC-20: a bare name with no working directory fails, naming the argument and the node.
#[tokio::test]
async fn node_name_without_cwd_fails_naming_argument() -> Result<(), Box<dyn std::error::Error>> {
    let envref = car().await?;
    let error = text(&envref, "-R/scenes/car/body.glb/-/attach-wheel")
        .await
        .expect_err("a node name needs a working directory");
    assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "{error}");
    assert!(error.message.contains("'part'"), "{error}");
    assert!(error.message.contains("'wheel'"), "{error}");
    Ok(())
}

/// AC-20: a link needs no working directory.
#[tokio::test]
async fn node_link_needs_no_cwd() -> Result<(), Box<dyn std::error::Error>> {
    let envref = car().await?;
    let value = text(
        &envref,
        "-R/scenes/car/body.glb/-/attach-~X~-R/scenes/car/wheel.glb~E",
    )
    .await?;
    assert_eq!(value, "body+wheel");
    Ok(())
}

/// AC-21: outside a strict tree a Node argument accepts any link.
#[tokio::test]
async fn node_argument_accepts_any_link_outside_strict() -> Result<(), Box<dyn std::error::Error>>
{
    let envref = car().await?;
    let value = text(
        &envref,
        "-R-cwd/scenes/car/-R/./body.glb/-/attach-~X~-R/data/x.glb~E",
    )
    .await?;
    assert_eq!(value, "body+x");
    Ok(())
}

const GEAR: &str = "recipes:\n  - {query: make_part/gear.glb, title: Gear}\n";

/// node-argument-recipe-form AC-1, AC-8: a bare name passes the node's recipe, the same value as
/// the `-R-recipe/` link; a node with no recipe passes none.
#[tokio::test]
async fn recipe_form_name_gives_the_recipe() -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    seed(&store, "scenes/car/body.glb", "body").await?;
    store
        .set(
            &parse_key("scenes/car/recipes.yaml")?,
            GEAR.as_bytes(),
            &Metadata::new(),
        )
        .await?;
    let envref = make_env(store)?;
    let cmr = envref.get_command_metadata_registry();
    let describe = cmr
        .get(CommandKey::new_name("describe"))
        .ok_or("describe not registered")?;
    assert_eq!(
        describe.arguments[0].argument_type,
        ArgumentType::Node(NodeArgument::new("glb").with_form(NodeLinkForm::Recipe))
    );
    assert_eq!(describe.arguments[0].gui_info, ArgumentGUIInfo::NodeSelector);
    let by_name = text(&envref, "-R-cwd/scenes/car/-/describe-gear").await?;
    let by_link = text(&envref, "-R-cwd/scenes/car/-/describe-~X~-R-recipe/./gear.glb~E").await?;
    assert_eq!(by_name, "Gear");
    assert_eq!(by_link, by_name);
    assert_eq!(text(&envref, "-R-cwd/scenes/car/-/describe-body").await?, "none");
    Ok(())
}

mod expiry {
    //! node-argument-recipe-form AC-7, on both built-in managers.
    use liquers_core::{
        assets::AssetManager,
        context::{EnvRef, Environment},
        error::Error,
        metadata::{MetadataRecord, Status},
        parse::parse_key,
        value::Value,
    };

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const BEFORE: &str = "recipes:\n  - {query: make_part/gear.glb, title: Gear}\n  - {query: describe-gear/d.txt}\n";
    const AFTER: &str = "recipes:\n  - {query: make_part/gear.glb, title: Cog}\n  - {query: describe-gear/d.txt}\n";

    async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::time::timeout(std::time::Duration::from_secs(20), future)
            .await
            .expect("call did not finish within 20 s")
    }

    /// One builder per manager kind: `register_command!` names the `CommandEnvironment` type of
    /// its scope, so the environment cannot be a generic parameter.
    macro_rules! builder_for {
        ($module:ident, $kind:ty) => {
            mod $module {
                use super::super::{describe, make_part};
                use liquers_core::{
                    context::{Context, EnvRef, GenericEnvironment},
                    environment_builder::EnvironmentBuilder,
                    error::Error,
                    query::Key,
                    recipes::DefaultRecipeProvider,
                    store::AsyncMemoryStore,
                    value::Value,
                };
                use liquers_macro::register_command;
                use std::sync::Arc;

                pub type CommandEnvironment = GenericEnvironment<Value, (), $kind>;

                pub fn build() -> Result<EnvRef<CommandEnvironment>, Error> {
                    let mut builder = EnvironmentBuilder::<Value, (), $kind>::new()
                        .with_async_store(Arc::new(AsyncMemoryStore::new(&Key::new())))
                        .with_recipe_provider(Arc::new(DefaultRecipeProvider::new()));
                    let cr = &mut builder.command_registry;
                    register_command!(cr,
                        fn describe(part: Value (node: local, ext: "glb", form: recipe)) -> result
                    )?;
                    register_command!(cr, fn make_part() -> result)?;
                    builder.build()
                }
            }
        };
    }
    builder_for!(queued, liquers_core::environment_builder::Queued);
    builder_for!(inline, liquers_core::environment_builder::Inline);

    async fn write_recipes<E: Environment>(envref: &EnvRef<E>, yaml: &str) -> Result<(), Error> {
        let record = MetadataRecord {
            type_identifier: "Text".to_string(),
            type_name: "text".to_string(),
            data_format: Some("txt".to_string()),
            ..Default::default()
        };
        within(envref.get_asset_manager().set_binary(
            &parse_key("data/recipes.yaml")?,
            yaml.as_bytes(),
            record,
        ))
        .await
    }

    async fn d_text<E: Environment<Value = Value>>(envref: &EnvRef<E>) -> Result<String, Error> {
        let asset = within(envref.get_asset_manager().get(&parse_key("data/d.txt")?)).await?;
        within(asset.get()).await?.try_into_string()
    }

    async fn status<E: Environment>(envref: &EnvRef<E>) -> Result<Status, Error> {
        for _ in 0..400 {
            let info = within(
                envref
                    .get_asset_manager()
                    .get_asset_info(&parse_key("data/d.txt")?),
            )
            .await?;
            if info.status == Status::Expired {
                return Ok(info.status);
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        Ok(within(
            envref
                .get_asset_manager()
                .get_asset_info(&parse_key("data/d.txt")?),
        )
        .await?
        .status)
    }

    /// AC-7: editing the named node's recipe expires the result through `-R-recipe/`.
    async fn recipe_form_result_expires_when_the_recipe_changes<E: Environment<Value = Value>>(
        envref: EnvRef<E>,
    ) -> TestResult {
        write_recipes(&envref, BEFORE).await?;
        assert_eq!(d_text(&envref).await?, "Gear");
        write_recipes(&envref, AFTER).await?;
        assert_eq!(status(&envref).await?, Status::Expired);
        assert_eq!(d_text(&envref).await?, "Cog");
        Ok(())
    }

    #[tokio::test]
    async fn recipe_form_result_expires_when_the_recipe_changes_queued() -> TestResult {
        recipe_form_result_expires_when_the_recipe_changes(queued::build()?).await
    }

    #[tokio::test]
    async fn recipe_form_result_expires_when_the_recipe_changes_inline() -> TestResult {
        recipe_form_result_expires_when_the_recipe_changes(inline::build()?).await
    }
}
