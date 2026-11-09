//! The assembled server (`specs/design/axum-server-assembly/`, Phase 3 test 7): the APIs a
//! configuration mounts, binding, and the environment source.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use liquers_axum::{Api, EnvSource, LiquersServerBuilder, ServerConfig, WebSetting};
use liquers_core::command_metadata::CommandKey;
use liquers_core::context::{EnvRef, Environment, SimpleEnvironment};
use liquers_core::error::{Error, ErrorType};
use liquers_core::metadata::Metadata;
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::{AsyncMemoryStore, AsyncStore};
use liquers_core::value::Value;
use tower::ServiceExt;

type Env = SimpleEnvironment<Value>;

async fn env_saying(text: &'static str) -> EnvRef<Env> {
    let mut env = Env::new();
    env.command_registry
        .register_command(CommandKey::new_name("hello"), move |_, _, _| Ok(Value::from(text)))
        .unwrap();
    let store = AsyncMemoryStore::new(&Key::new());
    store.set(&parse_key("web/index.html").unwrap(), b"<h1>app</h1>", &Metadata::new()).await.unwrap();
    store.set(&parse_key("data/x.txt").unwrap(), b"x", &Metadata::new()).await.unwrap();
    env.with_async_store(Box::new(store));
    env.to_ref()
}

async fn status(app: &axum::Router, method: &str, uri: &str) -> StatusCode {
    let request = Request::builder().method(method).uri(uri).body(Body::empty()).unwrap();
    app.clone().oneshot(request).await.unwrap().status()
}

fn local(config: ServerConfig) -> ServerConfig {
    ServerConfig { port: 0, ..config }
}

