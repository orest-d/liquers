//! The accessor API, `{prefix}/api/acc` (`specs/design/settable-value-accessor/`, Phase 3 test 6):
//! reads with an `ETag`, conditional writes, the opt-in server option, the info, log and describe
//! routes, and the event socket.

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::StreamExt;
use liquers_axum::{Api, LiquersServerBuilder, ServerConfig};
use liquers_core::assets::AssetManager;
use liquers_core::context::{EnvRef, Environment, SimpleEnvironment};
use liquers_core::error::{Error, ErrorType};
use liquers_core::metadata::MetadataRecord;
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::AsyncMemoryStore;
use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::value::Value;
use tower::ServiceExt;

// The `register_*_commands!` macros expand to code naming these unqualified.
#[allow(unused_imports)]
use liquers_core::{
    command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue},
    context::Context,
};

type CommandEnvironment = DefaultEnvironment<Value>;
type Env = CommandEnvironment;

const TITLE: &str = "/liquer/api/acc/q/-R-key/config.yaml/-/ns-acc/accessor/field-title";

fn text_record() -> MetadataRecord {
    let mut record = MetadataRecord::new();
    record.type_identifier = "Text".to_string();
    record.type_name = "text".to_string();
    record.data_format = Some("txt".to_string());
    record
}

fn register(env: &mut Env) -> Result<(), Error> {
    let cr = env.get_mut_command_registry();
    liquers_lib::register_core_commands!(cr)?;
    liquers_lib::register_accessor_commands!(cr)?;
    Ok(())
}

/// An environment whose store holds `config.yaml`.
async fn fixture() -> EnvRef<Env> {
    let mut env = Env::new();
    register(&mut env).unwrap();
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    let envref = env.to_ref();
    envref
        .get_asset_manager()
        .set_binary(
            &parse_key("config.yaml").unwrap(),
            b"title: Dashboard\nrows: 3\n",
            text_record(),
        )
        .await
        .unwrap();
    envref
}

fn app(envref: &EnvRef<Env>) -> axum::Router {
    LiquersServerBuilder::<Env>::new("/liquer")
        .with_api(Api::Accessors)
        .router()
        .with_state(envref.clone())
}

struct Answer {
    status: StatusCode,
    etag: Option<String>,
    body: Vec<u8>,
}

impl Answer {
    fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap()
    }
    fn text(&self) -> String {
        String::from_utf8(self.body.clone()).unwrap()
    }
}

async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Answer {
    let mut request = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let request = request.body(Body::from(body.to_string())).unwrap();
    let response = tokio::time::timeout(Duration::from_secs(10), app.clone().oneshot(request))
        .await
        .expect("answered within 10 s")
        .unwrap();
    let status = response.status();
    let etag = response
        .headers()
        .get("etag")
        .map(|v| v.to_str().unwrap().to_string());
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    Answer { status, etag, body }
}

async fn stored(envref: &EnvRef<Env>, key: &str) -> String {
    let bytes = envref
        .get_async_store()
        .get_bytes(&parse_key(key).unwrap())
        .await
        .unwrap();
    String::from_utf8(bytes).unwrap()
}

#[tokio::test]
async fn get_returns_value_and_etag() {
    let envref = fixture().await;
    let app = app(&envref);
    let text = call(&app, "GET", TITLE, &[], "").await;
    assert_eq!(text.status, StatusCode::OK, "{}", text.text());
    assert_eq!(text.text(), "Dashboard");
    let etag = text.etag.clone().expect("an ETag");
    assert!(etag.starts_with('"') && etag.ends_with('"'), "{etag}");
    let json = call(&app, "GET", &format!("{TITLE}?format=json"), &[], "").await;
    assert_eq!(json.json(), serde_json::json!("Dashboard"));
    assert_eq!(json.etag, Some(etag));
    // A value that does not exist: 404, with the accessor's info in the error.
    let missing = call(&app, "GET", "/liquer/api/acc/q/-R-key/none.txt/-/ns-acc/store", &[], "").await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert_eq!(missing.json()["error"]["metadata"]["status"], "None");
}

