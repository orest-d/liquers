# Phase 3: Examples & Testing — Assets API over the whole AssetManager

Written against `specs/design/axum-assets-endpoints/phase2-architecture.md`, re-approved
2026-09-28. Phase 2 wins every conflict with anything below. This revision **replaces** the prior
Phase 3 content in full: it targets the two-family routing (`/q/`, `/key/`, `/admin/`) rather than
the old unprefixed routes, which v1 of this document tested.

None of the code under test exists yet. Every test below is a runnable prototype against the
Phase 2 contract — signatures, field types, error mappings and route shapes are verified against
the current codebase wherever that code already exists (the four builders, `error.rs`,
`metadata.rs`, `store.rs`, `media_type.rs`, `WEB_API_SPECIFICATION.md`); the `AssetManager` methods
and Assets API routing that Phase 2 introduces are written to the signatures Phase 2 specifies.

## Example Type

Runnable Rust test prototypes (`#[tokio::test]` / `#[test]`), one behaviour per test, `eprintln!`
only, no `_ =>` on `ErrorType`/`Status`, tests may `unwrap()`. Distributed across eight target
files as instructed, one shared helper block per file (no duplicate helpers within a file). Three
worked examples (Example 1, Example 2, Example 2b) carry full HTTP/API transcripts; Example 3
documents the metadata allow-list contract that the AAE10–AAE17 tests exercise.

## Overview Table

Counts below are **counted**, not estimated, from the code blocks in this document.

| File | Target | Count | IDs |
|---|---|---|---|
| `liquers-core/tests/asset_manager_remove_expire_describe.rs` | core `AssetManager` (`remove`, `expire`, `set_description`, `removedir`, `lookup_query_asset`, `Removed` notification, `to_override`) | **37** | AMR01–AMR07, AMR10–AMR20, AMR22–AMR24, AMR30–AMR34, AMR40–AMR44, AMR50–AMR53, AMR60–AMR61 |
| `liquers-axum/src/assets/value_description.rs` `#[cfg(test)] mod tests` | `ValueDescription` allow-list, `from_json`, `from_params`, `or_previous`, `into_metadata_record` | **11** | VD01–VD11 |
| `liquers-axum/tests/assets_api_endpoints.rs` | Assets API HTTP routes, both families, builder switches | **63** | AAE01–AAE49, AAE60–AAE69, AAE80, AAE90–AAE92 |
| `liquers-axum/tests/assets_websocket.rs` | WebSocket forwarding, casing, limits, lifecycle | **16** | AWS01–AWS11, AWS12a, AWS12b, AWS13, AWS14a, AWS14b |
| `liquers-axum/tests/store_api_routes.rs` | `StoreApiBuilder` routes (I4) | **16** | SAR01–SAR16 |
| `liquers-axum/tests/query_api_routes.rs` | `QueryApiBuilder` routes, `with_timeout` (I4, I5) | **7** | QAR01–QAR07 |
| `liquers-axum/tests/recipes_api_routes.rs` | `RecipesApiBuilder` routes (I4) | **9** | RAR01–RAR09 |
| `liquers-core/src/media_type.rs` tests module | tabular media types (I9) | **10** | MT01–MT10 |
| **Total** | | **169** | |

Per-file breakdown (counted from the headings in each section below):

- AMR: 7 (remove) + 11 (expire/describe, AMR10–AMR20) + 3 (AMR22–24) + 5 (removedir) + 5 (lookup_query_asset) + 4 (notifications) + 2 (to_override) = **37**.
- AAE: 1 (Example 1) + 2 (AAE06–07, version zeros) + 2 (AAE08–09, submit GET alternative) + 4
  (AAE02–05) + 8 (AAE10–17) + 2 (AAE18–19, admin audit) + 10 (AAE20–29) + 4 (AAE30–33, override) +
  1 (AAE34, description GET) + 3 (AAE35–37, query observe/read) + 4 (AAE40–43) + 4 (AAE44–47, I1)
  + 7 (AAE60–66) + 2 (AAE38–39) + 2 (AAE48–49) + 3 (AAE67–69) + 1 (AAE80) + 3 (AAE90–92) = **63**.
- AWS: 11 (AWS01–11) + 2 (AWS12a/b) + 1 (AWS13) + 2 (AWS14a/b) = **16**.

---

## Example 1: Agent-Memory Primary Flow on `/key/`, with Submit and Poll

### Scenario

A single-flow narrative for the `agent-memory-mvp` client: write a note as a keyed `Source`,
browse its directory without evaluating anything, inspect a recipe key before it runs, request the
derived value (which evaluates it), poll via `submit`/`info` instead of waiting inline, edit the
note's description (version unchanged), overwrite the note's data (version changes), and see the
dependent recompute. All addressing is bare keys on `/key/`; the recipe `-R/notes/a.txt/-/upper/summary.txt`
defines `summary.txt` as `upper(notes/a.txt)`.

Query validated: `cargo run -p liquers-core --features cli --bin liquers-validate -- --command
make_text --command upper -- '-R/notes/a.txt/-/upper/summary.txt'` → `Ok`, one `Resource` segment
into a `Transform` segment, filename `summary.txt` (see "Validated Queries" in the Test Plan).

### HTTP Transcript (excerpt)

```
POST /api/assets/key/data/notes/a.txt?type_identifier=Text&data_format=txt&title=Note%20A&description=First%20note
  body: hello
→ 201 {"status":"OK","result":{"key":"notes/a.txt","status":"Source","title":"Note A", ...}}

GET /api/assets/key/listdir/notes
→ 200 {"status":"OK","result":{"assets":[{"key":"notes/a.txt","status":"Source",...}]}}

GET /api/assets/key/info/summary.txt          # before evaluation, never triggers it
→ 200 {"status":"OK","result":{"status":"Recipe","title":"Summary",...}}

POST /api/assets/key/submit/summary.txt       # (b) submit: returns immediately
→ 200 {"status":"OK","result":{"status":"Submitted"|"Processing"|"Ready",...}}

GET /api/assets/key/info/summary.txt          # (c) poll until Ready
→ 200 {"status":"OK","result":{"status":"Ready",...}}   (repeated until Ready)

GET /api/assets/key/data/summary.txt
→ 200  HELLO

POST /api/assets/key/description/notes/a.txt   Content-Type: application/json
  {"description":"Edited"}
→ 200 {"status":"OK","result":{"title":"Note A","description":"Edited",...}}

GET /api/assets/key/version/notes/a.txt  →  200 {"result":{"version":"<32 hex>"}}

POST /api/assets/key/data/notes/a.txt
  body: world
→ 201 (new Source; version changes)

GET /api/assets/key/version/notes/a.txt  →  version differs from before

GET /api/assets/key/data/summary.txt  → 200  WORLD   (dependent recomputed)
```

### Shared Helpers (`assets_api_endpoints.rs`)

Used by Example 1, Example 2b, and all `AAE*` tests in this file.

```rust
// liquers-axum/tests/assets_api_endpoints.rs — shared helpers for the whole file

use std::collections::HashMap;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use liquers_core::{
    assets::AssetManager,
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
use tower::ServiceExt;

/// Environment with `make_text` (returns "generated") and `upper` (uppercases input) plus
/// recipes stored at the store root (`recipes.yaml`, cwd = root).
async fn env_with(recipes: &[(&str, &str, &str)]) -> EnvRef<SimpleEnvironment<Value>> {
    env_with_at(&Key::new(), recipes).await
}

/// Same, but the `RecipeList` is stored at `<dir>/recipes.yaml`, so a recipe's plain filename
/// resolves under `dir` (`DefaultRecipeProvider::recipe_opt` reads `<key's directory>/recipes.yaml`
/// and joins the recipe's filename to that directory — a nested path segment in the recipe query
/// itself is NOT a directory: it parses as a second chained action and fails to resolve. See
/// "Fixes Made Against the Drafts" #4.
async fn env_with_at(
    dir: &Key,
    recipes: &[(&str, &str, &str)],
) -> EnvRef<SimpleEnvironment<Value>> {
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
    // I1: two commands returning non-Bytes/Text core Value variants (Value::I64, Value::Object).
    env.command_registry
        .register_command(CommandKey::new_name("make_number"), |_, _, _| {
            Ok(Value::I64(42))
        })
        .unwrap();
    env.command_registry
        .register_command(CommandKey::new_name("make_object"), |_, _, _| {
            let mut map = std::collections::BTreeMap::new();
            map.insert("a".to_string(), Value::I64(1));
            map.insert("b".to_string(), Value::Text("two".to_string()));
            Ok(Value::Object(map))
        })
        .unwrap();
    let mut rl = RecipeList::new();
    for (q, t, d) in recipes {
        rl.add_recipe(Recipe::new(q.to_string(), t.to_string(), d.to_string()).unwrap());
    }
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &dir.join("recipes.yaml"),
            serde_yaml::to_string(&rl).unwrap().as_bytes(),
            &Metadata::new(),
        )
        .await
        .unwrap();
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    env.to_ref()
}

/// Minimal MetadataRecord for a Source write via `AssetManager::set_binary`.
/// `type_identifier`/`type_name` are `String`, not `Option<String>`; `set_binary` takes the
/// record BY VALUE (no leading `&`).
fn metadata_text() -> MetadataRecord {
    MetadataRecord {
        type_identifier: "Text".to_string(),
        type_name: "text".to_string(),
        data_format: Some("txt".to_string()),
        ..Default::default()
    }
}

/// Build the Assets API router with the base path `/api/assets`.
fn build_app(envref: EnvRef<SimpleEnvironment<Value>>) -> axum::Router {
    liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .build()
        .with_state(envref)
}

/// Build the assets router merged with the Query API at `/q` (used by AAE45, I1's cross-check).
fn build_app_with_query_api(envref: EnvRef<SimpleEnvironment<Value>>) -> axum::Router {
    let assets = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .build();
    let query = liquers_axum::query::QueryApiBuilder::<SimpleEnvironment<Value>>::new("/q").build();
    assets.merge(query).with_state(envref)
}

/// Send a request, return (status, parsed JSON body). Missing/non-JSON body → `Value::Null`.
async fn send(app: axum::Router, method: &str, uri: &str, body: Body) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(Request::builder().method(method).uri(uri).body(body).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// Send a request, return (status, raw bytes, content-type header if present).
async fn send_raw(app: axum::Router, method: &str, uri: &str, body: Body) -> (StatusCode, Vec<u8>, Option<String>) {
    let resp = app
        .oneshot(Request::builder().method(method).uri(uri).body(body).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, bytes.to_vec(), content_type)
}

/// Send a JSON body request (sets Content-Type: application/json — axum's `Json` extractor
/// otherwise answers 415 before the handler runs; this was a real drafting bug in v1).
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

/// POST a JSON `DataEntry` to `key/entry`.
async fn post_entry_json(app: axum::Router, key: &str, entry: serde_json::Value) -> (StatusCode, serde_json::Value) {
    send_json(
        app,
        "POST",
        &format!("/api/assets/key/entry/{}?format=json", key),
        &serde_json::to_string(&entry).unwrap(),
    )
    .await
}

/// Poll `key/info` or `q/info` until `status` matches `want`, or panic after `max_attempts`.
async fn poll_until(app: axum::Router, uri: &str, want: &str, max_attempts: u32) -> serde_json::Value {
    for attempt in 1..=max_attempts {
        let (status, json) = send(app.clone(), "GET", uri, Body::empty()).await;
        assert_eq!(status, StatusCode::OK, "poll #{attempt} on {uri} must be 200");
        if json["result"]["status"] == want {
            return json;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("polling {uri} for status {want} timed out after {max_attempts} attempts");
}
```

### Example 1 test

```rust
#[tokio::test]
async fn aae01_agent_memory_primary_flow_submit_and_poll() {
    let envref = env_with(&[(
        "-R/notes/a.txt/-/upper/summary.txt",
        "Summary",
        "Upper-cased note",
    )])
    .await;
    let app = build_app(envref.clone());

    // 1. Write a note via key/data (mode: mutation).
    let (status, json) = send(
        app.clone(),
        "POST",
        "/api/assets/key/data/notes/a.txt?type_identifier=Text&data_format=txt&title=Note%20A&description=First%20note",
        Body::from("hello"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "POST key/data returns 201");
    assert_eq!(json["status"], "OK");
    assert_eq!(json["result"]["key"], "notes/a.txt");
    assert_eq!(json["result"]["status"], "Source");
    assert_eq!(json["result"]["title"], "Note A");

    // 2. Browse the directory — mode (c), never evaluates.
    let (status, json) = send(app.clone(), "GET", "/api/assets/key/listdir/notes", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    let assets = json["result"]["assets"].as_array().unwrap();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0]["key"], "notes/a.txt");
    assert_eq!(assets[0]["status"], "Source", "listing must not evaluate");

    // 3. Inspect the recipe key before it has ever run — mode (c).
    let (status, json) = send(app.clone(), "GET", "/api/assets/key/info/summary.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["status"], "Recipe");
    assert_eq!(json["result"]["title"], "Summary");

    // 4. Submit (mode b) instead of a blocking read — returns immediately.
    let (status, json) = send(app.clone(), "POST", "/api/assets/key/submit/summary.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "submit returns 200 immediately");
    assert!(
        ["Submitted", "Processing", "Ready", "Dependencies"].contains(&json["result"]["status"].as_str().unwrap()),
        "submit returns a live AssetInfo, not a fixed status"
    );

    // 5. Poll (mode c) until Ready — the reliable path, WebSocket or not.
    poll_until(app.clone(), "/api/assets/key/info/summary.txt", "Ready", 50).await;

    // 6. Read the now-cached derived value.
    let (status, body, _) = send_raw(app.clone(), "GET", "/api/assets/key/data/summary.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(String::from_utf8(body).unwrap(), "HELLO");

    // 7. Edit the note's description — title unchanged, version untouched.
    let (status, json) = send_json(
        app.clone(),
        "POST",
        "/api/assets/key/description/notes/a.txt",
        r#"{"description":"Edited"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["description"], "Edited");
    assert_eq!(json["result"]["title"], "Note A");

    let (_, json) = send(app.clone(), "GET", "/api/assets/key/version/notes/a.txt", Body::empty()).await;
    let version_before = json["result"]["version"].as_str().unwrap().to_string();

    // 8. Overwrite the note's data — version changes.
    let (status, _) = send(app.clone(), "POST", "/api/assets/key/data/notes/a.txt", Body::from("world")).await;
    assert_eq!(status, StatusCode::CREATED);

    let (_, json) = send(app.clone(), "GET", "/api/assets/key/version/notes/a.txt", Body::empty()).await;
    let version_after = json["result"]["version"].as_str().unwrap().to_string();
    assert_ne!(version_before, version_after, "version must change after overwrite");

    // 9. The dependent recomputes on next read.
    let (status, body, _) = send_raw(app, "GET", "/api/assets/key/data/summary.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(String::from_utf8(body).unwrap(), "WORLD");
}
```

---

## Example 2: Removal Decision Table (core)

### Scenario

Six rows and the directory conflict, exactly as Phase 2's `remove(key)` decision table, tested
directly against `AssetManager` (no HTTP): Source-without-recipe cascades and deletes; Override-
with-recipe recomputes on the next read; Ready-computed keeps its metadata record and version
(dropped to `Recipe`, no cascade); a nonexistent key with no recipe is `KeyNotFound`; a never-
evaluated recipe key is an idempotent no-op; a directory is `StatusConflict` (409), naming
`removedir`. See AMR01–AMR07 below (full code in "Unit Tests").

### Context

`liquers-core/tests/asset_manager_remove_expire_describe.rs`, target crate `liquers-core`, no HTTP
layer — this is the core contract every HTTP mutation ultimately calls through.

### Expected output (row-by-row)

| Row | Status | `has_recipe` | `remove` result | Store afterwards |
|---|---|---|---|---|
| AMR01 | `Source` | false | `Ok(())`, cascades | key deleted |
| AMR02 | `Override` | true | `Ok(())`, next read recomputes | key deleted |
| AMR03 | `Ready`/computed | true | `Ok(())`, no cascade | metadata kept, `status: Recipe`, version unchanged |
| AMR05 | absent | false | `Err(KeyNotFound)` | — |
| AMR06 | absent, never evaluated | true | `Ok(())` (idempotent) | unchanged |
| AMR07 | `Directory` | any | `Err(StatusConflict)` | unchanged |

---

## Example 2b: Submit-and-Poll on `/q/` Without WebSockets

### Scenario

The non-keyed counterpart of Example 1's submit/poll step, proving mode (b)→(c) works end to end
with no WebSocket involved, and that polling an unsubmitted query never evaluates it. Includes a
**deterministic** cancel test (fix against draft 5's non-deterministic AAE92): a slow command is
submitted, cancelled while `Processing`, then polled until `Cancelled`.

### HTTP Transcript

```
POST /api/assets/q/submit/make_text
→ 200 {"result":{"query":"make_text","status":"Submitted"|"Processing"|"Ready", ...}}

GET /api/assets/q/info/make_text     (poll, 20ms interval, bounded)
→ 200 {"result":{"status":"Processing", ...}}
  ... repeated ...
→ 200 {"result":{"status":"Ready", ...}}

GET /api/assets/q/data/make_text
→ 200  generated

GET /api/assets/q/info/never_submitted_query   (never POSTed)
→ 404 {"status":"ERROR","error":{"type":"NotAvailable"},"message":"...submit it first..."}
  (repeated 5 times, always 404 — no evaluation happens)

POST /api/assets/q/submit/sleep_long
→ 200 {"result":{"status":"Submitted"|"Processing", ...}}
GET /api/assets/q/info/sleep_long                # poll until Processing (deterministic gate)
→ 200 {"result":{"status":"Processing", ...}}
POST /api/assets/q/cancel/sleep_long
→ 200 {"result":{"status":"Cancelled"|"Processing", ...}}
GET /api/assets/q/info/sleep_long                # poll until Cancelled
→ 200 {"result":{"status":"Cancelled", ...}}
```

### Tests (appended to `assets_api_endpoints.rs`, after `AAE66`)

`sleep_long` is a blocking `std::thread::sleep` command, so `aae92` (and only that test — it is
the sole user of `sleep_long` in this file) runs on the multi-thread test runtime, per the
mandatory fix for blocking sync commands.

```rust
#[tokio::test]
async fn aae90_submit_and_poll_non_keyed_query() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);

    let (status, json) = send(app.clone(), "POST", "/api/assets/q/submit/make_text", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "submit must return 200");
    assert_eq!(json["status"], "OK");
    let initial_status = json["result"]["status"].as_str().expect("status field present");
    assert!(
        ["Submitted", "Processing", "Ready"].contains(&initial_status),
        "submit returns a valid AssetInfo status immediately, got: {initial_status}"
    );

    poll_until(app.clone(), "/api/assets/q/info/make_text", "Ready", 50).await;

    let (status, body, _) = send_raw(app, "GET", "/api/assets/q/data/make_text", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(String::from_utf8(body).unwrap(), "generated");
}

#[tokio::test]
async fn aae91_polling_unsubmitted_query_404_never_evaluates() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);

    for poll_num in 1..=5 {
        let (status, json) = send(app.clone(), "GET", "/api/assets/q/info/never_submitted_query", Body::empty()).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "poll #{poll_num}: q/info for unsubmitted query is 404");
        assert_eq!(json["error"]["type"], "NotAvailable");
        assert!(
            json["message"].as_str().unwrap_or("").to_lowercase().contains("submit"),
            "message must direct the client to submit first"
        );
    }
}

/// Blocking `std::thread::sleep` inside the registered command → multi-thread test runtime, so
/// the queued manager's worker (a spawned tokio task) is not starved by this test's own runtime.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aae92_cancel_while_processing_reports_cancelled_deterministically() {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("sleep_long"), |_, _, _| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            Ok(Value::from("slept"))
        })
        .unwrap();
    let envref = env.to_ref();
    let app = build_app(envref);

    let (status, _) = send(app.clone(), "POST", "/api/assets/q/submit/sleep_long", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);

    // Deterministic gate: wait until the asset is actually Processing before cancelling, so the
    // cancel is not racing a job that has not started yet.
    poll_until(app.clone(), "/api/assets/q/info/sleep_long", "Processing", 50).await;

    let (status, json) = send(app.clone(), "POST", "/api/assets/q/cancel/sleep_long", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "POST q/cancel must return 200");
    assert_eq!(json["status"], "OK");

    let info = poll_until(app, "/api/assets/q/info/sleep_long", "Cancelled", 50).await;
    assert_eq!(info["result"]["status"], "Cancelled");
}
```

---

## Example 3: Metadata Allow-List (`POST key/entry`)

### Scenario

`ValueDescription` is the client-settable **allow-list** of a value's metadata: exactly
`type_identifier`, `data_format`, `media_type`, `title`, `description`. A `POST key/entry` body's
`metadata` object may carry anything (a hostile client, a stale client library, a JSON echo of a
previous `GET entry`); every field outside the five is silently dropped and named in the response
`message`, never applied. This is proven twice: unit-level in `ValueDescription` (VD01–VD11, in
`value_description.rs`) and HTTP-level via `POST key/entry` (AAE10–AAE17, in
`assets_api_endpoints.rs`), so a regression in either layer is caught independently.

The record handed to `AssetManager::set_binary` is always **built fresh** from `ValueDescription`
(`into_metadata_record`), never a client `MetadataRecord` cleaned in place — so a field added to
`MetadataRecord` later is not client-settable by accident (Phase 2, "Data Structures").

---

## Unit Tests

### `liquers-core/tests/asset_manager_remove_expire_describe.rs` — Shared Helpers (AMR)

```rust
// liquers-core/tests/asset_manager_remove_expire_describe.rs

use liquers_core::{
    assets::{AssetData, AssetManager, AssetNotificationMessage},
    command_metadata::CommandKey,
    context::{Environment, EnvRef, SimpleEnvironment},
    error::ErrorType,
    metadata::{Metadata, MetadataRecord, Status},
    parse::{parse_key, parse_query},
    query::{Key, Query},
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};

/// Build an environment over a provided store (used directly by the restart test, AMR24).
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

/// Environment with a `RecipeList` stored at `<dir>/recipes.yaml`. `DefaultRecipeProvider` reads
/// `<key's directory>/recipes.yaml` and joins the recipe's own filename to that directory — a
/// nested path segment written directly into the recipe query (e.g. `"make_text/data/x.txt"`)
/// does NOT act as a directory: `data` there parses as a second chained action and fails to
/// resolve (verified with `liquers-validate --no-registry`; see "Fixes Made Against the Drafts"
/// #4). A recipe targeting a key under `data/` therefore needs its `RecipeList` stored at
/// `data/recipes.yaml`, with a plain filename in the query (`"make_text/x.txt"`).
async fn env_with_at(dir: &Key, recipes: &[(&str, &str, &str)]) -> EnvRef<SimpleEnvironment<Value>> {
    let mut rl = RecipeList::new();
    for (q, t, d) in recipes {
        rl.add_recipe(Recipe::new(q.to_string(), t.to_string(), d.to_string()).unwrap());
    }
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &dir.join("recipes.yaml"),
            serde_yaml::to_string(&rl).unwrap().as_bytes(),
            &Metadata::new(),
        )
        .await
        .unwrap();
    env_over(store)
}

