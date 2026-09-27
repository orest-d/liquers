# Phase 3: Examples & Testing - axum-assets-endpoints

## Example Type

**Runnable prototypes** (user choice). Every code block below is real Rust that Phase 4 pastes
verbatim into one of three files:

- `liquers-core/tests/asset_manager_remove_expire_describe.rs` — new integration test, prefix `AMR`
- `liquers-axum/tests/assets_api_endpoints.rs` — new integration test, prefix `AAE`
- `liquers-axum/src/assets/value_description.rs`, `#[cfg(test)] mod tests` — new inline unit tests, prefix `VD`

Each target file gets **one shared helper module** (`env_with`, `metadata_text`, HTTP/JSON
helpers) instead of a copy per test group. IDs are contiguous within each prefix. Drafts that
duplicated the same behaviour under different names were merged into one test.

## Overview Table

| ID | Type | Name | Demonstrates / checks | Target file |
|---|---|---|---|---|
| Example 1 | Scenario | Agent-memory primary flow over HTTP | write → browse → recipe read → describe → overwrite → cascade, through the router | AAE (assets_api_endpoints.rs) |
| Example 2 | Scenario | Removal decision table | all six rows of Phase 2's `remove` table, against `AssetManager` directly | AMR (asset_manager_remove_expire_describe.rs) |
| Example 3 | Scenario | Metadata allow-list under `POST entry` | five hostile manager-owned fields dropped and named; two malformed-input 400s; `POST metadata` 501 | AAE (assets_api_endpoints.rs) |
| AMR01 | Unit/Integration | `remove` — Source, no recipe, cascades | any-status-without-recipe row; dependent cascades to Expired | asset_manager_remove_expire_describe.rs |
| AMR02 | Unit/Integration | `remove` — Override with recipe, recomputes | Source/Override-with-recipe row; next read recomputes | asset_manager_remove_expire_describe.rs |
| AMR03 | Unit/Integration | `remove` — Ready computed value keeps metadata/version | recipe-computed row; store keeps `Recipe` status + version; no cascade | asset_manager_remove_expire_describe.rs |
| AMR04 | Unit/Integration | Re-evaluate after AMR03-style remove, no cascade | second-order confirmation that the drop did not cascade | asset_manager_remove_expire_describe.rs |
| AMR05 | Unit/Integration | `remove` — absent key, no recipe → `KeyNotFound` | nothing-live-nothing-stored, no recipe row | asset_manager_remove_expire_describe.rs |
| AMR06 | Unit/Integration | `remove` — never-evaluated recipe key → `Ok(())` | None/Recipe-with-recipe row | asset_manager_remove_expire_describe.rs |
| AMR07 | Unit/Integration | `remove` — `Directory` → `StatusConflict` (409) | directory row, refused | asset_manager_remove_expire_describe.rs |
| AMR10 | Unit/Integration | `expire` — live `Ready` cascades to dependents | expire cascades through the live asset | asset_manager_remove_expire_describe.rs |
| AMR11 | Unit/Integration | `expire` — cascades through a two-level dependency chain | multi-level cascade | asset_manager_remove_expire_describe.rs |
| AMR12 | Unit/Integration | `expire` — idempotent on an already-`Expired` asset | idempotence | asset_manager_remove_expire_describe.rs |
| AMR13 | Unit/Integration | `expire` — `Source` (no recipe) → `StatusConflict` | no recipe to recover from | asset_manager_remove_expire_describe.rs |
| AMR14 | Unit/Integration | `expire` — never-evaluated recipe key → `StatusConflict` | `Recipe` status cannot expire | asset_manager_remove_expire_describe.rs |
| AMR15 | Unit/Integration | `expire` — absent key → `KeyNotFound` | no live/stored/recipe | asset_manager_remove_expire_describe.rs |
| AMR16 | Unit | `Error::status_conflict` constructor shape | type, key (`Some(key.encode())`), message wording | asset_manager_remove_expire_describe.rs |
| AMR17 | Unit/Integration | `set_description` on a `Source` updates title/description, version unchanged | happy path | asset_manager_remove_expire_describe.rs |
| AMR18 | Unit/Integration | `set_description` — both `None` → `ParameterError` | validation | asset_manager_remove_expire_describe.rs |
| AMR19 | Unit/Integration | `set_description` on computed `Ready` → `StatusConflict` | only `Source` may be described | asset_manager_remove_expire_describe.rs |
| AMR20 | Unit/Integration | `set_description` on absent key → `KeyNotFound` | absent-key path | asset_manager_remove_expire_describe.rs |
| AMR22 | Unit/Integration | `get_asset_info` on an `Expired` live asset reports `Expired`, no re-evaluation | Phase 2's `get_asset_info` fix | asset_manager_remove_expire_describe.rs |
| AMR23 | Unit/Integration | `get_asset_info` on a never-evaluated recipe key reports `Recipe`, no evaluation | same fix, un-evaluated case | asset_manager_remove_expire_describe.rs |
| AMR24 | Integration | a dropped intermediate does not block its dependent's fast track after a restart | the kept version is worth something (final review) | asset_manager_remove_expire_describe.rs |
| VD01 | Unit | `from_json` keeps exactly the five allow-listed fields | allow-list construction | value_description.rs |
| VD02 | Unit | `from_json` on non-object JSON → `ParameterError` | input validation | value_description.rs |
| VD03 | Unit | `from_json` on a non-string field value → `ParameterError` | input validation | value_description.rs |
| VD04 | Unit | `from_json` reports dropped fields, sorted | reporting contract | value_description.rs |
| VD05 | Unit | `from_params` parses query parameters, drops unknown ones | `POST data` parameter path | value_description.rs |
| VD06 | Unit | `or_previous` fills missing fields from `AssetInfo`, never `media_type` | fill-from-previous rule | value_description.rs |
| VD07 | Unit | `into_metadata_record` defaults `type_identifier` to `"Bytes"` | default type | value_description.rs |
| VD08 | Unit | `into_metadata_record` resolves `type_name` from the registry | registry lookup | value_description.rs |
| VD09 | Unit | `into_metadata_record` on an unknown identifier → `ParameterError` | 400 at the boundary, not 500 | value_description.rs |
| VD10 | Unit | `into_metadata_record` — untouched `MetadataRecord` fields carry their real defaults | verified against `MetadataRecord::new()`/`Default` (see Fixes) | value_description.rs |
| VD11 | Unit | `or_previous` — type and format filled only as a pair, from a data-bearing previous | recipe keys and client-named types (final review) | value_description.rs |
| AAE01 | Integration | = Example 1 | see above | assets_api_endpoints.rs |
| AAE02 | Integration | `POST data` onto a recipe key → `Override`; `DELETE` → `Recipe` | Q11 at the HTTP level | assets_api_endpoints.rs |
| AAE03 | Integration | `GET metadata` of a Source written over HTTP | existing read, manager-owned record | assets_api_endpoints.rs |
| AAE04 | Integration | `GET entry` with `Accept: application/json` | Accept negotiation (Phase 2 drive-by fix) | assets_api_endpoints.rs |
| AAE05 | Integration | `POST cancel` on an existing asset → 200 | cancel happy path | assets_api_endpoints.rs |
| AAE10–AAE17 | Integration | = Example 3 (AAE10 hostile `status`, AAE11 hostile `stored`, AAE12 hostile `dependencies`, AAE13 hostile `expiration_time`, AAE14 unknown `type_identifier` 400, AAE15 non-object `metadata` 400, AAE16 `POST metadata` 501, AAE17 round-trip GET after hostile POST reads back only allow-listed fields) | see above | assets_api_endpoints.rs |
| AAE20 | Integration | `GET listdir` (root) returns `{assets: [AssetInfo…]}` | listing shape | assets_api_endpoints.rs |
| AAE21 | Integration | `GET listdir?deep=true` returns `{keys: [...]}` | deep-listing shape | assets_api_endpoints.rs |
| AAE22 | Integration | `GET info/{key}` 404 when absent | `KeyNotFound` envelope | assets_api_endpoints.rs |
| AAE23 | Integration | `GET contains/{key}` false for absent key | no evaluation, no error | assets_api_endpoints.rs |
| AAE24 | Integration | `GET contains/{key}` true for present key | positive case | assets_api_endpoints.rs |
| AAE25 | Integration | `GET version/{key}` null for unversioned | `Ok(None)` distinct from error | assets_api_endpoints.rs |
| AAE26 | Integration | `GET version/{key}` 32-hex-digit string for stored | `Version` serde form | assets_api_endpoints.rs |
| AAE27 | Integration | `GET recover/{key}` 404 when there is no data-bearing state | recovery-read refusal | assets_api_endpoints.rs |
| AAE28 | Integration | `GET recover/{key}` recovers an `Expired` value's last-known data | recovery read purpose | assets_api_endpoints.rs |
| AAE29 | Integration | `GET recover/{key}` honours `Accept: application/json` | format negotiation, real headers (unlike `get_entry_handler` before the drive-by fix) | assets_api_endpoints.rs |
| AAE30 | Integration | `POST override/{key}` 200 with `AssetInfo` when data exists | pin current value | assets_api_endpoints.rs |
| AAE31 | Integration | `POST override/{key}` 404 when there is no data | `to_override`'s `key_not_found` | assets_api_endpoints.rs |
| AAE32 | Integration | `POST expire/{key}` 409 on a `Source` | no recipe to recover from | assets_api_endpoints.rs |
| AAE33 | Integration | `POST expire/{key}` 200 on a computed `Ready` value | happy path | assets_api_endpoints.rs |
| AAE34 | Integration | `PUT makedir/{key}` 201 with directory `AssetInfo` | directory creation | assets_api_endpoints.rs |
| AAE35 | Integration | `POST audit/{key}` result has `checked`/`expired` arrays | per-key audit shape | assets_api_endpoints.rs |
| AAE36 | Integration | `POST audit` (all) result has `checked`/`expired` arrays | whole-manager audit shape | assets_api_endpoints.rs |
| AAE37 | Integration | `POST refresh_command_versions` 200, null result, non-empty message | manager-wide op shape | assets_api_endpoints.rs |
| AAE40 | Integration | `DELETE data/{key}` 200, `new_status: "Recipe"` for a computed value | remove response shape (recipe-computed row) | assets_api_endpoints.rs |
| AAE41 | Integration | `DELETE data/{key}` 200, `new_status: "None"` for a `Source` | remove response shape (source row) | assets_api_endpoints.rs |
| AAE42 | Integration | `DELETE data/{key}` 409 on a `Directory` | directory row, refused over HTTP | assets_api_endpoints.rs |
| AAE43 | Integration | `DELETE entry/{key}` delegates to the same result as `DELETE data/{key}` | route aliasing | assets_api_endpoints.rs |
| AAE44 | Integration | `POST data/{non-key-query}` 501 `NotSupported` | key-only guard on a real action query | assets_api_endpoints.rs |
| AAE45 | Integration | `DELETE data/{non-key-query}` 501 `NotSupported` | same guard, delete side | assets_api_endpoints.rs |
| AAE50 | Integration | `.read_only()` — `POST data` on `data/{key}` → 405 (GET still serves that path) | builder switch, correct HTTP code (Fixes §4) | assets_api_endpoints.rs |
| AAE51 | Integration | `.read_only()` — `GET data/{key}` still 200 | mutation-only switch | assets_api_endpoints.rs |
| AAE52 | Integration | `.read_only()` — `POST expire/{key}` → 404 (no other method on that path) | omitted route with no sibling method (Fixes §4) | assets_api_endpoints.rs |
| AAE53 | Integration | `.read_only()` — `POST cancel/{key}` still routed (not 404/405) | `cancel` survives `read_only` | assets_api_endpoints.rs |
| AAE54 | Integration | `.with_admin(false)` — `POST audit` → 404 | admin-only switch, no sibling method | assets_api_endpoints.rs |
| AAE55 | Integration | `.with_admin(false)` — `POST data/{key}` still 201 | admin switch does not touch key routes | assets_api_endpoints.rs |
| AAE60 | Integration | Two concurrent `POST data` to the same key: both succeed, final store value is exactly one of the two bodies | key-mutation-lock serialization observed from the HTTP layer | assets_api_endpoints.rs |

