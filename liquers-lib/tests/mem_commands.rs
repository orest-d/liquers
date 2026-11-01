//! `ns-mem` end to end: every command through query evaluation, as the MCP tools call them
//! (`specs/design/agent-memory-mvp/phase3-examples.md`, "liquers-lib/tests/mem_commands.rs").
#![cfg(feature = "mem")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use liquers_core::assets::AssetManager;
use liquers_core::context::EnvRef;
use liquers_core::environment_config::EnvironmentConfig;
use liquers_core::error::{Error, ErrorType};
use liquers_core::metadata::{LogEntryKind, Metadata, Status, Version};
use liquers_core::parse::parse_key;
use liquers_core::recipes::Recipe;
use liquers_core::state::State;
use liquers_core::store_factory::default_store_factory;
use liquers_core::value::ValueInterface;
use liquers_macro::register_command;

use liquers_lib::environment::{default_environment_builder, DefaultEnvironment};
use liquers_lib::records::{FieldValue, RecordView};
use liquers_lib::value::{ExtValueInterface, Value};
use liquers_lib::{register_mem_commands, register_records_commands};

#[allow(unused_imports)]
use liquers_core::{
    command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue},
    context::Context,
};
#[allow(unused_imports)]
use liquers_lib::value::simple::SimpleValue;

type CommandEnvironment = DefaultEnvironment<Value>;
type Env = EnvRef<CommandEnvironment>;
type Res = Result<(), Box<dyn std::error::Error>>;

// ------------------------------------------------------------------------------------------------
// Fixture: a memory in a temporary folder, mounted as the "liquers-specs" profile mounts one
// ------------------------------------------------------------------------------------------------

const SCHEMA: &str = r#"
version: 1
instructions: Search before you write.
tiers:
  l0: [meta:title, field:title, heading]
  l1: [meta:description, field:description, paragraph]
collections:
  - name: issues
    folder: issues
    description: What is wrong or missing
    sections: [Problem]
    history: { section: History, stamp: reviewed }
    checks: [reviewed, history, dead_links]
    template: _templates/issue.md
    fields:
      id: { required: true, equals: filename_stem }
      title: { required: true }
      description: { required: true }
      status: { required: true, default: draft, enum_from: _schema/issue-status.yaml, transitions: _schema/transitions.yaml }
      area: { list: true, enum: [core, lib, mcp] }
      reviewed: { type: date }
  - name: notes
    folder: notes
    id: { from: title, case: kebab }
  - name: archive
    folder: archive
    writable: false
"#;

const STATUS: &str = "[draft, in_progress, closed]\n";
const TRANSITIONS: &str = "draft: [in_progress, closed]\nin_progress: [closed, draft]\nclosed: []\n";

const README: &str = "---\ntitle: Test memory\ndescription: Issues and notes for the tests\n---\n# Test memory\n";

const A_EXPIRY: &str = "---\nid: A-EXPIRY\ntitle: Expired assets are served stale\ndescription: An expired asset is returned without recomputation\nstatus: draft\narea: [core]\nreviewed: 2026-01-01\n---\n# Expired assets are served stale\n\n## Problem\n\nAn expired asset is served stale; expiration is ignored, expiration never checked.\n\n## History\n\n| Date | Change |\n|---|---|\n| 2026-01-01 | Filed |\n";
const B_CLOSED: &str = "---\nid: B-CLOSED\ntitle: Expiration of closed things\ndescription: Expiration handled for closed assets\nstatus: closed\narea: [core]\nreviewed: 2026-01-01\n---\n# Expiration of closed things\n\n## Problem\n\nExpiration expiration expiration.\n\n## History\n\n| 2026-01-01 | Filed |\n";
const C_STORE: &str = "---\nid: C-STORE\ntitle: Store writes have no precondition\ndescription: Two writers can overwrite each other\nstatus: draft\narea: [lib]\nreviewed: 2026-01-01\n---\n# Store writes have no precondition\n\n## Problem\n\nNo compare-and-swap; expiration unrelated.\n\n## History\n\n| 2026-01-01 | Filed |\n";
const PLAIN: &str = "# A plain note\n\nThe first paragraph of a note without front-matter.\n";

fn counts() -> &'static Mutex<usize> {
    static COUNT: OnceLock<Mutex<usize>> = OnceLock::new();
    COUNT.get_or_init(|| Mutex::new(0))
}

/// The command behind the recipe-only key `notes/derived.md`.
fn count_derived() -> Result<Value, Error> {
    *counts().lock().unwrap_or_else(|p| p.into_inner()) += 1;
    Ok(Value::from("derived"))
}

struct Fixture {
    env: Env,
    dir: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn write_file(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixture folder");
    }
    std::fs::write(path, text).expect("write fixture file");
}