/// Root-level recipes (the common case).
async fn env_with(recipes: &[(&str, &str, &str)]) -> EnvRef<SimpleEnvironment<Value>> {
    env_with_at(&Key::new(), recipes).await
}

/// Minimal MetadataRecord for a plaintext Source write via `AssetManager::set_binary`.
/// `type_identifier`/`type_name` are `String`, not `Option<String>`; `set_binary` takes the
/// record BY VALUE.
fn metadata_text() -> MetadataRecord {
    MetadataRecord {
        type_identifier: "Text".to_string(),
        type_name: "text".to_string(),
        data_format: Some("txt".to_string()),
        ..Default::default()
    }
}

/// `AsyncStore::get_metadata` returns the `Metadata` enum; status reads through `.status()`.
fn stored_status(metadata: &Metadata) -> Status {
    metadata.status()
}
```

### Tests: `remove` Decision Table (AMR01–AMR07)

```rust
#[tokio::test]
async fn amr01_remove_source_no_recipe_cascades() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("-R/notes/a.txt/-/upper/summary.txt", "Summary Recipe", "Uppercase the note")]).await;
    let am = envref.get_asset_manager();
    let store = envref.get_async_store();

    let notes_key = parse_key("notes/a.txt")?;
    am.set_binary(&notes_key, b"hello world", metadata_text()).await?;
    assert_eq!(am.get_asset_info(&notes_key).await?.status, Status::Source);

    let summary_key = parse_key("summary.txt")?;
    let value = am.get(&summary_key).await?.get().await?;
    assert_eq!(value.try_into_string()?, "HELLO WORLD");
    assert_eq!(am.get_asset_info(&summary_key).await?.status, Status::Ready);

    am.remove(&notes_key).await?;

    assert!(!store.contains(&notes_key).await?, "store must not hold the removed source");
    let err = am.get_asset_info(&notes_key).await.expect_err("removed source is gone");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);
    assert_eq!(
        am.get_asset_info(&summary_key).await?.status,
        Status::Expired,
        "dependent cascades to Expired when its Source is removed"
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

    let value = am.get(&source_key).await?.get().await?;
    assert_eq!(value.try_into_string()?, "generated");
    assert_eq!(am.get_asset_info(&source_key).await?.status, Status::Ready);
    Ok(())
}

#[tokio::test]
async fn amr03_remove_ready_keeps_metadata_and_version() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("-R/notes/a.txt/-/upper/summary.txt", "Summary Recipe", "Uppercase the note")]).await;
    let am = envref.get_asset_manager();
    let store = envref.get_async_store();

    let notes_key = parse_key("notes/a.txt")?;
    am.set_binary(&notes_key, b"hello", metadata_text()).await?;

    let summary_key = parse_key("summary.txt")?;
    let value = am.get(&summary_key).await?.get().await?;
    assert_eq!(value.try_into_string()?, "HELLO");
    let version_before = am.version(&summary_key).await?;
    assert!(version_before.is_some());

    am.remove(&summary_key).await?;

    assert!(store.contains(&summary_key).await?, "metadata must survive the remove");
    let stored = store.get_metadata(&summary_key).await?;
    assert_eq!(stored_status(&stored), Status::Recipe);
    assert_eq!(am.version(&summary_key).await?, version_before, "version must survive the remove");
    assert_eq!(am.get_asset_info(&notes_key).await?.status, Status::Source, "the source is untouched");
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

    am.remove(&summary_key).await?;
    let value_again = am.get(&summary_key).await?.get().await?;
    assert_eq!(value_again.try_into_string()?, "HELLO");

    assert_eq!(
        am.get_asset_info(&summary2_key).await?.status,
        Status::Ready,
        "the grandchild stays Ready: dropping a recipe-computed value does not cascade"
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
    assert_eq!(err.key, Some(dir_key.encode()), "status_conflict sets the key");
    Ok(())
}
```

### Tests: `expire` and `set_description` (AMR10–AMR20)

```rust
#[tokio::test]
async fn amr10_expire_live_ready_cascades() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/a.txt", "A", "text A"), ("-R/a.txt/-/upper/b.txt", "B", "B depends on A")]).await;
    let am = envref.get_asset_manager();
    let (key_a, key_b) = (parse_key("a.txt")?, parse_key("b.txt")?);
    let _ = am.get(&key_a).await?.get().await?;
    let _ = am.get(&key_b).await?.get().await?;

    am.expire(&key_a).await.expect("expire should succeed");

    assert_eq!(am.get_asset_info(&key_a).await?.status, Status::Expired);
    assert_eq!(am.get_asset_info(&key_b).await?.status, Status::Expired, "dependent cascades");
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
    assert_eq!(err.key, Some(key.encode()), "Error.key is Option<String>, from key.encode()");
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
    assert_eq!(info.title, "New Title");
    assert_eq!(info.description, "New Desc");
    assert_eq!(am.version(&key).await?, version_before, "version unchanged (also covers set_description's happy-path idempotence on version)");
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

    let err = am.set_description(&key, Some("Title".to_string()), None).await.expect_err("only Source may be described");
    assert_eq!(err.error_type, ErrorType::StatusConflict);
    Ok(())
}

#[tokio::test]
async fn amr20_set_description_absent_key_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("nonexistent.txt")?;

    let err = am.set_description(&key, Some("Title".to_string()), None).await.expect_err("absent key");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);
    Ok(())
}
```

### Tests: `get_asset_info` Observation, Restart Fast-Track (AMR22–AMR24)

```rust
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