Test count per file: **asset_manager_remove_expire_describe.rs — 21** (AMR01–AMR07, AMR10–AMR20 minus AMR21 dropped, AMR22–AMR24; AMR21 duplicated AMR17's version-unchanged assertion and was merged into it), **value_description.rs — 11** (VD01–VD11), **assets_api_endpoints.rs — 44** (Example 1 as AAE01; review additions AAE02–AAE05; Example 3 as AAE10–AAE17; AAE20–AAE60 corner/integration cases).

---

## Example 1: Agent-Memory Primary Flow over HTTP

### Scenario

The agent-memory MVP's core loop, entirely through the router: write a note, browse the directory
without evaluating anything, inspect a not-yet-evaluated recipe, read the derived value (which
evaluates it), edit the note's description (metadata only, version unchanged), overwrite the note
(version changes), and see the dependent recomputed.

### Context

One recipe: `-R/notes/a.txt/-/upper/summary.txt` — reads `notes/a.txt` (a `Source`, no recipe of
its own), applies `upper`, stores the result under key `summary.txt`.

### Code

```rust
// liquers-axum/tests/assets_api_endpoints.rs

use std::collections::HashMap;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use liquers_core::{
    assets::AssetManager, // trait in scope for am.get/set_binary/remove/… (final review)
    command_metadata::CommandKey,
    context::{Environment, EnvRef, SimpleEnvironment},
    error::ErrorType,
    metadata::{Metadata, MetadataRecord, Status},
    parse::parse_key,
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};
use tower::ServiceExt; // oneshot

// ---------------------------------------------------------------------------
// Shared helpers for this file (used by AAE01-AAE05, AAE10-AAE17, AAE20-AAE60)
// ---------------------------------------------------------------------------

/// Environment with recipes read from `recipes.yaml` in the store root, and two test commands:
/// `make_text` (always returns `"generated"`) and `upper` (uppercases its string input).
async fn env_with(recipes: &[(&str, &str, &str)]) -> EnvRef<SimpleEnvironment<Value>> {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("make_text"), |_, _, _| {
            Ok(Value::from("generated"))
        })
        .unwrap();
    env.command_registry
        .register_command(CommandKey::new_name("upper"), |state: &State<Value>, _, _| {
            Ok(Value::from(state.try_into_string()?.to_uppercase()))
        })
        .unwrap();
    let mut rl = RecipeList::new();
    for (q, t, d) in recipes {
        rl.add_recipe(Recipe::new(q.to_string(), t.to_string(), d.to_string()).unwrap());
    }
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("recipes.yaml").unwrap(),
            serde_yaml::to_string(&rl).unwrap().as_bytes(),
            &Metadata::new(),
        )
        .await
        .unwrap();
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    env.to_ref()
}

/// A minimal `MetadataRecord` for a plain-text Source write via `AssetManager::set_binary`
/// directly (as opposed to through the router). `type_identifier`/`type_name` are `String`,
/// not `Option<String>` (verified against `liquers-core/src/metadata.rs`).
fn metadata_text() -> MetadataRecord {
    MetadataRecord {
        type_identifier: "Text".to_string(),
        type_name: "text".to_string(),
        data_format: Some("txt".to_string()),
        ..Default::default()
    }
}

fn build_app(envref: EnvRef<SimpleEnvironment<Value>>) -> axum::Router {
    liquers_axum::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .build()
        .with_state(envref)
}

/// Send a request, return (status, parsed JSON body).
async fn send(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Body,
) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(Request::builder().method(method).uri(uri).body(body).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// Same as `send`, but for a raw (non-JSON, non-enveloped) body, e.g. `GET data`.
async fn send_raw(app: axum::Router, method: &str, uri: &str, body: Body) -> (StatusCode, Vec<u8>) {
    let resp = app
        .oneshot(Request::builder().method(method).uri(uri).body(body).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, bytes.to_vec())
}

/// Like `send`, with `Content-Type: application/json` — required by handlers that use axum's
/// `Json` extractor (`POST description`); without it axum answers 415 before the handler runs.
async fn send_json(app: axum::Router, method: &str, uri: &str, json: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(json.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn aae01_agent_memory_primary_flow() {
    let envref = env_with(&[(
        "-R/notes/a.txt/-/upper/summary.txt",
        "Summary",
        "Upper-cased note",
    )])
    .await;
    let app = build_app(envref.clone());

    // 1. Write a note.
    let (status, json) = send(
        app.clone(),
        "POST",
        "/api/assets/data/-R/notes/a.txt?type_identifier=Text&data_format=txt&title=Note%20A&description=First%20note",
        Body::from("hello"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "POST data should return 201");
    assert_eq!(json["status"], "OK");
    assert_eq!(json["result"]["key"], "notes/a.txt");
    assert_eq!(json["result"]["status"], "Source");
    assert_eq!(json["result"]["title"], "Note A");
    assert_eq!(json["result"]["description"], "First note");
    assert_eq!(json["result"]["type_identifier"], "Text");

    // 2. Browse the directory — title/description visible, no evaluation triggered.
    let (status, json) = send(app.clone(), "GET", "/api/assets/listdir/-R/notes", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    let assets = json["result"]["assets"].as_array().unwrap();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0]["key"], "notes/a.txt");
    assert_eq!(assets[0]["title"], "Note A");
    assert_eq!(assets[0]["status"], "Source", "listing must not evaluate anything");

    // 3. Inspect the recipe key before it is ever evaluated.
    let (status, json) = send(app.clone(), "GET", "/api/assets/info/-R/summary.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["status"], "Recipe");
    assert_eq!(json["result"]["title"], "Summary");

    // 4. Read the derived value — this evaluates the recipe.
    let (status, body) = send_raw(app.clone(), "GET", "/api/assets/data/-R/summary.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(String::from_utf8(body).unwrap(), "HELLO");

    // 5. Edit the note's description — title unchanged, status still Source.
    let (status, json) = send_json(
        app.clone(),
        "POST",
        "/api/assets/description/-R/notes/a.txt",
        r#"{"description":"Edited"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["description"], "Edited");
    assert_eq!(json["result"]["title"], "Note A");
    assert_eq!(json["result"]["status"], "Source");

    let (_, json) = send(app.clone(), "GET", "/api/assets/version/-R/notes/a.txt", Body::empty()).await;
    let version_before = json["result"]["version"].as_str().map(|s| s.to_string());
    assert!(version_before.is_some(), "version should exist after the first write");

    // 6. Overwrite the note's data — version changes.
    let (status, json) = send(
        app.clone(),
        "POST",
        "/api/assets/data/-R/notes/a.txt",
        Body::from("world"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["result"]["status"], "Source");

    let (_, json) = send(app.clone(), "GET", "/api/assets/version/-R/notes/a.txt", Body::empty()).await;
    let version_after = json["result"]["version"].as_str().map(|s| s.to_string());
    assert_ne!(version_before, version_after, "version must change after a data overwrite");

    // 7. The dependent recipe recomputes on the next read.
    let (status, body) = send_raw(app.clone(), "GET", "/api/assets/data/-R/summary.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(String::from_utf8(body).unwrap(), "WORLD");
}
```

### Expected output

Every step's HTTP status and JSON assertions are inline above; the flow ends with `summary.txt`
serving `"WORLD"`, proving the cascade fired from the second `POST data`.

---

## Example 2: Removal Decision Table

### Scenario

Six runnable tests, one per row of Phase 2's `remove` decision table, driving `AssetManager`
directly (no HTTP). Two recipes are shared: `make_text/source.txt` (no dependency) and
`-R/notes/a.txt/-/upper/summary.txt` (depends on `notes/a.txt`).

### Context

`env_with` and `metadata_text` here are the **same helpers as Example 1**, restated because this
file is `liquers-core/tests/asset_manager_remove_expire_describe.rs` — a different crate, so the
helper module is not shared across files, only within one.

### Code

```rust
// liquers-core/tests/asset_manager_remove_expire_describe.rs

use liquers_core::{
    assets::{AssetData, AssetManager}, // AssetManager: trait methods; AssetData: AMR24
    command_metadata::CommandKey,
    context::{Environment, EnvRef, SimpleEnvironment},
    error::ErrorType,
    metadata::{Metadata, MetadataRecord, Status},
    parse::parse_key,
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};

// ---------------------------------------------------------------------------
// Shared helpers for this file (used by every AMR test)
// ---------------------------------------------------------------------------

/// An environment over an already-populated store (AMR24 rebuilds one over persisted entries to
/// simulate a restart).
fn env_over(store: AsyncMemoryStore) -> EnvRef<SimpleEnvironment<Value>> {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("make_text"), |_, _, _| {
            Ok(Value::from("generated"))
        })
        .unwrap();
    env.command_registry
        .register_command(CommandKey::new_name("upper"), |state: &State<Value>, _, _| {
            Ok(Value::from(state.try_into_string()?.to_uppercase()))
        })
        .unwrap();
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    env.to_ref()
}

async fn env_with(recipes: &[(&str, &str, &str)]) -> EnvRef<SimpleEnvironment<Value>> {
    let mut rl = RecipeList::new();
    for (q, t, d) in recipes {
        rl.add_recipe(Recipe::new(q.to_string(), t.to_string(), d.to_string()).unwrap());
    }
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("recipes.yaml").unwrap(),
            serde_yaml::to_string(&rl).unwrap().as_bytes(),
            &Metadata::new(),
        )
        .await
        .unwrap();
    env_over(store)
}

/// `type_identifier`/`type_name` are `String` (not `Option<String>`) on `MetadataRecord` —
/// verified against `liquers-core/src/metadata.rs`. `AssetManager::set_binary` takes the
/// record **by value**, not by reference.
fn metadata_text() -> MetadataRecord {
    MetadataRecord {
        type_identifier: "Text".to_string(),
        type_name: "text".to_string(),
        data_format: Some("txt".to_string()),
        ..Default::default()
    }
}

/// `AsyncStore::get_metadata` returns `Metadata` (an enum over `LegacyMetadata`/`MetadataRecord`),
/// whose status is read through the `.status()` method, not a `.status` field.
fn stored_status(metadata: &Metadata) -> Status {
    metadata.status()
}

#[tokio::test]
async fn amr01_remove_source_no_recipe_cascades() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[(
        "-R/notes/a.txt/-/upper/summary.txt",
        "Summary Recipe",
        "Uppercase the note",
    )])
    .await;
    let am = envref.get_asset_manager();
    let store = envref.get_async_store();

    let notes_key = parse_key("notes/a.txt")?;
    am.set_binary(&notes_key, b"hello world", metadata_text()).await?;
    assert_eq!(am.get_asset_info(&notes_key).await?.status, Status::Source);

    // Evaluate the dependent so it is live and Ready before the source is removed.
    let summary_key = parse_key("summary.txt")?;
    let value = am.get(&summary_key).await?.get().await?;
    assert_eq!(value.try_into_string()?, "HELLO WORLD");
    assert_eq!(am.get_asset_info(&summary_key).await?.status, Status::Ready);

    am.remove(&notes_key).await?;

    assert!(!store.contains(&notes_key).await?, "store must not hold the removed source");
    let err = am.get_asset_info(&notes_key).await.expect_err("removed source is gone");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);

    // The dependent, having no recipe-with-value of its own to fall back on the same way,
    // must be cascade-expired.
    assert_eq!(
        am.get_asset_info(&summary_key).await?.status,
        Status::Expired,
        "dependent must cascade to Expired when its source Source value is removed"
    );
    Ok(())
}

#[tokio::test]
async fn amr02_remove_override_with_recipe_recomputes() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/source.txt", "Text Source", "A text value")]).await;
    let am = envref.get_asset_manager();
    let source_key = parse_key("source.txt")?;

    am.set_binary(&source_key, b"my custom text", metadata_text()).await?;
    assert_eq!(am.get_asset_info(&source_key).await?.status, Status::Override);

    am.remove(&source_key).await?;

    // Next read recomputes from the recipe.
    let value = am.get(&source_key).await?.get().await?;
    assert_eq!(value.try_into_string()?, "generated");
    assert_eq!(am.get_asset_info(&source_key).await?.status, Status::Ready);
    Ok(())
}

#[tokio::test]
async fn amr03_remove_ready_keeps_metadata_and_version() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[(
        "-R/notes/a.txt/-/upper/summary.txt",
        "Summary Recipe",
        "Uppercase the note",
    )])
    .await;
    let am = envref.get_asset_manager();
    let store = envref.get_async_store();

    let notes_key = parse_key("notes/a.txt")?;
    am.set_binary(&notes_key, b"hello", metadata_text()).await?;

    let summary_key = parse_key("summary.txt")?;
    let value = am.get(&summary_key).await?.get().await?;
    assert_eq!(value.try_into_string()?, "HELLO");
    assert_eq!(am.get_asset_info(&summary_key).await?.status, Status::Ready);
    let version_before = am.version(&summary_key).await?;
    assert!(version_before.is_some());

    am.remove(&summary_key).await?;

    assert!(store.contains(&summary_key).await?, "metadata must survive the remove");
    let stored = store.get_metadata(&summary_key).await?;
    assert_eq!(stored_status(&stored), Status::Recipe);
    assert_eq!(am.version(&summary_key).await?, version_before, "version must survive the remove");
    assert_eq!(
        am.get_asset_info(&notes_key).await?.status,
        Status::Source,
        "the source must be untouched: this remove does not cascade"
    );
    Ok(())
}

#[tokio::test]
async fn amr04_reevaluate_after_remove_ready_no_cascade() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[
        ("-R/notes/a.txt/-/upper/summary.txt", "Summary", "depends on notes"),
        ("-R/summary.txt/-/upper/summary2.txt", "Summary2", "depends on summary"),
    ])
    .await;
    let am = envref.get_asset_manager();

    let notes_key = parse_key("notes/a.txt")?;
    am.set_binary(&notes_key, b"hello", metadata_text()).await?;

    let summary_key = parse_key("summary.txt")?;
    let summary2_key = parse_key("summary2.txt")?;
    let _ = am.get(&summary_key).await?.get().await?;
    let value2 = am.get(&summary2_key).await?.get().await?;
    assert_eq!(value2.try_into_string()?, "HELLO");
    assert_eq!(am.get_asset_info(&summary2_key).await?.status, Status::Ready);

    // Drop the intermediate computed value (AMR03's row) …
    am.remove(&summary_key).await?;
    // … and re-read it: recomputes to the same content, no cascade fired for the drop itself.
    let value_again = am.get(&summary_key).await?.get().await?;
    assert_eq!(value_again.try_into_string()?, "HELLO");

    assert_eq!(
        am.get_asset_info(&summary2_key).await?.status,
        Status::Ready,
        "the grandchild must stay Ready: dropping a recipe-computed value does not cascade"
    );
    Ok(())
}

#[tokio::test]
async fn amr05_remove_nonexistent_key_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("phantom/file.txt")?;

    let err = am.remove(&key).await.expect_err("no value, no recipe");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);
    Ok(())
}

#[tokio::test]
async fn amr06_remove_recipe_key_not_evaluated() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/source.txt", "Text Source", "A text value")]).await;
    let am = envref.get_asset_manager();
    let store = envref.get_async_store();
    let source_key = parse_key("source.txt")?;

    // Nothing live, nothing stored, but a recipe exists: remove is a no-op success.
    am.remove(&source_key).await.expect("remove of an un-evaluated recipe key succeeds");
    assert!(!store.contains(&source_key).await?);
    Ok(())
}

#[tokio::test]
async fn amr07_remove_directory_status_conflict() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let dir_key = parse_key("a_directory")?;
    am.makedir(&dir_key).await?;

    let err = am.remove(&dir_key).await.expect_err("removing a directory is refused");
    assert_eq!(err.error_type, ErrorType::StatusConflict);
    assert_eq!(err.key, Some(dir_key.encode()), "status_conflict sets the key (Error.key is a String)");
    Ok(())
}
```

### Expected output

Each test's `assert_eq!`/`expect_err` calls are the expected output; there is no separate
transcript for a manager-level test.

---

## Example 3: Metadata Allow-List under `POST entry`

### Scenario

A client tries to round-trip five manager-owned fields through `POST entry` (JSON format), plus
two malformed inputs, plus the always-refused `POST metadata`. Each hostile field must have zero
effect and be named in the response `message`; the two malformed inputs return 400 with
`ParameterError`; `POST metadata` always answers 501 `NotSupported`.

### Context

Shares `env_with` and `build_app` with Example 1 (same file, `assets_api_endpoints.rs`). Adds one
helper, `post_entry_json`, for the JSON `DataEntry` envelope, and reuses `send`/`send_raw`.

### Code

```rust
// continues liquers-axum/tests/assets_api_endpoints.rs — same file as Example 1, so `env_with`,
// `build_app`, `send`, `send_raw` are already in scope.

/// POST a JSON `DataEntry` (`?format=json`) to `entry/{path}` and return (status, parsed body).
async fn post_entry_json(
    app: axum::Router,
    path: &str,
    entry: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    send(
        app,
        "POST",
        &format!("/api/assets/entry/-R/{}?format=json", path),
        Body::from(serde_json::to_string(&entry).unwrap()),
    )
    .await
}

#[tokio::test]
async fn aae10_hostile_status_is_ignored() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());

    // `data` is base64 per the DataEntry JSON codec.
    use base64::prelude::*;
    let entry = serde_json::json!({
        "metadata": {
            "type_identifier": "Text",
            "title": "Test Note",
            "status": "Error" // hostile: try to make the store empty and status Error
        },
        "data": BASE64_STANDARD.encode(b"hello from test")
    });

    let (status, resp) = post_entry_json(app.clone(), "notes/a.txt", entry).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(resp["status"], "OK");
    assert!(
        resp["message"].as_str().unwrap().contains("status"),
        "message must name the dropped 'status' field"
    );

    let (status, resp) = send(app, "GET", "/api/assets/info/-R/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["result"]["status"], "Source", "status must be the real Source, not Error");
}

#[tokio::test]
async fn aae11_hostile_stored_false_is_ignored() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());

    use base64::prelude::*;
    let entry = serde_json::json!({
        "metadata": {"type_identifier": "Text", "stored": false},
        "data": BASE64_STANDARD.encode(b"should be stored despite tampering")
    });

    let (status, resp) = post_entry_json(app.clone(), "notes/c.txt", entry).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(resp["message"].as_str().unwrap().contains("stored"));

    let (status, resp) = send(app, "GET", "/api/assets/contains/-R/notes/c.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["result"]["contains"], true, "the manager always persists a user-supplied value");
}

#[tokio::test]
async fn aae12_hostile_dependencies_are_ignored() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());

    use base64::prelude::*;
    let entry = serde_json::json!({
        "metadata": {
            "type_identifier": "Text",
            "title": "Asset with Fake Deps",
            "dependencies": [{"key": "fake/key1.txt", "version": "00000000000000000000000000000000"}]
        },
        "data": BASE64_STANDARD.encode(b"asset with fake deps")
    });

    let (status, resp) = post_entry_json(app.clone(), "notes/d.txt", entry).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(resp["message"].as_str().unwrap().contains("dependencies"));
    // Registering the fake edge would show up as a KeyNotFound the moment the dependency graph is
    // consulted for this key (e.g. via a subsequent audit); the handler-level guarantee this test
    // checks is only the reported `message` above — Phase 2 gives no client-visible dependency
    // list to assert against directly (`AssetInfo` carries no `dependencies` field).
}

#[tokio::test]
async fn aae13_hostile_expiration_time_is_ignored() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());

    use base64::prelude::*;
    let entry = serde_json::json!({
        "metadata": {
            "type_identifier": "Text",
            "title": "Non-Expiring Asset",
            "expiration_time": "1970-01-01T00:00:00Z"
        },
        "data": BASE64_STANDARD.encode(b"not actually expiring")
    });

    let (status, resp) = post_entry_json(app.clone(), "notes/e.txt", entry).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(resp["message"].as_str().unwrap().contains("expiration_time"));

    let (status, body) = send_raw(app, "GET", "/api/assets/data/-R/notes/e.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "the value must still be readable, not treated as expired");
    assert_eq!(body, b"not actually expiring");
}

#[tokio::test]
async fn aae14_unknown_type_identifier_returns_400() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);

    use base64::prelude::*;
    let entry = serde_json::json!({
        "metadata": {"type_identifier": "UnknownType12345"},
        "data": BASE64_STANDARD.encode(b"some data")
    });

    let (status, resp) = post_entry_json(app, "notes/f.txt", entry).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["status"], "ERROR");
    assert_eq!(resp["error"]["type"], "ParameterError");
}

#[tokio::test]
async fn aae15_non_object_metadata_returns_400() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);

    use base64::prelude::*;
    let entry = serde_json::json!({
        "metadata": "this is a string, not an object",
        "data": BASE64_STANDARD.encode(b"some data")
    });

    let (status, resp) = post_entry_json(app, "notes/g.txt", entry).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["error"]["type"], "ParameterError");
}

#[tokio::test]
async fn aae16_post_metadata_always_returns_501() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);

    let (status, json) = send(
        app,
        "POST",
        "/api/assets/metadata/-R/notes/a.txt",
        Body::from(r#"{"title":"Attempt to write metadata"}"#),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(json["status"], "ERROR");
    assert_eq!(json["error"]["type"], "NotSupported");
    assert!(
        json["message"].as_str().unwrap().to_lowercase().contains("post description"),
        "the refusal must point at POST description as the alternative"
    );
}

#[tokio::test]
async fn aae17_post_entry_roundtrip_reads_back_only_allowlisted_fields() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());

    use base64::prelude::*;
    let entry = serde_json::json!({
        "metadata": {
            "type_identifier": "Text",
            "data_format": "txt",
            "media_type": "text/plain",
            "title": "Kept Title",
            "description": "Kept description",
            "status": "Ready",        // dropped
            "version": "deadbeef"     // dropped
        },
        "data": BASE64_STANDARD.encode(b"round trip")
    });

    let (status, resp) = post_entry_json(app.clone(), "notes/roundtrip.txt", entry).await;
    assert_eq!(status, StatusCode::CREATED);
    let dropped = resp["message"].as_str().unwrap();
    assert!(dropped.contains("status") && dropped.contains("version"));

    let (status, info) = send(app, "GET", "/api/assets/info/-R/notes/roundtrip.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(info["result"]["title"], "Kept Title");
    assert_eq!(info["result"]["description"], "Kept description");
    assert_eq!(info["result"]["status"], "Source", "the manager decides status, not the client");
}
```

### Expected output

Inline per test: 201 for every allow-list test with the dropped field named in `message`; 400
`ParameterError` for AAE14/AAE15; 501 `NotSupported` naming `POST description` for AAE16; AAE17's
final `GET info` shows only the five allow-listed fields took effect.

---

## Unit Tests

Eleven pure-function tests, inline in `liquers-axum/src/assets/value_description.rs`. No HTTP layer,
no `Environment`; `ValueDescription` and `TypeRegistry` only.

```rust
// liquers-axum/src/assets/value_description.rs — appended #[cfg(test)] mod tests

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::{error::ErrorType, metadata::{AssetInfo, Status}, type_system::TypeRegistry, value::Value};
    use serde_json::json;
    use std::collections::HashMap;

    /// `TypeRegistry::from_value_type` is the verified, public constructor
    /// (`liquers-core/src/type_system.rs`).
    fn test_registry() -> TypeRegistry {
        TypeRegistry::from_value_type::<Value>()
    }

    #[test]
    fn vd01_from_json_keeps_five_fields() {
        let value = json!({
            "type_identifier": "Text",
            "data_format": "txt",
            "media_type": "text/plain",
            "title": "My Note",
            "description": "A description"
        });

        let (desc, dropped) = ValueDescription::from_json(&value).expect("from_json");
        assert_eq!(desc.type_identifier, Some("Text".to_string()));
        assert_eq!(desc.data_format, Some("txt".to_string()));
        assert_eq!(desc.media_type, Some("text/plain".to_string()));
        assert_eq!(desc.title, Some("My Note".to_string()));
        assert_eq!(desc.description, Some("A description".to_string()));
        assert!(dropped.is_empty());
    }

    #[test]
    fn vd02_from_json_non_object_error() {
        let err = ValueDescription::from_json(&json!("not an object")).expect_err("non-object");
        assert_eq!(err.error_type, ErrorType::ParameterError);
    }

    #[test]
    fn vd03_from_json_non_string_field_error() {
        let value = json!({"type_identifier": "Text", "title": 123});
        let err = ValueDescription::from_json(&value).expect_err("non-string field");
        assert_eq!(err.error_type, ErrorType::ParameterError);
    }

    #[test]
    fn vd04_from_json_dropped_fields_sorted() {
        let value = json!({
            "type_identifier": "Text",
            "extra_field_z": "ignored",
            "extra_field_a": "ignored",
            "title": "Title"
        });
        let (_, dropped) = ValueDescription::from_json(&value).expect("from_json");
        assert_eq!(dropped, vec!["extra_field_a".to_string(), "extra_field_z".to_string()]);
    }

    #[test]
    fn vd05_from_params_drops_unknown_parameters() {
        let mut params = HashMap::new();
        params.insert("type_identifier".to_string(), "Bytes".to_string());
        params.insert("data_format".to_string(), "bin".to_string());
        params.insert("title".to_string(), "Binary Data".to_string());
        params.insert("unknown_param".to_string(), "ignored".to_string());

        let (desc, dropped) = ValueDescription::from_params(&params);
        assert_eq!(desc.type_identifier, Some("Bytes".to_string()));
        assert_eq!(desc.data_format, Some("bin".to_string()));
        assert_eq!(desc.title, Some("Binary Data".to_string()));
        assert_eq!(desc.description, None);
        assert_eq!(dropped, vec!["unknown_param".to_string()]);
    }

    #[test]
    fn vd06_or_previous_fills_from_asset_info_never_media_type() {
        // AssetInfo.title/description are `String`, not `Option<String>` — verified against
        // liquers-core/src/metadata.rs.
        let previous = AssetInfo {
            status: Status::Source,
            title: "Previous Title".to_string(),
            description: "Previous Desc".to_string(),
            type_identifier: "Text".to_string(),
            data_format: Some("txt".to_string()),
            media_type: "text/plain".to_string(),
            ..AssetInfo::new()
        };

        let desc = ValueDescription {
            type_identifier: None,
            data_format: None,
            media_type: None,
            title: Some("New Title".to_string()),
            description: None,
        };

        let merged = desc.or_previous(Some(&previous));
        assert_eq!(merged.type_identifier, Some("Text".to_string()));
        assert_eq!(merged.data_format, Some("txt".to_string()));
        assert_eq!(merged.title, Some("New Title".to_string()), "an explicit field is kept");
        assert_eq!(merged.description, Some("Previous Desc".to_string()), "a missing field is filled");
        assert_eq!(merged.media_type, None, "media_type is never filled from the previous value");
    }

    #[test]
    fn vd07_into_metadata_record_default_type_identifier_is_bytes() {
        let desc = ValueDescription {
            type_identifier: None,
            data_format: Some("txt".to_string()),
            media_type: None,
            title: Some("Note".to_string()),
            description: None,
        };
        let record = desc.into_metadata_record(&test_registry()).expect("into_metadata_record");
        assert_eq!(record.type_identifier, "Bytes");
    }

    #[test]
    fn vd08_into_metadata_record_type_name_from_registry() {
        let desc = ValueDescription {
            type_identifier: Some("Text".to_string()),
            data_format: None,
            media_type: None,
            title: None,
            description: None,
        };
        let record = desc.into_metadata_record(&test_registry()).expect("into_metadata_record");
        assert_eq!(record.type_name, "text");
    }

    #[test]
    fn vd09_into_metadata_record_unknown_identifier_is_parameter_error() {
        let desc = ValueDescription {
            type_identifier: Some("UnknownType".to_string()),
            data_format: None,
            media_type: None,
            title: None,
            description: None,
        };
        let err = desc.into_metadata_record(&test_registry()).expect_err("unknown type");
        assert_eq!(err.error_type, ErrorType::ParameterError);
    }

    /// `MetadataRecord` derives `Default`, and `Status` derives its own `Default` as
    /// `Status::None` (verified: `impl Default for Status { fn default() -> Self { Self::None } }`
    /// in `liquers-core/src/metadata.rs`). A draft of this test asserted `Status::Source` and
    /// `stored == false`; both were wrong — `stored` is `Option<bool>`, defaulting to `None`, not
    /// `bool` defaulting to `false`. `into_metadata_record` does not set `status` (that is decided
    /// later, by `AssetManager::set_binary`, from `recipe_opt`), so untouched fields carry the
    /// plain struct default.
    #[test]
    fn vd10_into_metadata_record_untouched_fields_carry_real_defaults() {
        let desc = ValueDescription {
            type_identifier: Some("Bytes".to_string()),
            data_format: Some("bin".to_string()),
            media_type: Some("application/octet-stream".to_string()),
            title: Some("Data".to_string()),
            description: Some("Some binary".to_string()),
        };
        let record = desc.into_metadata_record(&test_registry()).expect("into_metadata_record");

        assert_eq!(record.status, Status::None, "into_metadata_record does not decide status");
        assert_eq!(record.stored, None, "stored is Option<bool>; None means \"true\" by convention");
        assert!(record.dependencies.is_empty());
    }

    /// Final review: a never-evaluated recipe key reports `type_identifier: ""` and the recipe's
    /// `data_format`. Neither may be inherited (a plain `POST data` onto a recipe key would get a
    /// 400 or 422); a non-empty title is still inherited. A client-named type never inherits a
    /// format.
    #[test]
    fn vd11_or_previous_fills_type_and_format_only_as_a_pair_from_data() {
        let recipe_info = AssetInfo {
            status: Status::Recipe,
            title: "Recipe Title".to_string(),
            type_identifier: String::new(),
            data_format: Some("txt".to_string()),
            ..AssetInfo::new()
        };
        let merged = ValueDescription::default().or_previous(Some(&recipe_info));
        assert_eq!(merged.type_identifier, None);
        assert_eq!(merged.data_format, None);
        assert_eq!(merged.title, Some("Recipe Title".to_string()));

        let ready_text = AssetInfo {
            status: Status::Ready,
            type_identifier: "Text".to_string(),
            data_format: Some("txt".to_string()),
            ..AssetInfo::new()
        };
        let named = ValueDescription {
            type_identifier: Some("Bytes".to_string()),
            ..ValueDescription::default()
        };
        let merged = named.or_previous(Some(&ready_text));
        assert_eq!(merged.type_identifier, Some("Bytes".to_string()));
        assert_eq!(merged.data_format, None, "a client-named type does not inherit a format");
    }
}
```

| Test | Checks |
|---|---|
| VD01 | `from_json` preserves all five fields exactly |
| VD02 | `from_json` on non-object JSON → `ParameterError` |
| VD03 | `from_json` on a non-string field value → `ParameterError` |
| VD04 | `from_json` reports dropped fields, sorted |
| VD05 | `from_params` parses query parameters, drops unknown ones |
| VD06 | `or_previous` fills missing fields, never `media_type` |
| VD07 | `into_metadata_record` defaults `type_identifier` to `"Bytes"` |
| VD08 | `into_metadata_record` resolves `type_name` from the registry |
| VD09 | `into_metadata_record` on an unknown identifier → `ParameterError` |
| VD10 | `into_metadata_record`'s untouched fields carry `MetadataRecord`'s real defaults, not the draft's guessed ones |
| VD11 | `or_previous` fills `type_identifier`/`data_format` only as a pair from a data-bearing previous (final review) |

---

## Integration Tests

The remaining `AMR` (expire/set_description/get_asset_info) and `AAE` (corner-case) tests. Each
reuses the shared helper module of its file; only the bodies are new.

```rust
// liquers-core/tests/asset_manager_remove_expire_describe.rs — continues after Example 2's tests,
// same file, same helpers (env_with, metadata_text, stored_status already in scope).

#[tokio::test]
async fn amr10_expire_live_ready_cascades() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[
        ("make_text/a.txt", "A", "text A"),
        ("-R/a.txt/-/upper/b.txt", "B", "B depends on A"),
    ])
    .await;
    let am = envref.get_asset_manager();
    let key_a = parse_key("a.txt")?;
    let key_b = parse_key("b.txt")?;

    let _ = am.get(&key_a).await?.get().await?;
    let _ = am.get(&key_b).await?.get().await?;

    am.expire(&key_a).await.expect("expire should succeed");

    assert_eq!(am.get_asset_info(&key_a).await?.status, Status::Expired);
    assert_eq!(am.get_asset_info(&key_b).await?.status, Status::Expired, "dependent must cascade");
    Ok(())
}

#[tokio::test]
async fn amr11_expire_cascades_through_two_levels() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[
        ("make_text/root.txt", "Root", ""),
        ("-R/root.txt/-/upper/level1.txt", "L1", "depends on root"),
        ("-R/level1.txt/-/upper/level2.txt", "L2", "depends on level1"),
    ])
    .await;
    let am = envref.get_asset_manager();
    let (root, l1, l2) = (parse_key("root.txt")?, parse_key("level1.txt")?, parse_key("level2.txt")?);
    let _ = am.get(&root).await?.get().await?;
    let _ = am.get(&l1).await?.get().await?;
    let _ = am.get(&l2).await?.get().await?;

    am.expire(&root).await?;

    assert_eq!(am.get_asset_info(&root).await?.status, Status::Expired);
    assert_eq!(am.get_asset_info(&l1).await?.status, Status::Expired);
    assert_eq!(am.get_asset_info(&l2).await?.status, Status::Expired);
    Ok(())
}

#[tokio::test]
async fn amr12_expire_idempotent_on_already_expired() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/a.txt", "A", "")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("a.txt")?;
    let _ = am.get(&key).await?.get().await?;

    am.expire(&key).await.expect("first expire");
    am.expire(&key).await.expect("second expire is idempotent");
    assert_eq!(am.get_asset_info(&key).await?.status, Status::Expired);
    Ok(())
}

#[tokio::test]
async fn amr13_expire_source_no_recipe_rejects() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("stored_source.txt")?;
    am.set_binary(&key, b"source data", metadata_text()).await?;

    let err = am.expire(&key).await.expect_err("Source has no recipe to recover from");
    assert_eq!(err.error_type, ErrorType::StatusConflict);
    assert_eq!(err.key, Some(key.encode()));
    let msg = err.message.to_lowercase();
    assert!(msg.contains("expire") && msg.contains("source"));
    Ok(())
}

#[tokio::test]
async fn amr14_expire_never_evaluated_recipe_key_rejects() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/never_touched.txt", "Never", "recipe only")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("never_touched.txt")?;

    let err = am.expire(&key).await.expect_err("Recipe status cannot expire");
    assert_eq!(err.error_type, ErrorType::StatusConflict);
    assert_eq!(err.key, Some(key.encode()));
    Ok(())
}

#[tokio::test]
async fn amr15_expire_absent_key_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("nonexistent.txt")?;

    let err = am.expire(&key).await.expect_err("nothing live, nothing stored, no recipe");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);
    Ok(())
}

#[test]
fn amr16_status_conflict_constructor_shape() {
    let key = parse_key("test.txt").unwrap();
    let err = liquers_core::error::Error::status_conflict(&key, Status::Source, "expire");

    assert_eq!(err.error_type, ErrorType::StatusConflict);
    assert_eq!(err.key, Some(key.encode()), "Error.key is Option<String>, set from key.encode()");
    let msg = err.message.to_lowercase();
    assert!(msg.contains("expire"));
    assert!(msg.contains("source"));
}

#[tokio::test]
async fn amr17_set_description_on_source_updates_fields_version_unchanged() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("source.txt")?;
    am.set_binary(&key, b"content", metadata_text()).await?;
    let version_before = am.version(&key).await?;

    am.set_description(&key, Some("New Title".to_string()), Some("New Desc".to_string())).await?;

    let info = am.get_asset_info(&key).await?;
    // AssetInfo.title/description are `String`, not `Option<String>`.
    assert_eq!(info.title, "New Title");
    assert_eq!(info.description, "New Desc");
    assert_eq!(am.version(&key).await?, version_before, "version must be unchanged (§ AMR21 merged here)");
    Ok(())
}

#[tokio::test]
async fn amr18_set_description_both_none_is_parameter_error() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("source.txt")?;
    am.set_binary(&key, b"content", metadata_text()).await?;

    let err = am.set_description(&key, None, None).await.expect_err("both None must be rejected");
    assert_eq!(err.error_type, ErrorType::ParameterError);
    Ok(())
}

#[tokio::test]
async fn amr19_set_description_on_computed_ready_rejects() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/computed.txt", "Computed", "a recipe")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("computed.txt")?;
    let _ = am.get(&key).await?.get().await?;

    let err = am
        .set_description(&key, Some("Title".to_string()), None)
        .await
        .expect_err("only Source assets may be described");
    assert_eq!(err.error_type, ErrorType::StatusConflict);
    Ok(())
}

#[tokio::test]
async fn amr20_set_description_absent_key_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("nonexistent.txt")?;

    let err = am
        .set_description(&key, Some("Title".to_string()), None)
        .await
        .expect_err("absent key");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);
    Ok(())
}

#[tokio::test]
async fn amr22_get_asset_info_expired_live_reports_expired_without_reevaluating() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/a.txt", "A", "")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("a.txt")?;
    let _ = am.get(&key).await?.get().await?;
    am.expire(&key).await?;

    let info = am.get_asset_info(&key).await?;
    assert_eq!(info.status, Status::Expired, "get_asset_info must not call get() and re-evaluate");
    Ok(())
}

#[tokio::test]
async fn amr23_get_asset_info_never_evaluated_recipe_reports_recipe() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/never_eval.txt", "Never", "recipe")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("never_eval.txt")?;

    let info = am.get_asset_info(&key).await?;
    assert_eq!(info.status, Status::Recipe, "get_asset_info must not evaluate the recipe");
    Ok(())
}

/// AMR24 (final review) — the point of keeping the version: after a restart, a dependent of a
/// dropped intermediate is still reused from the store. Fails without the
/// `dependency_blocks_fast_track` change (a stored `Recipe` dependency would block it).
/// Restart = persisted entries replayed into a fresh store, as `keyed_version_cascade.rs` does.
#[tokio::test]
async fn amr24_dropped_intermediate_does_not_block_dependent_after_restart(
) -> Result<(), Box<dyn std::error::Error>> {
    let keys = ["recipes.yaml", "notes/a.txt", "summary.txt", "summary2.txt"]
        .iter()
        .map(|k| parse_key(k))
        .collect::<Result<Vec<_>, _>>()?;
    let summary_key = parse_key("summary.txt")?;
    let summary2_key = parse_key("summary2.txt")?;

    let persisted = {
        let envref = env_with(&[
            ("-R/notes/a.txt/-/upper/summary.txt", "Summary", "depends on notes"),
            ("-R/summary.txt/-/upper/summary2.txt", "Summary2", "depends on summary"),
        ])
        .await;
        let am = envref.get_asset_manager();
        am.set_binary(&keys[1], b"hello", metadata_text()).await?;
        let _ = am.get(&summary2_key).await?.get().await?;
        am.remove(&summary_key).await?; // drop the intermediate, keep its version
        let store = envref.get_async_store();
        let mut entries = Vec::new();
        for key in &keys {
            entries.push((key.clone(), store.get(key).await?));
        }
        entries
    };

    let store2 = AsyncMemoryStore::new(&Key::new());
    for (key, (bytes, metadata)) in &persisted {
        store2.set(key, bytes, metadata).await?;
    }
    let envref2 = env_over(store2);
    assert_eq!(
        stored_status(&envref2.get_async_store().get_metadata(&summary_key).await?),
        Status::Recipe,
        "precondition: the intermediate was dropped, not deleted"
    );

    let mut reloaded = AssetData::<SimpleEnvironment<Value>>::new(
        9601,
        summary2_key.clone().into(),
        Some(summary2_key.clone()),
        envref2.clone(),
    );
    assert!(
        reloaded.try_fast_track().await?,
        "a dependency dropped to Recipe (version kept) must not block the dependent's fast track"
    );
    Ok(())
}
```

```rust
// liquers-axum/tests/assets_api_endpoints.rs — continues after Example 3's tests, same file,
// same helpers (env_with, build_app, send, send_raw already in scope).

async fn write_source(envref: &EnvRef<SimpleEnvironment<Value>>, path: &str, data: &[u8]) {
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key(path).unwrap(), data, metadata_text()).await.unwrap();
}

#[tokio::test]
async fn aae20_listdir_root_returns_assets_array() {
    let envref = env_with(&[]).await;
    write_source(&envref, "notes/a.txt", b"hello").await;
    let app = build_app(envref);

    let (status, json) = send(app, "GET", "/api/assets/listdir", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"]["assets"].is_array());
}

#[tokio::test]
async fn aae21_listdir_deep_returns_key_array() {
    let envref = env_with(&[]).await;
    write_source(&envref, "notes/a.txt", b"hello").await;
    write_source(&envref, "notes/sub/b.txt", b"world").await;
    let app = build_app(envref);

    let (status, json) = send(app, "GET", "/api/assets/listdir?deep=true", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"]["keys"].is_array());
}

#[tokio::test]
async fn aae22_get_info_404_when_absent() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);

    let (status, json) = send(app, "GET", "/api/assets/info/-R/nonexistent/key.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"]["type"], "KeyNotFound");
}

#[tokio::test]
async fn aae23_contains_false_for_absent_key() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);

    let (status, json) = send(app, "GET", "/api/assets/contains/-R/missing/key.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["contains"], false);
}

#[tokio::test]
async fn aae24_contains_true_for_present_key() {
    let envref = env_with(&[]).await;
    write_source(&envref, "data/exists.txt", b"content").await;
    let app = build_app(envref);

    let (status, json) = send(app, "GET", "/api/assets/contains/-R/data/exists.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["contains"], true);
}

#[tokio::test]
async fn aae25_version_null_for_unversioned() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);

    let (status, json) = send(app, "GET", "/api/assets/version/-R/no/version.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"]["version"].is_null());
}

#[tokio::test]
async fn aae26_version_hex_string_for_stored() {
    let envref = env_with(&[]).await;
    write_source(&envref, "data/versioned.txt", b"content with version").await;
    let app = build_app(envref);

    let (status, json) = send(app, "GET", "/api/assets/version/-R/data/versioned.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    let v = json["result"]["version"].as_str().unwrap();
    assert_eq!(v.len(), 32);
    assert!(v.chars().all(|c| c.is_ascii_hexdigit()));
}

#[tokio::test]
async fn aae27_recover_404_when_no_data() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);

    let (status, json) = send(app, "GET", "/api/assets/recover/-R/no/data.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"]["type"], "KeyNotFound");
}

#[tokio::test]
async fn aae28_recover_returns_expired_value() {
    // Expiring a stored-only Source is a StatusConflict (AMR13), so this needs a recipe-computed
    // value that has since expired, not a plain write.
    let envref = env_with(&[("make_text/derived.txt", "Derived", "")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("derived.txt").unwrap();
    let _ = am.get(&key).await.unwrap().get().await;
    am.expire(&key).await.unwrap();
    let app = build_app(envref);

    let (status, _bytes) = send_raw(app, "GET", "/api/assets/recover/-R/derived.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "recover must return the last-known data of an Expired asset");
}

#[tokio::test]
async fn aae29_recover_honours_accept_json_header() {
    let envref = env_with(&[]).await;
    write_source(&envref, "data/json.txt", b"json data").await;
    let app = build_app(envref);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/assets/recover/-R/data/json.txt")
                .header("Accept", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("");
    assert!(
        content_type.contains("json"),
        "Accept: application/json must select the JSON DataEntry encoding, got {content_type}"
    );
}

#[tokio::test]
async fn aae30_override_200_when_data_exists() {
    // A computed value, not a Source: pinning is what `override` is for, and a stored-only
    // `Source` is promoted to a recipe-less `Override` by today's `to_override`
    // (`ASSET-TO-OVERRIDE-SOURCE-INCONSISTENT`), which this test must not pin down.
    let envref = env_with(&[("make_text/tooverride.txt", "Pinned", "")]).await;
    let am = envref.get_asset_manager();
    let _ = am.get(&parse_key("tooverride.txt").unwrap()).await.unwrap().get().await.unwrap();
    let app = build_app(envref);

    let (status, json) = send(app, "POST", "/api/assets/override/-R/tooverride.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["status"], "Override");
}

#[tokio::test]
async fn aae31_override_404_when_no_data() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);

    let (status, json) = send(app, "POST", "/api/assets/override/-R/nodata/key.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"]["type"], "KeyNotFound");
}

#[tokio::test]
async fn aae32_expire_409_on_source() {
    let envref = env_with(&[]).await;
    write_source(&envref, "notes/source.txt", b"source data").await;
    let app = build_app(envref);

    let (status, json) = send(app, "POST", "/api/assets/expire/-R/notes/source.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json["error"]["type"], "StatusConflict");
}

#[tokio::test]
async fn aae33_expire_200_on_computed_ready() {
    let envref = env_with(&[("make_text/derived.txt", "Derived", "A computed value")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("derived.txt").unwrap();
    let _ = am.get(&key).await.unwrap().get().await;
    let app = build_app(envref);

    let (status, json) = send(app, "POST", "/api/assets/expire/-R/derived.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["status"], "Expired");
}

#[tokio::test]
async fn aae34_makedir_201_created() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);

    let (status, json) = send(app, "PUT", "/api/assets/makedir/-R/newdir", Body::empty()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(json["result"]["key"].is_string());
}

#[tokio::test]
async fn aae35_audit_key_returns_checked_and_expired_arrays() {
    let envref = env_with(&[("make_text/dep.txt", "Dependency", "A value")]).await;
    let app = build_app(envref);

    let (status, json) = send(app, "POST", "/api/assets/audit/-R/dep.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"]["checked"].is_array());
    assert!(json["result"]["expired"].is_array());
}

#[tokio::test]
async fn aae36_audit_all_returns_checked_and_expired_arrays() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);

    let (status, json) = send(app, "POST", "/api/assets/audit", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"]["checked"].is_array());
    assert!(json["result"]["expired"].is_array());
}

#[tokio::test]
async fn aae37_refresh_command_versions_200_null_result_with_message() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);

    let (status, json) = send(app, "POST", "/api/assets/refresh_command_versions", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"].is_null());
    assert!(!json["message"].as_str().unwrap_or("").is_empty());
}

#[tokio::test]
async fn aae40_delete_data_recipe_computed_new_status_recipe() {
    let envref = env_with(&[("make_text/computed.txt", "Computed", "")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("computed.txt").unwrap();
    let _ = am.get(&key).await.unwrap().get().await;
    let app = build_app(envref);

    let (status, json) = send(app, "DELETE", "/api/assets/data/-R/computed.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["removed"], true);
    assert_eq!(json["result"]["new_status"], "Recipe");
}

#[tokio::test]
async fn aae41_delete_data_source_new_status_none() {
    let envref = env_with(&[]).await;
    write_source(&envref, "notes/source.txt", b"source data").await;
    let app = build_app(envref);

    let (status, json) = send(app, "DELETE", "/api/assets/data/-R/notes/source.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["removed"], true);
    assert_eq!(json["result"]["new_status"], "None");
}

#[tokio::test]
async fn aae42_delete_data_409_on_directory() {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    am.makedir(&parse_key("directory").unwrap()).await.unwrap();
    let app = build_app(envref);

    let (status, json) = send(app, "DELETE", "/api/assets/data/-R/directory", Body::empty()).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json["error"]["type"], "StatusConflict");
}

#[tokio::test]
async fn aae43_delete_entry_delegates_to_delete_data() {
    let envref = env_with(&[]).await;
    write_source(&envref, "data/delete_me.txt", b"to delete").await;
    let app = build_app(envref);

    let (status, json) = send(app, "DELETE", "/api/assets/entry/-R/data/delete_me.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["removed"], true);
    assert_eq!(json["result"]["new_status"], "None");
}

#[tokio::test]
async fn aae44_post_data_non_key_query_returns_501() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);

    // `make_text` parses as an action (Transform), not a key — verified with liquers-validate.
    let (status, json) = send(app, "POST", "/api/assets/data/make_text", Body::from("ignored")).await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(json["error"]["type"], "NotSupported");
}

#[tokio::test]
async fn aae45_delete_data_non_key_query_returns_501() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);

    let (status, json) = send(app, "DELETE", "/api/assets/data/make_text", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(json["error"]["type"], "NotSupported");
}

#[tokio::test]
async fn aae50_read_only_post_data_returns_405() {
    // `.read_only()` omits the POST route; `GET data/{key}` still serves that same path, so
    // axum's routing responds 405 Method Not Allowed, not 404 (verified: `read_only` doc in
    // Phase 2, "An omitted route answers axum's own 405, where the path serves other methods").
    let envref = env_with(&[]).await;
    let app = liquers_axum::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .read_only()
        .build()
        .with_state(envref);

    let (status, _) = send(app, "POST", "/api/assets/data/-R/notes/a.txt", Body::from("hello")).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn aae51_read_only_get_data_still_200() {
    let envref = env_with(&[]).await;
    write_source(&envref, "notes/readable.txt", b"readable").await;
    let app = liquers_axum::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .read_only()
        .build()
        .with_state(envref);

    let (status, _) = send_raw(app, "GET", "/api/assets/data/-R/notes/readable.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn aae52_read_only_post_expire_returns_404() {
    // `expire/{key}` has no other HTTP method registered on that path, so omitting it under
    // `.read_only()` leaves axum with no route at all: 404, not 405.
    let envref = env_with(&[]).await;
    write_source(&envref, "notes/a.txt", b"data").await;
    let app = liquers_axum::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .read_only()
        .build()
        .with_state(envref);

    let (status, _) = send(app, "POST", "/api/assets/expire/-R/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn aae53_read_only_post_cancel_still_routed() {
    let envref = env_with(&[]).await;
    let app = liquers_axum::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .read_only()
        .build()
        .with_state(envref);

    let (status, _) = send(app, "POST", "/api/assets/cancel/-R/somekey", Body::empty()).await;
    assert_ne!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_ne!(status, StatusCode::NOT_FOUND, "cancel must stay routed under read_only()");
}

#[tokio::test]
async fn aae54_with_admin_false_post_audit_returns_404() {
    // `audit` (no query) has no sibling method on that exact path, so omitting it under
    // `.with_admin(false)` leaves no route: 404.
    let envref = env_with(&[]).await;
    let app = liquers_axum::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .with_admin(false)
        .build()
        .with_state(envref);

    let (status, _) = send(app, "POST", "/api/assets/audit", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn aae55_with_admin_false_post_data_still_201() {
    let envref = env_with(&[]).await;
    let app = liquers_axum::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .with_admin(false)
        .build()
        .with_state(envref);

    let (status, _) = send(app, "POST", "/api/assets/data/-R/notes/a.txt", Body::from("hello")).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn aae60_concurrent_post_data_same_key_is_serialized_atomic() {
    let envref = env_with(&[]).await;
    let app1 = build_app(envref.clone());
    let app2 = build_app(envref.clone());

    let h1 = tokio::spawn(async move {
        app1.oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/assets/data/-R/concurrent/file.txt")
                .body(Body::from("body1"))
                .unwrap(),
        )
        .await
    });
    let h2 = tokio::spawn(async move {
        app2.oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/assets/data/-R/concurrent/file.txt")
                .body(Body::from("body2"))
                .unwrap(),
        )
        .await
    });

    let (r1, r2) = tokio::join!(h1, h2);
    assert_eq!(r1.unwrap().unwrap().status(), StatusCode::CREATED);
    assert_eq!(r2.unwrap().unwrap().status(), StatusCode::CREATED);

    // AsyncStore::get returns (Vec<u8>, Metadata) — not raw bytes.
    let store = envref.get_async_store();
    let key = parse_key("concurrent/file.txt").unwrap();
    let (data, _metadata) = store.get(&key).await.unwrap();
    assert!(
        data.as_slice() == b"body1" || data.as_slice() == b"body2",
        "the key_mutation_lock must serialize the two writes into one, not a torn merge"
    );
    eprintln!("concurrent POST data settled on one of the two writes, as expected");
}
```

| Test | Checks |
|---|---|
| AMR10 | live `Ready` asset expires and cascades to its dependent |
| AMR11 | expire cascades through a two-level dependency chain |
| AMR12 | expire is idempotent on an already-`Expired` asset |
| AMR13 | expire a `Source` (no recipe) → `StatusConflict`, key and message checked |
| AMR14 | expire a never-evaluated recipe key → `StatusConflict` |
| AMR15 | expire an absent key → `KeyNotFound` |
| AMR16 | `Error::status_conflict` sets type, `key` (as `Some(key.encode())`), and message |
| AMR17 | `set_description` on `Source` updates title/description (both `String`); version unchanged |
| AMR18 | `set_description` with both `None` → `ParameterError` |
| AMR19 | `set_description` on computed `Ready` → `StatusConflict` |
| AMR20 | `set_description` on an absent key → `KeyNotFound` |
| AMR22 | `get_asset_info` on an `Expired` live asset reports `Expired`, no re-evaluation |
| AMR23 | `get_asset_info` on a never-evaluated recipe key reports `Recipe`, no evaluation |
| AMR24 | after a restart, a dependent of a dropped intermediate is still fast-tracked |
| AAE20–AAE21 | `GET listdir` shapes: `{assets:[...]}` and, with `?deep=true`, `{keys:[...]}` |
| AAE22 | `GET info` 404 `KeyNotFound` when absent |
| AAE23–AAE24 | `GET contains` false/true |
| AAE25–AAE26 | `GET version` null vs. 32-hex-digit string |
| AAE27–AAE29 | `GET recover` 404 with no data; recovers an `Expired` value; honours `Accept: application/json` |
| AAE30–AAE31 | `POST override` 200 with data; 404 without |
| AAE32–AAE33 | `POST expire` 409 on `Source`; 200 on computed `Ready` |
| AAE34 | `PUT makedir` 201 |
| AAE35–AAE36 | `POST audit/{key}` and `POST audit` both return `{checked, expired}` arrays |
| AAE37 | `POST refresh_command_versions` 200, null result, non-empty message |
| AAE40–AAE41 | `DELETE data` `new_status` is `"Recipe"` for a computed value, `"None"` for a `Source` |
| AAE42 | `DELETE data` on a `Directory` → 409 |
| AAE43 | `DELETE entry` delegates to the same result as `DELETE data` |
| AAE44–AAE45 | `POST data` / `DELETE data` on a non-key query → 501 `NotSupported` |
| AAE50 | `.read_only()`: `POST data` → **405** (GET still serves that path) |
| AAE51 | `.read_only()`: `GET data` still 200 |
| AAE52 | `.read_only()`: `POST expire` → **404** (no sibling method on that path) |
| AAE53 | `.read_only()`: `POST cancel` still routed |
| AAE54 | `.with_admin(false)`: `POST audit` → 404 |
| AAE55 | `.with_admin(false)`: `POST data` still 201 |
| AAE60 | two concurrent `POST data` to the same key: both 201, store settles on exactly one body |

---

### Review-round additions (AAE02–AAE05)

Added after the Phase 3 review: Reviewer 1 found no HTTP-level test for Q11, none for the two
existing reads `GET metadata` / `GET entry` (the latter changes: Phase 2 makes it honour
`Accept`), and no positive `POST cancel`.

```rust
// liquers-axum/tests/assets_api_endpoints.rs — same helpers.

/// Q11: POST data onto a key that has a recipe makes it `Override`; removing it restores the recipe.
#[tokio::test]
async fn aae02_post_data_onto_recipe_key_is_override() {
    let envref = env_with(&[("make_text/source.txt", "Source", "generated text")]).await;
    let app = build_app(envref);

    let (status, json) = send(
        app.clone(),
        "POST",
        "/api/assets/data/-R/source.txt?type_identifier=Text&data_format=txt",
        Body::from("pinned by user"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["result"]["status"], "Override");

    let (status, body) = send_raw(app.clone(), "GET", "/api/assets/data/-R/source.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(String::from_utf8(body).unwrap(), "pinned by user");

    let (status, json) = send(app.clone(), "DELETE", "/api/assets/data/-R/source.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["new_status"], "Recipe");

    let (_, body) = send_raw(app, "GET", "/api/assets/data/-R/source.txt", Body::empty()).await;
    assert_eq!(String::from_utf8(body).unwrap(), "generated");
}

/// Existing `GET metadata` returns the manager's record for a Source written over HTTP.
#[tokio::test]
async fn aae03_get_metadata_of_source() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    let (status, _) = send(
        app.clone(),
        "POST",
        "/api/assets/data/-R/notes/a.txt?type_identifier=Text&data_format=txt&title=Note%20A",
        Body::from("hello"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, json) = send(app, "GET", "/api/assets/metadata/-R/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "OK");
    assert_eq!(json["result"]["status"], "Source");
    assert_eq!(json["result"]["title"], "Note A");
    assert_eq!(json["result"]["type_identifier"], "Text");
}

/// `GET entry` honours `Accept: application/json` (Phase 2 fixes the ignored header).
#[tokio::test]
async fn aae04_get_entry_honours_accept_json() {
    use base64::prelude::*;
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    let (status, _) = send(
        app.clone(),
        "POST",
        "/api/assets/data/-R/notes/a.txt?type_identifier=Text&data_format=txt",
        Body::from("hello"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/assets/entry/-R/notes/a.txt")
                .header("Accept", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(content_type.starts_with("application/json"), "got {content_type}");
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let entry: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let data = BASE64_STANDARD.decode(entry["data"].as_str().unwrap()).unwrap();
    assert_eq!(data, b"hello");
    assert_eq!(entry["metadata"]["status"], "Source");
}

/// Positive `POST cancel` on an existing asset: routed, envelope OK.
#[tokio::test]
async fn aae05_post_cancel_existing_asset() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    let (status, _) = send(
        app.clone(),
        "POST",
        "/api/assets/data/-R/notes/a.txt?type_identifier=Text&data_format=txt",
        Body::from("hello"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, json) = send(app, "POST", "/api/assets/cancel/-R/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "OK");
}
```

| Test | Checks |
|---|---|
| AAE02 | Q11 over HTTP: `POST data` on a recipe key → `Override`; `DELETE` → `new_status: "Recipe"`; next read recomputes |
| AAE03 | existing `GET metadata` returns the manager's record (`Source`, title, type) |
| AAE04 | `GET entry` negotiates JSON from `Accept` alone; data round-trips through base64 |
| AAE05 | `POST cancel` on an existing asset → 200 `OK` envelope |

## Corner Cases

### 1. Memory

- **`ValueDescription` is small and owned** (`Option<String>` × 5); no allocation-failure surface
  worth testing beyond the ordinary allocator. Not covered by a dedicated test.
- **`listdir_asset_info` is O(n) metadata reads** (Phase 2, Concurrency Considerations): acceptable
  at MVP scale, out of scope for a memory test here; tracked by `STORE-NO-CONTENT-OR-METADATA-SEARCH`.
- **`POST data`/`POST entry` bodies are held in memory as `Bytes`** before `set_binary`; no size
  limit is introduced by this design. Not a new corner case — the existing `GET data`/`POST data`
  stub already had this property.

### 2. Concurrency

- **Two concurrent `POST data` to the same key**: covered by AAE60. The `key_mutation_lock`
  (Phase 2) serializes `remove`/`expire`/`set_description`/`set_binary`/`set_state`/`to_override`,
  so the two writes cannot interleave; the store ends up holding exactly one of the two bodies,
  never a torn mix.
- **`remove`'s status check and its action are atomic** with respect to a concurrent `POST data`
  under the same lock — this is the race `ASSET-REMOVE-FORGETS-DEPENDENTS`/Phase 1 set out to
  close. Not independently tested here beyond AAE60, because reproducing the specific
  check-then-act race deterministically needs a hook the design does not add; Phase 2's own
  "Concurrency Considerations" section is the primary evidence for lock discipline, backed by the
  hand-verified call graph in its Review Log.
- **Reads take no manager lock** (`info`, `listdir`, `contains`, `version`, `recover`): may
  observe state just before or after a concurrent mutation, never a torn record, because store
  writes are per-key. No dedicated test — this is a property of the underlying store, exercised
  incidentally by AAE20–AAE29 running against a store that is also being written by `write_source`.

### 3. Errors

- Every error path in Phase 2's Error Handling table has a test: `ParseError` is implicit in a
  malformed path (not separately tested — `key_from_path` returning `Err(Response)` for a
  genuinely unparsable path is routing-layer plumbing, not asset-manager behaviour); `NotSupported`
  for a non-key query (AAE44, AAE45) and for `POST metadata` (AAE16); `ParameterError` for an
  unknown `type_identifier` (VD09, AAE14), non-object metadata (VD02, AAE15), and both-`None`
  `set_description` (AMR18); `KeyNotFound` (AMR05, AMR15, AMR20, AAE22, AAE27, AAE31); the new
  `StatusConflict`/409 (AMR07, AMR13, AMR14, AMR19, AAE32, AAE42).
- **`Error.key` is `Option<String>`, not `Option<Key>`.** A draft compared it against `Some(key)`;
  every fixed test now compares against `Some(key.encode())` (see Fixes §6).
- **`key_not_found` does not set `.key`** (checked directly in `liquers-core/src/error.rs`); tests
  for that error type do not assert on `err.key`. Only the new `status_conflict` constructor is
  documented to set it, so only those tests assert it.

### 4. Serialization

- `Status` serializes as its bare variant name (`"Source"`, `"Recipe"`, `"Ready"`, `"Expired"`,
  …) — a plain `derive(Serialize)` on a fieldless enum, no `#[serde(rename)]` anywhere in the
  match. Exercised throughout every `AAE*` JSON assertion.
- `AssetInfo.key`/`.query` serialize as a **plain string** (`key.encode()`), not a nested object —
  `option_key_format`/`option_query_format` in `liquers-core/src/metadata.rs`. AAE01 asserts
  `json["result"]["key"] == "notes/a.txt"` directly against a string.
- `Version` serializes as a 32-hex-digit lowercase string (AAE26); `Option<Version>` as that string
  or JSON `null`, never omitted (AAE25) — `Ok(None)` and a store-read `Err` must stay distinguishable,
  per Phase 2's error table; this design does not add a test that forces the `Err` branch (it needs
  a broken store), only asserts the `Ok(None)` → `null` mapping.
