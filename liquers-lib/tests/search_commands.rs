//! `ns-search` end to end: every command through `EnvRef::evaluate` on one fixture store
//! (`specs/design/store-and-asset-search/phase3-examples.md` §"The fixture store").
#![cfg(feature = "records")]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use liquers_core::assets::AssetManager;
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::metadata::{LogEntryKind, Metadata, MetadataRecord, Status};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::state::State;
use liquers_core::store::{AsyncMemoryStore, AsyncStore};
use liquers_core::value::ValueInterface;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::records::{
    FieldSchema, FieldType, FieldValue, InMemorySource, RecordBatchMut, RecordSchema, RecordView,
    RecordViewMut,
};
use liquers_lib::value::{ExtValueInterface, Value};
use liquers_lib::{register_records_commands, register_search_commands};

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

/// The command behind the recipe-only key `specs/issues/derived.md`; counts its runs per tag.
fn count_derived(tag: String) -> Result<Value, Error> {
    *counts().lock().unwrap_or_else(|p| p.into_inner()).entry(tag).or_insert(0) += 1;
    Ok(Value::from("derived"))
}

/// 1 001 views of 1 000 rows each — more than `DEFAULT_MATERIALIZE_MAX_ROWS`.
fn big_source() -> Result<Value, Error> {
    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("x", FieldType::Text)])?);
    let mut batch = RecordBatchMut::with_capacity(schema, 1000);
    for _ in 0..1000 {
        batch.append_row(&[FieldValue::Text(Arc::from("x"))])?;
    }
    let view: Arc<dyn RecordView> = Arc::new(batch.freeze()?);
    let views = vec![view; 1001];
    Ok(Value::from_record_source(Arc::new(InMemorySource::new(views))))
}

const A_EXPIRY: &str = "---
id: A-EXPIRY
title: Expired assets are served stale
status: draft
priority: P1
area: [core/assets]
links:
  github: 12
key: bogus
---
# Expired assets are served stale

The expiration safety of cached assets matters. More on expiration safety below.
";

const B_CLOSED: &str = "---
id: B-CLOSED
title: An old problem
status: closed
area: [core/store]
---
# An old problem

This was about expiration safety once.
";

const C_STORE: &str = "---
id: C-STORE
title: Store listing
status: draft
area: [core/store]
---
# Store listing

Nothing to see here.
";

const D_TOML: &str = "+++
id = \"D-TOML\"
status = \"accepted\"
created = 2026-10-01T00:00:00Z
+++
# TOML front-matter

Unrelated.
";

const DESIGN: &str = "---
id: X
phase: architecture
status: in_review
---
# Design X
";

const PHASE1: &str = "# Phase 1

Intro text.

## Purpose

Why it exists.
";

const LIB_RS: &str = "pub fn get_asset_info(key: &str) -> String {
    key.to_string()
}

