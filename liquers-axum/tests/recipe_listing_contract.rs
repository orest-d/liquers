//! Listed vs producible keys, and Store API writes notifying the recipe provider.
//! Design: `specs/design/recipe-provider-listing-contract/`.

use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use liquers_core::{
    command_metadata::CommandKey,
    context::{EnvRef, Environment, SimpleEnvironment},
    error::Error,
    parse::parse_key,
    plan::Plan,
    query::{Key, ResourceName},
    recipes::{AsyncRecipeProvider, Recipe},
    store::AsyncMemoryStore,
    value::Value,
};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

type Env = SimpleEnvironment<Value>;

/// Produces `<name>.gen` in any folder and lists nothing; records `directory_changed`.
#[derive(Default)]
struct PatternProvider {
    changed: Arc<Mutex<Vec<Key>>>,
}

#[async_trait]
impl AsyncRecipeProvider<Env> for PatternProvider {
    async fn has_recipes(&self, _key: &Key, _envref: EnvRef<Env>) -> Result<bool, Error> {
        Ok(false)
    }
    async fn assets_with_recipes(
        &self,
        _key: &Key,
        _envref: EnvRef<Env>,
    ) -> Result<Vec<ResourceName>, Error> {
        Ok(vec![])
    }
    async fn recipe_plan(&self, key: &Key, _envref: EnvRef<Env>) -> Result<Plan, Error> {
        Err(Error::key_not_found(key))
    }
    async fn recipe(&self, key: &Key, envref: EnvRef<Env>) -> Result<Recipe, Error> {
        self.recipe_opt(key, envref)
            .await?
            .ok_or_else(|| Error::key_not_found(key))
    }
    async fn recipe_opt(&self, key: &Key, _envref: EnvRef<Env>) -> Result<Option<Recipe>, Error> {
        let Some(name) = key.filename().filter(|n| n.name.ends_with(".gen")) else {
            return Ok(None);
        };
        Ok(Some(Recipe::new(
            format!("make_text/{}", name.name),
            String::new(),
            String::new(),
        )?))
    }
    async fn directory_changed(&self, dir: &Key) {
        if let Ok(mut changed) = self.changed.lock() {
            changed.push(dir.clone());
        }
    }
}

fn env() -> (EnvRef<Env>, Arc<Mutex<Vec<Key>>>) {
    let provider = PatternProvider::default();
    let changed = provider.changed.clone();
    let mut env: Env = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("make_text"), |_, _, _| {
            Ok(Value::from("generated"))
        })
        .unwrap();
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    env.with_recipe_provider(Box::new(provider));
    (env.to_ref(), changed)
}

fn app(envref: EnvRef<Env>) -> axum::Router {
    liquers_axum::assets::AssetsApiBuilder::<Env>::new("/api/assets")
        .build()
        .merge(liquers_axum::store::StoreApiBuilder::<Env>::new("/api/store").build())
        .with_state(envref)
}

async fn send(app: axum::Router, method: &str, uri: &str, body: Body) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(Request::builder().method(method).uri(uri).body(body).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

#[tokio::test]
async fn rplc01_can_make_route_reports_producible_keys_that_are_not_listed() {
    let (envref, _) = env();
    let app = app(envref);
    let (status, json) = send(app.clone(), "GET", "/api/assets/key/can_make/a/x.gen", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["can_make"], true);
    let (_, json) = send(app.clone(), "GET", "/api/assets/key/contains/a/x.gen", Body::empty()).await;
    assert_eq!(json["result"]["contains"], false);
    let (_, json) = send(app, "GET", "/api/assets/key/can_make/a/x.txt", Body::empty()).await;
    assert_eq!(json["result"]["can_make"], false);
}

#[tokio::test]
async fn rplc02_an_unlisted_producible_key_can_be_described_and_submitted() {
    let (envref, _) = env();
    let app = app(envref);
    let (status, _) = send(app.clone(), "GET", "/api/assets/key/info/a/x.gen", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(app.clone(), "GET", "/api/assets/key/submit/a/x.gen", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(app, "GET", "/api/assets/key/submit/a/x.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rplc03_store_api_writes_notify_the_provider_with_the_parent_directory() {
    let (envref, changed) = env();
    let app = app(envref);
    let (status, _) = send(app.clone(), "PUT", "/api/store/data/data/sales/a.manifest.yaml", Body::from("x")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(app, "DELETE", "/api/store/data/data/sales/a.manifest.yaml", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    let seen = changed.lock().unwrap().clone();
    assert_eq!(seen, vec![parse_key("data/sales").unwrap(), parse_key("data/sales").unwrap()]);
}
