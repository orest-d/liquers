//! Rendering from Rust with frames given directly: `render_top_level` and `render_in_context`
//! (`specs/design/template-render-in-env/`, AC-1 to AC-11; AC-12 is `template_commands.rs`).
#![cfg(feature = "templates")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use liquers_core::assets::AssetManager;
use liquers_core::context::{Context, EnvRef, Environment};
use liquers_core::data_node::AnyNode;
use liquers_core::error::Error;
use liquers_core::metadata::{LogEntryKind, Metadata, Status};
use liquers_core::parse::{parse_key, parse_query};
use liquers_core::query::Key;
use liquers_core::state::State;
use liquers_core::store::{AsyncMemoryStore, AsyncStore};
use liquers_core::value::ValueInterface;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::register_template_commands;
use liquers_lib::template::{
    render_top_level, ErrorMode, ExpandFormat, FrameData, FrameKind, RenderRequest,
    TemplateSource,
};
use liquers_lib::value::Value;

#[allow(unused_imports)]
use liquers_core::command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue};
#[allow(unused_imports)]
use liquers_lib::value::simple::SimpleValue;

type CommandEnvironment = DefaultEnvironment<Value>;
type Res = Result<(), Box<dyn std::error::Error>>;

fn number(n: i64) -> Result<Value, Error> {
    Ok(Value::from(n))
}

fn fail() -> Result<Value, Error> {
    Err(Error::general_error("fixture failure".to_string()))
}

static HANG_STARTED: AtomicBool = AtomicBool::new(false);

/// Waits until its asset is cancelled.
async fn hang(context: Context<CommandEnvironment>) -> Result<Value, Error> {
    HANG_STARTED.store(true, Ordering::SeqCst);
    while !context.is_cancelled() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(Error::cancelled("hang cancelled".to_string()))
}

async fn put(store: &AsyncMemoryStore, key: &str, data: &[u8]) -> Result<(), Error> {
    let key = parse_key(key)?;
    let mut metadata = Metadata::new();
    let filename = key
        .filename()
        .ok_or_else(|| Error::general_error(format!("key '{key}' has no filename")))?;
    metadata.set_filename(filename.encode().as_ref())?;
    metadata.set_status(Status::Source)?;
    store.set(&key, data, &metadata).await
}

const TRICKY: &str = "a-b/c~d e\n{{#items}}[{{.}}]{{/items}} žluťoučký kůň";

async fn fixture() -> Result<EnvRef<CommandEnvironment>, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    put(&store, "templates/greet.md", b"Hello, {{.}}!").await?;
    put(&store, "templates/header.md", b"Header").await?;
    put(&store, "header.md", b"Root header").await?;
    put(&store, "templates/tricky.md", TRICKY.as_bytes()).await?;
    let mut env = CommandEnvironment::new();
    {
        let cr = env.get_mut_command_registry();
        register_template_commands!(cr)?;
        register_command!(cr, fn number(n: i64) -> result)?;
        register_command!(cr, fn fail() -> result)?;
        register_command!(cr, async fn hang(context) -> result)?;
    }
    env.with_async_store(Box::new(store));
    Ok(env.to_ref())
}

fn request(template: &str) -> RenderRequest {
    RenderRequest {
        template: TemplateSource::Text(template.to_string()),
        frames: Vec::new(),
        errors: ErrorMode::Strict,
        cwd: Key::new(),
        html_escape: false,
    }
}

fn data(json: serde_json::Value) -> Result<FrameData, Error> {
    Ok(FrameData::State(
        State::new().with_data(Value::try_from_json_value(&json)?),
    ))
}

async fn text_of(envref: &EnvRef<CommandEnvironment>, query: &str) -> Result<String, Error> {
    envref
        .evaluate(&parse_query(query)?)
        .await?
        .get()
        .await?
        .try_into_string()
}

#[tokio::test]
async fn top01_expand_over_data() -> Res {
    let envref = fixture().await?;
    let mut req = request("{{name}} has {{$ number-42}}");
    req.frames = vec![(data(serde_json::json!({"name": "Ann"}))?, FrameKind::Data)];
    let render = render_top_level(&envref, req)?;
    let asset = render.asset().clone();
    let rendered = render.run().await?.ok_or("not run")?;
    assert_eq!(rendered.text, "Ann has 42");
    assert!(rendered.warnings.is_empty());
    assert_eq!(asset.status().await, Status::Ready);
    assert_eq!(asset.key().await, None);
    assert_eq!(asset.get().await?.try_into_string()?, "Ann has 42");
    Ok(())
}

#[tokio::test]
async fn top02_matches_the_command() -> Res {
    let envref = fixture().await?;
    // The command reads the stored text; the top level is given the same text directly.
    let by_command = text_of(&envref, "-R/templates/tricky.md/-/ns-template/expand").await?;
    let mut req = request(TRICKY);
    req.frames = vec![(data(serde_json::json!({"items": [1, 2]}))?, FrameKind::Data)];
    let with_items = render_top_level(&envref, req)?.run().await?.ok_or("not run")?;
    let plain = render_top_level(&envref, request(TRICKY))?
        .run()
        .await?
        .ok_or("not run")?;
    assert_eq!(plain.text, by_command);
    assert_eq!(with_items.text, "a-b/c~d e\n[1][2] žluťoučký kůň");
    // A template over a state, as `render` gives it.
    let command = text_of(&envref, "number-7/ns-template/render-~X~-R/templates/greet.md~E").await?;
    let mut req = request("Hello, {{.}}!");
    req.frames = vec![(FrameData::State(State::new().with_data(Value::from(7))), FrameKind::Data)];
    let top = render_top_level(&envref, req)?.run().await?.ok_or("not run")?;
    assert_eq!(top.text, command);
    Ok(())
}

