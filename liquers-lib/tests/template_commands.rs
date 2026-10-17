//! `ns-template` end to end through `EnvRef::evaluate`: the context pack of Phase 3 Example 1, the
//! template command of Example 4, and how `ContextHost` routes each tag
//! (`specs/design/liquers-mustache/phase3-examples.md`).
#![cfg(feature = "templates")]

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use liquers_core::assets::AssetManager;
use liquers_core::command_metadata::{ArgumentInfo, CommandKey};
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::metadata::{DependencyKey, LogEntryKind, Metadata, MetadataRecord, Status};
use liquers_core::parse::{parse_key, parse_query};
use liquers_core::query::Key;
use liquers_core::state::State;
use liquers_core::store::{AsyncMemoryStore, AsyncStore};
use liquers_core::value::ValueInterface;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::register_template_commands;
use liquers_lib::value::Value;

// The `register_*_commands!` macros expand to code naming these unqualified.
#[allow(unused_imports)]
use liquers_core::{
    command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue},
    context::Context,
};
#[allow(unused_imports)]
use liquers_lib::value::simple::SimpleValue;

type CommandEnvironment = DefaultEnvironment<Value>;
type Res = Result<(), Box<dyn std::error::Error>>;

// ------------------------------------------------------------------------------------------------
// Fixture
// ------------------------------------------------------------------------------------------------

fn counts() -> &'static Mutex<HashMap<String, usize>> {
    static COUNTS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    COUNTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn count_of(tag: &str) -> usize {
    counts().lock().unwrap_or_else(|p| p.into_inner()).get(tag).copied().unwrap_or(0)
}

/// Counts its runs per tag.
fn count(tag: String) -> Result<Value, Error> {
    let mut counts = counts().lock().unwrap_or_else(|p| p.into_inner());
    let n = counts.entry(tag).or_insert(0);
    *n += 1;
    Ok(Value::from(format!("run {n}")))
}

fn json(value: serde_json::Value) -> Result<Value, Error> {
    Value::try_from_json_value(&value)
}

/// `{"area": area}`: the context of a pack.
fn ctx(area: String) -> Result<Value, Error> {
    json(serde_json::json!({ "area": area }))
}

/// A state with fields of its own, for `data` to shadow.
fn pair() -> Result<Value, Error> {
    json(serde_json::json!({ "area": "core/assets", "title": "T", "x": "<b>" }))
}

fn tags() -> Result<Value, Error> {
    json(serde_json::json!({ "tags": ["p", "q"] }))
}

fn fail() -> Result<Value, Error> {
    Err(Error::general_error("fixture failure".to_string()))
}

fn upper(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(state.try_into_string()?.to_uppercase()))
}

fn join_all(items: Vec<String>) -> Result<Value, Error> {
    Ok(Value::from(items.join(",")))
}

fn rnd() -> Result<Value, Error> {
    Ok(Value::from("random"))
}

#[cfg(feature = "polars")]
fn frame() -> Result<Value, Error> {
    use liquers_lib::value::ExtValueInterface;
    use polars::prelude::*;
    let df = df!("name" => ["a", "b"], "n" => [1i64, 2])
        .map_err(|e| Error::general_error(e.to_string()))?;
    Ok(Value::from_polars_dataframe(df))
}

#[cfg(feature = "polars")]
fn frame_height(state: &State<Value>) -> Result<Value, Error> {
    use liquers_lib::value::ExtValueInterface;
    Ok(Value::from(format!("rows={}", state.data_unchecked().as_polars_dataframe()?.height())))
}

const A_EXPIRY: &str = "---
id: A-EXPIRY
title: Expired assets are served stale
status: draft
priority: P1
area: [core/assets]
---
# Expired assets are served stale
";

const B_CLOSED: &str = "---
id: B-CLOSED
title: An old problem
status: closed
area: [core/store]
---
# An old problem
";

const C_STORE: &str = "---
id: C-STORE
title: Store listing
status: draft
area: [core/store]
---
# Store listing
";

