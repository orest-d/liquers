//! Shared helpers of the argument-override suites (`specs/design/axum-query-named-arguments/`,
//! Phase 3): fixture commands, environments on either asset manager, HTTP and WebSocket helpers.
//! Each test binary uses a different part, hence `dead_code`.
#![allow(dead_code)]

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use axum::{
    body::{to_bytes, Body},
    http::{HeaderMap, Request, StatusCode},
    Router,
};
use futures::{SinkExt, StreamExt};
use liquers_core::{
    command_metadata::{ArgumentInfo, CommandKey, CommandParameterValue},
    commands::CommandRegistry,
    context::{EnvRef, Environment, GenericEnvironment},
    environment_builder::AssetManagerKind,
    error::Error,
    metadata::Metadata,
    parse::parse_key,
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};
use serde_json::Value as JsonValue;
use tokio::sync::Semaphore;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tower::ServiceExt;

pub type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// The shared state of the fixture commands: the gate `gated` waits for (closed until
/// [`Fixtures::open_gate`]), and the counter it increments after passing it.
#[derive(Clone)]
pub struct Fixtures {
    pub gate: Arc<Semaphore>,
    pub counter: Arc<AtomicUsize>,
}

impl Default for Fixtures {
    fn default() -> Self {
        Fixtures {
            gate: Arc::new(Semaphore::new(0)),
            counter: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl Fixtures {
    pub fn count(&self) -> usize {
        self.counter.load(Ordering::SeqCst)
    }
    /// Let every gated evaluation through, now and later.
    pub fn open_gate(&self) {
        self.gate.add_permits(Semaphore::MAX_PERMITS / 2);
    }
}

/// Register the Phase 3 fixtures:
/// - `echo_args(query: string = "", rank: bool = true, case_sensitive: bool = false,
///   limit: int = 50, fields: multiple string, ctx: injected)` — the shape of `ns-search/search`;
/// - `make_text`; `fail_with(reason: string = "")`, which always fails;
/// - `gated(tag: string = "")` and the volatile `gated_volatile` — wait for the gate, increment
///   the counter, return `"<tag>:<count>"`;
/// - `echo_alias` — an alias of `echo_args` with the head `query = "fixed"`; its own arguments
///   rename `rank` to `flag` and keep the rest.
pub fn register_fixtures<E: Environment<Value = Value>>(
    cr: &mut CommandRegistry<E>,
    fixtures: &Fixtures,
) -> Result<(), Error> {
    cr.register_command(CommandKey::new_name("echo_args"), |_, args, _| {
        let query: String = args.get(0, "query")?;
        let rank: bool = args.get(1, "rank")?;
        let case_sensitive: bool = args.get(2, "case_sensitive")?;
        let limit: i64 = args.get(3, "limit")?;
        let fields: Vec<String> = args.get_multiple(4, "fields")?;
        Ok(Value::from(format!(
            "query={query};rank={rank};case_sensitive={case_sensitive};limit={limit};fields={}",
            fields.join(",")
        )))
    })?
    .with_argument(ArgumentInfo::string_argument("query").with_default(""))
    .with_argument(ArgumentInfo::boolean_argument("rank").with_default(true))
    .with_argument(ArgumentInfo::boolean_argument("case_sensitive").with_default(false))
    .with_argument(ArgumentInfo::integer_argument("limit", false).with_default(50))
    .with_argument(ArgumentInfo::string_argument("fields").set_multiple())
    .with_argument(ArgumentInfo::argument("ctx").set_injected());
    cr.register_command(CommandKey::new_name("make_text"), |_, _, _| {
        Ok(Value::from("generated"))
    })?;
    cr.register_command(CommandKey::new_name("fail_with"), |_, args, _| {
        let reason: String = args.get(0, "reason")?;
        Err(Error::general_error(format!("failed on purpose: {reason}")))
    })?
    .with_argument(ArgumentInfo::string_argument("reason").with_default(""));
    for (name, volatile) in [("gated", false), ("gated_volatile", true)] {
        let gate = fixtures.gate.clone();
        let counter = fixtures.counter.clone();
        let metadata = cr
            .register_async_command(CommandKey::new_name(name), move |_, args, _| {
                let gate = gate.clone();
                let counter = counter.clone();
                Box::pin(async move {
                    let tag: String = args.get(0, "tag")?;
                    let _passed = gate
                        .acquire()
                        .await
                        .map_err(|e| Error::general_error(format!("gate closed: {e}")))?;
                    let count = counter.fetch_add(1, Ordering::SeqCst) + 1;
                    Ok(Value::from(format!("{tag}:{count}")))
                })
            })?
            .with_argument(ArgumentInfo::string_argument("tag").with_default(""));
        metadata.is_async = true;
        metadata.volatile = volatile;
    }
    cr.register_alias(
        CommandKey::new_name("echo_alias"),
        CommandKey::new_name("echo_args"),
        vec![CommandParameterValue::Value(JsonValue::from("fixed"))],
        vec![
            ArgumentInfo::boolean_argument("flag").with_default(true),
            ArgumentInfo::boolean_argument("case_sensitive").with_default(false),
            ArgumentInfo::integer_argument("limit", false).with_default(50),
            ArgumentInfo::string_argument("fields").set_multiple(),
            ArgumentInfo::argument("ctx").set_injected(),
        ],
    )?;
    Ok(())
}

/// An environment on the manager `K` (queued `SimpleEnvironment` or inline
/// `ImmediateEnvironment`) with the fixtures, a memory store, and `recipes` — `(query ending in
/// the recipe's filename, arguments as a JSON object)` — in the root `recipes.yaml`.
pub async fn env_with<K: AssetManagerKind>(
    fixtures: &Fixtures,
    recipes: &[(&str, JsonValue)],
) -> EnvRef<GenericEnvironment<Value, (), K>>
where
    GenericEnvironment<Value, (), K>: Environment<Value = Value>,
{
    let mut env: GenericEnvironment<Value, (), K> = GenericEnvironment::new();
    register_fixtures(&mut env.command_registry, fixtures).expect("fixtures");
    let mut list = RecipeList::new();
    for (query, arguments) in recipes {
        let mut recipe =
            Recipe::new(query.to_string(), String::new(), String::new()).expect("recipe");
        if let JsonValue::Object(members) = arguments {
            for (name, value) in members {
                recipe.arguments.insert(name.clone(), value.clone());
            }
        }
        list.add_recipe(recipe);
    }
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("recipes.yaml").expect("key"),
            serde_yaml::to_string(&list).expect("yaml").as_bytes(),
            &Metadata::new(),
        )
        .await
        .expect("store recipes");
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    env.to_ref()
}

/// A response: status, headers, body bytes.
pub struct Answer {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Answer {
    pub fn json(&self) -> JsonValue {
        serde_json::from_slice(&self.body).unwrap_or(JsonValue::Null)
    }
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
    pub fn managed(&self) -> Option<String> {
        self.headers
            .get("x-liquers-managed")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    }
}

/// Send one request; `body` with a JSON content type when given.
pub async fn request(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<&str>,
    headers: &[(&str, &str)],
) -> Answer {
    let mut builder = Request::builder().method(method).uri(uri);
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let request = builder
        .body(body.map(|b| Body::from(b.to_string())).unwrap_or_else(Body::empty))
        .expect("request");
    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body")
        .to_vec();
    Answer { status, headers, body }
}

pub async fn get(app: &Router, uri: &str) -> Answer {
    request(app, "GET", uri, None, &[]).await
}

pub async fn post(app: &Router, uri: &str, body: &str) -> Answer {
    request(app, "POST", uri, Some(body), &[]).await
}

/// Poll `condition` every 20 ms for up to 5 s.
pub async fn eventually(mut condition: impl FnMut() -> bool) -> bool {
    for _ in 0..250 {
        if condition() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    condition()
}

/// Start an in-process server on an OS-assigned port and return its `ws://` base URL.
pub async fn start_server(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("ws://{addr}")
}

pub async fn connect(base_url: &str, path: &str) -> Ws {
    let (ws, _) = connect_async(&format!("{base_url}{path}"))
        .await
        .expect("connect");
    ws
}

pub async fn send_json(ws: &mut Ws, message: JsonValue) {
    ws.send(Message::Text(message.to_string().into()))
        .await
        .expect("send");
}

/// The next JSON text message within `timeout`, skipping control frames.
pub async fn recv_json(ws: &mut Ws, timeout: Duration) -> Option<JsonValue> {
    loop {
        match tokio::time::timeout(timeout, ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => return serde_json::from_str(&text).ok(),
            Ok(Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Binary(_) | Message::Frame(_)))) => {
                continue
            }
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Ok(Some(Err(_))) | Err(_) => return None,
        }
    }
}

/// A receiver that keeps the messages a matcher skipped, so waiting for one subscription's
/// message does not drop another's.
pub struct Inbox {
    pub ws: Ws,
    pending: Vec<JsonValue>,
}

impl Inbox {
    pub fn new(ws: Ws) -> Self {
        Inbox { ws, pending: Vec::new() }
    }

    pub async fn send(&mut self, message: JsonValue) {
        send_json(&mut self.ws, message).await;
    }

    /// The first message (kept or new) for which `predicate` holds, within 5 s.
    pub async fn recv_matching(
        &mut self,
        predicate: impl Fn(&JsonValue) -> bool,
    ) -> Option<JsonValue> {
        if let Some(i) = self.pending.iter().position(|m| predicate(m)) {
            return Some(self.pending.remove(i));
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline {
            match recv_json(&mut self.ws, Duration::from_millis(100)).await {
                Some(message) if predicate(&message) => return Some(message),
                Some(message) => self.pending.push(message),
                None => {}
            }
        }
        None
    }

    /// Everything received within `wait`, kept messages included.
    pub async fn drain(&mut self, wait: Duration) -> Vec<JsonValue> {
        let mut all = std::mem::take(&mut self.pending);
        while let Some(message) = recv_json(&mut self.ws, wait).await {
            all.push(message);
        }
        all
    }
}

pub fn has_type(message: &JsonValue, kind: &str) -> bool {
    message.get("type").and_then(JsonValue::as_str) == Some(kind)
}

pub fn status_of(message: &JsonValue) -> Option<&str> {
    message
        .get("info")
        .and_then(|i| i.get("status"))
        .and_then(JsonValue::as_str)
}