- `MetadataRecord`/`AssetInfo` derive `Default`; `MetadataRecord::new()` sets `is_error = false`
  and calls `set_updated_now()` but otherwise takes the struct default — `status: Status::None`,
  `stored: None`, `dependencies: vec![]`, `title`/`description`: `String::new()`. VD10 pins this
  down explicitly because a draft guessed `Status::Source` / `stored: false`.
- `DataEntry`'s JSON `data` field is base64-encoded bytes (Example 3, `BASE64_STANDARD`); CBOR is
  the request/response default elsewhere (`?format=json` opts into JSON explicitly, as in AAE29).

### 5. Integration

- **Store**: every `AMR*` test goes through `AssetManager` onto `AsyncMemoryStore`; `AAE*` goes
  through the router onto the same store via `SimpleEnvironment`. No store-specific behaviour is
  exercised (that is the Store conformance suite's job, unaffected by this design).
- **Recipes**: recipe evaluation, dependency registration and cascade all run through the real
  `DefaultRecipeProvider` reading `recipes.yaml`, not a mock — Example 1/2 and AMR10/AMR11 depend
  on real recipe resolution to prove the cascade.
- **Command registry**: two trivial commands (`make_text`, `upper`) are registered per-environment;
  no `register_command!` macro is used, matching the brief's note that `liquers-axum` has no
  `liquers-macro` dev-dependency — the closure form registers directly against
  `CommandRegistry::register_command`.
- **Builder switches** (`read_only`, `with_admin`) are asset-manager-agnostic; AAE50–AAE55 test
  only routing, and the exact HTTP code (405 vs. 404) is asserted per Fixes §4 below, not the
  Phase 2 drafts' looser "404 or 405."

---

## Test Plan

| Command | Runs | New tests |
|---|---|---|
| `cargo test -p liquers-core --test asset_manager_remove_expire_describe` | `liquers-core/tests/asset_manager_remove_expire_describe.rs` | AMR01–AMR07, AMR10–AMR20, AMR22–AMR24 (21 tests) |
| `cargo test -p liquers-axum --test assets_api_endpoints` | `liquers-axum/tests/assets_api_endpoints.rs` | AAE01–AAE05, AAE10–AAE17, AAE20–AAE60 (44 tests) |
| `cargo test -p liquers-axum --lib` | `liquers-axum/src/assets/value_description.rs`, `#[cfg(test)] mod tests` | VD01–VD11 (11 tests) |
| `cargo test -p liquers-core --lib --tests` | the two changed `ErrorType`-exhaustive sites in `liquers-core/src/assets.rs` and `error.rs`, plus every other core test (regression) | none new, but must still pass after `ErrorType::StatusConflict` is added |
| `cargo test -p liquers-lib --lib --tests` | the default loop (CLAUDE.md); transitively builds `liquers-core`, `liquers-macro`, `liquers-store` | regression only — this design touches no `liquers-lib` file |

### The `ErrorType`-exhaustive crates: `liquers-py` and `liquers-web`

Adding `ErrorType::StatusConflict` breaks every exhaustive `match ErrorType` until an arm is
added (Phase 2, Compilation Validation). Two crates outside the default test loop above have such
a match and must be checked, not just `liquers-core`/`liquers-axum`:

- **`liquers-py` is a default-member** of the workspace (`Cargo.toml`: `default-members` includes
  `liquers-py`), so it *is* built by a plain `cargo build`/`cargo test --workspace`, but this
  project's routine loop (CLAUDE.md) deliberately runs `-p liquers-lib`/`-p liquers-records`
  instead of the workspace default to fit the disk budget. Check it explicitly:
  ```bash
  cargo check -p liquers-py
  ```
  (needs Python dev headers for PyO3, per the crate's own build requirements; use `cargo test -p
  liquers-py` if a fuller check is wanted and headers are available).
- **`liquers-web` is wasm32-only and excluded from `default-members`** (CLAUDE.md, confirmed
  against `Cargo.toml`: `members` lists it, `default-members` does not), so nothing above touches
  it. Check it explicitly, after `cargo clean` and separately from the native loop:
  ```bash
  cargo check -p liquers-web --target wasm32-unknown-unknown --features debug-handles
  ```
  A full run of its test suites (`cargo test -p liquers-web --target wasm32-unknown-unknown
  --features debug-handles`, then the `browser-tests`/e2e loops) is the CLAUDE.md-documented deep
  check; a `cargo check` is the minimum this design's `ErrorType` addition requires to catch a
  missed match arm before Phase 4 is considered done.

### Order

1. `cargo test -p liquers-core --test asset_manager_remove_expire_describe` — the contract the
   `liquers-axum` handlers are built on; fix here first if it fails.
2. `cargo test -p liquers-axum --lib` (VD01–VD11) — pure-function tests, fast, no environment.
3. `cargo test -p liquers-axum --test assets_api_endpoints` — needs the `tower` `util`
   dev-dependency Phase 4 adds to `liquers-axum/Cargo.toml` (not present today).
4. `cargo check -p liquers-py` and `cargo check -p liquers-web --target wasm32-unknown-unknown
   --features debug-handles` — confirm the new `ErrorType` variant did not leave an unmatched arm
   anywhere outside the two crates above.
5. `cargo test -p liquers-lib --lib --tests` and `bash scripts/check-build-matrix.sh` — full
   regression, per CLAUDE.md's default loop.

---

## Fixes made against the five drafts

Verified against the actual code (`liquers-core/src/metadata.rs`, `type_system.rs`, `error.rs`,
`query.rs`, `assets.rs`) and corrected:

1. **`MetadataRecord`/`AssetInfo` field types and defaults.**
   - `type_identifier` and `type_name` are `String`, not `Option<String>`. Draft 2's
     `metadata_text()` helper wrote `Some("Text".to_string())` into both — does not compile. Fixed
     in the shared `metadata_text()` (struct-literal form, matching drafts 3/5's correct style).
   - `AssetInfo.title`/`.description` are `String`, not `Option<String>`. Draft 4's VD06 built an
     `AssetInfo { title: Some(...), ... }` (does not compile) and draft 1's AMR17-equivalent
     assertion compared `info.title` against `Some("New Title".to_string())`. Both fixed to plain
     `String` comparisons/literals.
   - `MetadataRecord::default()`'s `status` is `Status::None` (`impl Default for Status` returns
     `Self::None`), not `Status::Source`; `stored` is `Option<bool>` defaulting to `None`, not
     `bool` defaulting to `false`. Draft 4's VD10 asserted "status Source, stored false" — fixed to
     `Status::None` / `None`, with a comment explaining `into_metadata_record` does not decide
     status (that happens later, in `set_binary`, from `recipe_opt`).
   - `AssetManager::set_binary` takes `metadata: MetadataRecord` **by value**. Drafts 2, 3 and 5
     called it as `am.set_binary(&key, b"...", &metadata_text())` (a `&MetadataRecord` where an
     owned value is required) — does not compile. Fixed by dropping the `&` everywhere in the
     shared helper's call sites.

2. **`TypeRegistry::from_value_type::<Value>()`** — confirmed to exist and be `pub`
   (`liquers-core/src/type_system.rs:267`). No change needed; kept as drafted.

3. **JSON serialization of `AssetInfo`.**
   - `key`/`query` serialize as a **plain encoded string** (`option_key_format`/
     `option_query_format`, both `serializer.serialize_str(&k.encode())`), not a nested object.
     The drafts already assumed a plain string (`json["result"]["key"] == "notes/a.txt"`); this
     was correct and is called out explicitly in Corner Cases §4 so Phase 4 does not second-guess
     it.
   - `Status` serializes as its bare variant name (plain `derive(Serialize)`, no rename) — also
     already correct in the drafts, confirmed rather than changed.

4. **Routing results under builder switches, made exact instead of "404 or 405."**
   - `.read_only()` + `POST data/{key}`: **405**, because `GET data/{key}` still serves that exact
     path (axum reports Method Not Allowed, not a missing route). Draft 5's AAE104 already asserted
     405 correctly; kept as AAE50.
   - `.read_only()` + `POST expire/{key}`: **404**, because `expire/{key}` has no other HTTP method
     registered on it — omitting the only method leaves no route at all. This case was **not** in
     any draft (drafts only tested `POST data` and `POST cancel` under `read_only`); added as
     AAE52, with the reasoning inline in a comment so Phase 4 does not need to re-derive it.
   - `.with_admin(false)` + `POST audit`: **404**, same "no sibling method" reasoning; draft 5's
     AAE107 already asserted this correctly, kept as AAE54.
   - `.read_only()` + `POST cancel/{key}`: draft 5's AAE106 asserted only `!= 405`; tightened to
     also assert `!= 404` (cancel must be *routed*, even if the key itself then fails for other
     reasons), since a plain "not 405" would also pass if the route were silently dropped.

5. **The non-key path example.** `make_text` parses as a `Transform` (an action query with no
   key), confirmed with `liquers-validate --no-registry`. AAE44/AAE45 keep it as the non-key
   example. **Corrected in the final review:** the claim that a plain `dir/file.ext` path is a pure
   key was wrong — `liquers-validate --no-registry -- notes/a.txt` gives a `Transform` segment
   (action `notes`, filename `a.txt`), and with the registry it fails with `ActionNotRegistered`.
   Every key path in the HTTP tests is therefore written `-R/<key>` (all 26 distinct ones
   re-validated as a single `Resource` segment); without that, every key-only operation would
   answer 501 and every read would evaluate the wrong query.

6. **`Error.key` is `Option<String>`, not `Option<Key>`.** Draft 4's AMR13/AMR14/AMR16 compared
   `err.key` against `Some(key.clone())` where `key: Key` — does not compile (`Option<Key>` vs.
   `Option<String>` type mismatch on `assert_eq!` is a compile error, not a runtime failure). Fixed
   to `Some(key.encode())` throughout, matching the existing `key_not_supported` constructor's
   pattern (`error.key = Some(key.encode())`) that `status_conflict` is documented to follow.
   Also confirmed that `Error::key_not_found` does **not** set `.key` at all (it calls the
   two-argument `Error::new` and stops) — tests for `KeyNotFound` in this document do not assert on
   `err.key`, whereas a draft implicitly assumed every error carries a matching key.

7. **`AsyncStore::get` returns `(Vec<u8>, Metadata)`, not raw bytes.** Draft 5's AAE109 (now
   AAE60) wrote `let data = store.get(&key).await.unwrap(); assert!(data == b"body1" ...)` — type
   mismatch, does not compile. Fixed to destructure the tuple and compare `data.as_slice()`.

8. **`AsyncStore::get_metadata` returns `Metadata` (an enum), whose status reads through the
   `.status()` method, not a `.status` field.** Draft 2/3's AMR03 wrote
   `stored_metadata.status == Status::Recipe` — `Metadata` has no `status` field (it is
   `LegacyMetadata(serde_json::Value) | MetadataRecord(MetadataRecord)`). Fixed via a one-line
   `stored_status()` helper calling `.status()`.

9. **Dropped duplicate tests.** Draft 4's AMR21 ("set_description leaves version unchanged") is
   the same assertion AMR17 already makes on its happy path; merged into AMR17 rather than kept as
   a separate test. Draft 3's AMR04 (re-evaluate after a recipe-computed drop) and AMR03 together
   covered the same "no cascade from a recipe-computed remove" claim from two angles (direct drop,
   then re-evaluation); both were kept because they check different observables (stored metadata
   immediately after `remove` vs. the *grandchild's* status after a subsequent re-`get`), but no
   third variant was added.

10. **Dropped draft 3's `AAE23` dependency-round-trip assertion that reads
    `info_resp["result"]["metadata"]["dependencies"]`.** `AssetInfo` (what `GET info` actually
    returns per Phase 2's Web Endpoints table) carries no `dependencies` field — only
    `MetadataRecord` does, and `GET info`'s response type is `AssetInfo`. Rewritten as AAE12,
    asserting only what is client-visible: the response `message` names the dropped field. A
    dependency-registration check would need a store/manager-level assertion instead, which is out
    of scope for an HTTP-layer test and already covered at the manager level by the removal tests'
    cascade assertions.

## No unverifiable claims left as guesses

Every `TypeRegistry`, `MetadataRecord`, `AssetInfo`, `Error`, `AsyncStore` and `Query::key`/
`is_key` claim above was checked against the current source in this session (`liquers-core/src/
metadata.rs`, `type_system.rs`, `error.rs`, `query.rs`, `assets.rs`, `store.rs`, `context.rs`,
`commands.rs`, `recipes.rs`) or against a live `liquers-validate --no-registry` run. Two items
could not be verified without code that does not exist yet, because Phase 2 leaves their exact
shape to Phase 4's implementation rather than fixing it:

- The **exact JSON shape of a dependency record** if it were ever exposed to a client (it is not,
  per Phase 2's Web Endpoints table — `AssetInfo` has no `dependencies` field). AAE12 therefore
  checks only the `message` contract, not a dependency list.
- The **exact wording** of `POST metadata`'s refusal `message` beyond "mentions POST description"
  (Phase 2 gives the wording as an example, not a contract: `"asset metadata is owned by the asset
  manager; use POST description for a Source asset's title and description"`). AAE16 asserts the
  substring `"post description"` (case-insensitively), not the full sentence, so a Phase 4 wording
  tweak does not break the test.

## Open Issues for Review

None. No draft assertion was found to contradict Phase 2's Web Endpoints table or remove decision
table in a way that would mean Phase 2 itself is wrong — every conflict found (listed under Fixes
above) was a draft error against an otherwise-consistent Phase 2 contract.

## Review Log

Multi-agent review, 2026-09-27.

- **Reviewer 1 (Phase 1 conformity):** found four gaps, all fixed by adding AAE02–AAE05:
  - Q11 was tested only at the manager level;
  - `GET metadata` had no test;
  - `GET entry` had no test, although Phase 2 changes its `Accept` handling;
  - there was no positive `POST cancel`.
- **Reviewer 2 (Phase 2 conformity):** no findings. Signatures, routes, codes, result shapes, every
  row of the `remove` table, and the error mappings all match.
- **Reviewer 3 (codebase and queries):** no findings.
  - All 11 queries validate with `liquers-validate --command make_text --command upper`.
  - `make_text` is a non-key query and the key paths are pure keys, as the tests assume.
  - The existing APIs compile as written.
- **Found while applying the fixes:** AAE01 posted the `POST description` JSON without
  `Content-Type: application/json`. axum's `Json` extractor rejects that with 415 before the handler
  runs. Added a `send_json` helper and used it there.
- **Count correction:** the synthesized document stated 30 `AAE` tests; there were 40 (44
  with AAE02–AAE05). Totals: 20 AMR + 10 VD + 44 AAE = 74.

**Final cross-phase review, 2026-09-27** (changes made in this document):
- Every assets-API key path is now `-R/<key>` (Fixes §5 corrected); `make_text` stays the non-key example.
- `assets::AssetManager` added to both test files' imports — trait methods do not resolve without it.
- AMR file: `env_with` split into `env_over(store)` + `env_with`; new **AMR24** (restart: a dropped
  intermediate does not block its dependent's fast track).
- New **VD11** (`or_previous` pair rule). AAE30 now pins a computed value and asserts `Override`
  instead of promoting a `Source` (`ASSET-TO-OVERRIDE-SOURCE-INCONSISTENT`).
- Counts: 21 AMR + 11 VD + 44 AAE = 76.