fn temp_dir(name: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "liquers-mem-{}-{}-{}",
        name,
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn files(dir: &Path, extra: &[(&str, &str)]) {
    let mem = dir.join("mem");
    write_file(&mem.join("_schema/memory.yaml"), SCHEMA);
    write_file(&mem.join("_schema/issue-status.yaml"), STATUS);
    write_file(&mem.join("_schema/transitions.yaml"), TRANSITIONS);
    write_file(&mem.join("README.md"), README);
    write_file(&mem.join("issues/A-EXPIRY.md"), A_EXPIRY);
    write_file(&mem.join("issues/B-CLOSED.md"), B_CLOSED);
    write_file(&mem.join("issues/C-STORE.md"), C_STORE);
    write_file(&mem.join("notes/plain.md"), PLAIN);
    write_file(&mem.join("notes/README.md"), "# Notes\n\nShort notes kept by agents.\n");
    write_file(&mem.join("notes/recipes.yaml"), "recipes:\n  - query: \"ns-fixture/count_derived/derived.md\"\n");
    write_file(&mem.join("_templates/issue.md"), "---\ntitle: template\n---\n# Title\n\n## Problem\n\n## History\n\n| Date | Change |\n|---|---|\n");
    write_file(&dir.join("computed/recipes.yaml"), "recipes:\n  - query: \"ns-fixture/count_derived/derived.md\"\n");
    write_file(&dir.join("archive/archive/OLD.md"), "---\ntitle: Old\ndescription: An archived note\n---\n# Old\n");
    for (path, text) in extra {
        write_file(&mem.join(path), text);
    }
}

fn config(dir: &Path) -> String {
    format!(
        r#"
store:
  stores:
    - type: filesystem
      prefix: archive
      read_only: true
      config: {{ path: "{archive}" }}
    - type: filesystem
      prefix: computed
      config: {{ path: "{dir}" }}
    - type: filesystem
      prefix: ""
      config: {{ path: "{mem}" }}
      metadata:
        storage: none
        on_read: "media_type/file_facts/content_hash-on_change/front_matter/markdown_tiers-Problem"
        on_write: "defaults/media_type-fill/file_facts/front_matter/front_matter_sync"
"#,
        archive = dir.join("archive").display(),
        dir = dir.display(),
        mem = dir.join("mem").display()
    )
}

async fn fixture_with(name: &str, extra: &[(&str, &str)]) -> Result<Fixture, Error> {
    let dir = temp_dir(name);
    files(&dir, extra);
    let cfg = EnvironmentConfig::from_yaml(&config(&dir))?;
    let mut builder = default_environment_builder::<Value, ()>();
    {
        let cr = &mut builder.command_registry;
        register_records_commands!(cr)?;
        register_mem_commands!(cr)?;
        register_command!(cr, fn count_derived() -> result namespace: "fixture")?;
    }
    let env = builder
        .with_config(cfg, Box::new(default_store_factory()))
        .build()?;
    Ok(Fixture { env, dir })
}

async fn fixture(name: &str) -> Result<Fixture, Error> {
    fixture_with(name, &[]).await
}

/// Evaluates `query` with `args` as overrides of its last action, as a tool call does.
async fn call(env: &Env, query: &str, args: serde_json::Value) -> Result<State<Value>, Error> {
    let mut recipe = Recipe::new(query.to_string(), String::new(), String::new())?;
    if let serde_json::Value::Object(map) = args {
        for (name, value) in map {
            recipe = recipe.with_argument(name, value);
        }
    }
    let asset = env.get_asset_manager().apply(recipe, State::new(), None).await?;
    asset.get().await
}

fn view(state: &State<Value>) -> Result<Arc<dyn RecordView>, Error> {
    state.data_unchecked().as_record_view()
}

fn text_of(state: &State<Value>) -> Result<String, Error> {
    state.data_unchecked().try_into_string()
}

fn json_of(state: &State<Value>) -> Result<serde_json::Value, Error> {
    state.data_unchecked().try_into_json_value()
}

fn column(view: &Arc<dyn RecordView>, name: &str) -> Result<Vec<String>, Error> {
    let col = view
        .schema()
        .index_of(name)
        .ok_or_else(|| Error::general_error(format!("no column {name}")))?;
    (0..view.len())
        .map(|row| {
            Ok(match view.value(row, col)? {
                FieldValue::Text(t) => t.to_string(),
                FieldValue::Int(i) => i.to_string(),
                FieldValue::Float(f) => f.to_string(),
                FieldValue::Bool(b) => b.to_string(),
                FieldValue::Null => String::new(),
                other => format!("{other:?}"),
            })
        })
        .collect()
}

fn notes(state: &State<Value>) -> Vec<String> {
    match &*state.metadata {
        Metadata::MetadataRecord(record) => record
            .log
            .iter()
            .filter(|entry| entry.kind == LogEntryKind::Info)
            .map(|entry| entry.message.clone())
            .collect(),
        Metadata::LegacyMetadata(_) => vec![],
    }
}

fn disk(fixture: &Fixture, path: &str) -> String {
    std::fs::read_to_string(fixture.dir.join("mem").join(path)).unwrap_or_default()
}

fn log_text(fixture: &Fixture) -> String {
    let month = chrono::Utc::now().format("%Y-%m").to_string();
    disk(fixture, &format!("_log/{month}.md"))
}

fn tree(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(base, &path, out);
                } else if let Ok(rel) = path.strip_prefix(base) {
                    out.push(rel.display().to_string());
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

// ------------------------------------------------------------------------------------------------
// Reading
// ------------------------------------------------------------------------------------------------

/// AC-1.
#[tokio::test]
async fn list_entries_carry_l0_l1() -> Res {
    let f = fixture("list").await?;
    let state = call(&f.env, "-R-key/issues/-/ns-mem/list/list.md", serde_json::json!({})).await?;
    let view = view(&state)?;
    assert_eq!(column(&view, "key")?, vec!["issues/A-EXPIRY.md", "issues/B-CLOSED.md", "issues/C-STORE.md"]);
    assert_eq!(column(&view, "l0")?[0], "Expired assets are served stale");
    assert_eq!(column(&view, "l1")?[2], "Two writers can overwrite each other");
    assert_eq!(column(&view, "kind")?, vec!["issues"; 3]);
    assert_eq!(column(&view, "status")?, vec!["draft", "closed", "draft"]);
    assert!(notes(&state).contains(&"shown 3 of 3".to_string()), "{:?}", notes(&state));
    // The root lists folders with their README tiers; `_schema` and `_log` are hidden.
    let root = view_of(&f.env, "-R-key/./-/ns-mem/list").await?;
    let keys = column(&root, "key")?;
    assert!(keys.contains(&"notes".to_string()) && !keys.iter().any(|k| k.starts_with('_')), "{keys:?}");
    let notes_row = keys.iter().position(|k| k == "notes").unwrap_or(usize::MAX);
    assert_eq!(column(&root, "l1")?[notes_row], "Short notes kept by agents.");
    // Markdown is the default format: a pipe table.
    let md = f.env.get_asset_manager();
    let _ = md;
    Ok(())
}

async fn view_of(env: &Env, query: &str) -> Result<Arc<dyn RecordView>, Error> {
    view(&call(env, query, serde_json::json!({})).await?)
}

/// AC-2.
#[tokio::test]
async fn search_ranks_and_filters_before_limit() -> Res {
    let f = fixture("search").await?;
    let state = call(
        &f.env,
        "-R-key/issues/-/ns-mem/search/search.md",
        serde_json::json!({"text": "expiration", "filter": "-status:closed*", "limit": 1}),
    )
    .await?;
    let view = view(&state)?;
    assert_eq!(column(&view, "key")?, vec!["issues/A-EXPIRY.md"]);
    let names: Vec<String> = view.schema().fields.iter().map(|f| f.name.clone()).collect();
    assert_eq!(names, ["key", "l0", "l1", "excerpt", "score", "matched", "heading", "line"]);
    assert!(notes(&state).iter().any(|n| n == "shown 1 of 2; raise limit to see more"), "{:?}", notes(&state));
    // Without the filter the closed issue ranks first: it says expiration most often.
    let all = view_of_args(&f.env, "-R-key/issues/-/ns-mem/search", serde_json::json!({"text": "expiration"})).await?;
    assert_eq!(column(&all, "key")?[0], "issues/B-CLOSED.md");
    let scores: Vec<f64> = column(&all, "score")?.iter().map(|s| s.parse().unwrap_or(0.0)).collect();
    assert!(scores.windows(2).all(|w| w[0] >= w[1]), "{scores:?}");
    // By section: the heading and line of the match.
    let sections = view_of_args(
        &f.env,
        "-R-key/issues/-/ns-mem/search",
        serde_json::json!({"text": "compare-and-swap", "unit": "section"}),
    )
    .await?;
    assert_eq!(column(&sections, "heading")?, vec!["Store writes have no precondition > Problem"]);
    Ok(())
}

async fn view_of_args(env: &Env, query: &str, args: serde_json::Value) -> Result<Arc<dyn RecordView>, Error> {
    view(&call(env, query, args).await?)
}

/// AC-3.
#[tokio::test]
async fn read_section_returns_section_and_version_note() -> Res {
    let f = fixture("read").await?;
    let state = call(
        &f.env,
        "-R/issues/A-EXPIRY.md/-/ns-mem/read/read.md",
        serde_json::json!({"section": "Problem"}),
    )
    .await?;
    assert_eq!(text_of(&state)?, "## Problem\n\nAn expired asset is served stale; expiration is ignored, expiration never checked.\n\n");
    let version = Version::from_content(A_EXPIRY.as_bytes()).to_string();
    assert!(notes(&state).contains(&format!("version {version}")), "{:?}", notes(&state));
    assert!(notes(&state).contains(&"key issues/A-EXPIRY.md".to_string()));
    let l1 = call(&f.env, "-R/issues/A-EXPIRY.md/-/ns-mem/read-l1", serde_json::json!({})).await?;
    assert_eq!(
        text_of(&l1)?,
        "Expired assets are served stale\n\nAn expired asset is returned without recomputation"
    );
    Ok(())
}

/// AC-11: a key state is read from the store, so a hand edit is seen after the document loaded.
#[tokio::test]
async fn read_of_a_key_sees_a_hand_edit() -> Res {
    let f = fixture("read-fresh").await?;
    let loaded = call(&f.env, "-R/issues/A-EXPIRY.md/-/ns-mem/read-l0", serde_json::json!({})).await?;
    assert_eq!(text_of(&loaded)?, "Expired assets are served stale");
    let path = f.dir.join("mem/issues/A-EXPIRY.md");
    let edited = std::fs::read_to_string(&path)?.replacen("title: Expired", "title: Edited, expired", 1);
    std::fs::write(&path, &edited)?;
    let fresh = call(&f.env, "-R-key/issues/A-EXPIRY.md/-/ns-mem/read-l0", serde_json::json!({})).await?;
    assert_eq!(text_of(&fresh)?, "Edited, expired assets are served stale");
    let version = Version::from_content(edited.as_bytes()).to_string();
    assert!(notes(&fresh).contains(&format!("version {version}")), "{:?}", notes(&fresh));
    Ok(())
}

/// AC-15.
#[tokio::test]
async fn overview_and_describe_orient() -> Res {
    let f = fixture("overview").await?;
    let overview = text_of(&call(&f.env, "-R-key/./-/ns-mem/overview/overview.md", serde_json::json!({})).await?)?;
    assert!(overview.starts_with("# Test memory\n\nIssues and notes for the tests\n"), "{overview}");
    assert!(overview.contains("Issues and notes for the tests\n\nSearch before you write.\n\n## Collections"), "{overview}");
    assert!(overview.contains("| issues | issues | 3 | yes | What is wrong or missing |"), "{overview}");
    assert!(overview.contains("| archive | archive | 1 | no |"), "{overview}");
    assert!(overview.contains("No writes logged yet."));
    let describe = text_of(&call(&f.env, "-R-key/./-/ns-mem/describe-issues", serde_json::json!({})).await?)?;
    assert!(describe.contains("## issues") && !describe.contains("## notes"), "{describe}");
    assert!(describe.contains("| status | yes | draft, in_progress, closed (transitions limited) | draft |"), "{describe}");
    assert!(describe.contains("- History: rows appended to `History` stamp `reviewed`"));
    Ok(())
}

/// AC-17: no front-matter, no store title — the bridge falls back to the heading and paragraph.
#[tokio::test]
async fn fallback_tiers_without_front_matter() -> Res {
    let f = fixture("fallback").await?;
    let view = view_of(&f.env, "-R-key/notes/-/ns-mem/list").await?;
    let keys = column(&view, "key")?;
    let row = keys.iter().position(|k| k == "notes/plain.md").ok_or("plain.md listed")?;
    assert_eq!(column(&view, "l0")?[row], "A plain note");
    assert_eq!(column(&view, "l1")?[row], "The first paragraph of a note without front-matter.");
    Ok(())
}

/// AC-12.
#[tokio::test]
async fn list_and_search_do_not_evaluate_recipes() -> Res {
    let f = fixture("recipes").await?;
    let before = *counts().lock().unwrap_or_else(|p| p.into_inner());
    let view = view_of(&f.env, "-R-key/notes/-/ns-mem/list").await?;
    let keys = column(&view, "key")?;
    let row = keys.iter().position(|k| k == "notes/derived.md").ok_or("derived.md listed")?;
    assert_eq!(column(&view, "asset_status")?[row], "Recipe");
    view_of_args(&f.env, "-R-key/notes/-/ns-mem/search", serde_json::json!({"text": "derived"})).await?;
    assert_eq!(*counts().lock().unwrap_or_else(|p| p.into_inner()), before);
    Ok(())
}

/// AC-18 (migration guide §1): reading writes nothing into the store.
#[tokio::test]
async fn mem_results_are_never_stored() -> Res {
    let f = fixture("unstored").await?;
    let before = tree(&f.dir);
    for query in [
        "-R-key/./-/ns-mem/overview/overview.md",
        "-R-key/./-/ns-mem/describe/describe.md",
        "-R-key/issues/-/ns-mem/list/list.md",
        "-R-key/issues/-/ns-mem/search-expiration/search.md",
        "-R/issues/A-EXPIRY.md/-/ns-mem/read/read.md",
        "-R-key/./-/ns-mem/check/check.md",
    ] {
        call(&f.env, query, serde_json::json!({})).await?;
    }
    call(&f.env, "-R-key/./-/ns-mem/similar-issues-expiration", serde_json::json!({})).await?;
    assert_eq!(tree(&f.dir), before);
    Ok(())
}

/// AC-15: a schema from a newer service is refused, naming both versions.
#[tokio::test]
async fn newer_schema_version_is_refused() -> Res {
    let f = fixture_with("version", &[("_schema/memory.yaml", "version: 2\ncollections: []\n")]).await?;
    let error = call(&f.env, "-R-key/./-/ns-mem/overview", serde_json::json!({}))
        .await
        .err()
        .ok_or("a newer schema was accepted")?;
    assert!(error.message.contains("version 2") && error.message.contains("version 1"), "{}", error.message);
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Writing
// ------------------------------------------------------------------------------------------------

/// AC-4.
#[tokio::test]
async fn new_returns_candidates_then_writes_with_force() -> Res {
    let f = fixture("new").await?;
    let args = serde_json::json!({
        "collection": "issues",
        "title": "Expired assets served stale again",
        "description": "Stale values after expiration",
        "fields": {"area": ["core"]},
        "reason": "test",
    });
    let first = json_of(&call(&f.env, "-R-key/./-/ns-mem/new/new.json", args.clone()).await?)?;
    assert_eq!(first["written"], false);
    assert_eq!(first["candidates"][0]["key"], "issues/A-EXPIRY.md");
    let mut forced = args.clone();
    forced["force"] = serde_json::json!(true);
    let second = json_of(&call(&f.env, "-R-key/./-/ns-mem/new/new.json", forced).await?)?;
    assert_eq!(second["written"], true);
    assert_eq!(second["key"], "issues/EXPIRED-ASSETS-SERVED-STALE-AGAIN.md");
    let text = disk(&f, "issues/EXPIRED-ASSETS-SERVED-STALE-AGAIN.md");
    assert!(text.starts_with("---\nid: EXPIRED-ASSETS-SERVED-STALE-AGAIN\ntitle: Expired assets served stale again\ndescription: Stale values after expiration\nstatus: draft\narea: [core]\n"), "{text}");
    assert_eq!(second["version"], Version::from_content(text.as_bytes()).to_string());
    // A note: kebab id, no candidates.
    let note = json_of(
        &call(
            &f.env,
            "-R-key/./-/ns-mem/new",
            serde_json::json!({"collection": "notes", "title": "Café meeting", "description": "Met at the café", "body": "Details.\n"}),
        )
        .await?,
    )?;
    assert_eq!(note["key"], "notes/cafe-meeting.md");
    Ok(())
}

/// AC-5.
#[tokio::test]
async fn write_refuses_value_outside_vocabulary() -> Res {
    let f = fixture("vocabulary").await?;
    let error = call(
        &f.env,
        "-R-key/issues/A-EXPIRY.md/-/ns-mem/write/write.json",
        serde_json::json!({"fields": {"area": ["nope"]}}),
    )
    .await
    .err()
    .ok_or("an unknown area was accepted")?;
    assert!(error.message.contains("'nope' is not one of: core, lib, mcp"), "{}", error.message);
    assert_eq!(disk(&f, "issues/A-EXPIRY.md"), A_EXPIRY);
    Ok(())
}

/// AC-5.
#[tokio::test]
async fn create_without_title_or_description_is_refused() -> Res {
    let f = fixture("untitled").await?;
    let error = call(
        &f.env,
        "-R-key/./-/ns-mem/new",
        serde_json::json!({"collection": "issues", "title": "Something", "description": "", "force": true}),
    )
    .await
    .err()
    .ok_or("a memory without a description was created")?;
    assert!(error.message.contains("description"), "{}", error.message);
    Ok(())
}

/// AC-6.
#[tokio::test]
async fn stale_expected_version_is_refused() -> Res {
    let f = fixture("stale").await?;
    let stale = Version::from_content(b"an older text").to_string();
    let error = call(
        &f.env,
        "-R-key/issues/A-EXPIRY.md/-/ns-mem/edit",
        serde_json::json!({"field": "status", "value": "in_progress", "expected_version": stale}),
    )
    .await
    .err()
    .ok_or("a stale version was accepted")?;
    assert_eq!(error.error_type, ErrorType::DependencyVersionMismatch);
    let malformed = call(
        &f.env,
        "-R-key/issues/A-EXPIRY.md/-/ns-mem/edit",
        serde_json::json!({"field": "status", "value": "in_progress", "expected_version": "3f9c"}),
    )
    .await
    .err()
    .ok_or("a malformed version was accepted")?;
    assert_eq!(malformed.error_type, ErrorType::ParseError);
    assert_eq!(disk(&f, "issues/A-EXPIRY.md"), A_EXPIRY);
    Ok(())
}

/// AC-7.
#[tokio::test]
async fn write_stamps_and_logs() -> Res {
    let f = fixture("stamps").await?;
    let version = Version::from_content(A_EXPIRY.as_bytes()).to_string();
    let result = json_of(
        &call(
            &f.env,
            "-R-key/issues/A-EXPIRY.md/-/ns-mem/write/write.json",
            serde_json::json!({"body": "# Expired assets are served stale\n\n## Problem\n\nRewritten.\n\n## History\n\n| 2026-01-01 | Filed |\n", "expected_version": version, "reason": "rewrite"}),
        )
        .await?,
    )?;
    assert_eq!(result["version_before"], version);
    let text = disk(&f, "issues/A-EXPIRY.md");
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    assert!(text.contains(&format!("updated: {today}\n")), "{text}");
    assert!(text.contains("Rewritten."));
    assert_eq!(result["version_after"], Version::from_content(text.as_bytes()).to_string());
    let log = log_text(&f);
    assert_eq!(log.lines().count(), 1, "{log}");
    assert!(log.contains(&format!(" write issues/A-EXPIRY.md {}→", &version[..8])), "{log}");
    assert!(log.trim_end().ends_with("— rewrite"), "{log}");
    Ok(())
}

/// AC-16, AC-7.
#[tokio::test]
async fn edit_combined_pairs_and_forbidden_transition() -> Res {
    let f = fixture("edit").await?;
    let result = json_of(
        &call(
            &f.env,
            "-R-key/issues/A-EXPIRY.md/-/ns-mem/edit/edit.json",
            serde_json::json!({
                "old": "expiration is ignored", "new": "expiration is not checked",
                "field": "status", "value": "in_progress",
                "section": "History", "row": "| 2026-10-19 | Started |",
                "reason": "starting work",
            }),
        )
        .await?,
    )?;
    assert!(result["version_after"].is_string());
    let text = disk(&f, "issues/A-EXPIRY.md");
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    assert!(text.contains("expiration is not checked"));
    assert!(text.contains("status: in_progress\n"));
    assert!(text.ends_with("| 2026-01-01 | Filed |\n| 2026-10-19 | Started |\n"), "{text}");
    assert!(text.contains(&format!("reviewed: {today}\n")));
    assert_eq!(log_text(&f).lines().count(), 1);
    // closed -> in_progress is not in the transition table: nothing written.
    let error = call(
        &f.env,
        "-R-key/issues/B-CLOSED.md/-/ns-mem/edit",
        serde_json::json!({"field": "status", "value": "in_progress"}),
    )
    .await
    .err()
    .ok_or("a forbidden transition was accepted")?;
    assert!(error.message.contains("closed → in_progress"), "{}", error.message);
    assert_eq!(disk(&f, "issues/B-CLOSED.md"), B_CLOSED);
    assert_eq!(log_text(&f).lines().count(), 1);
    Ok(())
}

/// AC-8.
#[tokio::test]
async fn supersedes_marks_old_and_hides_it() -> Res {
    let f = fixture("supersede").await?;
    let result = json_of(
        &call(
            &f.env,
            "-R-key/issues/C-STORE.md/-/ns-mem/write/write.json",
            serde_json::json!({"supersedes": "issues/B-CLOSED.md", "reason": "merged"}),
        )
        .await?,
    )?;
    assert_eq!(result["superseded"]["key"], "issues/B-CLOSED.md");
    assert!(disk(&f, "issues/C-STORE.md").contains("supersedes: issues/B-CLOSED.md\n"));
    assert!(disk(&f, "issues/B-CLOSED.md").contains("superseded_by: issues/C-STORE.md\n"));
    assert_eq!(log_text(&f).lines().count(), 2);
    let listed = column(&view_of(&f.env, "-R-key/issues/-/ns-mem/list").await?, "key")?;
    assert!(!listed.contains(&"issues/B-CLOSED.md".to_string()), "{listed:?}");
    let all = column(&view_of(&f.env, "-R-key/issues/-/ns-mem/list-1-true").await?, "key")?;
    assert!(all.contains(&"issues/B-CLOSED.md".to_string()));
    let found = view_of_args(&f.env, "-R-key/issues/-/ns-mem/search", serde_json::json!({"text": "expiration"})).await?;
    assert!(!column(&found, "key")?.contains(&"issues/B-CLOSED.md".to_string()));
    Ok(())
}

/// AC-11: a hand edit is seen at once (the document not loaded).
#[tokio::test]
async fn front_matter_projection_follows_external_edit() -> Res {
    let f = fixture("projection").await?;
    // Not loaded: the record is derived on every read (storage: none).
    std::fs::write(f.dir.join("mem/issues/C-STORE.md"), C_STORE.replace("Store writes have no precondition\ndescription", "Edited by hand\ndescription"))?;
    let l0 = text_of(&call(&f.env, "-R/issues/C-STORE.md/-/ns-mem/read-l0", serde_json::json!({})).await?)?;
    assert_eq!(l0, "Edited by hand");
    let info = f.env.get_asset_manager().get_asset_info(&parse_key("issues/C-STORE.md")?).await?;
    assert_eq!(info.title, "Edited by hand");
    Ok(())
}

/// AC-13.
#[tokio::test]
async fn read_only_mount_refuses_write_and_leaves_no_sidecar() -> Res {
    let f = fixture("readonly").await?;
    let listed = column(&view_of(&f.env, "-R-key/archive/-/ns-mem/list").await?, "key")?;
    assert_eq!(listed, vec!["archive/OLD.md"]);
    let read = text_of(&call(&f.env, "-R/archive/OLD.md/-/ns-mem/read-l0", serde_json::json!({})).await?)?;
    assert_eq!(read, "Old");
    let error = call(
        &f.env,
        "-R-key/archive/OLD.md/-/ns-mem/edit",
        serde_json::json!({"field": "title", "value": "New"}),
    )
    .await
    .err()
    .ok_or("a write into the archive was accepted")?;
    assert!(error.message.contains("not writable"), "{}", error.message);
    assert_eq!(tree(&f.dir.join("archive")), vec!["archive/OLD.md"]);
    // The writable mount gains no sidecar on reads either.
    assert!(!tree(&f.dir.join("mem")).iter().any(|p| p.contains("__metadata__")));
    Ok(())
}

/// AC-12 (migration guide §3.2): a computed recipe result removed drops back to its recipe.
#[tokio::test]
async fn recipe_results_drop_back_to_recipe() -> Res {
    let f = fixture("recipe-drop").await?;
    let key = parse_key("computed/derived.md")?;
    let before = *counts().lock().unwrap_or_else(|p| p.into_inner());
    f.env.get_asset_manager().get(&key).await?.get().await?;
    // The value is written to the store in the background; remove once it has landed.
    for _ in 0..100 {
        if f.env.get_async_store().get_asset_info(&key).await.map(|i| i.status) == Ok(Status::Ready) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    f.env.get_asset_manager().remove(&key).await?;
    let info = f.env.get_asset_manager().get_asset_info(&key).await?;
    assert_eq!(info.status, Status::Recipe);
    f.env.get_asset_manager().get(&key).await?.get().await?;
    assert!(*counts().lock().unwrap_or_else(|p| p.into_inner()) >= before + 2);
    Ok(())
}

/// `storage-none-computed-values` AC-8: in the memory's own `storage: none` mount a recipe result's
/// record is inferred, so `remove` deletes the result file and the key drops back to its recipe,
/// instead of the file being read and deleted as a user's `Source`.
#[tokio::test]
async fn recipe_results_drop_back_to_recipe_in_a_storage_none_mount() -> Res {
    let f = fixture("recipe-drop-none").await?;
    let key = parse_key("notes/derived.md")?;
    let path = f.dir.join("mem/notes/derived.md");
    f.env.get_asset_manager().get(&key).await?.get().await?;
    // The value is written to the store in the background; remove once it has landed.
    for _ in 0..200 {
        if path.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(path.exists(), "the result is written into the checkout");
    assert!(f.env.get_async_store().get_metadata(&key).await?.is_inferred());
    f.env.get_asset_manager().remove(&key).await?;
    assert!(!path.exists(), "the result file is deleted");
    let info = f.env.get_asset_manager().get_asset_info(&key).await?;
    assert_eq!(info.status, Status::Recipe);
    Ok(())
}

/// AC-7 (Phase 2 preflight): writes keep no live asset for the keys they write.
#[tokio::test]
async fn many_writes_do_not_grow_retained_assets() -> Res {
    let f = fixture("many").await?;
    for i in 0..100 {
        let value = if i % 2 == 0 { "core" } else { "lib" };
        call(
            &f.env,
            "-R-key/issues/A-EXPIRY.md/-/ns-mem/edit",
            serde_json::json!({"field": "area", "value": format!("[{value}]")}),
        )
        .await?;
    }
    let am = f.env.get_asset_manager();
    assert!(am.lookup_key_asset(&parse_key("issues/A-EXPIRY.md")?).is_none());
    assert_eq!(log_text(&f).lines().count(), 100);
    Ok(())
}

/// AC-19: the same code over an OpenViking-like layout.
#[tokio::test]
async fn openviking_layout_lists_creates_and_searches() -> Res {
    let schema = r#"
readme: .overview.md
collections:
  - { name: profile, folder: user/memories, file: profile.md }
  - { name: preferences, folder: user/memories/preferences, id: { from: title, case: kebab } }
  - name: events
    folder: user/memories/events
    id: { from: date_title, case: kebab }
    fields: { date: { type: date, required: true } }
"#;
    let f = fixture_with(
        "openviking",
        &[
            ("_schema/memory.yaml", schema),
            (".overview.md", "---\ntitle: Agent memory\ndescription: What the agent remembers about its user\n---\n"),
            ("user/memories/profile.md", "---\ntitle: Profile\ndescription: Who the user is\n---\nLikes tea.\n"),
            ("user/memories/preferences/tea.md", "---\ntitle: Tea\ndescription: Prefers green tea\n---\n"),
        ],
    )
    .await?;
    let overview = text_of(&call(&f.env, "-R-key/./-/ns-mem/overview", serde_json::json!({})).await?)?;
    assert!(overview.starts_with("# Agent memory\n\nWhat the agent remembers about its user"), "{overview}");
    assert!(overview.contains("| profile | user/memories | 1 |"), "{overview}");
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let created = json_of(
        &call(
            &f.env,
            "-R-key/./-/ns-mem/new",
            serde_json::json!({"collection": "events", "title": "Tea tasting", "description": "Went to a tea tasting", "fields": {"date": today}}),
        )
        .await?,
    )?;
    assert_eq!(created["key"], format!("user/memories/events/{today}-tea-tasting.md"));
    let found = view_of_args(&f.env, "-R-key/user/-/ns-mem/search", serde_json::json!({"text": "tea"})).await?;
    assert_eq!(column(&found, "key")?.len(), 3);
    let missing_date = call(
        &f.env,
        "-R-key/./-/ns-mem/new",
        serde_json::json!({"collection": "events", "title": "No date", "description": "Lacks a date"}),
    )
    .await;
    assert!(missing_date.err().is_some_and(|e| e.message.contains("date")));
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// check
// ------------------------------------------------------------------------------------------------

/// AC-9: the fixture is clean; one violation per rule is reported under its rule name.
#[tokio::test]
async fn check_reports_rules_by_name() -> Res {
    let f = fixture_with(
        "check",
        &[
            ("issues/D-BAD.md", "---\nid: WRONG\ntitle: Bad\ndescription: d\nstatus: open\nsupersedes: issues/A-EXPIRY.md\n---\n# Bad\n\nSee [missing](./NOPE.md).\n"),
        ],
    )
    .await?;
    let state = call(&f.env, "-R-key/./-/ns-mem/check/check.md", serde_json::json!({})).await?;
    let view = view(&state)?;
    let rules = column(&view, "rule")?;
    let keys = column(&view, "key")?;
    for rule in ["id", "enum", "sections", "reviewed", "history", "dead_links", "supersede_reciprocal"] {
        assert!(rules.iter().zip(&keys).any(|(r, k)| r == rule && k == "issues/D-BAD.md"), "{rule}: {rules:?}");
    }
    assert!(keys.iter().all(|k| k == "issues/D-BAD.md"), "{keys:?}");
    assert!(notes(&state).iter().any(|n| n.starts_with("unsupported: index.csv is stale")), "{:?}", notes(&state));
    Ok(())
}

/// markdown-navigation AC-5: `dead_links` now also checks reference-style links, and ignores
/// links inside code spans and fenced blocks as before.
#[tokio::test]
async fn dead_links_checks_reference_links() -> Res {
    let f = fixture_with(
        "check_reference_links",
        &[(
            "issues/E-REF.md",
            "---\nid: E-REF\ntitle: Ref\ndescription: d\nstatus: open\n---\n# Ref\n\nSee [the spec][gone], `[x](./CODE.md)`.\n\n[gone]: ./GONE.md\n",
        )],
    )
    .await?;
    let state = call(&f.env, "-R-key/./-/ns-mem/check/check.md", serde_json::json!({})).await?;
    let view = view(&state)?;
    let rules = column(&view, "rule")?;
    let keys = column(&view, "key")?;
    let messages = column(&view, "message")?;
    let dead: Vec<&String> = rules
        .iter()
        .zip(&keys)
        .zip(&messages)
        .filter(|((r, k), _)| *r == "dead_links" && *k == "issues/E-REF.md")
        .map(|(_, m)| m)
        .collect();
    assert_eq!(dead.len(), 1, "{dead:?}");
    assert!(dead[0].contains("GONE.md") && !dead[0].contains("CODE.md"), "{dead:?}");
    Ok(())
}
