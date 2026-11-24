//! Node arguments (design `recipe-tree`, AC-17, AC-20, AC-21): the `register_command!` DSL, a bare
//! node name planned as a link into the working folder, and links given directly.

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

fn make_env(store: AsyncMemoryStore) -> Result<EnvRef<CommandEnvironment>, Error> {
    let mut env = CommandEnvironment::new();
    {
        let registry = &mut env.command_registry;
        register_command!(registry, fn attach(state, part: Value (node: local, ext: "glb")) -> result)?;
        register_command!(registry, fn locate(dir: Key (node: local, ext: "glb", form: key)) -> result)?;
        register_command!(registry,
            fn locate_value(dir: Value (label: "Directory", node: local, ext: "glb", form: key)) -> result
        )?;
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