#[tokio::test]
async fn amr24_dropped_intermediate_does_not_block_dependent_after_restart() -> Result<(), Box<dyn std::error::Error>> {
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
        am.remove(&summary_key).await?;
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

### Tests: `removedir` (AMR30–AMR34)

Recipe-under-directory cases (AMR32, AMR33) use `env_with_at` with a `RecipeList` at
`data/recipes.yaml`, so recipe filenames stay plain (`make_text/computed.txt`) and resolve under
`data/` through the directory, not through a nested path segment inside the query.

```rust
#[tokio::test]
async fn amr30_removedir_deletes_stored_keys_recursively() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let store = envref.get_async_store();

    let root_key = parse_key("data")?;
    let file1 = parse_key("data/file1.txt")?;
    let file2 = parse_key("data/file2.txt")?;
    let subdir_file = parse_key("data/sub/file3.txt")?;

    am.set_binary(&file1, b"content1", metadata_text()).await?;
    am.set_binary(&file2, b"content2", metadata_text()).await?;
    am.set_binary(&subdir_file, b"content3", metadata_text()).await?;
    am.makedir(&root_key).await?;

    am.removedir(&root_key).await?;

    assert!(!store.contains(&file1).await?);
    assert!(!store.contains(&file2).await?);
    assert!(!store.contains(&subdir_file).await?);
    let err = am.get_asset_info(&root_key).await.expect_err("directory removed");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);
    Ok(())
}

#[tokio::test]
async fn amr31_removedir_source_child_cascades_dependents() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with_at(
        &parse_key("data")?,
        &[("-R/data/source.txt/-/upper/derived.txt", "Derived", "")],
    )
    .await;
    let am = envref.get_asset_manager();

    let data_key = parse_key("data")?;
    let source_key = parse_key("data/source.txt")?;
    let derived_key = parse_key("data/derived.txt")?;

    am.set_binary(&source_key, b"hello", metadata_text()).await?;
    let _ = am.get(&derived_key).await?.get().await?;

    am.makedir(&data_key).await?;
    am.removedir(&data_key).await?;

    let err = am.get_asset_info(&source_key).await.expect_err("source is gone");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);
    assert_eq!(am.get_asset_info(&derived_key).await?.status, Status::Expired, "dependent cascades");
    Ok(())
}

// BLOCKED on Phase 2 O15 (final review): `store.removedir` deletes the kept `Recipe` entry and
// `data/recipes.yaml`, so AMR32 and AMR33 fail as written. Rewrite both once O15 is answered.
#[tokio::test]
async fn amr32_removedir_computed_child_dropped_with_version() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with_at(&parse_key("data")?, &[("make_text/computed.txt", "Computed", "")]).await;
    let am = envref.get_asset_manager();
    let store = envref.get_async_store();

    let data_key = parse_key("data")?;
    let computed_key = parse_key("data/computed.txt")?;

    let _ = am.get(&computed_key).await?.get().await?;
    let version_before = am.version(&computed_key).await?;

    am.makedir(&data_key).await?;
    am.removedir(&data_key).await?;

    assert!(store.contains(&computed_key).await?, "metadata should survive");
    let stored = store.get_metadata(&computed_key).await?;
    assert_eq!(stored_status(&stored), Status::Recipe);
    assert_eq!(am.version(&computed_key).await?, version_before, "version preserved");
    Ok(())
}

#[tokio::test]
async fn amr33_removedir_recipe_declared_keys_survive() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with_at(&parse_key("data")?, &[("make_text/recipe_only.txt", "RecipeOnly", "")]).await;
    let am = envref.get_asset_manager();

    let data_key = parse_key("data")?;
    let recipe_only_key = parse_key("data/recipe_only.txt")?;

    am.makedir(&data_key).await?;
    am.removedir(&data_key).await?;

    let info = am.get_asset_info(&recipe_only_key).await?;
    assert_eq!(info.status, Status::Recipe, "recipe-declared key survives directory removal");
    Ok(())
}

#[tokio::test]
async fn amr34_removedir_absent_directory_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let nonexistent_key = parse_key("nonexistent_dir")?;

    let err = am.removedir(&nonexistent_key).await.expect_err("no such directory");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);
    Ok(())
}
```

### Tests: `lookup_query_asset` (AMR40–AMR44)

```rust
#[tokio::test]
async fn amr40_lookup_query_asset_none_before_request() {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let query = parse_query("make_text").unwrap();
    assert!(am.lookup_query_asset(&query).is_none(), "query must not exist before get_asset");
}

#[tokio::test]
async fn amr41_lookup_query_asset_some_after_get_asset() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let query = parse_query("make_text")?;
    let _ = am.get_asset(&query).await?;
    assert!(am.lookup_query_asset(&query).is_some(), "query is cached after get_asset");
    Ok(())
}

#[tokio::test]
async fn amr42_lookup_query_asset_never_creates() {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let query = parse_query("make_text").unwrap();
    assert!(am.lookup_query_asset(&query).is_none());
    assert!(am.lookup_query_asset(&query).is_none(), "second lookup is also None: no side effect");
}

#[tokio::test]
async fn amr43_lookup_query_asset_pure_key_delegates() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("notes/a.txt")?;
    am.set_binary(&key, b"content", metadata_text()).await?;

    let pure_key_query = liquers_core::parse::parse_query("-R/notes/a.txt")?;
    assert!(am.lookup_query_asset(&pure_key_query).is_some(), "pure-key query delegates to lookup_key_asset");
    Ok(())
}

#[tokio::test]
async fn amr44_lookup_query_asset_after_removal_none() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("notes/a.txt")?;
    am.set_binary(&key, b"content", metadata_text()).await?;

    let query = liquers_core::parse::parse_query("-R/notes/a.txt")?;
    let _ = am.get_asset(&query).await?;
    am.remove(&key).await?;

    assert!(am.lookup_query_asset(&query).is_none(), "after removal, lookup returns None");
    Ok(())
}
```

### Tests: `AssetNotificationMessage::Removed` (AMR50–AMR53)

```rust
#[tokio::test]
async fn amr50_notification_removed_on_remove() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("data/asset.txt")?;

    am.set_binary(&key, b"content", metadata_text()).await?;
    let asset_ref = am.get(&key).await?;
    let mut notification_rx = asset_ref.subscribe_to_notifications();

    am.remove(&key).await?;

    tokio::time::timeout(std::time::Duration::from_secs(1), notification_rx.changed()).await??;
    assert_eq!(*notification_rx.borrow(), AssetNotificationMessage::Removed);
    Ok(())
}

#[tokio::test]
async fn amr51_notification_removed_on_set_binary_replacement() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("data/asset.txt")?;

    am.set_binary(&key, b"content1", metadata_text()).await?;
    let asset_ref = am.get(&key).await?;
    let mut notification_rx = asset_ref.subscribe_to_notifications();

    am.set_binary(&key, b"content2", metadata_text()).await?;

    tokio::time::timeout(std::time::Duration::from_secs(1), notification_rx.changed()).await??;
    assert_eq!(*notification_rx.borrow(), AssetNotificationMessage::Removed);
    Ok(())
}

#[tokio::test]
async fn amr52_notification_removed_not_sent_by_expiration() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/derived.txt", "Derived", "")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("derived.txt")?;

    let _ = am.get(&key).await?.get().await?;
    let asset_ref = am.get(&key).await?;
    let mut notification_rx = asset_ref.subscribe_to_notifications();

    am.expire(&key).await?;

    tokio::time::timeout(std::time::Duration::from_secs(1), notification_rx.changed()).await??;
    assert_eq!(*notification_rx.borrow(), AssetNotificationMessage::Expired);
    assert_ne!(*notification_rx.borrow(), AssetNotificationMessage::Removed);
    Ok(())
}

#[tokio::test]
async fn amr53_subscription_ends_after_removed() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("data/asset.txt")?;

    am.set_binary(&key, b"content", metadata_text()).await?;
    let asset_ref = am.get(&key).await?;
    let mut notification_rx = asset_ref.subscribe_to_notifications();

    am.remove(&key).await?;
    tokio::time::timeout(std::time::Duration::from_secs(1), notification_rx.changed()).await??;

    let result = tokio::time::timeout(std::time::Duration::from_millis(100), notification_rx.changed()).await;
    assert!(result.is_err(), "no further changes after Removed: the watch has no more senders");
    Ok(())
}
```

### Tests: `to_override` on `Source` (AMR60–AMR61)

```rust
#[tokio::test]
async fn amr60_to_override_stored_only_source_noop() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("data/source.txt")?;
    am.set_binary(&key, b"content", metadata_text()).await?;
    assert_eq!(am.get_asset_info(&key).await?.status, Status::Source);

    am.to_override(&key).await?;

    assert_eq!(am.get_asset_info(&key).await?.status, Status::Source, "to_override on stored Source is a no-op");
    Ok(())
}

#[tokio::test]
async fn amr61_to_override_live_source_noop() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("data/source.txt")?;
    am.set_binary(&key, b"content", metadata_text()).await?;
    let _ = am.get(&key).await?; // make it live

    assert_eq!(am.get_asset_info(&key).await?.status, Status::Source);
    am.to_override(&key).await?;
    assert_eq!(am.get_asset_info(&key).await?.status, Status::Source, "to_override on a live Source is a no-op");
    Ok(())
}
```

---

### `liquers-axum/src/assets/value_description.rs` `#[cfg(test)] mod tests` (VD01–VD11)

```rust
// liquers-axum/src/assets/value_description.rs — #[cfg(test)] mod tests

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::{
        error::ErrorType,
        metadata::{AssetInfo, Status},
        type_system::TypeRegistry,
        value::Value,
    };
    use serde_json::json;
    use std::collections::HashMap;

    /// `TypeRegistry::from_value_type` is the verified public constructor
    /// (`liquers-core/src/type_system.rs`).
    fn test_registry() -> TypeRegistry {
        TypeRegistry::from_value_type::<Value>()
    }

    #[test]
    fn vd01_from_json_keeps_five_fields() {
        let value = json!({
            "type_identifier": "Text", "data_format": "txt", "media_type": "text/plain",
            "title": "My Note", "description": "A description"
        });
        let (desc, dropped) = ValueDescription::from_json(&value).expect("from_json");
        assert_eq!(desc.type_identifier, Some("Text".to_string()));
        assert_eq!(desc.data_format, Some("txt".to_string()));
        assert_eq!(desc.media_type, Some("text/plain".to_string()));
        assert_eq!(desc.title, Some("My Note".to_string()));
        assert_eq!(desc.description, Some("A description".to_string()));
        assert!(dropped.is_empty(), "no allow-listed field should be dropped");
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
            "type_identifier": "Text", "extra_field_z": "ignored", "extra_field_a": "ignored", "title": "Title"
        });
        let (_, dropped) = ValueDescription::from_json(&value).expect("from_json");
        assert_eq!(dropped, vec!["extra_field_a".to_string(), "extra_field_z".to_string()], "sorted alphabetically");
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
        // AssetInfo.title/description are String, not Option<String>.
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
            type_identifier: None, data_format: None, media_type: None,
            title: Some("New Title".to_string()), description: None,
        };
        let merged = desc.or_previous(Some(&previous));
        assert_eq!(merged.type_identifier, Some("Text".to_string()));
        assert_eq!(merged.data_format, Some("txt".to_string()));
        assert_eq!(merged.title, Some("New Title".to_string()), "explicit field is kept");
        assert_eq!(merged.description, Some("Previous Desc".to_string()), "missing field is filled");
        assert_eq!(merged.media_type, None, "media_type is never filled from previous");
    }

    #[test]
    fn vd07_into_metadata_record_default_type_identifier_is_bytes() {
        let desc = ValueDescription {
            type_identifier: None, data_format: Some("txt".to_string()), media_type: None,
            title: Some("Note".to_string()), description: None,
        };
        let record = desc.into_metadata_record(&test_registry()).expect("into_metadata_record");
        assert_eq!(record.type_identifier, "Bytes", "None defaults to Bytes");
    }

    #[test]
    fn vd08_into_metadata_record_type_name_from_registry() {
        let desc = ValueDescription {
            type_identifier: Some("Text".to_string()), data_format: None, media_type: None,
            title: None, description: None,
        };
        let record = desc.into_metadata_record(&test_registry()).expect("into_metadata_record");
        assert_eq!(record.type_name, "text", "type_name resolved from the registry");
    }

    #[test]
    fn vd09_into_metadata_record_unknown_identifier_is_parameter_error() {
        let desc = ValueDescription {
            type_identifier: Some("UnknownType".to_string()), data_format: None, media_type: None,
            title: None, description: None,
        };
        let err = desc.into_metadata_record(&test_registry()).expect_err("unknown type");
        assert_eq!(err.error_type, ErrorType::ParameterError, "unknown type_identifier is a 400 at the boundary");
    }

    #[test]
    fn vd10_into_metadata_record_untouched_fields_carry_real_defaults() {
        // MetadataRecord::default().status == Status::None (Status::default() = Self::None);
        // `stored` is Option<bool>, default None. into_metadata_record does not decide status —
        // that happens later, in set_binary, from recipe_opt.
        let desc = ValueDescription {
            type_identifier: Some("Bytes".to_string()), data_format: Some("bin".to_string()),
            media_type: Some("application/octet-stream".to_string()),
            title: Some("Data".to_string()), description: Some("Some binary".to_string()),
        };
        let record = desc.into_metadata_record(&test_registry()).expect("into_metadata_record");
        assert_eq!(record.status, Status::None, "into_metadata_record does not decide status");
        assert_eq!(record.stored, None, "stored: Option<bool>; None means true by convention");
        assert!(record.dependencies.is_empty());
    }

    #[test]
    fn vd11_or_previous_fills_type_and_format_only_as_a_pair_from_data() {
        // A never-evaluated recipe key reports type_identifier: "" and the recipe's data_format;
        // neither is inherited (a plain POST data onto a recipe key would otherwise get a 400 or
        // 422). Title is still inherited. A client-named type never inherits a format.
        let recipe_info = AssetInfo {
            status: Status::Recipe, title: "Recipe Title".to_string(),
            type_identifier: String::new(), data_format: Some("txt".to_string()),
            ..AssetInfo::new()
        };
        let merged = ValueDescription::default().or_previous(Some(&recipe_info));
        assert_eq!(merged.type_identifier, None, "empty type_identifier from Recipe is not inherited");
        assert_eq!(merged.data_format, None, "format is not inherited from Recipe");
        assert_eq!(merged.title, Some("Recipe Title".to_string()), "title is still inherited");

        let ready_text = AssetInfo {
            status: Status::Ready, type_identifier: "Text".to_string(), data_format: Some("txt".to_string()),
            ..AssetInfo::new()
        };
        let named = ValueDescription { type_identifier: Some("Bytes".to_string()), ..ValueDescription::default() };
        let merged = named.or_previous(Some(&ready_text));
        assert_eq!(merged.type_identifier, Some("Bytes".to_string()), "client-named type kept");
        assert_eq!(merged.data_format, None, "a client-named type does not inherit a format");
    }
}
```

---

## Integration Tests

### `liquers-axum/tests/assets_api_endpoints.rs` (AAE)

Shared helpers: see Example 1 above (`env_with`, `env_with_at`, `metadata_text`, `build_app`,
`build_app_with_query_api`, `send`, `send_raw`, `send_json`, `post_entry_json`, `poll_until`).

#### AAE02–AAE05: Q11 Override, Metadata Read, `Accept` Negotiation, Positive Cancel

```rust
#[tokio::test]
async fn aae02_post_data_onto_recipe_key_creates_override() {
    let envref = env_with(&[("make_text/source.txt", "Text", "A text")]).await;
    let app = build_app(envref.clone());

    let (_, info1) = send(app.clone(), "GET", "/api/assets/key/info/source.txt", Body::empty()).await;
    assert_eq!(info1["result"]["status"], "Recipe");

    let (status, json) = send(app.clone(), "POST", "/api/assets/key/data/source.txt", Body::from("override")).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["result"]["status"], "Override");

    let (status, json) = send(app, "DELETE", "/api/assets/key/data/source.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["new_status"], "Recipe", "DELETE on an Override with a recipe returns Recipe");
}

#[tokio::test]
async fn aae03_get_metadata_reads_source() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"test", metadata_text()).await.unwrap();

    let (status, json) = send(app, "GET", "/api/assets/key/metadata/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["type_identifier"], "Text");
}

#[tokio::test]
async fn aae04_get_entry_negotiates_accept_json() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"hello", metadata_text()).await.unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/assets/key/entry/notes/a.txt")
                .header("accept", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(axum::http::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()),
        Some("application/json"),
        "Accept: application/json must be honored, not ignored (Phase 2 fixes the empty-HeaderMap bug)"
    );
}

#[tokio::test]
async fn aae05_post_cancel_on_live_asset_returns_200() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"test", metadata_text()).await.unwrap();
    let _ = am.get(&parse_key("notes/a.txt").unwrap()).await.unwrap();

    let (status, json) = send(app, "POST", "/api/assets/key/cancel/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "cancel on a live asset returns 200");
    assert_eq!(json["status"], "OK");
}
```

#### AAE10–AAE17: Metadata Allow-List under `POST key/entry`

```rust
#[tokio::test]
async fn aae10_hostile_status_field_ignored() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    use base64::prelude::*;
    let entry = serde_json::json!({
        "metadata": {"type_identifier": "Text", "title": "Test", "status": "Error"},
        "data": BASE64_STANDARD.encode(b"hello")
    });

    let (status, resp) = post_entry_json(app.clone(), "notes/a.txt", entry).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(resp["message"].as_str().unwrap().to_lowercase().contains("status"), "message names the dropped field");

    let (_, info) = send(app, "GET", "/api/assets/key/info/notes/a.txt", Body::empty()).await;
    assert_eq!(info["result"]["status"], "Source", "status is the real Source, not the hostile Error");
}

#[tokio::test]
async fn aae11_hostile_stored_false_ignored() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    use base64::prelude::*;
    let entry = serde_json::json!({"metadata": {"type_identifier": "Text", "stored": false}, "data": BASE64_STANDARD.encode(b"data")});

    let (status, resp) = post_entry_json(app.clone(), "notes/c.txt", entry).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(resp["message"].as_str().unwrap().to_lowercase().contains("stored"));

    let (_, info) = send(app, "GET", "/api/assets/key/contains/notes/c.txt", Body::empty()).await;
    assert_eq!(info["result"]["contains"], true, "the manager always persists a user value");
}

#[tokio::test]
async fn aae12_hostile_dependencies_ignored() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    use base64::prelude::*;
    let entry = serde_json::json!({
        "metadata": {"type_identifier": "Text", "dependencies": [{"key": "fake/key.txt", "version": "00000000000000000000000000000000"}]},
        "data": BASE64_STANDARD.encode(b"data")
    });
    let (status, resp) = post_entry_json(app, "notes/d.txt", entry).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(resp["message"].as_str().unwrap().to_lowercase().contains("dependencies"));
}

#[tokio::test]
async fn aae13_hostile_expiration_time_ignored() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    use base64::prelude::*;
    let entry = serde_json::json!({
        "metadata": {"type_identifier": "Text", "expiration_time": "1970-01-01T00:00:00Z"},
        "data": BASE64_STANDARD.encode(b"data")
    });
    let (status, resp) = post_entry_json(app.clone(), "notes/e.txt", entry).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(resp["message"].as_str().unwrap().to_lowercase().contains("expiration_time"));

    let (status, _, _) = send_raw(app, "GET", "/api/assets/key/data/notes/e.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "value is readable, not treated as expired");
}

#[tokio::test]
async fn aae14_unknown_type_identifier_400() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    use base64::prelude::*;
    let entry = serde_json::json!({"metadata": {"type_identifier": "UnknownType12345"}, "data": BASE64_STANDARD.encode(b"data")});
    let (status, resp) = post_entry_json(app, "notes/f.txt", entry).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["error"]["type"], "ParameterError");
}

#[tokio::test]
async fn aae15_non_object_metadata_400() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    use base64::prelude::*;
    let entry = serde_json::json!({"metadata": "not an object", "data": BASE64_STANDARD.encode(b"data")});
    let (status, resp) = post_entry_json(app, "notes/g.txt", entry).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["error"]["type"], "ParameterError");
}

#[tokio::test]
async fn aae16_post_metadata_always_501() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    let (status, json) = send_json(app, "POST", "/api/assets/key/metadata/notes/a.txt", r#"{"title":"Test"}"#).await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(json["error"]["type"], "NotSupported");
    // Wording is an example in Phase 2, not a contract: assert only the substring it promises.
    assert!(json["error"]["message"].as_str().unwrap_or("").to_lowercase().contains("post description"));
}

#[tokio::test]
async fn aae17_post_entry_roundtrip_allows_only_five_fields() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    use base64::prelude::*;
    let entry = serde_json::json!({
        "metadata": {
            "type_identifier": "Text", "data_format": "txt", "media_type": "text/plain",
            "title": "Kept", "description": "Kept desc",
            "status": "Ready", "version": "deadbeef"
        },
        "data": BASE64_STANDARD.encode(b"round trip")
    });
    let (status, resp) = post_entry_json(app.clone(), "notes/rt.txt", entry).await;
    assert_eq!(status, StatusCode::CREATED);
    let msg = resp["message"].as_str().unwrap().to_lowercase();
    assert!(msg.contains("status") && msg.contains("version"));

    let (_, info) = send(app, "GET", "/api/assets/key/info/notes/rt.txt", Body::empty()).await;
    assert_eq!(info["result"]["title"], "Kept");
    assert_eq!(info["result"]["description"], "Kept desc");
}
```

#### AAE20–AAE29: Access Modes — Submit vs Info/Observe

```rust
#[tokio::test]
async fn aae20_q_submit_returns_asset_info_immediately() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    let (status, json) = send(app, "POST", "/api/assets/q/submit/make_text", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        ["Submitted", "Processing", "Ready"].contains(&json["result"]["status"].as_str().unwrap()),
        "submit returns AssetInfo immediately"
    );
    assert!(json["result"].get("query").is_some());
}

#[tokio::test]
async fn aae21_q_info_on_uncached_query_returns_404_not_available() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    let (status, json) = send(app.clone(), "GET", "/api/assets/q/info/make_text", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"]["type"], "NotAvailable");

    let (status, _) = send(app, "GET", "/api/assets/q/info/make_text", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a second poll is still 404: no evaluation happened");
}

#[tokio::test]
async fn aae22_key_info_on_recipe_key_never_evaluates() {
    let envref = env_with(&[("make_text/source.txt", "Text", "A text")]).await;
    let app = build_app(envref.clone());
    let (status, json) = send(app.clone(), "GET", "/api/assets/key/info/source.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["status"], "Recipe");

    let (_, json) = send(app, "GET", "/api/assets/key/info/source.txt", Body::empty()).await;
    assert_eq!(json["result"]["status"], "Recipe", "repeated polling never evaluates");
}

#[tokio::test]
async fn aae23_version_is_32_hex_or_zeros() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"test", metadata_text()).await.unwrap();

    let (_, json) = send(app, "GET", "/api/assets/key/version/notes/a.txt", Body::empty()).await;
    let version_str = json["result"]["version"].as_str().unwrap();
    assert_eq!(version_str.len(), 32, "version is 32 hex digits");
    assert!(version_str.chars().all(|c| c.is_ascii_hexdigit()));
}

#[tokio::test]
async fn aae24_q_data_equals_key_data_for_pure_key_query() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"same content", metadata_text()).await.unwrap();

    let (_, body_key, ct_key) = send_raw(app.clone(), "GET", "/api/assets/key/data/notes/a.txt", Body::empty()).await;
    let (_, body_query, ct_query) = send_raw(app, "GET", "/api/assets/q/data/-R/notes/a.txt", Body::empty()).await;
    assert_eq!(body_key, body_query, "same bytes for the same asset, reached via either family");
    assert_eq!(ct_key, ct_query, "same Content-Type");
}

#[tokio::test]
async fn aae25_key_data_with_query_syntax_returns_400_with_hint() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    let (status, json) = send(app, "GET", "/api/assets/key/data/-R/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["type"], "ParseError");
    assert!(
        json["error"]["message"].as_str().unwrap_or("").contains("/q/"),
        "key routes hint at /q/ for a query-syntax path"
    );
}

#[tokio::test]
async fn aae26_every_status_route_returns_apiresponse_envelope() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"test", metadata_text()).await.unwrap();

    let (status, json) = send(app, "GET", "/api/assets/key/info/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "OK");
    assert!(json["result"].is_object());
}

#[tokio::test]
async fn aae27_key_contains_on_absent_key() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    let (status, json) = send(app, "GET", "/api/assets/key/contains/phantom/key.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["contains"], false);
}

#[tokio::test]
async fn aae28_key_recover_reads_expired_without_evaluating() {
    let envref = env_with(&[("make_text/derived.txt", "Derived", "")]).await;
    let app = build_app(envref.clone());
    let am = envref.get_asset_manager();
    let key = parse_key("derived.txt").unwrap();
    let value = am.get(&key).await.unwrap().get().await.unwrap();
    assert_eq!(value.try_into_string().unwrap(), "generated");
    am.expire(&key).await.unwrap();
    assert_eq!(am.get_asset_info(&key).await.unwrap().status, Status::Expired);

    let (status, body, _) = send_raw(app, "GET", "/api/assets/key/recover/derived.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "recover reads the last known value of an Expired asset");
    assert!(!body.is_empty());
}

#[tokio::test]
async fn aae29_key_listdir_on_directory() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    let am = envref.get_asset_manager();
    am.makedir(&parse_key("notes").unwrap()).await.unwrap();

    let (status, json) = send(app, "GET", "/api/assets/key/listdir/notes", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"].get("assets").is_some());
}
```

#### AAE40–AAE43: Removal — Status and Directory Handling

```rust
#[tokio::test]
async fn aae40_delete_key_data_on_source_returns_none() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"test", metadata_text()).await.unwrap();

    let (status, json) = send(app, "DELETE", "/api/assets/key/data/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["new_status"], "None");
}

#[tokio::test]
async fn aae41_delete_key_data_on_computed_ready_returns_recipe() {
    let envref = env_with(&[("-R/notes/a.txt/-/upper/summary.txt", "Summary", "Derived")]).await;
    let app = build_app(envref.clone());
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"hello", metadata_text()).await.unwrap();
    let _ = am.get(&parse_key("summary.txt").unwrap()).await.unwrap().get().await.unwrap();

    let (status, json) = send(app, "DELETE", "/api/assets/key/data/summary.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["new_status"], "Recipe", "dropping a computed value returns Recipe");
}

#[tokio::test]
async fn aae42_delete_key_data_on_directory_returns_409() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    let am = envref.get_asset_manager();
    am.makedir(&parse_key("mydir").unwrap()).await.unwrap();

    let (status, json) = send(app, "DELETE", "/api/assets/key/data/mydir", Body::empty()).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json["error"]["type"], "StatusConflict");
    assert!(json["error"]["message"].as_str().unwrap_or("").contains("removedir"));
}

#[tokio::test]
async fn aae43_delete_key_removedir_recursive() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    let am = envref.get_asset_manager();
    am.makedir(&parse_key("mydir").unwrap()).await.unwrap();
    am.set_binary(&parse_key("mydir/file.txt").unwrap(), b"content", metadata_text()).await.unwrap();

    let (status, json) = send(app.clone(), "DELETE", "/api/assets/key/removedir/mydir", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["removed"], true);

    let (status, json) = send(app, "GET", "/api/assets/key/contains/mydir/file.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["contains"], false, "children are gone after removedir");
}
```

#### AAE44–AAE47: I1 — Extended Value Types Served by `data`/`entry` (mandatory addition)

`AXUM-ASSETS-API-SERVES-ONLY-BYTES-AND-TEXT` closes here: `q/data`, `key/data` and `GET /q` all go
through `AssetRef::get_binary()`, so a core `Value` that is not `Bytes`/`Text` (an integer, a JSON
object) round-trips identically through all three, and a keyed JSON value written with
`set_binary` reads back correctly through `key/data`.

```rust
#[tokio::test]
async fn aae44_q_data_serves_integer_value() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    let (status, body, ct) = send_raw(app, "GET", "/api/assets/q/data/make_number", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.is_empty(), "an I64 value must serialize to non-empty bytes, not be refused");
    eprintln!("make_number q/data: content-type={ct:?} bytes={body:?}");
}

#[tokio::test]
async fn aae45_q_data_serves_json_object_value() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    let (status, body, _) = send_raw(app, "GET", "/api/assets/q/data/make_object", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).expect("an Object value must serialize to JSON");
    assert_eq!(json["a"], 1);
    assert_eq!(json["b"], "two");
}

/// `q/data` on the Assets API must match the standalone Query API's `GET /q/<query>` byte-for-byte
/// and header-for-header, for the same non-Bytes/Text value — both go through the same
/// `AssetRef::get_binary()` path.
#[tokio::test]
async fn aae46_q_data_matches_query_api_for_extended_value() {
    let envref = env_with(&[]).await;
    let app = build_app_with_query_api(envref);
    let (status_assets, body_assets, ct_assets) =
        send_raw(app.clone(), "GET", "/api/assets/q/data/make_object", Body::empty()).await;
    let (status_query, body_query, ct_query) = send_raw(app, "GET", "/q/make_object", Body::empty()).await;
    assert_eq!(status_assets, StatusCode::OK);
    assert_eq!(status_query, StatusCode::OK);
    assert_eq!(body_assets, body_query, "identical bytes from both APIs for the same query");
    assert_eq!(ct_assets, ct_query, "identical Content-Type from both APIs");
}

#[tokio::test]
async fn aae47_key_data_serves_keyed_json_value_stored_as_bytes() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    let am = envref.get_asset_manager();
    let key = parse_key("data/record.json").unwrap();
    let json_bytes = serde_json::to_vec(&serde_json::json!({"n": 7, "s": "seven"})).unwrap();
    am.set_binary(
        &key,
        &json_bytes,
        MetadataRecord {
            type_identifier: "Bytes".to_string(),
            type_name: "bytes".to_string(),
            data_format: Some("json".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let (status, body, _) = send_raw(app, "GET", "/api/assets/key/data/data/record.json", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["n"], 7);
    assert_eq!(value["s"], "seven");
}
```

#### AAE60–AAE66: GET Alternatives and Builder Switches

Exact status codes throughout: an omitted method on a path that still serves another method →
**405**; a path with no route at all → **404** (this is the mandatory exactness fix; see "Fixes
Made Against the Drafts" #7).

```rust
#[tokio::test]
async fn aae60_get_remove_without_destructive_gets_404() {
    let envref = env_with(&[]).await;
    // GET remove has no sibling method on that exact path when with_destructive_gets() is off:
    // the route itself does not exist → 404 (not 405).
    let app = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .build()
        .with_state(envref);
    let (status, _) = send(app, "GET", "/api/assets/key/remove/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "GET remove route does not exist by default");
}

#[tokio::test]
async fn aae61_get_remove_with_destructive_gets_200() {
    let envref = env_with(&[]).await;
    let app = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .with_destructive_gets()
        .build()
        .with_state(envref.clone());
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"test", metadata_text()).await.unwrap();

    let (status, json) = send(app, "GET", "/api/assets/key/remove/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "GET remove is routed with the flag");
    assert_eq!(json["result"]["new_status"], "None");
}

#[tokio::test]
async fn aae62_read_only_blocks_post_data_405() {
    let envref = env_with(&[]).await;
    // read_only() drops POST from key/data's method set, but GET key/data still exists on the
    // same path → 405 Method Not Allowed, not 404.
    let app = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .read_only()
        .build()
        .with_state(envref);
    let (status, _) = send(app, "POST", "/api/assets/key/data/notes/a.txt", Body::from("test")).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn aae63_read_only_blocks_post_expire_404() {
    let envref = env_with(&[]).await;
    // read_only() drops key/expire entirely: no other method is registered on that exact path,
    // so omitting the only method leaves no route at all → 404, not 405.
    let app = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .read_only()
        .build()
        .with_state(envref);
    let (status, _) = send(app, "POST", "/api/assets/key/expire/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "expire has no sibling method: the whole route is gone");
}

#[tokio::test]
async fn aae64_read_only_allows_get_data_and_cancel() {
    let envref = env_with(&[]).await;
    let app_rw = build_app(envref.clone());
    let (_, json) = send(app_rw, "POST", "/api/assets/key/data/notes/a.txt", Body::from("test")).await;
    assert_eq!(json["status"], "OK");

    let am = envref.get_asset_manager();
    let _ = am.get(&parse_key("notes/a.txt").unwrap()).await.unwrap();

    let app_ro = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .read_only()
        .build()
        .with_state(envref);

    let (status, _, _) = send_raw(app_ro.clone(), "GET", "/api/assets/key/data/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "GET data still works in read_only");

    // cancel must be routed even in read_only (it changes no data); it must be neither 404 nor 405.
    let (status, _) = send(app_ro, "POST", "/api/assets/key/cancel/notes/a.txt", Body::empty()).await;
    assert_ne!(status, StatusCode::NOT_FOUND, "cancel stays routed in read_only");
    assert_ne!(status, StatusCode::METHOD_NOT_ALLOWED, "cancel is not blocked by read_only");
}

#[tokio::test]
async fn aae65_with_admin_false_blocks_audit_404() {
    let envref = env_with(&[]).await;
    let app = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .with_admin(false)
        .build()
        .with_state(envref);
    let (status, _) = send(app, "POST", "/api/assets/admin/audit", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "admin/audit has no sibling method: the whole route is gone");
}

#[tokio::test]
async fn aae66_with_admin_true_includes_audit_200() {
    let envref = env_with(&[]).await;
    let app = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .with_admin(true) // default
        .build()
        .with_state(envref);
    let (status, json) = send(app, "POST", "/api/assets/admin/audit", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"].get("checked").is_some());
    assert!(json["result"].get("expired").is_some());
}
```

#### AAE06–AAE07: Version Zeros (O3, Review 2 Finding 1)

Purpose: `key/version` and `q/version` must report `Version::unknown()` (32 zeros), never null or
an error, whenever nothing has finished yet — a never-evaluated recipe key, or a query still
`Processing` right after `submit`.

```rust
#[tokio::test]
async fn aae06_key_version_zero_for_never_evaluated_recipe() {
    // O3: a recipe key that has never produced a value has no version yet; the route must
    // report the all-zero sentinel (Version::unknown()), never null or an error.
    let envref = env_with(&[("make_text/never.txt", "Never", "recipe only, never evaluated")]).await;
    let app = build_app(envref);

    let (status, json) = send(app, "GET", "/api/assets/key/version/never.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["version"], "0".repeat(32));
}

/// Blocking `std::thread::sleep` inside the registered command — multi-thread test runtime, so
/// the queued manager's worker is not starved by this test's own runtime (see Fixes #3).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aae07_q_version_zero_right_after_submit_before_slow_query_finishes() {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("sleep_version"), |_, _, _| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            Ok(Value::from("slept"))
        })
        .unwrap();
    let envref = env.to_ref();
    let app = build_app(envref);

    let (status, _) = send(app.clone(), "POST", "/api/assets/q/submit/sleep_version", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);

    // Right after submit, before the 500ms sleep finishes: version must be zeros (O3).
    let (status, json) = send(app.clone(), "GET", "/api/assets/q/version/sleep_version", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["version"], "0".repeat(32), "unfinished query reports the unknown-version sentinel");

    // Drain to completion so no background task outlives the test.
    poll_until(app, "/api/assets/q/info/sleep_version", "Ready", 100).await;
}
```

#### AAE08–AAE09: `submit` GET Alternative, Always On (Review 2 Finding 2)

Purpose: `GET q/submit` and `GET key/submit` are the one GET alternative that needs no
`with_destructive_gets()` (Phase 2 Routes table: submit "always on", it changes no data), and must
answer 200 with a live `AssetInfo`.

```rust
#[tokio::test]
async fn aae08_get_q_submit_returns_200_asset_info() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    let (status, json) = send(app, "GET", "/api/assets/q/submit/make_text", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "GET q/submit is always on, no destructive_gets needed");
    assert_eq!(json["status"], "OK");
    assert!(
        ["Submitted", "Processing", "Ready"].contains(&json["result"]["status"].as_str().unwrap()),
        "GET q/submit returns a live AssetInfo, like POST"
    );
}

#[tokio::test]
async fn aae09_get_key_submit_returns_200_asset_info() {
    let envref = env_with(&[("make_text/computed.txt", "Computed", "recipe")]).await;
    let app = build_app(envref);
    let (status, json) = send(app, "GET", "/api/assets/key/submit/computed.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "GET key/submit is always on, no destructive_gets needed");
    assert_eq!(json["status"], "OK");
    assert!(
        ["Submitted", "Processing", "Ready", "Dependencies"].contains(&json["result"]["status"].as_str().unwrap()),
        "GET key/submit returns a live AssetInfo, like POST"
    );
}
```

#### AAE18–AAE19: Admin Audit Routes (Review 2 Finding 3)

Purpose: `POST admin/audit/{key}` (always on), and, with `with_destructive_gets()`, `GET
admin/audit` and `GET admin/audit/{key}` — every shape returns `AuditResult`'s `{checked,
expired}`.

```rust
#[tokio::test]
async fn aae18_post_admin_audit_key_returns_checked_and_expired() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"test", metadata_text()).await.unwrap();

    let (status, json) = send(app, "POST", "/api/assets/admin/audit/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"]["checked"].is_array(), "AuditResult.checked is present");
    assert!(json["result"]["expired"].is_array(), "AuditResult.expired is present");
}