#[tokio::test]
async fn put_preconditions_and_conflicts() {
    let envref = fixture().await;
    let app = app(&envref);
    let json = [("content-type", "application/json")];
    // No precondition: 428.
    let unconditional = call(&app, "PUT", TITLE, &json, "\"Sales\"").await;
    assert_eq!(unconditional.status, StatusCode::PRECONDITION_REQUIRED);
    // If-Match with the current token: 200 and a new token.
    let old = call(&app, "GET", TITLE, &[], "").await.etag.unwrap();
    let set = call(
        &app,
        "PUT",
        TITLE,
        &[("content-type", "application/json"), ("if-match", &old)],
        "\"Sales\"",
    )
    .await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    let new = set.etag.clone().expect("the new ETag");
    assert_ne!(new, old);
    assert_eq!(set.json()["result"]["etag"], new.trim_matches('"'));
    assert_eq!(stored(&envref, "config.yaml").await, "title: Sales\nrows: 3\n");
    // The old token again: 412 with the current one, and nothing written.
    let stale = call(
        &app,
        "PUT",
        TITLE,
        &[("content-type", "application/json"), ("if-match", &old)],
        "\"Stale\"",
    )
    .await;
    assert_eq!(stale.status, StatusCode::PRECONDITION_FAILED);
    assert_eq!(stale.etag, Some(new.clone()));
    assert_eq!(stored(&envref, "config.yaml").await, "title: Sales\nrows: 3\n");
    // If-None-Match: * on a value that exists conflicts; on a new key it creates.
    let exists = call(&app, "PUT", TITLE, &[("if-none-match", "*")], "\"X\"").await;
    assert_eq!(exists.status, StatusCode::PRECONDITION_FAILED);
    let notes = "/liquer/api/acc/q/-R-key/notes.txt/-/ns-acc/store";
    let created = call(
        &app,
        "PUT",
        notes,
        &[("content-type", "text/plain"), ("if-none-match", "*")],
        "hello",
    )
    .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.text());
    assert_eq!(stored(&envref, "notes.txt").await, "hello");
    // If-Match: * sets whatever value exists.
    let any = call(
        &app,
        "PUT",
        notes,
        &[("content-type", "text/plain"), ("if-match", "*")],
        "again",
    )
    .await;
    assert_eq!(any.status, StatusCode::OK, "{}", any.text());
    assert_eq!(stored(&envref, "notes.txt").await, "again");
    // A query that is not an accessor is read-only: 405.
    let plain = call(&app, "PUT", "/liquer/api/acc/q/-R/config.yaml", &[("if-match", "*")], "1").await;
    assert_eq!(plain.status, StatusCode::METHOD_NOT_ALLOWED);
    let read = call(&app, "GET", "/liquer/api/acc/q/-R/config.yaml", &[], "").await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.text(), "title: Sales\nrows: 3\n");
}

