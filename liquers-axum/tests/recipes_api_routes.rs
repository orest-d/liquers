//! Recipes API routes (`RecipesApiBuilder`). Design: `specs/design/axum-assets-endpoints/`
//! (Phase 3, RAR01–RAR09).

use axum::{body::{to_bytes, Body}, http::{Request, StatusCode}};
use liquers_axum::recipes::RecipesApiBuilder;
use liquers_core::{
    command_metadata::CommandKey,
    state::State,
    context::{EnvRef, Environment, SimpleEnvironment},
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
    // The recipes' commands must exist for `resolve` to build a plan.
    env.command_registry
        .register_command(CommandKey::new_name("make_text"), |_, _, _| Ok(Value::from("generated")))
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
    store.set(&parse_key("recipes.yaml").unwrap(), serde_yaml::to_string(&rl).unwrap().as_bytes(), &Metadata::new()).await.unwrap();
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
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
    assert_eq!(status, StatusCode::OK, "{json}");
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

// --- recipe metadata and entry (`specs/design/axum-recipes-metadata-entry/`) ---

/// Sends a GET, optionally with an `Accept` header, and returns the status, `Content-Type` and
/// raw body.
async fn get_raw(app: axum::Router, uri: &str, accept: Option<&str>) -> (StatusCode, String, Vec<u8>) {
    let mut request = Request::builder().method("GET").uri(uri);
    if let Some(accept) = accept {
        request = request.header("accept", accept);
    }
    let resp = app.oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
    let status = resp.status();
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, content_type, bytes.to_vec())
}

const REPORT: (&str, &str, &str) = ("make_text/report.txt", "Report", "The daily report");

/// The metadata is the recipe provider's asset info for the key, not a placeholder `{}`.
#[tokio::test]
async fn recipe_metadata_returns_recipe_asset_info() {
    let app = build_app(env_with(&[REPORT]).await);
    let (status, json) = send(app, "GET", "/api/recipes/metadata/report.txt").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["title"], "Report");
    assert_eq!(json["result"]["description"], "The daily report");
    assert_eq!(json["result"]["filename"], "report.txt");
}

#[tokio::test]
async fn recipe_entry_honours_format_parameter() {
    let app = build_app(env_with(&[REPORT]).await);
    let (status, content_type, body) =
        get_raw(app, "/api/recipes/entry/report.txt?format=json", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type, "application/json");
    // `data` is base64 in JSON; `DataEntry`'s own deserializer decodes it.
    let entry: liquers_axum::api_core::response::DataEntry = serde_json::from_slice(&body).unwrap();
    assert!(String::from_utf8(entry.data).unwrap().contains("make_text/report.txt"));
    assert_eq!(entry.metadata["title"], "Report");
}

#[tokio::test]
async fn recipe_entry_honours_accept_header() {
    let app = build_app(env_with(&[REPORT]).await);
    let (status, content_type, _) = get_raw(
        app.clone(),
        "/api/recipes/entry/report.txt",
        Some("application/json"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type, "application/json");

    let (status, content_type, _) = get_raw(app, "/api/recipes/entry/report.txt", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type, "application/cbor", "CBOR stays the default");
}

#[tokio::test]
async fn recipe_metadata_and_entry_of_an_unknown_key_fail() {
    let app = build_app(env_with(&[REPORT]).await);
    let (status, _) = send(app.clone(), "GET", "/api/recipes/metadata/nonexistent.txt").await;
    assert_ne!(status, StatusCode::OK);
    let (status, _, _) = get_raw(app, "/api/recipes/entry/nonexistent.txt", None).await;
    assert_ne!(status, StatusCode::OK);
}