#[tokio::test]
async fn aae19_get_admin_audit_and_audit_key_with_destructive_gets() {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"test", metadata_text()).await.unwrap();
    let app = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .with_destructive_gets()
        .build()
        .with_state(envref);

    let (status, json) = send(app.clone(), "GET", "/api/assets/admin/audit", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "GET admin/audit is routed with the flag");
    assert!(json["result"]["checked"].is_array());
    assert!(json["result"]["expired"].is_array());

    let (status, json) = send(app, "GET", "/api/assets/admin/audit/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "GET admin/audit/{key} is routed with the flag");
    assert!(json["result"]["checked"].is_array());
    assert!(json["result"]["expired"].is_array());
}
```

#### AAE30–AAE33: `POST`/`GET key/override` (Review 2 Finding 4)

Purpose: override on a computed `Ready` value promotes it to `Override` (both `POST` and, with
`with_destructive_gets()`, `GET`); override on a `Source` with no recipe is a no-op (Q19); override
with no data at all is `to_override`'s `key_not_found`, 404.

```rust
#[tokio::test]
async fn aae30_post_key_override_on_computed_ready_becomes_override() {
    let envref = env_with(&[("make_text/computed.txt", "Computed", "recipe")]).await;
    let app = build_app(envref.clone());
    let am = envref.get_asset_manager();
    let key = parse_key("computed.txt").unwrap();
    let _ = am.get(&key).await.unwrap().get().await.unwrap();
    assert_eq!(am.get_asset_info(&key).await.unwrap().status, Status::Ready);

    let (status, json) = send(app, "POST", "/api/assets/key/override/computed.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["status"], "Override", "to_override promotes a computed Ready value");
}

#[tokio::test]
async fn aae31_get_key_override_with_destructive_gets() {
    let envref = env_with(&[("make_text/computed.txt", "Computed", "recipe")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("computed.txt").unwrap();
    let _ = am.get(&key).await.unwrap().get().await.unwrap();

    let app = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .with_destructive_gets()
        .build()
        .with_state(envref);

    let (status, json) = send(app, "GET", "/api/assets/key/override/computed.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "GET override is routed with the flag");
    assert_eq!(json["result"]["status"], "Override");
}

#[tokio::test]
async fn aae32_override_on_source_is_a_noop_q19() {
    let envref = env_with(&[]).await;
    let app = build_app(envref.clone());
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"test", metadata_text()).await.unwrap();
    assert_eq!(am.get_asset_info(&parse_key("notes/a.txt").unwrap()).await.unwrap().status, Status::Source);

    let (status, json) = send(app, "POST", "/api/assets/key/override/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["status"], "Source", "to_override on a Source with no recipe is a no-op (Q19)");
}

#[tokio::test]
async fn aae33_override_with_no_data_404() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    let (status, json) = send(app, "POST", "/api/assets/key/override/phantom/nothing.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "to_override's key_not_found when there is no data");
    assert_eq!(json["error"]["type"], "KeyNotFound");
}
```

#### AAE34: `GET key/description` (Review 2 Finding 5)

Purpose: the GET alternative of `POST key/description`, with `with_destructive_gets()` and
`?title=&description=` query parameters, updates a `Source`'s title and description.

```rust
#[tokio::test]
async fn aae34_get_key_description_with_destructive_gets_updates_source() {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"test", metadata_text()).await.unwrap();

    let app = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .with_destructive_gets()
        .build()
        .with_state(envref);

    let (status, json) = send(
        app,
        "GET",
        "/api/assets/key/description/notes/a.txt?title=T&description=D",
        Body::empty(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "GET description is routed with the flag");
    assert_eq!(json["result"]["title"], "T");
    assert_eq!(json["result"]["description"], "D");
}
```

#### AAE35–AAE37: Query-Family Observe/Read Routes (Review 2 Finding 6)

Purpose: `q/metadata` after `submit` reaches `Ready` returns the cached metadata record; on an
uncached query it is 404 `NotAvailable`, exactly like `q/info`; `q/entry?format=json` returns a
negotiated `DataEntry` with base64-encoded `data`, at the top level (not under `result` — value
transfers are the one exception to the `ApiResponse` envelope, Phase 2 "Web Endpoints").

```rust
#[tokio::test]
async fn aae35_q_metadata_after_submit_and_ready() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    let (status, _) = send(app.clone(), "POST", "/api/assets/q/submit/make_text", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    poll_until(app.clone(), "/api/assets/q/info/make_text", "Ready", 50).await;

    let (status, json) = send(app, "GET", "/api/assets/q/metadata/make_text", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"]["type_identifier"].is_string(), "metadata record of a cached query");
}

#[tokio::test]
async fn aae36_q_metadata_uncached_404_not_available() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    let (status, json) = send(app, "GET", "/api/assets/q/metadata/make_text", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"]["type"], "NotAvailable");
}

#[tokio::test]
async fn aae37_q_entry_json_format_gives_data_entry_with_base64_data() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    use base64::prelude::*;
    let (status, json) = send(app, "GET", "/api/assets/q/entry/make_text?format=json", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["metadata"].is_object(), "negotiated DataEntry carries metadata, no result wrapper");
    let data = json["data"].as_str().expect("data is a base64 string");
    let decoded = BASE64_STANDARD.decode(data).expect("valid base64");
    assert_eq!(decoded, b"generated");
}
```

#### AAE38–AAE39, AAE48–AAE49, AAE67–AAE69: Remaining GET Alternatives (Review 2 Finding 7)

Purpose: one focused test per remaining `with_destructive_gets()` GET alternative
(`q/cancel`, `key/cancel`, `key/removedir`, `key/makedir`, `key/expire`,
`admin/refresh_command_versions`), plus one test proving the *exact* status of each when the flag
is off. Every one of these six routes keeps another method (POST, PUT or DELETE) registered on the
same path regardless of `with_destructive_gets()`, so turning the flag off never removes the route
— it only removes the GET registration on it. axum therefore answers **405** (the path matches,
the method does not), never 404. This is the opposite case from AAE63 (`read_only()` + `key/expire`
→ 404), where the *only* method on the path is removed and the whole route disappears.

```rust
/// Blocking `std::thread::sleep` inside the registered command — multi-thread test runtime
/// (see Fixes #3).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aae38_get_q_cancel_with_destructive_gets() {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("sleep_cancel_q"), |_, _, _| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            Ok(Value::from("slept"))
        })
        .unwrap();
    let envref = env.to_ref();
    let app = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .with_destructive_gets()
        .build()
        .with_state(envref);

    let (status, _) = send(app.clone(), "POST", "/api/assets/q/submit/sleep_cancel_q", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    poll_until(app.clone(), "/api/assets/q/info/sleep_cancel_q", "Processing", 50).await;

    let (status, json) = send(app.clone(), "GET", "/api/assets/q/cancel/sleep_cancel_q", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "GET q/cancel is routed with the flag");
    assert_eq!(json["status"], "OK");

    poll_until(app, "/api/assets/q/info/sleep_cancel_q", "Cancelled", 50).await;
}

#[tokio::test]
async fn aae39_get_key_cancel_with_destructive_gets() {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"test", metadata_text()).await.unwrap();
    let _ = am.get(&parse_key("notes/a.txt").unwrap()).await.unwrap();

    let app = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .with_destructive_gets()
        .build()
        .with_state(envref);

    let (status, json) = send(app, "GET", "/api/assets/key/cancel/notes/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "GET key/cancel is routed with the flag");
    assert_eq!(json["status"], "OK");
}

#[tokio::test]
async fn aae48_get_key_removedir_with_destructive_gets() {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    am.makedir(&parse_key("mydir").unwrap()).await.unwrap();
    am.set_binary(&parse_key("mydir/file.txt").unwrap(), b"content", metadata_text()).await.unwrap();

    let app = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .with_destructive_gets()
        .build()
        .with_state(envref.clone());

    let (status, json) = send(app.clone(), "GET", "/api/assets/key/removedir/mydir", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "GET key/removedir is routed with the flag");
    assert_eq!(json["result"]["removed"], true);

    let (status, json) = send(app, "GET", "/api/assets/key/contains/mydir/file.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["contains"], false, "children are gone after GET removedir");
}

#[tokio::test]
async fn aae49_get_key_makedir_with_destructive_gets_201() {
    let envref = env_with(&[]).await;
    let app = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .with_destructive_gets()
        .build()
        .with_state(envref);

    let (status, json) = send(app, "GET", "/api/assets/key/makedir/newdir", Body::empty()).await;
    assert_eq!(status, StatusCode::CREATED, "PUT key/makedir returns 201; the GET alternative matches");
    assert_eq!(json["result"]["status"], "Directory");
}

#[tokio::test]
async fn aae67_get_key_expire_with_destructive_gets() {
    let envref = env_with(&[("make_text/a.txt", "A", "")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("a.txt").unwrap();
    let _ = am.get(&key).await.unwrap().get().await.unwrap();

    let app = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .with_destructive_gets()
        .build()
        .with_state(envref);

    let (status, json) = send(app, "GET", "/api/assets/key/expire/a.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "GET key/expire is routed with the flag");
    assert_eq!(json["result"]["status"], "Expired");
}

#[tokio::test]
async fn aae68_get_admin_refresh_command_versions_with_destructive_gets() {
    let envref = env_with(&[]).await;
    let app = liquers_axum::assets::AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .with_destructive_gets()
        .build()
        .with_state(envref);

    let (status, json) = send(app, "GET", "/api/assets/admin/refresh_command_versions", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "GET admin/refresh_command_versions is routed with the flag");
    assert_eq!(json["status"], "OK");
    assert!(json["result"].is_null(), "refresh_command_versions returns null in result, with a message");
}

#[tokio::test]
async fn aae69_get_alternatives_without_destructive_gets_are_405_not_404() {
    // None of these six paths loses its only method when with_destructive_gets() is off: q/cancel,
    // key/cancel, key/expire and admin/refresh_command_versions keep their POST route,
    // key/removedir keeps its DELETE route, and key/makedir keeps its PUT route. So a GET on any
    // of them matches the path but not the method: axum answers 405 Method Not Allowed, never 404
    // (contrast AAE63, where read_only() removes the *only* registered method on key/expire and
    // the whole route disappears, giving 404).
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"test", metadata_text()).await.unwrap();
    am.makedir(&parse_key("mydir").unwrap()).await.unwrap();
    let app = build_app(envref);

    for uri in [
        "/api/assets/q/cancel/make_text",             // POST q/cancel/{*query} exists
        "/api/assets/key/cancel/notes/a.txt",         // POST key/cancel/{*key} exists
        "/api/assets/key/removedir/mydir",            // DELETE key/removedir/{*key} exists
        "/api/assets/key/makedir/mydir",              // PUT key/makedir/{*key} exists
        "/api/assets/key/expire/notes/a.txt",         // POST key/expire/{*key} exists
        "/api/assets/admin/refresh_command_versions", // POST …, with_admin defaults to true
    ] {
        let (status, _) = send(app.clone(), "GET", uri, Body::empty()).await;
        assert_eq!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "GET {uri} without with_destructive_gets() hits an existing route with the wrong method"
        );
    }
}
```

#### AAE80: Concurrent Writes

```rust
#[tokio::test]
async fn aae80_two_concurrent_post_data_same_key_both_succeed() {
    let envref = env_with(&[]).await;
    let app1 = build_app(envref.clone());
    let app2 = build_app(envref.clone());

    let (tx1, rx1) = tokio::sync::oneshot::channel();
    let (tx2, rx2) = tokio::sync::oneshot::channel();

    tokio::spawn(async move {
        let (status, json) = send(app1, "POST", "/api/assets/key/data/notes/a.txt", Body::from("FIRST")).await;
        let _ = tx1.send((status, json));
    });
    tokio::spawn(async move {
        let (status, json) = send(app2, "POST", "/api/assets/key/data/notes/a.txt", Body::from("SECOND")).await;
        let _ = tx2.send((status, json));
    });

    let (status1, _) = rx1.await.unwrap();
    let (status2, _) = rx2.await.unwrap();
    assert_eq!(status1, StatusCode::CREATED);
    assert_eq!(status2, StatusCode::CREATED, "both writes succeed; key_mutation_lock serializes them");

    let (_, body, _) = send_raw(build_app(envref), "GET", "/api/assets/key/data/notes/a.txt", Body::empty()).await;
    let final_value = String::from_utf8(body).unwrap();
    assert!(matches!(final_value.as_str(), "FIRST" | "SECOND"), "final value is exactly one of the two request bodies");
}
```

#### AAE90–AAE92 (Example 2b, full code above)

`aae90_submit_and_poll_non_keyed_query`, `aae91_polling_unsubmitted_query_404_never_evaluates`,
`aae92_cancel_while_processing_reports_cancelled_deterministically` — see "Example 2b" above.

---

## WebSocket Tests

### `liquers-axum/tests/assets_websocket.rs` — Shared Helpers (AWS)

```rust
// liquers-axum/tests/assets_websocket.rs

use axum::Router;
use futures::{SinkExt, StreamExt};
use liquers_axum::assets::{AssetsApiBuilder, WebSocketLimits};
use liquers_core::{
    assets::AssetManager,
    command_metadata::CommandKey,
    context::{Environment, EnvRef, SimpleEnvironment},
    metadata::{Metadata, MetadataRecord},
    parse::parse_key,
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    state::State,
    store::AsyncMemoryStore,
    value::Value,
};
use serde_json::{json, Value as JsonValue};
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn env_with(recipes: &[(&str, &str, &str)]) -> EnvRef<SimpleEnvironment<Value>> {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("make_text"), |_, _, _| Ok(Value::from("generated")))
        .unwrap();
    env.command_registry
        .register_command(CommandKey::new_name("upper"), |state: &State<Value>, _, _| {
            Ok(Value::from(state.try_into_string()?.to_uppercase()))
        })
        .unwrap();
    // Blocking sync command; tests that submit it run on the multi-thread runtime (AWS09, AWS13).
    env.command_registry
        .register_command(CommandKey::new_name("sleep_sync"), |_, _, _| {
            std::thread::sleep(Duration::from_millis(300));
            Ok(Value::from("slept"))
        })
        .unwrap();

    let mut rl = RecipeList::new();
    for (q, t, d) in recipes {
        rl.add_recipe(Recipe::new(q.to_string(), t.to_string(), d.to_string()).unwrap());
    }
    let store = AsyncMemoryStore::new(&Key::new());
    liquers_core::store::AsyncStore::set(&store, &parse_key("recipes.yaml").unwrap(), serde_yaml::to_string(&rl).unwrap().as_bytes(), &Metadata::new())
        .await
        .unwrap();
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    env.to_ref()
}

fn metadata_text() -> MetadataRecord {
    MetadataRecord { type_identifier: "Text".to_string(), type_name: "text".to_string(), data_format: Some("txt".to_string()), ..Default::default() }
}

fn build_app(envref: EnvRef<SimpleEnvironment<Value>>) -> Router {
    AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets").build().with_state(envref)
}

fn build_app_with_limits(envref: EnvRef<SimpleEnvironment<Value>>, limits: WebSocketLimits) -> Router {
    AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .with_websocket_limits(limits)
        .build()
        .with_state(envref)
}

/// Start an in-process server on an OS-assigned port and return its `ws://` base URL.
async fn start_server(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
    format!("ws://{}", addr)
}

async fn connect(base_url: &str, path: &str) -> Ws {
    let (ws, _) = connect_async(&format!("{}{}", base_url, path)).await.unwrap();
    ws
}

/// Receive and parse one JSON text message, skipping ping/pong/binary frames.
async fn recv_json(ws: &mut Ws, timeout: Duration) -> Option<JsonValue> {
    loop {
        match tokio::time::timeout(timeout, ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => return Some(serde_json::from_str(&text).unwrap()),
            Ok(Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Binary(_)))) => continue,
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Ok(Some(Err(_))) | Err(_) => return None,
            Ok(Some(Ok(Message::Frame(_)))) => continue,
        }
    }
}

async fn send_json(ws: &mut Ws, msg: JsonValue) {
    ws.send(Message::Text(serde_json::to_string(&msg).unwrap())).await.unwrap();
}

/// Poll `recv_json` up to 100 times (100ms each) for a message whose `type` matches.
async fn recv_by_type(ws: &mut Ws, expected_type: &str) -> Option<JsonValue> {
    for _ in 0..100 {
        if let Some(msg) = recv_json(ws, Duration::from_millis(100)).await {
            if msg.get("type").and_then(JsonValue::as_str) == Some(expected_type) {
                return Some(msg);
            }
        }
    }
    None
}
```

### AWS01–AWS11: Core Protocol

```rust
#[tokio::test]
async fn aws01_subscribe_by_query() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;
    let mut ws = connect(&base_url, "/api/assets/ws/q").await;

    send_json(&mut ws, json!({"action": "subscribe", "query": "make_text"})).await;

    let initial = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(initial["type"], "Initial");
    assert!(initial["asset_id"].is_number());
    assert_eq!(initial["query"], "make_text");
    assert!(initial["timestamp"].is_string());
    assert!(initial["info"].is_object());

    let finished = recv_by_type(&mut ws, "JobFinished").await.unwrap();
    assert_eq!(finished["query"], "make_text");
    assert_eq!(finished["info"]["status"], "Ready");
}