#[tokio::test]
async fn accessors_api_is_opt_in_and_configurable() {
    let envref = fixture().await;
    let info = "api/acc/info/-R-key/config.yaml/-/ns-acc/accessor";
    // Not mounted by default.
    assert_eq!(ServerConfig::default().apis, Api::defaults());
    let default = LiquersServerBuilder::<Env>::new("/liquer").router().with_state(envref.clone());
    assert_eq!(call(&default, "GET", &format!("/liquer/{info}"), &[], "").await.status, StatusCode::NOT_FOUND);
    // `apis: [..., accessors]` mounts it under the configured prefix.
    let config: ServerConfig = serde_yaml::from_str("{prefix: /svc, apis: [query, accessors]}").unwrap();
    let configured = LiquersServerBuilder::<Env>::from_config(config).router().with_state(envref.clone());
    assert_eq!(call(&configured, "GET", &format!("/svc/{info}"), &[], "").await.status, StatusCode::OK);
    // `read_only` leaves out the PUT.
    let config: ServerConfig = serde_yaml::from_str("{apis: [accessors], read_only: true}").unwrap();
    let read_only = LiquersServerBuilder::<Env>::from_config(config).router().with_state(envref.clone());
    let put = call(&read_only, "PUT", TITLE, &[("if-match", "*")], "\"X\"").await;
    assert_eq!(put.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(call(&read_only, "GET", TITLE, &[], "").await.status, StatusCode::OK);
    // A core-`Value` environment has no accessors: `bind` refuses the API.
    let core = SimpleEnvironment::<liquers_core::value::Value>::new().to_ref();
    let local = ServerConfig { port: 0, apis: vec![Api::Query, Api::Accessors], ..ServerConfig::default() };
    let refused = LiquersServerBuilder::<SimpleEnvironment<liquers_core::value::Value>>::from_config(local.clone())
        .bind(core)
        .await
        .expect_err("no accessors in core Value");
    assert_eq!(refused.error_type, ErrorType::ConfigurationError);
    assert!(refused.to_string().contains("accessors"), "{refused}");
    // The library's value type has them.
    let server = LiquersServerBuilder::<Env>::from_config(local).bind(envref).await.unwrap();
    server.shutdown(Duration::from_secs(1)).await;
}

#[tokio::test]
async fn info_log_and_describe_routes() {
    let envref = fixture().await;
    let app = app(&envref);
    let etag = call(&app, "GET", TITLE, &[], "").await.etag.unwrap();
    let info = call(&app, "GET", &TITLE.replace("/acc/q/", "/acc/info/"), &[], "").await;
    assert_eq!(info.status, StatusCode::OK, "{}", info.text());
    assert_eq!(info.json()["result"]["etag"], etag.trim_matches('"'));
    assert_eq!(info.etag, Some(etag));
    assert!(info.json()["result"]["status"].is_string());
    let describe = call(&app, "GET", &TITLE.replace("/acc/q/", "/acc/describe/"), &[], "").await;
    assert_eq!(describe.status, StatusCode::OK, "{}", describe.text());
    assert_eq!(describe.json()["result"]["type_identifier"], "Text");
    assert_eq!(describe.json()["result"]["read_only"], false);
    let log = call(&app, "GET", &format!("{}?since=0", TITLE.replace("/acc/q/", "/acc/log/")), &[], "").await;
    assert_eq!(log.status, StatusCode::OK, "{}", log.text());
    assert!(log.json()["result"].is_array());
    let bad = call(&app, "GET", &format!("{}?since=x", TITLE.replace("/acc/q/", "/acc/log/")), &[], "").await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
}

/// The next JSON text frame, within 10 s.
async fn next_event<S>(socket: &mut S) -> Option<serde_json::Value>
where
    S: futures::Stream<Item = Result<tokio_tungstenite::tungstenite::Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    loop {
        let message = tokio::time::timeout(Duration::from_secs(10), socket.next())
            .await
            .expect("a frame within 10 s")?
            .ok()?;
        match message {
            tokio_tungstenite::tungstenite::Message::Text(text) => {
                return Some(serde_json::from_str(&text).unwrap())
            }
            tokio_tungstenite::tungstenite::Message::Close(_) => return None,
            _ => {}
        }
    }
}

#[tokio::test]
async fn websocket_relays_accessor_events() {
    let envref = fixture().await;
    let config = ServerConfig { port: 0, apis: vec![Api::Accessors], ..ServerConfig::default() };
    let server = LiquersServerBuilder::<Env>::from_config(config).bind(envref.clone()).await.unwrap();
    let base = format!("{}/liquer/api/acc", server.local_addr());
    let field = "-R-key/config.yaml/-/ns-acc/accessor/field-title";
    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{base}/ws/{field}"))
        .await
        .unwrap();
    let first = next_event(&mut socket).await.expect("the current event");
    assert!(first["event"].is_string(), "{first}");

    // A set through HTTP is relayed as `changed`, with the new token.
    let client = reqwest::Client::new();
    let put = client
        .put(format!("http://{base}/q/{field}"))
        .header("content-type", "application/json")
        .header("if-match", "*")
        .body("\"Sales\"")
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), reqwest::StatusCode::OK);
    let etag = put.headers()["etag"].to_str().unwrap().trim_matches('"').to_string();
    loop {
        let event = next_event(&mut socket).await.expect("a changed event");
        if event["event"] == "changed" {
            assert_eq!(event["etag"], etag);
            break;
        }
    }

    // The document loses its title: the field is invalidated and the socket closes.
    let put = client
        .put(format!("http://{base}/q/-R-key/config.yaml/-/ns-acc/accessor"))
        .header("content-type", "text/plain")
        .header("if-match", "*")
        .body("rows: 3\n")
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), reqwest::StatusCode::OK);
    let mut invalidated = false;
    while let Some(event) = next_event(&mut socket).await {
        if event["event"] == "invalidated" {
            invalidated = true;
        }
    }
    assert!(invalidated, "the socket closed without an invalidated event");
    server.shutdown(Duration::from_secs(1)).await;
}