const D_TOML: &str = "+++
id = \"D-TOML\"
status = \"accepted\"
+++
# TOML front-matter
";

const OPEN_ISSUES: &str = "# Open issues in {{area}}
{{#$ -R-key/specs/issues/-/ns-search/field_extract/search-area~ncolon~{{area}}~.~_status~ncolon~closed~nast~-f-f-10}}
- [{{title}}]({{key}}){{#priority}} — {{priority}}{{/priority}}
{{/}}
{{^$ -R-key/specs/issues/-/ns-search/field_extract/search-area~ncolon~{{area}}~.~_status~ncolon~closed~nast~-f-f-10}}
No open issues.
{{/}}
";

const PACK: &str = "# Open issues in core/store\n- [Store listing](specs/issues/C-STORE.md)\n";

fn text_metadata(filename: &str) -> MetadataRecord {
    let mut metadata = MetadataRecord {
        type_identifier: "Text".to_string(),
        type_name: "text".to_string(),
        data_format: Some("txt".to_string()),
        ..Default::default()
    };
    metadata.filename = Some(filename.to_string());
    metadata.status = Status::Source;
    metadata
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

async fn fixture() -> Result<EnvRef<CommandEnvironment>, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    put(&store, "specs/issues/A-EXPIRY.md", A_EXPIRY.as_bytes()).await?;
    put(&store, "specs/issues/B-CLOSED.md", B_CLOSED.as_bytes()).await?;
    put(&store, "specs/issues/C-STORE.md", C_STORE.as_bytes()).await?;
    put(&store, "specs/issues/D-TOML.md", D_TOML.as_bytes()).await?;
    put(&store, "specs/issues/notes.txt", b"plain notes").await?;
    put(&store, "data/word.txt", b"abc").await?;

    let mut env = CommandEnvironment::new();
    {
        let cr = env.get_mut_command_registry();
        liquers_lib::register_records_commands!(cr)?;
        liquers_lib::register_search_commands!(cr)?;
        register_template_commands!(cr)?;
        register_command!(cr, fn count(tag: String) -> result namespace: "fixture")?;
        register_command!(cr, fn ctx(area: String) -> result namespace: "fixture")?;
        register_command!(cr, fn pair() -> result namespace: "fixture")?;
        register_command!(cr, fn tags() -> result namespace: "fixture")?;
        register_command!(cr, fn fail() -> result namespace: "fixture")?;
        register_command!(cr, fn upper(state) -> result namespace: "fixture")?;
        register_command!(cr, fn join_all(items: Vec<String> multiple) -> result namespace: "fixture")?;
        register_command!(cr, fn rnd() -> result namespace: "fixture" volatile: true)?;
        #[cfg(feature = "polars")]
        register_command!(cr, fn frame() -> result namespace: "fixture")?;
        #[cfg(feature = "polars")]
        register_command!(cr, fn frame_height(state) -> result namespace: "fixture")?;
        cr.register_alias(
            CommandKey::new("", "pack", "open_issues"),
            CommandKey::new("", "template", "render"),
            vec![CommandParameterValue::Query(parse_query("-R/templates/open_issues.md")?)],
            vec![
                ArgumentInfo::any_argument("data").with_default(""),
                ArgumentInfo::string_argument("errors").with_default("strict"),
                ArgumentInfo::any_argument("cwd").with_default_link(parse_query("-R-key/.")?),
            ],
        )?
        .with_label("Open issues");
    }
    env.with_async_store(Box::new(store));
    let envref = env.to_ref();
    set(&envref, "templates/open_issues.md", OPEN_ISSUES).await?;
    Ok(envref)
}

/// Writes through the asset manager, so a rewrite is seen by the next evaluation.
async fn set(envref: &EnvRef<CommandEnvironment>, key: &str, text: &str) -> Result<(), Error> {
    let key = parse_key(key)?;
    let filename = key.filename().map(|n| n.encode()).unwrap_or_default();
    envref
        .get_asset_manager()
        .set_binary(&key, text.as_bytes(), text_metadata(&filename))
        .await
}

async fn eval(envref: &EnvRef<CommandEnvironment>, query: &str) -> Result<State<Value>, Error> {
    let state = envref.evaluate(query).await?.get().await?;
    state.error_result()?;
    Ok(state)
}

async fn text(envref: &EnvRef<CommandEnvironment>, query: &str) -> Result<String, Error> {
    eval(envref, query).await?.try_into_string()
}

async fn failure(envref: &EnvRef<CommandEnvironment>, query: &str) -> Result<Error, Box<dyn std::error::Error>> {
    match eval(envref, query).await {
        Ok(state) => Err(format!("{query} should fail, gave {:?}", state.try_into_string()).into()),
        Err(error) => Ok(error),
    }
}

fn log_messages(state: &State<Value>, kind: LogEntryKind) -> Vec<String> {
    match &*state.metadata {
        Metadata::MetadataRecord(record) => record
            .log
            .iter()
            .filter(|entry| entry.kind == kind)
            .map(|entry| entry.message.clone())
            .collect(),
        Metadata::LegacyMetadata(_) => vec![],
    }
}

async fn dependencies(envref: &EnvRef<CommandEnvironment>, query: &str) -> Result<Vec<DependencyKey>, Error> {
    let asset = envref.get_asset_manager().get_asset(&parse_query(query)?).await?;
    asset.get().await?.error_result()?;
    Ok(asset.get_metadata().await?.get_dependencies().iter().map(|d| d.key.clone()).collect())
}

fn has_key(deps: &[DependencyKey], key: &str) -> Result<bool, Error> {
    let key = DependencyKey::from(&parse_key(key)?);
    Ok(deps.contains(&key))
}

// ------------------------------------------------------------------------------------------------
// Example 1: the context pack
// ------------------------------------------------------------------------------------------------

#[cfg(feature = "records")]
#[tokio::test]
async fn context_pack_renders_open_issues() -> Res {
    let env = fixture().await?;
    let out = text(
        &env,
        "ns-fixture/ctx-core~/store/ns-template/render-~X~-R/templates/open_issues.md~E--strict",
    )
    .await?;
    assert_eq!(out, PACK);
    let none = text(
        &env,
        "ns-fixture/ctx-core~/nothing/ns-template/render-~X~-R/templates/open_issues.md~E",
    )
    .await?;
    assert_eq!(none, "# Open issues in core/nothing\nNo open issues.\n");
    Ok(())
}

#[cfg(feature = "records")]
#[tokio::test]
async fn data_argument_supplies_names() -> Res {
    let env = fixture().await?;
    let out = text(
        &env,
        "ns-template/render-~X~-R/templates/open_issues.md~E-~X~ns-fixture/ctx-core~/store~E-strict",
    )
    .await?;
    assert_eq!(out, PACK);
    Ok(())
}

#[tokio::test]
async fn data_shadows_state_fields() -> Res {
    let env = fixture().await?;
    set(&env, "templates/both.md", "{{area}} {{title}}").await?;
    let out = text(
        &env,
        "ns-fixture/pair/ns-template/render-~X~-R/templates/both.md~E-~X~ns-fixture/ctx-core~/store~E",
    )
    .await?;
    assert_eq!(out, "core/store T");
    Ok(())
}

#[tokio::test]
async fn data_must_be_a_map() -> Res {
    let env = fixture().await?;
    set(&env, "templates/hello.md", "Hello {{area}}").await?;
    let error = failure(&env, "ns-template/render-~X~-R/templates/hello.md~E-abc").await?;
    assert!(error.to_string().contains("data must be a map"), "{error}");
    let error = failure(&env, "ns-template/render-~X~-R/templates/hello.md~E-~X~ns-fixture/tags/ns-search/field_extract~E").await;
    // A record view is a list of rows, so it is no map either; when the search commands refuse
    // the input first, that is an error too.
    assert!(error.is_ok());
    Ok(())
}

#[tokio::test]
async fn expand_renders_state_as_template() -> Res {
    let env = fixture().await?;
    set(&env, "templates/hello.md", "Hello {{area}}").await?;
    let out = text(&env, "-R/templates/hello.md/-/ns-template/expand-~X~ns-fixture/ctx-core~/store~E").await?;
    assert_eq!(out, "Hello core/store");
    Ok(())
}

#[tokio::test]
async fn dot_is_state_in_render_and_data_in_expand() -> Res {
    let env = fixture().await?;
    set(&env, "templates/dot.md", "{{#.}}[{{area}}]{{/.}}").await?;
    let render = text(
        &env,
        "ns-fixture/pair/ns-template/render-~X~-R/templates/dot.md~E-~X~ns-fixture/ctx-core~/store~E",
    )
    .await?;
    assert_eq!(render, "[core/assets]", "{{{{#.}}}} pushes the state above data, so its names win inside");
    set(&env, "templates/self.md", "{{$. ns-fixture/upper}}").await?;
    let applied = text(&env, "-R/data/word.txt/-/ns-template/render-~X~-R/templates/self.md~E").await?;
    assert_eq!(applied, "ABC", "{{{{$. q}}}} applies q to the state in render");
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Example 4: a template command
// ------------------------------------------------------------------------------------------------

#[cfg(feature = "records")]
#[tokio::test]
async fn template_command_alias_renders() -> Res {
    let env = fixture().await?;
    assert_eq!(text(&env, "ns-fixture/ctx-core~/store/ns-pack/open_issues").await?, PACK);
    let deps = dependencies(&env, "ns-fixture/ctx-core~/store/ns-pack/open_issues").await?;
    assert!(has_key(&deps, "templates/open_issues.md")?, "{deps:?}");
    Ok(())
}

#[cfg(feature = "records")]
#[tokio::test]
async fn template_command_invalidates_on_template_edit() -> Res {
    let env = fixture().await?;
    let query = "ns-fixture/ctx-core~/store/ns-pack/open_issues";
    assert_eq!(text(&env, query).await?, PACK);
    set(&env, "templates/open_issues.md", &OPEN_ISSUES.replace("# Open issues in {{area}}", "## Open")).await?;
    let out = text(&env, query).await?;
    assert!(out.starts_with("## Open\n"), "{out:?}");
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Dependencies, media type, volatility
// ------------------------------------------------------------------------------------------------

#[tokio::test]
async fn query_dependency_recomputes() -> Res {
    let env = fixture().await?;
    set(&env, "data/count.txt", "one").await?;
    set(&env, "templates/count.md", "n={{$ -R/data/count.txt}}").await?;
    let query = "ns-template/render-~X~-R/templates/count.md~E";
    assert_eq!(text(&env, query).await?, "n=one");
    assert!(has_key(&dependencies(&env, query).await?, "data/count.txt")?);
    set(&env, "data/count.txt", "two").await?;
    assert_eq!(text(&env, query).await?, "n=two");
    Ok(())
}

#[tokio::test]
async fn linked_template_arrives_as_text() -> Res {
    let env = fixture().await?;
    set(&env, "templates/hello.md", "Hello {{area}}").await?;
    let out = text(&env, "ns-fixture/ctx-world/ns-template/render-~X~-R/templates/hello.md~E").await?;
    assert_eq!(out, "Hello world");
    Ok(())
}

#[tokio::test]
async fn media_type_follows_filename() -> Res {
    let env = fixture().await?;
    set(&env, "templates/x.md", "{{x}}").await?;
    let html = eval(&env, "ns-fixture/pair/ns-template/render-~X~-R/templates/x.md~E/page.html").await?;
    assert_eq!(html.try_into_string()?, "&lt;b&gt;");
    assert_eq!(html.metadata.get_media_type(), "text/html");
    let plain = text(&env, "ns-fixture/pair/ns-template/render-~X~-R/templates/x.md~E").await?;
    assert_eq!(plain, "<b>");
    Ok(())
}

#[tokio::test]
async fn volatile_query_needs_declared_volatility() -> Res {
    let env = fixture().await?;
    set(&env, "templates/rnd.md", "{{$ ns-fixture/rnd}}").await?;
    let manager = env.get_asset_manager();
    let asset = manager.get_asset(&parse_query("ns-template/render-~X~-R/templates/rnd.md~E")?).await?;
    assert_eq!(asset.get().await?.try_into_string()?, "random");
    assert!(!asset.is_volatile().await, "a volatile query does not make the render volatile");
    let asset = manager.get_asset(&parse_query("v/ns-template/render-~X~-R/templates/rnd.md~E")?).await?;
    assert_eq!(asset.get().await?.try_into_string()?, "random");
    assert!(asset.is_volatile().await, "a render query starting with v is volatile");
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Partials
// ------------------------------------------------------------------------------------------------

#[tokio::test]
async fn partial_from_store_is_dependency() -> Res {
    let env = fixture().await?;
    set(&env, "templates/header.md", "<{{area}}>").await?;
    set(&env, "templates/page.md", "A{{> templates/header.md}}B").await?;
    let query = "ns-fixture/ctx-x/ns-template/render-~X~-R/templates/page.md~E";
    assert_eq!(text(&env, query).await?, "A<x>B");
    assert!(has_key(&dependencies(&env, query).await?, "templates/header.md")?);
    Ok(())
}

#[tokio::test]
async fn dynamic_partial_is_dependency() -> Res {
    let env = fixture().await?;
    set(&env, "templates/header.md", "<{{area}}>").await?;
    set(&env, "templates/dyn.md", "A{{>$ -R/templates/header.md}}B").await?;
    let query = "ns-fixture/ctx-x/ns-template/render-~X~-R/templates/dyn.md~E";
    assert_eq!(text(&env, query).await?, "A<x>B");
    assert!(has_key(&dependencies(&env, query).await?, "templates/header.md")?);
    Ok(())
}

#[tokio::test]
async fn relative_partial_uses_cwd_link() -> Res {
    let env = fixture().await?;
    set(&env, "templates/header.md", "<h>").await?;
    set(&env, "templates/rel.md", "A{{> ./header.md}}B").await?;
    let out = text(&env, "-R-cwd/templates/-/ns-template/render-~X~-R/./rel.md~E").await?;
    assert_eq!(out, "A<h>B");
    let passed = text(&env, "-R/templates/rel.md/-/ns-template/expand--strict-templates").await?;
    assert_eq!(passed, "A<h>B");
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Routing through ContextHost
// ------------------------------------------------------------------------------------------------

#[tokio::test]
async fn apply_tag_routes_through_apply_ad_hoc() -> Res {
    let env = fixture().await?;
    set(&env, "templates/apply.md", "{{$. ns-fixture/upper}}|{{$$. ns-fixture/upper}}|{{>$$ -R/data/word.txt}}").await?;
    let out = text(&env, "-R/data/word.txt/-/ns-template/render-~X~-R/templates/apply.md~E").await?;
    assert_eq!(out, "ABC|ABC|abc");
    Ok(())
}

#[tokio::test]
async fn override_list_routes_through_apply() -> Res {
    let env = fixture().await?;
    set(&env, "templates/tags.md", "{{$ ns-fixture/join_all-{{tags}}}}").await?;
    let out = text(&env, "ns-fixture/tags/ns-template/render-~X~-R/templates/tags.md~E").await?;
    assert_eq!(out, "p,q");
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Error modes, arguments, validation
// ------------------------------------------------------------------------------------------------

#[tokio::test]
async fn error_modes_log_warnings() -> Res {
    let env = fixture().await?;
    set(&env, "templates/fail.md", "a{{$ ns-fixture/fail}}b").await?;
    let error = failure(&env, "ns-template/render-~X~-R/templates/fail.md~E--strict").await?;
    assert!(error.to_string().contains("fixture failure"), "{error}");
    let expanded = eval(&env, "ns-template/render-~X~-R/templates/fail.md~E--expand").await?;
    let out = expanded.try_into_string()?;
    assert!(out.starts_with("a[template error at 1:2: ") && out.ends_with("]b"), "{out:?}");
    // The other warning, if any, is the `cwd` link's own: `-R-key/.` with no working directory.
    let failures = |state: &State<Value>| {
        log_messages(state, LogEntryKind::Warning)
            .into_iter()
            .filter(|m| m.contains("fixture failure"))
            .count()
    };
    assert_eq!(failures(&expanded), 1);
    let silent = eval(&env, "ns-template/render-~X~-R/templates/fail.md~E--silent").await?;
    assert_eq!(silent.try_into_string()?, "ab");
    assert_eq!(failures(&silent), 1);
    Ok(())
}

#[tokio::test]
async fn unknown_error_mode_fails() -> Res {
    let env = fixture().await?;
    set(&env, "templates/hello.md", "Hello").await?;
    let error = failure(&env, "ns-template/render-~X~-R/templates/hello.md~E--warp").await?;
    assert!(error.to_string().contains("warp"), "{error}");
    Ok(())
}

#[tokio::test]
async fn cwd_not_a_key_fails() -> Res {
    let env = fixture().await?;
    set(&env, "templates/hello.md", "Hello").await?;
    failure(&env, "ns-template/render-~X~-R/templates/hello.md~E--strict-~X~ns-fixture/pair~E").await?;
    Ok(())
}

#[tokio::test]
async fn validate_command_runs_nothing() -> Res {
    let env = fixture().await?;
    set(&env, "templates/bad.md", "{{$ ns-fixture/count-validate}}{{a..b}}{{$ ns-nope/missing}}").await?;
    let state = eval(&env, "-R/templates/bad.md/-/ns-template/validate").await?;
    let json = state.data_unchecked().try_into_json_value()?;
    let messages: Vec<String> = json
        .as_array()
        .ok_or("a list of diagnostics")?
        .iter()
        .map(|d| d["message"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(messages.len(), 2, "{messages:?}");
    assert!(messages.iter().any(|m| m.contains("missing")), "{messages:?}");
    assert_eq!(count_of("validate"), 0, "validate runs no query");
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Any value as data (AC-15)
// ------------------------------------------------------------------------------------------------

#[cfg(feature = "polars")]
#[tokio::test]
async fn frame_rows_as_data() -> Res {
    let env = fixture().await?;
    set(&env, "templates/rows.md", "{{#.}}{{name}}={{n}};{{/.}}").await?;
    let out = text(&env, "ns-fixture/frame/ns-template/render-~X~-R/templates/rows.md~E").await?;
    assert_eq!(out, "a=1;b=2;");
    Ok(())
}

#[tokio::test]
async fn folder_as_data() -> Res {
    let env = fixture().await?;
    set(&env, "templates/folder.md", "[{{issues[\"notes.txt\"]}}]").await?;
    let query = "-R-dir/specs/-/ns-template/render-~X~-R/templates/folder.md~E";
    assert_eq!(text(&env, query).await?, "[plain notes]");
    assert!(has_key(&dependencies(&env, query).await?, "specs/issues/notes.txt")?);
    Ok(())
}

/// An apply tag passes an extended value on as itself, not as its JSON.
#[cfg(feature = "polars")]
#[tokio::test]
async fn apply_tag_keeps_extended_value() -> Res {
    let env = fixture().await?;
    set(&env, "templates/height.md", "{{$. ns-fixture/frame_height}}").await?;
    let out = text(&env, "ns-fixture/frame/ns-template/render-~X~-R/templates/height.md~E").await?;
    assert_eq!(out, "rows=2");
    Ok(())
}

/// `_meta` is the state's metadata record, for the rendered state and for a query result.
#[tokio::test]
async fn meta_reads_state_metadata() -> Res {
    let env = fixture().await?;
    set(&env, "templates/meta.md", "{{_meta.key}}|{{#$ -R/specs/issues/notes.txt}}{{.}} {{_meta.filename}}{{/}}").await?;
    let out = text(&env, "-R/data/word.txt/-/ns-template/render-~X~-R/templates/meta.md~E").await?;
    assert_eq!(out, "data/word.txt|plain notes notes.txt");
    Ok(())
}