#[tokio::test]
async fn aws02_url_path_subscription_on_connect() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;
    let mut ws = connect(&base_url, "/api/assets/ws/q/make_text").await;

    let initial = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(initial["type"], "Initial");
    assert_eq!(initial["query"], "make_text");
}

#[tokio::test]
async fn aws03_subscribe_by_key() {
    let envref = env_with(&[("-R/notes/a.txt/-/upper/summary.txt", "Summary", "Upper-cased note")]).await;
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"hello", metadata_text()).await.unwrap();

    let base_url = start_server(build_app(envref)).await;
    let mut ws = connect(&base_url, "/api/assets/ws/key").await;
    // Final review: subscribe to the recipe key, not the Source. A Source is already finished
    // (status `Source`, never `Ready`), so it never emits `JobFinished` after the subscribe.
    send_json(&mut ws, json!({"action": "subscribe", "key": "summary.txt"})).await;

    let initial = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(initial["type"], "Initial");
    assert_eq!(initial["key"], "summary.txt");

    let finished = recv_by_type(&mut ws, "JobFinished").await.unwrap();
    assert_eq!(finished["key"], "summary.txt");
    assert_eq!(finished["info"]["status"], "Ready");
}

#[tokio::test]
async fn aws04_key_subscription_ends_after_delete() {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"test", metadata_text()).await.unwrap();

    let base_url = start_server(build_app(envref)).await;
    let mut ws = connect(&base_url, "/api/assets/ws/key").await;
    send_json(&mut ws, json!({"action": "subscribe", "key": "notes/a.txt"})).await;
    let initial = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(initial["type"], "Initial");

    let client = reqwest::Client::new();
    let delete_url = format!("{}/api/assets/key/data/notes/a.txt", base_url.replace("ws://", "http://"));
    let resp = client.delete(&delete_url).send().await.unwrap();
    assert!(resp.status().is_success());

    let removed = recv_by_type(&mut ws, "Removed").await.unwrap();
    assert_eq!(removed["key"], "notes/a.txt");

    let none_msg = recv_json(&mut ws, Duration::from_millis(500)).await;
    assert_eq!(none_msg, None, "subscription ends: no further messages after Removed");
}