fn caller() {
    let _ = get_asset_info(\"x\");
}
";

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

async fn fixture(tag: &str) -> Result<EnvRef<CommandEnvironment>, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    put(&store, "specs/issues/A-EXPIRY.md", A_EXPIRY.as_bytes()).await?;
    put(&store, "specs/issues/B-CLOSED.md", B_CLOSED.as_bytes()).await?;
    put(&store, "specs/issues/C-STORE.md", C_STORE.as_bytes()).await?;
    put(&store, "specs/issues/D-TOML.md", D_TOML.as_bytes()).await?;
    put(&store, "specs/issues/notes.txt", b"plain notes about expiration").await?;
    store
        .set(
            &parse_key("specs/issues/recipes.yaml")?,
            format!("recipes:\n  - query: \"ns-fixture/count_derived-{tag}/derived.md\"\n").as_bytes(),
            &Metadata::new(),
        )
        .await?;
    put(&store, "specs/design/x/DESIGN.md", DESIGN.as_bytes()).await?;
    put(&store, "specs/design/x/phase1-high-level-design.md", PHASE1.as_bytes()).await?;
    put(&store, "src/lib.rs", LIB_RS.as_bytes()).await?;
    put(&store, "data/table.csv", b"city,country\nParis,France\nLyon,France\nOslo,Norway\n").await?;
    put(&store, "data/config.json", br#"{"name": "demo", "size": 3}"#).await?;
    put(&store, "data/settings.yaml", b"mode: fast\nlevel: 2\n").await?;
    put(&store, "data/blob.bin", &[0u8, 159, 146, 150, 255, 1, 2]).await?;
    put(&store, "extra/collide.json", br#"{"a-b": 1, "a_b": 2, "big": 18446744073709551615}"#).await?;

    let mut env = CommandEnvironment::new();
    {
        let cr = env.get_mut_command_registry();
        register_records_commands!(cr)?;
        register_search_commands!(cr)?;
        register_command!(cr, fn count_derived(tag: String) -> result namespace: "fixture")?;
        register_command!(cr, fn big_source() -> result namespace: "fixture")?;
    }
    env.with_async_store(Box::new(store));
    Ok(env.to_ref())
}

async fn eval(envref: &EnvRef<CommandEnvironment>, query: &str) -> Result<State<Value>, Error> {
    envref.evaluate(query).await?.get().await
}

async fn view_of(envref: &EnvRef<CommandEnvironment>, query: &str) -> Result<Arc<dyn RecordView>, Error> {
    eval(envref, query).await?.value()?.as_record_view()
}

fn column(view: &Arc<dyn RecordView>, name: &str) -> Vec<FieldValue> {
    let index = view.schema().index_of(name).unwrap_or_else(|| panic!("no column '{name}'"));
    (0..view.len()).map(|row| view.value(row, index).expect("cell")).collect()
}

fn texts(view: &Arc<dyn RecordView>, name: &str) -> Vec<String> {
    column(view, name)
        .into_iter()
        .map(|v| match v {
            FieldValue::Text(t) => t.to_string(),
            FieldValue::Null => String::new(),
            other => format!("{other:?}"),
        })
        .collect()
}

fn row_of(view: &Arc<dyn RecordView>, key: &str) -> usize {
    texts(view, "key").iter().position(|k| k == key).unwrap_or_else(|| panic!("no row '{key}'"))
}

/// Writes `text` to `key` through the asset manager, as a user or a command would: the write
/// registers a new version and refreshes the folder's listing.
async fn write(envref: &EnvRef<CommandEnvironment>, key: &str, text: &str) -> Result<(), Error> {
    let key = parse_key(key)?;
    let filename = key
        .filename()
        .ok_or_else(|| Error::general_error(format!("key '{key}' has no filename")))?
        .encode();
    let mut metadata = MetadataRecord {
        type_identifier: "Text".into(),
        type_name: "text".into(),
        data_format: Some(filename.rsplit('.').next().unwrap_or("txt").to_string()),
        ..Default::default()
    };
    metadata.set_filename(filename.as_ref());
    envref.get_asset_manager().set_binary(&key, text.as_bytes(), metadata).await
}

fn dependencies_of(state: &State<Value>) -> Vec<String> {
    match &*state.metadata {
        Metadata::MetadataRecord(record) => {
            record.dependencies.iter().map(|d| d.key.as_str().to_string()).collect()
        }
        Metadata::LegacyMetadata(_) => vec![],
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

// ------------------------------------------------------------------------------------------------
// catalog
// ------------------------------------------------------------------------------------------------

#[tokio::test]
async fn catalog_lists_folder_recursively() -> Res {
    let env = fixture("catalog_lists_folder_recursively").await?;
    let view = view_of(&env, "-R-key/specs/-/ns-search/catalog").await?;
    let keys = texts(&view, "key");
    assert!(keys.contains(&"specs/issues/A-EXPIRY.md".to_string()), "{keys:?}");
    assert!(keys.contains(&"specs/design/x/DESIGN.md".to_string()), "{keys:?}");
    assert_eq!(view.schema().id_field(), view.schema().index_of("key"));
    for name in ["filename", "extension", "parent", "is_dir", "status", "type_identifier",
        "media_type", "title", "description", "size", "updated"] {
        assert!(view.schema().index_of(name).is_some(), "column {name}");
    }
    let a = row_of(&view, "specs/issues/A-EXPIRY.md");
    assert_eq!(texts(&view, "extension")[a], "md");
    assert_eq!(texts(&view, "parent")[a], "specs/issues");
    Ok(())
}

#[tokio::test]
async fn catalog_without_input_starts_at_root() -> Res {
    let env = fixture("catalog_without_input_starts_at_root").await?;
    let keys = texts(&view_of(&env, "ns-search/catalog").await?, "key");
    for key in ["data/table.csv", "src/lib.rs", "specs/issues/C-STORE.md"] {
        assert!(keys.contains(&key.to_string()), "{key} in {keys:?}");
    }
    Ok(())
}

#[tokio::test]
async fn catalog_folder_only() -> Res {
    let env = fixture("catalog_folder_only").await?;
    let view = view_of(&env, "-R-key/specs/-/ns-search/catalog-f").await?;
    assert_eq!(texts(&view, "key"), vec!["specs/design", "specs/issues"]);
    assert_eq!(column(&view, "is_dir"), vec![FieldValue::Bool(true), FieldValue::Bool(true)]);
    Ok(())
}

#[tokio::test]
async fn catalog_never_evaluates_recipe_keys() -> Res {
    let tag = "catalog_never_evaluates_recipe_keys";
    let env = fixture(tag).await?;
    let view = view_of(&env, "-R-key/specs/issues/-/ns-search/catalog-t-t").await?;
    let derived = row_of(&view, "specs/issues/derived.md");
    assert_eq!(texts(&view, "status")[derived], "Recipe");
    assert_eq!(column(&view, "content")[derived], FieldValue::Null);
    assert_eq!(count_of(tag), 0, "the recipe's command must not run");
    let key = parse_key("specs/issues/derived.md")?;
    assert!(env.get_asset_manager().lookup_key_asset(&key).is_none(), "no asset was started");
    Ok(())
}

#[tokio::test]
async fn catalog_reads_text_content_only() -> Res {
    let env = fixture("catalog_reads_text_content_only").await?;
    let view = view_of(&env, "-R-key/data/-/ns-search/catalog-t-t").await?;
    let content = column(&view, "content");
    for key in ["data/table.csv", "data/config.json", "data/settings.yaml"] {
        assert!(matches!(content[row_of(&view, key)], FieldValue::Text(_)), "{key} has content");
    }
    assert_eq!(content[row_of(&view, "data/blob.bin")], FieldValue::Null);
    let cut = view_of(&env, "-R-key/data/-/ns-search/catalog-t-t-16").await?;
    match &column(&cut, "content")[row_of(&cut, "data/table.csv")] {
        FieldValue::Text(text) => assert_eq!(text.len(), 16, "{text:?}"),
        other => panic!("{other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn catalog_markdown_title_from_heading() -> Res {
    let env = fixture("catalog_markdown_title_from_heading").await?;
    let view = view_of(&env, "-R-key/specs/design/-/ns-search/catalog-t-t").await?;
    let row = row_of(&view, "specs/design/x/phase1-high-level-design.md");
    assert_eq!(texts(&view, "title")[row], "Phase 1");
    Ok(())
}

#[tokio::test]
async fn catalog_stops_at_max_entries() -> Res {
    let env = fixture("catalog_stops_at_max_entries").await?;
    let state = eval(&env, "ns-search/catalog-t-f-0-2").await?;
    assert_eq!(state.value()?.as_record_view()?.len(), 2);
    assert!(state.metadata.description().contains("max_entries"), "{}", state.metadata.description());
    assert!(log_messages(&state, LogEntryKind::Info).iter().any(|m| m.contains("max_entries")));
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// search
// ------------------------------------------------------------------------------------------------

#[tokio::test]
async fn search_ranked_example_1() -> Res {
    let env = fixture("search_ranked_example_1").await?;
    let view = view_of(
        &env,
        "-R-key/specs/issues/-/ns-search/catalog-t-t/field_extract/search-expiration~.safety~.~_status~ncolon~closed~nast~",
    )
    .await?;
    assert_eq!(texts(&view, "key"), vec!["specs/issues/A-EXPIRY.md"]);
    match column(&view, "score")[0] {
        FieldValue::Float(score) => assert!(score > 0.0),
        ref other => panic!("{other:?}"),
    }
    assert_eq!(texts(&view, "matched"), vec!["content"]);
    let excerpt = &texts(&view, "excerpt")[0];
    assert!(excerpt.contains("expiration safety"), "{excerpt}");
    assert!(!excerpt.contains("---") && !excerpt.contains("priority"), "{excerpt}");
    Ok(())
}

#[tokio::test]
async fn search_limit_reports_n_of_m() -> Res {
    let env = fixture("search_limit_reports_n_of_m").await?;
    let state =
        eval(&env, "-R-key/specs/issues/-/ns-search/catalog-t-t/search-expiration~.safety-t-f-1").await?;
    assert_eq!(state.value()?.as_record_view()?.len(), 1);
    assert_eq!(state.metadata.description(), "1 of 2 matches shown");
    assert!(log_messages(&state, LogEntryKind::Info).contains(&"1 of 2 matches shown".to_string()));
    let lazy = eval(&env, "-R/data/table.csv/-/ns-rec/to_record_source/ns-search/search-france-f-f-1").await?;
    assert_eq!(lazy.metadata.description(), "first 1 matches; more may exist");
    Ok(())
}

#[tokio::test]
async fn search_accepts_all_input_kinds() -> Res {
    let env = fixture("search_accepts_all_input_kinds").await?;
    for query in [
        "ns-search/search",
        "-R-key/specs/issues/-/ns-search/search",
        "-R-dir/specs/issues/-/ns-search/search",
    ] {
        assert!(view_of(&env, query).await?.len() > 0, "{query}");
    }
    for query in [
        "-R/data/table.csv/-/ns-rec/to_record/ns-search/search-france",
        "-R-key/data/table.csv/-/ns-search/search-france",
        "-R/data/table.csv/-/q/ns-search/search-france",
        "-R/data/table.csv/-/ns-search/search-france",
    ] {
        assert_eq!(view_of(&env, query).await?.len(), 2, "{query}");
    }
    let source = eval(&env, "-R/data/table.csv/-/ns-rec/to_record_source/ns-search/search-france").await?;
    assert_eq!(source.value()?.as_record_view()?.len(), 2, "ranked source → view");
    Ok(())
}

#[tokio::test]
async fn search_unranked_source_stays_source() -> Res {
    let env = fixture("search_unranked_source_stays_source").await?;
    let state = eval(&env, "-R/data/table.csv/-/ns-rec/to_record_source/ns-search/search-france-f").await?;
    assert_eq!(state.value()?.identifier(), "RecordSource");
    let rows = view_of(&env, "-R/data/table.csv/-/ns-rec/to_record_source/ns-search/search-france-f/ns-rec/materialize").await?;
    assert_eq!(rows.len(), 2);
    Ok(())
}

#[tokio::test]
async fn rank_over_large_source_advises_rank_false() -> Res {
    let env = fixture("rank_over_large_source_advises_rank_false").await?;
    let state = eval(&env, "ns-fixture/big_source/ns-search/search-x").await?;
    let error = state.value().expect_err("too large");
    assert!(error.to_string().contains("rank=false"), "{error}");
    Ok(())
}

#[tokio::test]
async fn search_malformed_expression_warns_and_searches_words() -> Res {
    let env = fixture("search_malformed_expression_warns_and_searches_words").await?;
    let state = eval(&env, "-R-key/specs/issues/-/ns-search/catalog-t-t/search-~nlpar~expiration").await?;
    let keys = texts(&state.value()?.as_record_view()?, "key");
    assert!(keys.contains(&"specs/issues/A-EXPIRY.md".to_string()), "{keys:?}");
    assert!(log_messages(&state, LogEntryKind::Warning).iter().any(|m| m.contains("unbalanced")));
    Ok(())
}

#[tokio::test]
async fn unknown_field_logs_warning() -> Res {
    let env = fixture("unknown_field_logs_warning").await?;
    let state = eval(&env, "-R-key/specs/issues/-/ns-search/catalog/search-nosuch~ncolon~x").await?;
    assert_eq!(state.value()?.as_record_view()?.len(), 0);
    assert!(log_messages(&state, LogEntryKind::Warning).iter().any(|m| m.contains("nosuch")));
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// commands
// ------------------------------------------------------------------------------------------------

/// The `store_config` realm (declared for configuration, never executed) is not catalogued.
#[tokio::test]
async fn command_catalog_omits_the_store_config_realm() -> Res {
    let env = fixture("command_catalog_omits_the_store_config_realm").await?;
    let all = view_of(&env, "ns-search/commands").await?;
    assert!(all.len() > 5);
    assert!(texts(&all, "realm").iter().all(|realm| realm != "store_config"));
    assert!(!texts(&all, "name").contains(&"front_matter".to_string()));
    Ok(())
}

#[tokio::test]
async fn commands_catalog_and_discovery() -> Res {
    let env = fixture("commands_catalog_and_discovery").await?;
    let found = texts(&view_of(&env, "ns-search/commands/search-record").await?, "query");
    for query in ["ns-rec/to_record", "ns-rec/to_record_source"] {
        assert!(found.contains(&query.to_string()), "{query} in {found:?}");
    }
    let by_name = texts(&view_of(&env, "ns-search/commands/search-catalog").await?, "query");
    assert!(by_name.contains(&"ns-search/catalog".to_string()), "{by_name:?}");
    let rec = view_of(&env, "ns-search/commands-rec").await?;
    assert!(rec.len() > 5);
    assert!(texts(&rec, "namespace").iter().all(|ns| ns == "rec"));
    let search = view_of(&env, "ns-search/commands-search").await?;
    let row = texts(&search, "name").iter().position(|n| n == "search").expect("search row");
    let signature = &texts(&search, "description")[row];
    assert!(signature.starts_with("ns-search/search(query: String = \"\""), "{signature}");
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// field_extract
// ------------------------------------------------------------------------------------------------

#[tokio::test]
async fn field_extract_reads_folder_directly() -> Res {
    let env = fixture("field_extract_reads_folder_directly").await?;
    let view = view_of(&env, "-R-key/specs/issues/-/ns-search/field_extract").await?;
    assert_eq!(
        texts(&view, "key"),
        vec![
            "specs/issues/A-EXPIRY.md",
            "specs/issues/B-CLOSED.md",
            "specs/issues/C-STORE.md",
            "specs/issues/D-TOML.md",
        ]
    );
    assert_eq!(texts(&view, "status"), vec!["draft", "closed", "draft", "accepted"]);
    assert_eq!(texts(&view, "created")[3], "2026-10-01T00:00:00Z");
    Ok(())
}

#[tokio::test]
async fn field_extract_from_catalog_records() -> Res {
    let env = fixture("field_extract_from_catalog_records").await?;
    let view = view_of(&env, "-R-key/specs/issues/-/ns-search/catalog-t-t/field_extract").await?;
    assert_eq!(view.len(), 4);
    assert!(view.schema().index_of("content").is_some());
    assert_eq!(texts(&view, "priority")[0], "P1");
    Ok(())
}

#[tokio::test]
async fn field_extract_json_and_yaml() -> Res {
    let env = fixture("field_extract_json_and_yaml").await?;
    let json = view_of(&env, "-R-key/data/-/ns-search/field_extract-json").await?;
    assert_eq!(texts(&json, "name"), vec!["demo"]);
    assert_eq!(column(&json, "size"), vec![FieldValue::Int(3)]);
    let yaml = view_of(&env, "-R-key/data/-/ns-search/field_extract-yaml").await?;
    assert_eq!(texts(&yaml, "mode"), vec!["fast"]);
    assert_eq!(column(&yaml, "level"), vec![FieldValue::Int(2)]);
    Ok(())
}

#[tokio::test]
async fn field_extract_prefix_and_flatten() -> Res {
    let env = fixture("field_extract_prefix_and_flatten").await?;
    let prefixed = view_of(&env, "-R-key/specs/issues/-/ns-search/field_extract-md-x_").await?;
    assert!(prefixed.schema().index_of("x_status").is_some());
    assert!(prefixed.schema().index_of("status").is_some(), "the catalog status stays");
    let flat = view_of(&env, "-R-key/specs/issues/-/ns-search/field_extract-md--t").await?;
    assert_eq!(column(&flat, "links_github")[0], FieldValue::Int(12));
    let nested = view_of(&env, "-R-key/specs/issues/-/ns-search/field_extract").await?;
    let links = nested.schema().index_of("links").expect("links");
    assert_eq!(nested.schema().fields[links].media_type.as_deref(), Some("application/json"));
    let found = view_of(
        &env,
        "-R-key/specs/issues/-/ns-search/field_extract/search-links.github~ncolon~12~.area~ncolon~core~/assets-f-f-0",
    )
    .await?;
    assert_eq!(texts(&found, "key"), vec!["specs/issues/A-EXPIRY.md"]);
    Ok(())
}

#[tokio::test]
async fn field_extract_replaces_columns_keeps_id() -> Res {
    let env = fixture("field_extract_replaces_columns_keeps_id").await?;
    let state = eval(&env, "-R-key/specs/issues/-/ns-search/catalog/field_extract").await?;
    let view = state.value()?.as_record_view()?;
    assert_eq!(texts(&view, "key")[0], "specs/issues/A-EXPIRY.md", "the Id is not replaced");
    assert_eq!(texts(&view, "status")[0], "draft");
    assert_eq!(texts(&view, "title")[0], "Expired assets are served stale");
    let title = view.schema().index_of("title").expect("title");
    assert!(view.schema().text_fields().contains(&title), "title keeps the text role");
    assert!(log_messages(&state, LogEntryKind::Warning).iter().any(|m| m.contains("'key'")));
    assert!(log_messages(&state, LogEntryKind::Info).iter().any(|m| m.contains("replaced columns")));
    Ok(())
}

#[tokio::test]
async fn field_extract_drops_files_without_requested_fields() -> Res {
    let env = fixture("field_extract_drops_files_without_requested_fields").await?;
    let state = eval(&env, "-R-key/specs/issues/-/ns-search/field_extract-md--f-priority").await?;
    let view = state.value()?.as_record_view()?;
    assert_eq!(texts(&view, "key"), vec!["specs/issues/A-EXPIRY.md"]);
    // B, C and D lack `priority`; the recipe-only `derived.md` is never read.
    assert!(log_messages(&state, LogEntryKind::Info).iter().any(|m| m.contains("4 file(s) without fields")));
    Ok(())
}

#[tokio::test]
async fn field_extract_name_collisions_and_large_integers() -> Res {
    let env = fixture("field_extract_name_collisions_and_large_integers").await?;
    let state = eval(&env, "-R-key/extra/-/ns-search/field_extract-json").await?;
    let view = state.value()?.as_record_view()?;
    assert_eq!(column(&view, "a_b"), vec![FieldValue::Int(1)], "the first in key order is kept");
    assert!(log_messages(&state, LogEntryKind::Warning).iter().any(|m| m.contains("a_b")));
    assert_eq!(column(&view, "big"), vec![FieldValue::UInt(u64::MAX)]);
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// md_split
// ------------------------------------------------------------------------------------------------

#[tokio::test]
async fn md_split_sections_and_lines() -> Res {
    let env = fixture("md_split_sections_and_lines").await?;
    let lines = view_of(&env, "-R-key/src/-/ns-search/md_split-line/search-~/fn~.get_asset_info~/-f-f-0-text").await?;
    assert_eq!(texts(&lines, "key"), vec!["src/lib.rs"]);
    assert_eq!(texts(&lines, "filename"), vec!["lib.rs"]);
    assert_eq!(column(&lines, "line"), vec![FieldValue::Int(1)]);
    assert_eq!(column(&lines, "n"), vec![FieldValue::Int(1)]);

    let sections = view_of(&env, "-R-key/specs/design/-/ns-search/md_split").await?;
    let headings = texts(&sections, "heading");
    assert!(headings.contains(&"Phase 1 > Purpose".to_string()), "{headings:?}");
    for name in ["id", "key", "filename", "n", "line", "end_line", "level", "text"] {
        assert!(sections.schema().index_of(name).is_some(), "{name}");
    }
    let everything = view_of(&env, "ns-search/md_split").await?;
    assert!(!texts(&everything, "key").contains(&"src/lib.rs".to_string()), "sections are .md only");
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Dependencies (specs/design/search-read-dependencies/)
// ------------------------------------------------------------------------------------------------

const C_STORE_CHANGED: &str = "---
id: C-STORE
title: Store listing
status: closed
area: [core/store]
---
# Store listing

Changed text, now about a wombat.
";

fn content_of(view: &Arc<dyn RecordView>, key: &str) -> String {
    texts(view, "content")[row_of(view, key)].clone()
}

/// AC-1
#[tokio::test]
async fn search_catalog_content_follows_a_rewrite() -> Res {
    let env = fixture("search_catalog_content_follows_a_rewrite").await?;
    let query = "-R-key/specs/issues/-/ns-search/catalog-t-t";
    let before = view_of(&env, query).await?;
    assert!(content_of(&before, "specs/issues/C-STORE.md").contains("Nothing to see here."));
    write(&env, "specs/issues/C-STORE.md", C_STORE_CHANGED).await?;
    let after = view_of(&env, query).await?;
    let content = content_of(&after, "specs/issues/C-STORE.md");
    assert!(content.contains("Changed text"), "{content}");
    Ok(())
}

/// AC-2, AC-3: a file added to the folder and to a subfolder; a file removed. (A file that
/// creates a new subfolder is not seen: `NEW-FOLDER-WRITE-DOES-NOT-REFRESH-THE-PARENT-LISTING`.)
#[tokio::test]
async fn search_catalog_follows_added_and_removed_files() -> Res {
    let env = fixture("search_catalog_follows_added_and_removed_files").await?;
    let query = "-R-key/specs/-/ns-search/catalog";
    let keys = texts(&view_of(&env, query).await?, "key");
    assert!(keys.contains(&"specs/issues/B-CLOSED.md".to_string()));
    for added in ["specs/issues/E-NEW.md", "specs/design/x/phase2.md"] {
        write(&env, added, "# New\n").await?;
        let keys = texts(&view_of(&env, query).await?, "key");
        assert!(keys.contains(&added.to_string()), "{added} in {keys:?}");
    }
    env.get_asset_manager().remove(&parse_key("specs/issues/B-CLOSED.md")?).await?;
    let keys = texts(&view_of(&env, query).await?, "key");
    assert!(!keys.contains(&"specs/issues/B-CLOSED.md".to_string()), "{keys:?}");
    Ok(())
}

/// AC-4: `updated` (the memory store records no `size`).
#[tokio::test]
async fn search_catalog_metadata_columns_follow_a_rewrite() -> Res {
    let env = fixture("search_catalog_metadata_columns_follow_a_rewrite").await?;
    let query = "-R-key/specs/issues/-/ns-search/catalog-t-f";
    // The fixture's files carry no `updated`; one written through Liquers does.
    write(&env, "specs/issues/C-STORE.md", C_STORE).await?;
    let before = view_of(&env, query).await?;
    let updated = column(&before, "updated")[row_of(&before, "specs/issues/C-STORE.md")].clone();
    assert!(matches!(updated, FieldValue::Timestamp(_)), "{updated:?}");
    // `updated` has millisecond resolution.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    write(&env, "specs/issues/C-STORE.md", C_STORE_CHANGED).await?;
    let after = view_of(&env, query).await?;
    assert_ne!(column(&after, "updated")[row_of(&after, "specs/issues/C-STORE.md")], updated);
    Ok(())
}

/// AC-5. `search` over a folder searches its catalog without content, so it is shown a new file
/// whose name matches.
#[tokio::test]
async fn search_document_commands_follow_a_rewrite() -> Res {
    let env = fixture("search_document_commands_follow_a_rewrite").await?;
    let extract = "-R-key/specs/issues/-/ns-search/field_extract";
    let split = "-R-key/specs/issues/-/ns-search/md_split";
    let search = "-R-key/specs/issues/-/ns-search/search-wombat";
    let fields = view_of(&env, extract).await?;
    assert_eq!(texts(&fields, "status")[row_of(&fields, "specs/issues/C-STORE.md")], "draft");
    assert!(!texts(&view_of(&env, split).await?, "text").iter().any(|t| t.contains("wombat")));
    assert_eq!(view_of(&env, search).await?.len(), 0);
    write(&env, "specs/issues/C-STORE.md", C_STORE_CHANGED).await?;
    write(&env, "specs/issues/wombat.md", "# Wombat\n").await?;
    let fields = view_of(&env, extract).await?;
    assert_eq!(texts(&fields, "status")[row_of(&fields, "specs/issues/C-STORE.md")], "closed");
    assert!(texts(&view_of(&env, split).await?, "text").iter().any(|t| t.contains("wombat")));
    assert_eq!(texts(&view_of(&env, search).await?, "key"), vec!["specs/issues/wombat.md"]);
    Ok(())
}

/// AC-6: the catalog prefix is a predecessor asset of its own.
#[tokio::test]
async fn search_over_a_cut_catalog_follows_a_rewrite() -> Res {
    let env = fixture("search_over_a_cut_catalog_follows_a_rewrite").await?;
    let query = "-R-key/specs/issues/-/ns-search/catalog-t-t/search-wombat";
    assert_eq!(view_of(&env, query).await?.len(), 0);
    write(&env, "specs/issues/C-STORE.md", C_STORE_CHANGED).await?;
    assert_eq!(texts(&view_of(&env, query).await?, "key"), vec!["specs/issues/C-STORE.md"]);
    Ok(())
}

/// AC-7
#[tokio::test]
async fn search_root_catalog_follows_a_root_write() -> Res {
    let env = fixture("search_root_catalog_follows_a_root_write").await?;
    let query = "ns-search/catalog-f";
    assert!(!texts(&view_of(&env, query).await?, "key").contains(&"ROOT.md".to_string()));
    write(&env, "ROOT.md", "# Root\n").await?;
    let keys = texts(&view_of(&env, query).await?, "key");
    assert!(keys.contains(&"ROOT.md".to_string()), "{keys:?}");
    Ok(())
}

/// AC-8: a recipe key whose query is the catalog.
#[tokio::test]
async fn search_keyed_recipe_follows_a_rewrite() -> Res {
    let env = fixture("search_keyed_recipe_follows_a_rewrite").await?;
    env.get_async_store()
        .set(
            &parse_key("index/recipes.yaml")?,
            b"recipes:\n  - query: \"-R-key/specs/issues/-/ns-search/catalog-t-t/issues.csv\"\n",
            &Metadata::new(),
        )
        .await?;
    let key = parse_key("index/issues.csv")?;
    let state = env.get_asset_manager().get(&key).await?.get().await?;
    let view = state.value()?.as_record_view()?;
    assert!(content_of(&view, "specs/issues/C-STORE.md").contains("Nothing to see here."));
    let dependencies = dependencies_of(&state);
    assert!(dependencies.contains(&"-R/specs/issues/C-STORE.md".to_string()), "{dependencies:?}");
    write(&env, "specs/issues/C-STORE.md", C_STORE_CHANGED).await?;
    let state = env.get_asset_manager().get(&key).await?.get().await?;
    let view = state.value()?.as_record_view()?;
    let content = content_of(&view, "specs/issues/C-STORE.md");
    assert!(content.contains("Changed text"), "{content}");
    Ok(())
}

/// AC-9: the same asset while nothing under the folder changes, a write elsewhere included.
#[tokio::test]
async fn search_catalog_is_reused_while_nothing_changes() -> Res {
    let env = fixture("search_catalog_is_reused_while_nothing_changes").await?;
    let query = "-R-key/specs/issues/-/ns-search/catalog-t-t";
    let first = env.evaluate(query).await?;
    first.get().await?;
    let second = env.evaluate(query).await?;
    assert_eq!(first.id(), second.id());
    write(&env, "data/elsewhere.txt", "unrelated").await?;
    let third = env.evaluate(query).await?;
    assert_eq!(first.id(), third.id(), "a write outside the folder expires nothing");
    Ok(())
}

/// AC-11
#[tokio::test]
async fn search_catalog_records_listing_and_file_dependencies() -> Res {
    let env = fixture("search_catalog_records_listing_and_file_dependencies").await?;
    let state = eval(&env, "-R-key/specs/-/ns-search/catalog-t-t").await?;
    let dependencies = dependencies_of(&state);
    for expected in [
        "-R-dir/specs",
        "-R-dir/specs/issues",
        "-R-dir/specs/design",
        "-R-dir/specs/design/x",
        "-R/specs/issues/A-EXPIRY.md",
        "-R/specs/design/x/DESIGN.md",
    ] {
        assert!(dependencies.contains(&expected.to_string()), "{expected} in {dependencies:?}");
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// guides/SEARCH_GUIDE.md
// ------------------------------------------------------------------------------------------------

/// Every query in a bare (no language) fenced block of the guide, one per line.
fn guide_queries() -> Vec<String> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../specs/guides/SEARCH_GUIDE.md");
    let guide = std::fs::read_to_string(path).expect("SEARCH_GUIDE.md");
    let mut queries = Vec::new();
    let mut in_block = None;
    for line in guide.lines() {
        match (in_block, line.trim_end()) {
            (None, fence) if fence.starts_with("```") => in_block = Some(fence == "```"),
            (Some(_), "```") => in_block = None,
            (Some(true), query) if !query.trim().is_empty() => queries.push(query.trim().to_string()),
            (Some(_), _) | (None, _) => {}
        }
    }
    queries
}

#[tokio::test]
async fn guide_queries_run() -> Res {
    let queries = guide_queries();
    assert!(queries.len() >= 10, "{queries:?}");
    let env = fixture("guide_queries_run").await?;
    for query in &queries {
        let state = eval(&env, query).await.unwrap_or_else(|e| panic!("{query}: {e}"));
        let value = state.value().unwrap_or_else(|e| panic!("{query}: {e}"));
        if let Ok(view) = value.as_record_view() {
            assert!(view.len() > 0, "{query}: no rows");
        }
    }
    Ok(())
}
