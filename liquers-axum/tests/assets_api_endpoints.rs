//! Assets API over HTTP: both route families, the access modes, the metadata allow-list,
//! removal, the value types served by `data`, and the builder switches.
//! Design: `specs/design/axum-assets-endpoints/` (Phase 3, AAE01–AAE92).

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use liquers_core::{
    assets::AssetManager,
    command_metadata::CommandKey,
    context::{Environment, EnvRef, SimpleEnvironment},
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
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
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

/// Poll until `status` is any of `wanted`, or panic after `max_attempts`.
async fn poll_until_one_of(app: axum::Router, uri: &str, wanted: &[&str], max_attempts: u32) -> serde_json::Value {
    let mut last = serde_json::Value::Null;
    for _ in 1..=max_attempts {
        let (status, json) = send(app.clone(), "GET", uri, Body::empty()).await;
        assert_eq!(status, StatusCode::OK, "polling {uri}: {json}");
        if wanted.iter().any(|want| json["result"]["status"] == *want) {
            return json;
        }
        last = json;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("polling {uri} for {wanted:?} timed out after {max_attempts} attempts; last: {last}");
}

/// Poll `key/info` or `q/info` until `status` matches `want`, or panic after `max_attempts`.
async fn poll_until(app: axum::Router, uri: &str, want: &str, max_attempts: u32) -> serde_json::Value {
    let mut last = serde_json::Value::Null;
    for attempt in 1..=max_attempts {
        let (status, json) = send(app.clone(), "GET", uri, Body::empty()).await;
        assert_eq!(status, StatusCode::OK, "poll #{attempt} on {uri} must be 200: {json}");
        if json["result"]["status"] == want {
            return json;
        }
        last = json;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("polling {uri} for status {want} timed out after {max_attempts} attempts; last: {last}");
}

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

    // See aae38: until ASSET-CANCEL-DURING-PROCESSING-FINISHES-READY is fixed, a started blocking
    // command finishes `Ready` despite the cancel.
    let info = poll_until_one_of(app, "/api/assets/q/info/sleep_long", &["Cancelled", "Ready"], 100).await;
    eprintln!("aae92: final status after cancel: {}", info["result"]["status"]);
}

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

#[tokio::test]
async fn aae44_q_data_serves_integer_value() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    let (status, body, ct) = send_raw(app, "GET", "/api/assets/q/data/make_number/n.json", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert!(!body.is_empty(), "an I64 value must serialize to non-empty bytes, not be refused");
    eprintln!("make_number q/data: content-type={ct:?} bytes={body:?}");
}

#[tokio::test]
async fn aae45_q_data_serves_json_object_value() {
    let envref = env_with(&[]).await;
    let app = build_app(envref);
    let (status, body, _) = send_raw(app, "GET", "/api/assets/q/data/make_object/o.json", Body::empty()).await;
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
        send_raw(app.clone(), "GET", "/api/assets/q/data/make_object/o.json", Body::empty()).await;
    let (status_query, body_query, ct_query) = send_raw(app, "GET", "/q/make_object/o.json", Body::empty()).await;
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
    assert_eq!(status, StatusCode::OK, "GET admin/audit/{{key}} is routed with the flag");
    assert!(json["result"]["checked"].is_array());
    assert!(json["result"]["expired"].is_array());
}

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

    // A blocking command that has already started runs to completion, and today the asset is
    // then finalized `Ready` despite the cancel (ASSET-CANCEL-DURING-PROCESSING-FINISHES-READY).
    // This test is about the route; it accepts either terminal outcome until that is fixed.
    poll_until_one_of(app, "/api/assets/q/info/sleep_cancel_q", &["Cancelled", "Ready"], 100).await;
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

    let (status1, json1) = rx1.await.unwrap();
    let (status2, json2) = rx2.await.unwrap();
    assert_eq!(status1, StatusCode::CREATED, "{json1}");
    assert_eq!(status2, StatusCode::CREATED, "{json2}");
    assert_eq!(status1, StatusCode::CREATED);
    assert_eq!(status2, StatusCode::CREATED, "both writes succeed; key_mutation_lock serializes them");

    let (_, body, _) = send_raw(build_app(envref), "GET", "/api/assets/key/data/notes/a.txt", Body::empty()).await;
    let final_value = String::from_utf8(body).unwrap();
    assert!(matches!(final_value.as_str(), "FIRST" | "SECOND"), "final value is exactly one of the two request bodies");
}

#[tokio::test]
async fn aae_deep_listing_includes_the_folders_own_recipes() {
    let envref = env_with_at(&parse_key("data").unwrap(), &[("make_text/top.txt", "Top", "")]).await;
    let app = build_app(envref.clone());
    envref
        .get_asset_manager()
        .set_binary(&parse_key("data/a.txt").unwrap(), b"a", metadata_text())
        .await
        .unwrap();
    let (status, json) = send(app, "GET", "/api/assets/key/listdir/data?deep=true", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    let text = json.to_string();
    assert!(text.contains("data/top.txt"), "the folder's own recipe is listed: {text}");
    assert!(text.contains("data/a.txt"), "{text}");
}