#[tokio::test]
async fn aws05_subscription_ends_after_error() {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("fail_always"), |_, _, _| {
            Err(liquers_core::error::Error::general_error("intentional error".to_string()))
        })
        .unwrap();
    let base_url = start_server(build_app(env.to_ref())).await;

    let mut ws = connect(&base_url, "/api/assets/ws/q").await;
    send_json(&mut ws, json!({"action": "subscribe", "query": "fail_always"})).await;
    let mut msg = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(msg["type"], "Initial");

    // Final review: the core never sends StatusChanged(Error) — a failure is ErrorOccurred then
    // JobFinished, and the watch may coalesce them — so the terminal message is recognised by its
    // `info.status`, whatever its `type` (the Initial snapshot itself may already say Error).
    let mut seen = 0;
    while msg["info"]["status"] != "Error" {
        seen += 1;
        assert!(seen < 50, "no message reported status Error");
        msg = recv_json(&mut ws, Duration::from_secs(2))
            .await
            .expect("the subscription ended before reporting Error");
    }

    let none_msg = recv_json(&mut ws, Duration::from_millis(500)).await;
    assert_eq!(none_msg, None, "the terminal (Error) notification ends the subscription");
}

#[tokio::test]
async fn aws06_snake_case_accepted_old_casing_rejected() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;

    let mut ws = connect(&base_url, "/api/assets/ws/q").await;
    send_json(&mut ws, json!({"action": "subscribe", "query": "make_text"})).await;
    let msg = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(msg["type"], "Initial");

    let mut ws2 = connect(&base_url, "/api/assets/ws/q").await;
    send_json(&mut ws2, json!({"action": "Subscribe", "query": "make_text"})).await; // old, Rust-cased
    let err_msg = recv_json(&mut ws2, Duration::from_secs(2)).await.unwrap();
    assert_eq!(err_msg["type"], "Error");
    assert!(err_msg["error"].is_object());
}

#[tokio::test]
async fn aws07_wrong_address_field_returns_error() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;

    let mut ws_q = connect(&base_url, "/api/assets/ws/q").await;
    send_json(&mut ws_q, json!({"action": "subscribe", "key": "notes/a.txt"})).await;
    assert_eq!(recv_json(&mut ws_q, Duration::from_secs(2)).await.unwrap()["type"], "Error");

    let mut ws_key = connect(&base_url, "/api/assets/ws/key").await;
    send_json(&mut ws_key, json!({"action": "subscribe", "query": "make_text"})).await;
    assert_eq!(recv_json(&mut ws_key, Duration::from_secs(2)).await.unwrap()["type"], "Error");
}

#[tokio::test]
async fn aws08_malformed_json_returns_error() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;
    let mut ws = connect(&base_url, "/api/assets/ws/q").await;
    ws.send(Message::Text("not valid json {".to_string())).await.unwrap();

    let err_msg = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(err_msg["type"], "Error");
    assert!(err_msg["error"].is_object());
}

/// Final review: `make_text` finishes at once, so its `JobFinished` could already be on the wire
/// before `unsubscribe` is processed. A 300 ms `sleep_sync` makes the check meaningful: messages
/// sent before the unsubscribe (JobSubmitted/JobStarted) are tolerated, the completion is not.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aws09_unsubscribe_stops_messages() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;
    let mut ws = connect(&base_url, "/api/assets/ws/q").await;
    send_json(&mut ws, json!({"action": "subscribe", "query": "sleep_sync"})).await;
    assert_eq!(recv_json(&mut ws, Duration::from_secs(2)).await.unwrap()["type"], "Initial");

    send_json(&mut ws, json!({"action": "unsubscribe", "query": "sleep_sync"})).await;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(800);
    while let Some(msg) =
        recv_json(&mut ws, deadline.saturating_duration_since(tokio::time::Instant::now())).await
    {
        assert_ne!(msg["type"], "JobFinished", "no completion is forwarded after unsubscribe");
    }
}

#[tokio::test]
async fn aws10_unsubscribe_all() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;
    let mut ws = connect(&base_url, "/api/assets/ws/q").await;
    send_json(&mut ws, json!({"action": "subscribe", "query": "make_text"})).await;
    let _ = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();

    send_json(&mut ws, json!({"action": "unsubscribe_all"})).await;
    // The subscription's own JobFinished may arrive first (final review).
    let all_unsub = recv_by_type(&mut ws, "UnsubscribedAll").await.unwrap();
    assert_eq!(all_unsub["type"], "UnsubscribedAll");
}

#[tokio::test]
async fn aws11_ping_returns_pong() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;
    let mut ws = connect(&base_url, "/api/assets/ws/q").await;
    send_json(&mut ws, json!({"action": "ping"})).await;

    let pong = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(pong["type"], "Pong");
    assert!(pong["timestamp"].is_string());
}
```

### AWS12a/b: Limits (I6)

```rust
#[tokio::test]
async fn aws12a_subscription_limit_enforced() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app_with_limits(envref, WebSocketLimits { max_message_size: 65536, max_subscriptions: 2 })).await;
    let mut ws = connect(&base_url, "/api/assets/ws/q").await;

    // Final review: two queries that end `Ready` (a live subscription, not a terminal one — a
    // failing `upper` without input would end and free its slot), and a third that is a valid
    // `query` address (a `key` field on ws/q is an Error for the wrong reason, AWS07).
    send_json(&mut ws, json!({"action": "subscribe", "query": "make_text"})).await;
    recv_by_type(&mut ws, "Initial").await.unwrap();
    send_json(&mut ws, json!({"action": "subscribe", "query": "make_text/upper"})).await;
    recv_by_type(&mut ws, "Initial").await.unwrap();

    // Third distinct subscription exceeds max_subscriptions: 2.
    send_json(&mut ws, json!({"action": "subscribe", "query": "make_text/upper/upper"})).await;
    let err_msg = recv_by_type(&mut ws, "Error").await.unwrap();
    assert_eq!(err_msg["type"], "Error");
}

#[tokio::test]
async fn aws12b_oversized_message_closed_or_errors() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app_with_limits(envref, WebSocketLimits { max_message_size: 1024, max_subscriptions: 256 })).await;
    let mut ws = connect(&base_url, "/api/assets/ws/q").await;

    let huge_query = "x".repeat(2000);
    let msg_text = serde_json::to_string(&json!({"action": "subscribe", "query": huge_query})).unwrap();
    let send_result = ws.send(Message::Text(msg_text)).await;

    // Either the frame is rejected at the WebSocket layer, or the server replies Error; both are
    // acceptable, but silence (a message accepted with no reply at all) is not.
    if send_result.is_ok() {
        let result = recv_json(&mut ws, Duration::from_secs(2)).await;
        if let Some(msg) = result {
            assert_eq!(msg["type"], "Error");
        }
    }
}
```

### AWS13: Disconnect Mid-Evaluation (I6)

Uses `sleep_sync`, a blocking `std::thread::sleep` command — multi-thread test runtime.

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aws13_disconnect_mid_evaluation_completes_and_leaks_nothing() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;

    let mut ws = connect(&base_url, "/api/assets/ws/q").await;
    send_json(&mut ws, json!({"action": "subscribe", "query": "sleep_sync"})).await;
    let initial = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(initial["type"], "Initial");

    drop(ws); // disconnect while the evaluation is still running

    let client = reqwest::Client::new();
    let info_url = format!("{}/api/assets/q/info/sleep_sync", base_url.replace("ws://", "http://"));
    for _ in 0..100 {
        let resp = client.get(&info_url).send().await.unwrap();
        if let Ok(body) = resp.text().await {
            if let Ok(json) = serde_json::from_str::<JsonValue>(&body) {
                if json["result"]["status"] == "Ready" {
                    return; // evaluation completed despite the disconnect
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("sleep_sync did not complete within the poll budget after disconnect");
}
```

### AWS14a/b: Builder Sanity (I10)

```rust
#[tokio::test]
async fn aws14a_build_with_defaults_does_not_panic() {
    let envref = env_with(&[]).await;
    let _app = build_app(envref); // axum 0.8's `{*query}` route syntax fix (I10); this must not panic
}

#[tokio::test]
async fn aws14b_without_websocket_removes_both_routes() {
    let envref = env_with(&[]).await;
    let app = AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .without_websocket()
        .build()
        .with_state(envref);
    let base_url = start_server(app).await;

    let result = connect_async(&format!("{}/api/assets/ws/q", base_url)).await;
    assert!(result.is_err(), "ws/q must not exist when without_websocket() is set");
    let result = connect_async(&format!("{}/api/assets/ws/key", base_url)).await;
    assert!(result.is_err(), "ws/key must not exist when without_websocket() is set");
}
```

---

## Other Builders (I4, I5, I9)

### `liquers-axum/tests/store_api_routes.rs` (SAR)

```rust
// liquers-axum/tests/store_api_routes.rs

use axum::{body::Body, body::to_bytes, http::{Request, StatusCode}};
use liquers_axum::store::StoreApiBuilder;
use liquers_core::{
    context::{EnvRef, SimpleEnvironment},
    metadata::Metadata,
    parse::parse_key,
    query::Key,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};
use tower::ServiceExt;

fn env_with_store() -> (EnvRef<SimpleEnvironment<Value>>, AsyncMemoryStore) {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    let store = AsyncMemoryStore::new(&Key::new());
    env.with_async_store(Box::new(store.clone()));
    (env.to_ref(), store)
}

fn build_app(envref: EnvRef<SimpleEnvironment<Value>>) -> axum::Router {
    StoreApiBuilder::<SimpleEnvironment<Value>>::new("/api/store").build().with_state(envref)
}

fn build_app_with_gets(envref: EnvRef<SimpleEnvironment<Value>>) -> axum::Router {
    StoreApiBuilder::<SimpleEnvironment<Value>>::new("/api/store")
        .with_destructive_gets()
        .build()
        .with_state(envref)
}

async fn send(app: axum::Router, method: &str, uri: &str, body: Body) -> (StatusCode, serde_json::Value) {
    let resp = app.oneshot(Request::builder().method(method).uri(uri).body(body).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

async fn send_raw(app: axum::Router, method: &str, uri: &str, body: Body) -> (StatusCode, Vec<u8>) {
    let resp = app.oneshot(Request::builder().method(method).uri(uri).body(body).unwrap()).await.unwrap();
    let status = resp.status();
    (status, to_bytes(resp.into_body(), usize::MAX).await.unwrap().to_vec())
}

#[tokio::test]
async fn sar01_data_get_success() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/file.txt").unwrap(), b"hello", &Metadata::new()).await.unwrap();
    let (status, body) = send_raw(build_app(envref), "GET", "/api/store/data/data/file.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"hello");
}

#[tokio::test]
async fn sar02_data_get_not_found() {
    let (envref, _) = env_with_store();
    let (status, json) = send(build_app(envref), "GET", "/api/store/data/nonexistent/key", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"]["type"], "KeyNotFound");
}

#[tokio::test]
async fn sar03_data_put_success_then_delete() {
    let (envref, _) = env_with_store();
    let app = build_app(envref);
    let (status, json) = send(app.clone(), "PUT", "/api/store/data/data/file.txt", Body::from("hello")).await;
    assert_eq!(status, StatusCode::OK, "the spec's write verb, POST, does not exist: the code uses PUT (see Spec Divergences)");
    assert_eq!(json["status"], "OK");

    let (status, json) = send(app, "DELETE", "/api/store/data/data/file.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "OK");
}

#[tokio::test]
async fn sar04_metadata_get_after_put() {
    let (envref, _) = env_with_store();
    let app = build_app(envref);
    let (status, _) = send(app.clone(), "PUT", "/api/store/data/data/file.txt", Body::from("hello")).await;
    assert_eq!(status, StatusCode::OK);

    let (status, json) = send(app, "GET", "/api/store/metadata/data/file.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"].is_object());
}

#[tokio::test]
async fn sar05_entry_get_success() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/entry.txt").unwrap(), b"content", &Metadata::new()).await.unwrap();
    let (status, _) = send(build_app(envref), "GET", "/api/store/entry/data/entry.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn sar06_listdir_returns_children() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/a.txt").unwrap(), b"a", &Metadata::new()).await.unwrap();
    store.set(&parse_key("data/b.txt").unwrap(), b"b", &Metadata::new()).await.unwrap();
    let (status, json) = send(build_app(envref), "GET", "/api/store/listdir/data", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"].as_array().unwrap().len() >= 2);
}

#[tokio::test]
async fn sar07_is_dir_true_and_false() {
    let (envref, _) = env_with_store();
    let am_store = envref.get_async_store();
    am_store.makedir(&parse_key("adir").unwrap()).await.unwrap();
    let app = build_app(envref);
    let (status, json) = send(app.clone(), "GET", "/api/store/is_dir/adir", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"], true);

    let (status, json) = send(app, "GET", "/api/store/is_dir/no_such_thing", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"], false);
}

#[tokio::test]
async fn sar08_contains_true_and_false() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/x.txt").unwrap(), b"x", &Metadata::new()).await.unwrap();
    let app = build_app(envref);
    let (status, json) = send(app.clone(), "GET", "/api/store/contains/data/x.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"], true);

    let (status, json) = send(app, "GET", "/api/store/contains/nowhere.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "contains on an absent key is OK, not an error");
    assert_eq!(json["result"], false);
}

#[tokio::test]
async fn sar09_keys_root_and_prefix() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/a.txt").unwrap(), b"a", &Metadata::new()).await.unwrap();
    store.set(&parse_key("other/b.txt").unwrap(), b"b", &Metadata::new()).await.unwrap();
    let app = build_app(envref);
    let (status, json) = send(app.clone(), "GET", "/api/store/keys", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"].as_array().unwrap().len() >= 2);

    let (status, json) = send(app, "GET", "/api/store/keys?prefix=data", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    let keys: Vec<&str> = json["result"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert!(keys.iter().all(|k| k.starts_with("data")), "prefix filter applied");
}

#[tokio::test]
async fn sar10_makedir_then_removedir() {
    let (envref, _) = env_with_store();
    let app = build_app(envref);
    let (status, _) = send(app.clone(), "PUT", "/api/store/makedir/newdir", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);

    let (status, json) = send(app.clone(), "GET", "/api/store/contains/newdir", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"], true);

    let (status, _) = send(app, "DELETE", "/api/store/removedir/newdir", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn sar11_upload_single_file_multipart() {
    let (envref, _) = env_with_store();
    let boundary = "----testboundary";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\nContent-Type: text/plain\r\n\r\nhello world\r\n--{b}--\r\n",
        b = boundary
    );
    let app = build_app(envref);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/store/upload/uploads")
                .header("Content-Type", format!("multipart/form-data; boundary={}", boundary))
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["status"], "OK");
    assert!(!json["result"]["uploaded"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn sar12_get_remove_disabled_by_default() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/x.txt").unwrap(), b"x", &Metadata::new()).await.unwrap();
    let (status, _) = send(build_app(envref), "GET", "/api/store/remove/data/x.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "GET remove route does not exist without with_destructive_gets()");
}

#[tokio::test]
async fn sar13_get_remove_enabled_with_flag() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/x.txt").unwrap(), b"x", &Metadata::new()).await.unwrap();
    let (status, json) = send(build_app_with_gets(envref), "GET", "/api/store/remove/data/x.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "OK");
}

#[tokio::test]
async fn sar14_get_makedir_enabled_with_flag() {
    let (envref, _) = env_with_store();
    let (status, _) = send(build_app_with_gets(envref), "GET", "/api/store/makedir/gdir", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn sar15_get_removedir_disabled_by_default() {
    let (envref, _) = env_with_store();
    let (status, _) = send(build_app(envref), "GET", "/api/store/removedir/somedir", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sar16_builder_build_smoke_test() {
    let (envref, _) = env_with_store();
    let _app = build_app(envref); // must not panic
}
```

### `liquers-axum/tests/query_api_routes.rs` (QAR)