#[tokio::test]
async fn config_without_store_api_and_read_only() {
    let envref = env_saying("Hello").await;
    let full = LiquersServerBuilder::<Env>::new("/liquer").router().with_state(envref.clone());
    assert_eq!(status(&full, "GET", "/liquer/api/store/data/data/x.txt").await, StatusCode::OK);
    assert_eq!(status(&full, "GET", "/liquer/api/recipes/listdir").await, StatusCode::OK);
    assert_eq!(status(&full, "GET", "/liquer/q/hello").await, StatusCode::OK);

    let config: ServerConfig =
        serde_yaml::from_str("{port: 8000, apis: [query, assets], read_only: true, web: false}").unwrap();
    let narrowed = LiquersServerBuilder::<Env>::from_config(config).router().with_state(envref);
    assert_eq!(status(&narrowed, "GET", "/liquer/q/hello").await, StatusCode::OK);
    assert_eq!(status(&narrowed, "GET", "/liquer/api/store/data/data/x.txt").await, StatusCode::NOT_FOUND);
    assert_eq!(status(&narrowed, "GET", "/liquer/api/recipes/listdir").await, StatusCode::NOT_FOUND);
    assert_eq!(status(&narrowed, "GET", "/liquer/api/assets/key/info/data/x.txt").await, StatusCode::OK);
    let delete = status(&narrowed, "DELETE", "/liquer/api/assets/key/data/data/x.txt").await;
    assert!(
        delete == StatusCode::METHOD_NOT_ALLOWED || delete == StatusCode::NOT_FOUND,
        "a read-only server has no assets mutation route, got {delete}"
    );
    assert_eq!(status(&narrowed, "GET", "/").await, StatusCode::NOT_FOUND);
    assert_eq!(status(&narrowed, "GET", "/liquer/web/").await, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn root_prefix_has_no_double_slash() {
    let envref = env_saying("Hello").await;
    let app = LiquersServerBuilder::<Env>::new("/").router().with_state(envref);
    assert_eq!(status(&app, "GET", "/web/").await, StatusCode::OK);
    assert_eq!(status(&app, "GET", "/q/hello").await, StatusCode::OK);
    assert_eq!(status(&app, "GET", "/api/store/data/data/x.txt").await, StatusCode::OK);
    let response = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers()["location"], "/web/");
}

#[tokio::test]
async fn without_api_and_web_root_in_code() {
    let envref = env_saying("Hello").await;
    let app = LiquersServerBuilder::<Env>::new("/api")
        .without_api(Api::Recipes)
        .read_only()
        .with_web(WebSetting::Root("data".into()))
        .with_query_timeout(Duration::from_secs(30))
        .router()
        .with_state(envref);
    assert_eq!(status(&app, "GET", "/api/api/recipes/listdir").await, StatusCode::NOT_FOUND);
    assert_eq!(status(&app, "GET", "/api/api/store/data/data/x.txt").await, StatusCode::NOT_FOUND);
    assert_eq!(status(&app, "GET", "/api/web/x.txt").await, StatusCode::OK);
}

#[tokio::test]
async fn bind_port_zero_reports_port() {
    let envref = env_saying("Hello").await;
    let server = LiquersServerBuilder::<Env>::from_config(local(ServerConfig::default()))
        .bind(envref)
        .await
        .unwrap();
    let addr = server.local_addr();
    assert_ne!(addr.port(), 0);
    assert!(server.is_running());
    let text = reqwest::get(format!("http://{addr}/liquer/q/hello")).await.unwrap().text().await.unwrap();
    assert_eq!(text, "Hello");
    let page = reqwest::get(format!("http://{addr}/")).await.unwrap();
    assert_eq!(page.status(), reqwest::StatusCode::OK, "reqwest follows the redirect to the app");
    assert_eq!(page.text().await.unwrap(), "<h1>app</h1>");
    server.shutdown(Duration::from_secs(1)).await;
    assert!(!server.is_running());
}

#[tokio::test]
async fn bind_port_in_use_is_an_error() {
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    let envref = env_saying("Hello").await;
    let error = LiquersServerBuilder::<Env>::from_config(ServerConfig { port, ..ServerConfig::default() })
        .bind(envref.clone())
        .await
        .expect_err("the port is taken");
    assert_eq!(error.error_type, ErrorType::General);
    assert!(error.to_string().contains(&format!("127.0.0.1:{port}")), "{error}");

    let bad_host = LiquersServerBuilder::<Env>::from_config(local(ServerConfig {
        host: "no such host.invalid".into(),
        ..ServerConfig::default()
    }))
    .bind(envref.clone())
    .await
    .expect_err("not a host");
    assert!(bad_host.to_string().contains("no such host.invalid"), "{bad_host}");

    let bad_prefix = LiquersServerBuilder::<Env>::new("liquer").bind(envref).await.expect_err("prefix");
    assert_eq!(bad_prefix.error_type, ErrorType::ConfigurationError);
}

/// A host's source that fails on demand, or switches to a new build.
struct Switchable {
    current: Mutex<EnvRef<Env>>,
    fail: AtomicBool,
    calls: AtomicUsize,
}

/// The server's handle on the shared source.
struct Handle(Arc<Switchable>);

#[async_trait]
impl EnvSource<Env> for Handle {
    async fn current(&self) -> Result<EnvRef<Env>, Error> {
        let this = &self.0;
        this.calls.fetch_add(1, Ordering::SeqCst);
        if this.fail.load(Ordering::SeqCst) {
            return Err(Error::general_error("the rebuild failed".to_string()));
        }
        Ok(this.current.lock().unwrap().clone())
    }
}

#[tokio::test]
async fn failing_source_is_503() {
    let source = Arc::new(Switchable {
        current: Mutex::new(env_saying("Hello").await),
        fail: AtomicBool::new(true),
        calls: AtomicUsize::new(0),
    });
    let server = LiquersServerBuilder::<Env>::from_config(local(ServerConfig::default()))
        .bind(Handle(source.clone()))
        .await
        .unwrap();
    let response = reqwest::get(format!("http://{}/liquer/q/hello", server.local_addr())).await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert!(response.text().await.unwrap().contains("the rebuild failed"));
    source.fail.store(false, Ordering::SeqCst);
    let response = reqwest::get(format!("http://{}/liquer/q/hello", server.local_addr())).await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    server.shutdown(Duration::from_secs(1)).await;
}

#[tokio::test]
async fn new_envref_is_used() {
    let source = Arc::new(Switchable {
        current: Mutex::new(env_saying("first").await),
        fail: AtomicBool::new(false),
        calls: AtomicUsize::new(0),
    });
    let server = LiquersServerBuilder::<Env>::from_config(local(ServerConfig::default()))
        .bind(Handle(source.clone()))
        .await
        .unwrap();
    let url = format!("http://{}/liquer/q/hello", server.local_addr());
    assert_eq!(reqwest::get(&url).await.unwrap().text().await.unwrap(), "first");
    *source.current.lock().unwrap() = env_saying("second").await;
    assert_eq!(reqwest::get(&url).await.unwrap().text().await.unwrap(), "second");
    assert_eq!(reqwest::get(&url).await.unwrap().text().await.unwrap(), "second");
    assert!(source.calls.load(Ordering::SeqCst) >= 3);
    server.shutdown(Duration::from_secs(1)).await;
}

#[tokio::test]
async fn stopped_server_releases_the_environment() {
    let envref = env_saying("Hello").await;
    let server = LiquersServerBuilder::<Env>::from_config(local(ServerConfig::default()))
        .bind(envref.clone())
        .await
        .unwrap();
    let body = reqwest::get(format!("http://{}/liquer/q/hello", server.local_addr())).await.unwrap();
    assert_eq!(body.text().await.unwrap(), "Hello");
    server.shutdown(Duration::from_secs(1)).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(Arc::strong_count(&envref.0), 1, "the server keeps no reference after it stopped");
}