#[tokio::test]
async fn top03_template_by_link_is_a_dependency() -> Res {
    let envref = fixture().await?;
    let mut req = request("");
    req.template = TemplateSource::Link(parse_query("-R/templates/greet.md")?);
    req.frames = vec![(FrameData::State(State::new().with_data(Value::from("Bob"))), FrameKind::Data)];
    let render = render_top_level(&envref, req)?;
    let asset = render.asset().clone();
    assert_eq!(render.run().await?.ok_or("not run")?.text, "Hello, Bob!");
    let metadata = asset.get_metadata().await?;
    let record = metadata.metadata_record().ok_or("no record")?;
    let deps: Vec<String> = record.dependencies.iter().map(|d| format!("{:?}", d.key)).collect();
    assert!(deps.iter().any(|d| d.contains("greet.md")), "{deps:?}");
    Ok(())
}

#[tokio::test]
async fn top04_recovered_errors_carry_positions() -> Res {
    let envref = fixture().await?;
    for errors in [ErrorMode::Silent, ErrorMode::Expand(ExpandFormat::Text)] {
        let mut req = request("ok\n{{$ fail}}");
        req.errors = errors;
        let render = render_top_level(&envref, req)?;
        let asset = render.asset().clone();
        let rendered = render.run().await?.ok_or("not run")?;
        assert!(rendered.text.starts_with("ok\n"), "{errors:?}: {}", rendered.text);
        assert_eq!(rendered.warnings.len(), 1, "{errors:?}");
        let warning = &rendered.warnings[0];
        assert_eq!(warning.kind, LogEntryKind::Warning);
        assert!(warning.message.contains("fixture failure"), "{}", warning.message);
        assert_eq!(warning.position.line, 2, "{errors:?}");
        let metadata = asset.get_metadata().await?;
        let log = &metadata.metadata_record().ok_or("no record")?.log;
        assert!(log.iter().any(|entry| entry == warning), "{log:?}");
    }
    Ok(())
}

#[tokio::test]
async fn top05_strict_fails_with_position() -> Res {
    let envref = fixture().await?;
    let error = render_top_level(&envref, request("ok\n{{$ fail}}"))?
        .run()
        .await
        .expect_err("strict fails");
    assert_eq!(error.position.line, 2, "{error}");
    Ok(())
}

#[tokio::test]
async fn top06_html_escape_is_explicit() -> Res {
    let envref = fixture().await?;
    for (escape, expected) in [(true, "&lt;b&gt;"), (false, "<b>")] {
        let mut req = request("{{name}}");
        req.frames = vec![(data(serde_json::json!({"name": "<b>"}))?, FrameKind::Data)];
        req.html_escape = escape;
        let rendered = render_top_level(&envref, req)?.run().await?.ok_or("not run")?;
        assert_eq!(rendered.text, expected);
    }
    Ok(())
}

#[tokio::test]
async fn top07_partials_follow_cwd() -> Res {
    let envref = fixture().await?;
    let mut req = request("{{> ./header.md}} body");
    req.cwd = parse_key("templates")?;
    let rendered = render_top_level(&envref, req)?.run().await?.ok_or("not run")?;
    assert_eq!(rendered.text, "Header body");
    let rendered = render_top_level(&envref, request("{{> ./header.md}} body"))?
        .run()
        .await?
        .ok_or("not run")?;
    assert_eq!(rendered.text, "Root header body");
    Ok(())
}

#[tokio::test]
async fn top08_parse_error_creates_no_asset() -> Res {
    let envref = fixture().await?;
    let manager = envref.get_asset_manager();
    let before = manager.closure_asset("probe")?.asset().id();
    assert!(render_top_level(&envref, request("{{#open}} never closed")).is_err());
    // The next asset takes the next id: the refused render created none.
    assert_eq!(manager.closure_asset("probe")?.asset().id(), before + 1);
    Ok(())
}

#[tokio::test]
async fn top09_two_frames() -> Res {
    let envref = fixture().await?;
    let mut req = request("{{a}}{{b}}");
    req.frames = vec![
        (data(serde_json::json!({"a": 1}))?, FrameKind::Data),
        (FrameData::Node(AnyNode::from_json(serde_json::json!({"b": 2}))), FrameKind::Names),
    ];
    let rendered = render_top_level(&envref, req)?.run().await?.ok_or("not run")?;
    assert_eq!(rendered.text, "12");
    Ok(())
}

#[tokio::test]
async fn top10_cancel_during_a_query_tag() -> Res {
    let envref = fixture().await?;
    let render = render_top_level(&envref, request("before {{$ hang}} after"))?;
    let asset = render.asset().clone();
    let run = tokio::spawn(render.run());
    tokio::time::timeout(Duration::from_secs(5), async {
        while !HANG_STARTED.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    asset.cancel().await?;
    let result = tokio::time::timeout(Duration::from_secs(5), run).await??;
    match result {
        Ok(None) => {}
        Ok(Some(rendered)) => panic!("rendered after a cancel: {}", rendered.text),
        Err(error) => assert!(error.is_cancelled(), "{error}"),
    }
    assert_eq!(asset.status().await, Status::Cancelled);
    Ok(())
}