```rust
// liquers-axum/tests/query_api_routes.rs

use axum::{body::{to_bytes, Body}, http::{Request, StatusCode}};
use liquers_axum::query::QueryApiBuilder;
use liquers_core::{
    command_metadata::CommandKey,
    context::{Environment, EnvRef, SimpleEnvironment},
    value::Value,
};
use tower::ServiceExt;

fn env_with_commands() -> EnvRef<SimpleEnvironment<Value>> {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("make_text"), |_, _, _| Ok(Value::from("generated")))
        .unwrap();
    env.to_ref()
}

async fn send(app: axum::Router, method: &str, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app.oneshot(Request::builder().method(method).uri(uri).body(Body::empty()).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

#[tokio::test]
async fn qar01_get_query_success() {
    let app = QueryApiBuilder::<SimpleEnvironment<Value>>::new("/q").build().with_state(env_with_commands());
    let resp = app
        .oneshot(Request::builder().method("GET").uri("/q/make_text").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body.as_ref(), b"generated");
}

#[tokio::test]
async fn qar02_post_query_success() {
    let app = QueryApiBuilder::<SimpleEnvironment<Value>>::new("/q").build().with_state(env_with_commands());
    let resp = app
        .oneshot(Request::builder().method("POST").uri("/q/make_text").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn qar03_get_parse_error_400() {
    let app = QueryApiBuilder::<SimpleEnvironment<Value>>::new("/q").build().with_state(env_with_commands());
    let (status, json) = send(app, "GET", "/q/-R/").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["type"], "ParseError");
}

#[tokio::test]
async fn qar04_get_unregistered_command_error() {
    let app = QueryApiBuilder::<SimpleEnvironment<Value>>::new("/q").build().with_state(env_with_commands());
    let (status, json) = send(app, "GET", "/q/no_such_command_registered").await;
    assert_ne!(status, StatusCode::OK);
    assert!(json["error"]["type"].is_string(), "an unregistered action must produce an error envelope");
}

/// `with_timeout` and the "use q/submit" wording are Phase 2 I5 work, not yet implemented — this
/// test is written against the Phase 2 contract, per the mandatory instruction, and will fail to
/// compile/pass until I5 lands. It is not treated as a blocker for the rest of this suite.
#[tokio::test]
async fn qar05_with_timeout_message_points_to_assets_api() {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("slow"), |_, _, _| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            Ok(Value::from("done"))
        })
        .unwrap();
    let app = QueryApiBuilder::<SimpleEnvironment<Value>>::new("/q")
        .with_timeout(std::time::Duration::from_millis(100)) // Phase 2 I5: not yet implemented
        .build()
        .with_state(env.to_ref());
    let (status, json) = send(app, "GET", "/q/slow").await;
    assert_ne!(status, StatusCode::OK, "the 100ms timeout must fire before the 500ms sleep finishes");
    let msg = json["error"]["message"].as_str().unwrap_or("").to_lowercase();
    assert!(msg.contains("q/submit") || msg.contains("q/info"), "timeout message points to the long-running path");
}

#[tokio::test]
async fn qar06_default_timeout_unchanged_for_fast_query() {
    // Regression: QueryApiBuilder::new(...).build() with no with_timeout() call must keep behaving
    // as it does today (30s default) for a query that finishes immediately.
    let app = QueryApiBuilder::<SimpleEnvironment<Value>>::new("/q").build().with_state(env_with_commands());
    let (status, _) = send(app, "GET", "/q/make_text").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn qar07_builder_custom_path() {
    let app = QueryApiBuilder::<SimpleEnvironment<Value>>::new("/custom/q").build().with_state(env_with_commands());
    let (status, _) = send(app, "GET", "/custom/q/make_text").await;
    assert_eq!(status, StatusCode::OK);
}
```

### `liquers-axum/tests/recipes_api_routes.rs` (RAR)

```rust
// liquers-axum/tests/recipes_api_routes.rs

use axum::{body::{to_bytes, Body}, http::{Request, StatusCode}};
use liquers_axum::recipes::RecipesApiBuilder;
use liquers_core::{
    context::{EnvRef, SimpleEnvironment},
    metadata::Metadata,
    parse::parse_key,
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};
use tower::ServiceExt;

async fn env_with(recipes: &[(&str, &str, &str)]) -> EnvRef<SimpleEnvironment<Value>> {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    let mut rl = RecipeList::new();
    for (q, t, d) in recipes {
        rl.add_recipe(Recipe::new(q.to_string(), t.to_string(), d.to_string()).unwrap());
    }
    let store = AsyncMemoryStore::new(&Key::new());
    store.set(&parse_key("recipes.yaml").unwrap(), serde_yaml::to_string(&rl).unwrap().as_bytes(), &Metadata::new()).await.unwrap();
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    env.to_ref()
}

fn build_app(envref: EnvRef<SimpleEnvironment<Value>>) -> axum::Router {
    RecipesApiBuilder::<SimpleEnvironment<Value>>::new("/api/recipes").build().with_state(envref)
}

async fn send(app: axum::Router, method: &str, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app.oneshot(Request::builder().method(method).uri(uri).body(Body::empty()).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

#[tokio::test]
async fn rar01_listdir_empty() {
    let app = build_app(env_with(&[]).await);
    let (status, json) = send(app, "GET", "/api/recipes/listdir").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn rar02_listdir_non_empty() {
    let app = build_app(env_with(&[("make_text/a.txt", "A", ""), ("make_text/b.txt", "B", "")]).await);
    let (status, json) = send(app, "GET", "/api/recipes/listdir").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!json["result"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn rar03_data_get_success() {
    let app = build_app(env_with(&[("-R/notes/a.txt/-/upper/summary.txt", "Summary", "")]).await);
    let (status, _) = send(app, "GET", "/api/recipes/data/summary.txt").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn rar04_data_get_missing_key() {
    let app = build_app(env_with(&[]).await);
    let (status, _) = send(app, "GET", "/api/recipes/data/nonexistent/recipe.txt").await;
    assert_ne!(status, StatusCode::OK, "a key with no recipe is not a 200");
}

#[tokio::test]
async fn rar05_metadata_get_success() {
    let app = build_app(env_with(&[("make_text/a.txt", "A", "")]).await);
    let (status, json) = send(app, "GET", "/api/recipes/metadata/a.txt").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"].is_object());
}

#[tokio::test]
async fn rar06_entry_get_success() {
    let app = build_app(env_with(&[("make_text/a.txt", "A", "")]).await);
    let (status, _) = send(app, "GET", "/api/recipes/entry/a.txt").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn rar07_resolve_success() {
    let app = build_app(env_with(&[("-R/notes/a.txt/-/upper/summary.txt", "Summary", "")]).await);
    let (status, json) = send(app, "GET", "/api/recipes/resolve/summary.txt").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"].is_object(), "resolve returns the recipe's execution plan");
}

#[tokio::test]
async fn rar08_resolve_missing_key() {
    let app = build_app(env_with(&[]).await);
    let (status, _) = send(app, "GET", "/api/recipes/resolve/nonexistent.txt").await;
    assert_ne!(status, StatusCode::OK);
}

#[tokio::test]
async fn rar09_builder_build_smoke_test() {
    let _app = build_app(env_with(&[]).await); // must not panic
}
```

### `liquers-core/src/media_type.rs` tests module (MT)

I9's mapping targets, verified against the current `file_extension_to_media_type` match (read
2026-09-28): `arrow`, `feather` and `parquet` currently fall through to the `_ =>
"application/octet-stream"` default; `ipc` is absent from the match entirely (same default);
`jsonl` currently maps to `"application/jsonlines"`, which I9 changes to `"application/jsonl"`;
`ndjson` is absent (default). The `MT` tests below assert the **target** values from Phase 2 I9,
so they fail today and pass once I9 lands — that is the point of writing them now.

```rust
// liquers-core/src/media_type.rs — #[cfg(test)] mod tests (appended)

#[cfg(test)]
mod media_type_tests {
    use super::*;

    #[test]
    fn mt01_ndjson_maps_to_iana_type() {
        assert_eq!(file_extension_to_media_type("ndjson"), "application/x-ndjson");
    }

    #[test]
    fn mt02_jsonl_maps_to_application_jsonl() {
        assert_eq!(file_extension_to_media_type("jsonl"), "application/jsonl");
    }

    #[test]
    fn mt03_arrow_maps_to_apache_arrow_file() {
        assert_eq!(file_extension_to_media_type("arrow"), "application/vnd.apache.arrow.file");
    }

    #[test]
    fn mt04_feather_maps_to_apache_arrow_file() {
        assert_eq!(file_extension_to_media_type("feather"), "application/vnd.apache.arrow.file");
    }

    #[test]
    fn mt05_ipc_maps_to_apache_arrow_file() {
        assert_eq!(file_extension_to_media_type("ipc"), "application/vnd.apache.arrow.file");
    }

    #[test]
    fn mt06_parquet_maps_to_apache_parquet() {
        assert_eq!(file_extension_to_media_type("parquet"), "application/vnd.apache.parquet");
    }

    #[test]
    fn mt07_csv_regression_unchanged() {
        assert_eq!(file_extension_to_media_type("csv"), "text/csv");
    }

    #[test]
    fn mt08_tsv_regression_unchanged() {
        assert_eq!(file_extension_to_media_type("tsv"), "text/tab-separated-values");
    }

    #[test]
    fn mt09_md_regression_unchanged() {
        assert_eq!(file_extension_to_media_type("md"), "text/markdown");
    }

    #[test]
    fn mt10_unknown_extension_defaults_to_octet_stream() {
        assert_eq!(file_extension_to_media_type("xyz123unknown"), "application/octet-stream");
    }
}
```

---

## Corner Cases

1. **Memory.** `AsyncMemoryStore` holds every version of a key it has ever seen unless removed; the
   `queued` `AssetManager` keeps live `AssetRef`s in a concurrent map (`scc`) keyed by id and by
   query/key. `AMR30`/`AAE43` populate three or more keys per test and rely on `removedir` clearing
   both layers; a leaked live asset after `removedir` would show up as a `key/contains` mismatch
   the assertions already check.
2. **Concurrency.** `AAE80` exercises the `key_mutation_lock` across two concurrent `POST key/data`
   calls to the same key: Phase 2 documents this as *serialized, not merged* — both succeed, and
   the final stored value is exactly one of the two bodies, never a splice. `AWS13` exercises the
   opposite axis: a client disconnecting must not cancel an evaluation other subscribers (or a
   plain poller) still depend on.
3. **Errors.** Every `Error` assertion in this suite compares `error_type` and, where Phase 2 fixes
   the field, `key: Option<String>` (`key.encode()`, never `Option<Key>` — a real drafting bug
   fixed here, see Fixes #3). `KeyNotFound` never carries `.key` (`Error::key_not_found` calls the
   two-argument constructor and stops); no test in this suite asserts `.key` on a `KeyNotFound`.
4. **Serialization.** `AssetInfo`/`MetadataRecord`'s `key`/`query` fields serialize as plain encoded
   strings (`option_key_format`/`option_query_format`), not nested objects; `Status` serializes as
   its bare variant name. Both are asserted directly (`json["result"]["key"] == "notes/a.txt"`,
   `json["result"]["status"] == "Ready"`) throughout AAE and AWS.
5. **Integration.** `AAE24`/`AAE46` are the load-bearing cross-API checks: the same bytes and the
   same `Content-Type` must come back whether a value is reached through `/key/`, through
   `/q/` on the Assets API, or through the standalone Query API's `GET /q/<query>` — all three
   paths converge on `AssetRef::get_binary()` (I1). A divergence there is exactly the kind of bug
   `AXUM-ASSETS-API-SERVES-ONLY-BYTES-AND-TEXT` used to hide.
6. **Recipe nesting.** A recipe query is **not** a directory path: `parse_query("make_text/data/x.txt")`
   parses `data` as a *second chained action* (verified with `liquers-validate --no-registry`), not
   as a subdirectory of the recipe's target key. A recipe under a subdirectory needs its own
   `<dir>/recipes.yaml` with a plain filename in the query. AMR31–AMR33 and RAR's fixtures follow
   this; see "Fixes Made Against the Drafts" #4.
7. **`removedir` is not atomic.** Phase 2 documents a partial failure as leaving already-removed
   keys removed. No test in this suite injects a mid-walk store failure (the in-memory store does
   not fail), so this corner case is **not exercised** here — noted as a gap for Phase 4's
   implementation to cover with a store double, not a defect in this document.

---

## Test Plan

### Exact commands, per file

```bash
# Core AssetManager contract
cargo test -p liquers-core --test asset_manager_remove_expire_describe

# Media types (I9)
cargo test -p liquers-core --lib media_type::media_type_tests

# Assets API value_description unit tests (inline)
cargo test -p liquers-axum --lib assets::value_description::tests

# Assets API HTTP routes (both families, builder switches)
cargo test -p liquers-axum --test assets_api_endpoints

# Assets API WebSocket
cargo test -p liquers-axum --test assets_websocket

# The other three builders (I4)
cargo test -p liquers-axum --test store_api_routes
cargo test -p liquers-axum --test query_api_routes
cargo test -p liquers-axum --test recipes_api_routes

# Everything in liquers-axum in one pass, once all of the above compile
cargo test -p liquers-axum --lib --tests
```

### liquers-web (wasm) check

`liquers-web` does not implement or test the Assets API (it is `liquers-axum`-only), but Phase 2's
`ErrorType::StatusConflict` addition touches `liquers-web/src/error.rs` and
`liquers-web/tests/objects_OBJECT.rs` (both directions of the conversion, plus the string list).
After that change:

```bash
cargo clean   # liquers-web is wasm32-only and excluded from default-members; build it separately
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

This is a compile-and-conformance check on the new `ErrorType` variant, not new Assets API
coverage — there is none in this crate.

### liquers-py check

Same shape: `ErrorType::StatusConflict` touches `liquers-py/src/error.rs` in both directions.

```bash
cargo test -p liquers-py
# or, if the Python extension harness is used instead of `cargo test`:
maturin develop -m liquers-py/Cargo.toml && python -m pytest liquers-py/tests
```

### Feature matrix

None of the new tests are behind an optional feature (`records`, `polars`, `egui`,
`image-support`); they build under `liquers-axum`'s default features. No entry needed in
`scripts/check-build-matrix.sh` beyond what `liquers-core`'s and `liquers-axum`'s existing
default-feature rows already cover.

### Order

1. Core (`AMR`) and `MT` first — they need no HTTP layer and exercise the trait contract the HTTP
   handlers all call through.
2. `VD` next — pure functions, no environment.
3. `AAE`, then `AWS` — HTTP and WebSocket, both depend on the Assets API existing.
4. `SAR`/`QAR`/`RAR` — independent of the Assets API; can run in any order relative to steps 1–3.
5. `liquers-py`/`liquers-web` checks last, after `ErrorType::StatusConflict` is threaded through.

### Validated Queries

Every Liquers query string quoted in this document (recipe queries, `q/` path segments, and the
example transcripts) was run through `liquers-validate` against the exact command set the fixtures
register (`make_text`, `upper`, `make_number`, `make_object`, `sleep_sync`/`sleep_long`,
`fail_always`, `slow`):

```bash
cargo run -p liquers-core --features cli --bin liquers-validate -- \
  --command make_text --command upper --command make_number --command make_object \
  --command sleep_sync --command sleep_long --command fail_always --command slow -- \
  '-R/notes/a.txt/-/upper/summary.txt' 'make_text/source.txt' \
  '-R/summary.txt/-/upper/summary2.txt' '-R/notes/a.txt' 'make_text' \
  '-R/a.txt/-/upper/b.txt' '-R/root.txt/-/upper/level1.txt' '-R/level1.txt/-/upper/level2.txt' \
  'make_number' 'make_object' 'sleep_sync' 'sleep_long' 'fail_always'
```

All queries in the command above validate `Ok`. **Two classes of query in the drafts did not, and
were fixed** (see Fixes #4, #5 below):

- A bare key used **as a query** (`notes/a.txt`, `data/computed.txt`) parses as an *action chain*
  (`notes`, then filename `a.txt`), not as a resource path — expected and by design: these strings
  are only ever used as `/key/` route paths (parsed with `parse_key`), never passed to
  `liquers-validate` or to `parse_query` in a test.
- A recipe query with a literal subdirectory segment after the action
  (`"make_text/data/computed.txt"`, `"-R/data/source.txt/-/upper/data/derived.txt"`) parses **but
  means something other than what the drafts intended**: the subdirectory segment (`data`) is
  parsed as a second chained action, not as part of the target key's path. Confirmed with
  `liquers-validate --no-registry`, which shows two entries in the `Transform`'s `query` array
  (`make_text`, then `data`) instead of one. Fixed throughout this document by storing the
  `RecipeList` at `<dir>/recipes.yaml` and keeping the recipe's own query filename plain (see
  Corner Case 6 and Fixes #4).

---

## Spec Divergences for the I8 Audit

Every row below was checked against **both** `specs/reference/WEB_API_SPECIFICATION.md` and the
current code (2026-09-28); only rows confirmed against both are kept. Rows draft 4 raised but that
turned out to be already correct, already tracked elsewhere, or unverifiable without code that
does not exist yet, are dropped (see "Fixes Made Against the Drafts" #6).

| # | Spec says | Code does | Evidence |
|---|---|---|---|
| D1 | §4.1.2 "POST /api/store/data/{*key}" | `PUT` | `liquers-axum/src/store/builder.rs:44` `.put(crate::store::handlers::put_data_handler::<E>)` |
| D2 | §4.1.4 "POST /api/store/metadata/{*key}" | `PUT` | `liquers-axum/src/store/builder.rs:52` `.put(crate::store::handlers::put_metadata_handler::<E>)` |
| D3 | §4.1.14 "POST /api/store/entry/{*key}" | `PUT` | `liquers-axum/src/store/builder.rs:59` `.put(crate::store::handlers::put_entry_handler::<E>)` |
| D4 | §5.2.1 `WS /ws/assets/{*query}`, one endpoint, query-only | Phase 2: two endpoints, `ws/q` and `ws/key` (O5), each also key- or query-typed | `WEB_API_SPECIFICATION.md:1389`; Phase 2 "WebSocket Notifications" §"Routes" |
| D5 | §10 usage examples import `liquers_web::{StoreApiBuilder, AssetsApiBuilder, QueryApiBuilder}` and `liquers_web::FullApiBuilder` / `liquers_web_axum::{FullApiBuilder, serve}` | No `liquers_web` crate of this shape exists; the real crate is `liquers-axum` (`liquers_axum::{store, query, assets, recipes}`), and no `FullApiBuilder` exists anywhere — assembly is `Router::merge` of the four builders | `WEB_API_SPECIFICATION.md:2006,2026,2278`; confirmed no `FullApiBuilder` definition anywhere in the workspace (`liquers-axum/src` has no such type) |
| D6 | §4/§5 comparison table row: `GET /api/assets/remove/{*query}` (unprefixed) | Removed by Phase 2 O1; no unprefixed Assets API route exists after this design | `WEB_API_SPECIFICATION.md:877`; Phase 2 "Routes" table has no unprefixed row |
| D7 | §5.1 uses `/api/assets/data/{*query}`, `/api/assets/metadata/{*query}`, `/api/assets/listdir/{*query}` (unprefixed, query-only, no key family) | Phase 2 replaces the whole family split with `/api/assets/q/…` and `/api/assets/key/…`, plus `/api/assets/admin/…` | `WEB_API_SPECIFICATION.md:899,939,974`; Phase 2 "Addressing: two route families" |

Rows D1–D3 are already tracked in Phase 2's Known-Issue Preflight under
`WEB-API-SPECIFICATION-DIVERGES-FROM-IMPLEMENTATION` (I8); D4–D7 are the routing/crate drift the
same issue's scope already names ("§5 as a whole", "the WebSocket path", "`FullApiBuilder` does not
exist"). No new issue needs filing: I8's "Change" already covers rewriting §5 and auditing §2–§4 and
§6–§10 route by route with the SAR/QAR/RAR/AAE/AWS tests in this document as the evidence.

**Dropped from draft 4's list** (verified false, already-known, or unverifiable without new code):
its item 9 ("Recipes API is read-only, as documented") is not a divergence, it is a confirmation;
its item 10 ("verify Status match exhaustiveness in query handlers") is a code-quality note, not a
spec-vs-code divergence, and belongs with I4's route coverage instead; its items 6–7 (query timeout
hardcoded, message wording) are Phase 2 I5 work in progress, not spec drift — see Fixes #5.

---

## Fixes Made Against the Drafts

Verified against the current code (`liquers-core/src/metadata.rs`, `error.rs`, `assets.rs`,
`store.rs`, `media_type.rs`, `type_system.rs`, `query.rs`, `recipes.rs`, `liquers-axum/src/{store,
query,recipes,assets}/builder.rs`, `liquers-axum/src/query/handlers.rs`,
`specs/reference/WEB_API_SPECIFICATION.md`) and `liquers-validate` runs, 2026-09-28.

1. **Draft 2 omitted the I1 value-type tests entirely.** Added AAE44–AAE47: `q/data` serving an
   `Value::I64` and a `Value::Object` (verified constructors in `liquers-core/src/value.rs`:
   `Value::I64(i64)`, `Value::Object(BTreeMap<String, Value>)`); a byte-for-byte, header-for-header
   cross-check between the Assets API's `q/data` and the standalone Query API's `GET /q/<query>`
   for the same non-Bytes/Text query, via a new `build_app_with_query_api` helper that merges
   `AssetsApiBuilder` and `QueryApiBuilder` into one router; and a `key/data` test for a JSON value
   written through `set_binary` as raw bytes with `data_format: "json"`.

2. **Draft 5's AAE92 accepted `Cancelled` OR `Ready`, which is not deterministic.** Rewritten as
   `aae92_cancel_while_processing_reports_cancelled_deterministically`: submit a slow
   (`std::thread::sleep`) command, `poll_until` the asset reaches `Processing` (a deterministic
   gate — the cancel is never racing a job that has not started), cancel, then `poll_until`
   `Cancelled`. Runs on `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]` per fix #3.

3. **Blocking sync commands need the multi-thread test runtime.** `sleep_sync` (`AWS13`) and the
   rewritten `sleep_long` (`AAE92`) both call `std::thread::sleep` inside a registered command. The
   queued `AssetManager` runs commands on a spawned task; on the default single-threaded
   `#[tokio::test]` runtime that task can starve the test's own polling loop. Both tests (and only
   those — no other test in this suite submits a blocking command) now carry `#[tokio::test(flavor
   = "multi_thread", worker_threads = 2)]`.

4. **Recipe queries with a literal subdirectory segment do not parse as directories.** Draft 1's
   AMR31–AMR33 wrote recipe queries like `"make_text/data/computed.txt"` and
   `"-R/data/source.txt/-/upper/data/derived.txt"`, intending `data/computed.txt` as the resulting
   key. Verified with `liquers-validate --no-registry`: `data` there parses as a **second chained
   action** in the `Transform`'s query array, not as a directory component — the grammar has no
   "path-valued filename". `DefaultRecipeProvider::recipe_opt` reads `<key's own directory>/
   recipes.yaml` and joins the recipe's filename to that directory (`recipes.rs`, `add_recipe`/
   `filename()`/`cwd`), so the correct fixture is a **second `RecipeList` stored at
   `data/recipes.yaml`**, with plain filenames in its recipes (`"make_text/computed.txt"`,
   `"-R/data/source.txt/-/upper/derived.txt"`). Added `env_with_at(dir, recipes)` to both the AMR
   and AAE helper blocks; AMR31–AMR33 use it. See Corner Case 6.

5. **Draft 4 treated `QueryApiBuilder::with_timeout` and the timeout message as blockers to route
   around.** They are Phase 2 I5 work to be implemented, not evidence the tests cannot be written.
   `QAR05` calls `.with_timeout(Duration::from_millis(100))` and asserts the documented message
   substring (`"q/submit"` or `"q/info"`) directly against the Phase 2 contract; it will not compile
   until I5 adds the builder method, which is the intended effect — the same "safety net" pattern
   the exhaustive `ErrorType` match already relies on elsewhere in this design.

6. **Every item in draft 4's "Spec Divergences Observed" was re-checked against both
   `WEB_API_SPECIFICATION.md` and the code**, quoting section and line for each kept row (see
   "Spec Divergences for the I8 Audit" above). Three store-API method mismatches (POST vs PUT for
   `data`/`metadata`/`entry`) were confirmed by reading `liquers-axum/src/store/builder.rs` directly
   and are kept; the WebSocket-path and crate-name/`FullApiBuilder` divergences were confirmed by
   reading the spec's own text and grepping the workspace for `FullApiBuilder` (no match) and are
   kept; the media-type gaps are I9 itself, not a separate divergence, and are folded into the MT
   tests instead of duplicated in the divergence table; the query-timeout items are Phase 2 I5 work
   in progress (see #5), not spec drift, and are dropped from the divergence table; the "Recipes API
   is read-only" and "verify Status match exhaustiveness" items are not divergences at all and are
   dropped.

7. **Draft 1's test counts were internally inconsistent** ("Test count: 61" in its overview, but
   its own numbered list contains 37 tests, and its ID ranges — `AMR01–AMR07, AMR10–AMR24,
   AMR30–AMR34, AMR40–AMR44, AMR50–AMR53, AMR60–AMR61` — describe 39 slots against 37 actual
   functions, since `AMR21` and `AMR08–AMR09` were never defined). Recounted by hand from the
   actual `#[tokio::test]`/`#[test]` functions in this document: **37** AMR tests, listed exactly
   in the Overview Table. `AMR21` ("set_description leaves version unchanged") duplicated AMR17's
   own assertion and was folded into AMR17 rather than kept separate (same call already asserted in
   the same test) — same merge v1 made, re-verified here rather than re-derived.

8. **Exact routing status codes, made precise instead of "404 or 405" placeholders.** Per the
   instruction: an omitted method on a path that still serves another method → **405** (axum
   reports Method Not Allowed on a route that exists for other verbs); a path with no route
   registered at all → **404**. Applied throughout AAE60–AAE66: `read_only()` + `POST key/data` is
   405 (GET/DELETE key/data still exist on that path); `read_only()` + `POST key/expire` is 404
   (expire has no sibling method, so the whole route disappears); `with_admin(false)` + `POST
   admin/audit` is 404 (same reasoning); `GET key/remove` without `with_destructive_gets()` is 404
   (the route does not exist at all, not merely a wrong verb). Every assertion in this document that
   checks a disabled-route status now names the exact code, not a `matches!(status, 404 | 405)`.

9. **`AssetManager` import, JSON `Content-Type`, `CommandKey`, `set_binary` by value — carried
   forward from `phase3-v1.md`'s own fixes, re-verified against the current code rather than
   re-derived from scratch:** every test file imports `liquers_core::assets::AssetManager` (`am.*`
   calls do not resolve otherwise); `send_json`/`post_entry_json` always set `content-type:
   application/json` (axum's `Json` extractor answers 415 otherwise, a real bug v1 found in an
   earlier draft's `AAE01`); `CommandKey` is `liquers_core::command_metadata::CommandKey`;
   `AssetManager::set_binary` takes `MetadataRecord` **by value**, confirmed again by reading
   `liquers-core/src/assets.rs`'s trait signature (drafts 2/3/5 in this round again wrote `&metadata_text()`
   in a couple of call sites; fixed).

10. **`AssetManager::makedir` returns `Result<AssetRef<E>, Error>`, not `Result<(), Error>`.**
    Verified in `liquers-core/src/assets.rs`. Every `am.makedir(&key).await?;` call site in this
    document ends in a semicolon, discarding the returned `AssetRef` — this compiles (Rust allows
    discarding a non-`#[must_use]`-marked expression statement) and is intentional: the tests that
    need the directory's `AssetRef` call `get_asset_info` afterward instead.

11. **`AssetManager::version` returns `Result<Option<Version>, Error>`.** AMR03/AMR17/AMR32 compare
    the `Option<Version>` directly (`assert_eq!(am.version(&key).await?, version_before)`); the
    HTTP-level version tests (AAE23, Example 1 step 8) instead compare the **hex-string** field of
    the JSON response, which is a different, already-serialized representation — the two are not
    mixed in any one assertion.

12. **`GET key/entry`'s `Accept` negotiation was untested in draft 2** (its AAE04 sent
    `Accept: application/json` but asserted only `200`, not that the header was honored — the
    handler could ignore it and still pass). AAE04 (renamed from AAE04 in draft 2, same slot) now
    asserts `Content-Type: application/json` on the response, which is exactly the bug Phase 2's
    "Handlers — new or changed" section calls out fixing (`get_entry_handler` passing an empty
    `HeaderMap` today).

13. **Draft 2's AAE28 was not a real test** (a `key/recover` "scenario" with the assertion
    commented out because a `Source` cannot be expired). Rewritten as
    `aae28_key_recover_reads_expired_without_evaluating`: a recipe-computed value is evaluated,
    expired, and then `key/recover` is asserted to return its last known bytes with a 200, which is
    exactly the scenario `EXPIRATION-RECOVERY-WEB-API` (I2) exists to fix.

14. **Draft 3's helper imports do not match the rest of the suite**
    (`liquers_core::environment::SimpleEnvironment`, `liquers_core::key::parse_key` — neither
    module exists; the real paths are `liquers_core::context::SimpleEnvironment` and
    `liquers_core::parse::parse_key`, as used throughout `liquers-core/tests/` today). Fixed in the
    AWS helper block.

15. **Draft 4's `TestEnv::new()` does not compile against the real `SimpleEnvironment` API**
    (`SimpleEnvironment::new(store.clone())` — the constructor takes no arguments;
    `EnvRef::new(env.env.clone())` where `env.env` is an `Arc<SimpleEnvironment<Value>>` and
    `EnvRef::new` takes `E` by value, not `Arc<E>`). SAR/QAR/RAR are written from scratch in this
    document against the verified `SimpleEnvironment::new()` + `.with_async_store(Box::new(store))`
    + `.to_ref()` pattern already proven correct in AMR/AAE/AWS.

## Review Log

Multi-agent review, 2026-09-28 (Phase 3 review pass, three reviewers), fixer pass applied in this
revision.

- **Reviewer 1 (Phase 1 conformity):** no blocking findings. One gap was fixed: version-zeros
  behavior (O3) had no test on either family. Added AAE06 (`key/version` on a never-evaluated
  recipe key) and AAE07 (`q/version` read right after `q/submit` of a slow query, before it
  finishes), both asserting the 32-zero `Version::unknown()` sentinel. Reviewer 1 also claimed the
  `MT` tests (media types, I9) had no code in this document — **that claim was checked and found
  wrong**: `liquers-core/src/media_type.rs` tests module already carries ten full `#[test]`
  functions (MT01–MT10, counted directly by `grep -c 'fn mt'` against this document), so no action
  was taken on that point.
- **Reviewer 2 (Phase 2 route-coverage gaps):** found that several routes named in Phase 2's
  "Routes" and "GET alternatives for every operation" tables had no test anywhere in this document.
  All are fixed here, with free `AAE` IDs:
  - Finding 1 (version zeros) — folded into Reviewer 1's item above (AAE06–AAE07).
  - Finding 2: `GET q/submit`/`GET key/submit`, the one GET alternative that is always on — AAE08,
    AAE09.
  - Finding 3: `POST admin/audit/{key}`, and with `with_destructive_gets()`, `GET admin/audit` and
    `GET admin/audit/{key}` — AAE18, AAE19.
  - Finding 4: `POST`/`GET key/override` on a computed `Ready` value (→ `Override`), on a `Source`
    with no recipe (no-op, Q19), and with no data at all (404) — AAE30–AAE33.
  - Finding 5: `GET key/description?title=&description=` with `with_destructive_gets()` — AAE34.
  - Finding 6: the query-family observe/read routes `q/metadata` (cached and uncached) and
    `q/entry?format=json` — AAE35–AAE37.
  - Finding 7: the remaining `with_destructive_gets()` GET alternatives (`q/cancel`, `key/cancel`,
    `key/removedir`, `key/makedir`, `key/expire`, `admin/refresh_command_versions`), one focused
    test each, plus one test (AAE69) proving that, with the flag off, every one of those six paths
    answers **405** rather than 404, because each keeps another method (POST/PUT/DELETE)
    registered on the same path — the reasoning is stated inline in that test, path by path, and
    contrasted with AAE63 (`read_only()` removes `key/expire`'s only method, giving 404 instead) —
    AAE38–AAE39, AAE48–AAE49, AAE67–AAE69.

  Overview Table, per-file counts and the Test Plan are updated to match: `assets_api_endpoints.rs`
  grew from 42 to **63** tests (IDs `AAE01–AAE49, AAE60–AAE69, AAE80, AAE90–AAE92` — the ranges
  `01`–`49` are now contiguous), and the document total from 148 to **169**, both counted by
  grepping `fn aae`/`fn amr`/`fn vd`/`fn aws`/`fn sar`/`fn qar`/`fn rar`/`fn mt` against this
  document rather than estimated.
- **Reviewer 3 (codebase and query alignment):** no findings. Re-ran `liquers-validate` over every
  query string quoted or introduced in this revision (the new tests reuse `make_text`, `upper`,
  `make_number`, `make_object`, plus new one-off slow commands local to their own tests, none of
  which are queries — only `make_text`/`computed.txt`/`a.txt`/`never.txt` recipe targets and bare
  `/key/` paths appear, already covered by the existing "Validated Queries" run) — no new query
  shapes were added, so no new validation gap exists. Confirmed the per-directory `recipes.yaml`
  approach (`env_with_at`, Corner Case 6) against `DefaultRecipeProvider::recipe_opt` again: it
  still reads `<key's own directory>/recipes.yaml` and joins the recipe's own filename to that
  directory, exactly as the existing AMR31–AMR33 fixtures assume; none of the new tests introduce a
  nested-path recipe query, so the existing fix is sufficient and nothing further was needed.

No test in this pass duplicates an existing ID; free ranges (`AAE06–AAE09`, `AAE18–AAE19`,
`AAE30–AAE39`, `AAE48–AAE49`, `AAE67–AAE69`) were used exactly as assigned.

**Final review, 2026-09-28** (cross-phase, against the core code):
- AWS03 subscribed to a `Source`, which is already finished and never reports `Ready` or a later
  `JobFinished`; it now subscribes to the recipe key `summary.txt`.
- AWS05 waited for `StatusChanged` with status `Error`, which the core never sends (a failure is
  `ErrorOccurred` + `JobFinished`); it now waits for any message whose `info.status` is `Error`.
- AWS09 raced `make_text`'s immediate `JobFinished`; it now uses `sleep_sync` (multi-thread).
  AWS10 and AWS12a use `recv_by_type`, and AWS12a's third subscription is a valid query (a `key`
  field on `ws/q` was an `Error` for the wrong reason).
- AMR32/AMR33 are marked blocked on Phase 2 O15 (`store.removedir` deletes the kept entries and
  `recipes.yaml`).
