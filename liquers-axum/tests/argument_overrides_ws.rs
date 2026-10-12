//! WebSocket `subscribe` with argument overrides: unmanaged subscriptions, subscription ids,
//! `cancel`, `unsubscribe` by id, limits. Design: `specs/design/axum-query-named-arguments/`
//! (Phase 3, AOW01–AOW13).

mod common;

use std::time::Duration;

use axum::Router;
use common::{connect, env_with, eventually, has_type, start_server, status_of, Fixtures, Inbox};
use liquers_axum::assets::{AssetsApiBuilder, WebSocketLimits};
use liquers_core::{
    context::{Environment, GenericEnvironment},
    environment_builder::{AssetManagerKind, Inline, Queued},
    value::Value,
};
use serde_json::{json, Value as JsonValue};

const A: &str = "/api/assets";

async fn server<K: AssetManagerKind>(
    fixtures: &Fixtures,
    recipes: &[(&str, JsonValue)],
    limits: WebSocketLimits,
) -> String
where
    GenericEnvironment<Value, (), K>: Environment<Value = Value>,
{
    let envref = env_with::<K>(fixtures, recipes).await;
    let app: Router = AssetsApiBuilder::<GenericEnvironment<Value, (), K>>::new(A)
        .with_websocket_limits(limits)
        .build()
        .with_state(envref);
    start_server(app).await
}

async fn open_inbox<K: AssetManagerKind>(fixtures: &Fixtures, path: &str) -> Inbox
where
    GenericEnvironment<Value, (), K>: Environment<Value = Value>,
{
    let base = server::<K>(fixtures, &[], WebSocketLimits::default()).await;
    Inbox::new(connect(&base, &format!("{A}{path}")).await)
}

fn id_of(message: &JsonValue) -> Option<String> {
    message
        .get("subscription_id")
        .and_then(JsonValue::as_str)
        .map(str::to_string)
}

fn for_id<'a>(id: &'a str) -> impl Fn(&JsonValue) -> bool + 'a {
    move |m: &JsonValue| id_of(m).as_deref() == Some(id)
}

/// Subscribe with `arguments` and return the id from its `Initial` notification.
async fn subscribe_overridden(inbox: &mut Inbox, query: &str, arguments: JsonValue) -> String {
    inbox
        .send(json!({"action": "subscribe", "query": query, "arguments": arguments}))
        .await;
    let initial = inbox
        .recv_matching(|m| has_type(m, "Initial") && id_of(m).is_some())
        .await
        .expect("Initial of the override subscription");
    assert_eq!(initial["managed"], false);
    id_of(&initial).expect("id")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aow01_subscribe_with_arguments_follows_to_ready() {
    let fixtures = Fixtures::default();
    let mut inbox = open_inbox::<Queued>(&fixtures, "/ws/q").await;
    let id = subscribe_overridden(&mut inbox, "echo_args", json!({"query": "a", "limit": 3})).await;
    let ready = inbox
        .recv_matching(|m| for_id(&id)(m) && status_of(m) == Some("Ready"))
        .await
        .expect("the overridden evaluation reaches Ready");
    assert_eq!(ready["managed"], false);
    assert_eq!(ready["query"], "echo_args");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aow02_two_subscriptions_same_query_different_arguments() {
    let fixtures = Fixtures::default();
    let mut inbox = open_inbox::<Queued>(&fixtures, "/ws/q").await;
    let first = subscribe_overridden(&mut inbox, "gated", json!({"tag": "x"})).await;
    let second = subscribe_overridden(&mut inbox, "gated", json!({"tag": "y"})).await;
    assert_ne!(first, second);
    fixtures.open_gate();
    let a = inbox
        .recv_matching(|m| for_id(&first)(m) && status_of(m) == Some("Ready"))
        .await
        .expect("first Ready");
    let b = inbox
        .recv_matching(|m| for_id(&second)(m) && status_of(m) == Some("Ready"))
        .await
        .expect("second Ready");
    assert_ne!(a["asset_id"], b["asset_id"], "two independent evaluations");
    assert!(eventually(|| fixtures.count() == 2).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aow03_empty_arguments_is_managed_without_id() {
    let fixtures = Fixtures::default();
    let mut inbox = open_inbox::<Queued>(&fixtures, "/ws/q").await;
    for message in [
        json!({"action": "subscribe", "query": "echo_args", "arguments": {}}),
        json!({"action": "subscribe", "query": "make_text"}),
    ] {
        inbox.send(message).await;
        let initial = inbox.recv_matching(|m| has_type(m, "Initial")).await.expect("Initial");
        assert_eq!(initial["managed"], true, "{initial}");
        assert!(initial.get("subscription_id").is_none(), "{initial}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aow04_cancel_by_id() {
    let fixtures = Fixtures::default();
    let mut inbox = open_inbox::<Queued>(&fixtures, "/ws/q").await;
    let id = subscribe_overridden(&mut inbox, "gated", json!({"tag": "c"})).await;
    inbox.send(json!({"action": "cancel", "subscription_id": id})).await;
    let cancelled = inbox
        .recv_matching(|m| for_id(&id)(m) && status_of(m) == Some("Cancelled"))
        .await;
    assert!(cancelled.is_some(), "the cancelled evaluation reports Cancelled");
    fixtures.open_gate();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(fixtures.count(), 0, "the cancelled command did not complete");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aow05_unknown_or_managed_id_is_generic_error() {
    let fixtures = Fixtures::default();
    let mut inbox = open_inbox::<Queued>(&fixtures, "/ws/q").await;
    // A managed subscription exists, but its id is never shown, so nothing can name it.
    inbox.send(json!({"action": "subscribe", "query": "make_text"})).await;
    inbox.recv_matching(|m| has_type(m, "Initial")).await.expect("Initial");
    let mut texts = Vec::new();
    for message in [
        json!({"action": "cancel", "subscription_id": "made-up"}),
        json!({"action": "cancel", "subscription_id": uuid_like()}),
        json!({"action": "unsubscribe", "subscription_id": "made-up"}),
    ] {
        inbox.send(message).await;
        let error = inbox.recv_matching(|m| has_type(m, "Error")).await.expect("Error");
        texts.push(error["error"]["message"].as_str().unwrap_or("").to_string());
    }
    assert!(texts.iter().all(|t| t == "unknown subscription id"), "{texts:?}");
}

fn uuid_like() -> String {
    "00000000-0000-4000-8000-000000000000".to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aow06_ws_key_subscribe_recipe_managed() {
    let fixtures = Fixtures::default();
    let base = server::<Queued>(&fixtures, &[("echo_args/report.txt", json!({"limit": 7}))], WebSocketLimits::default()).await;
    let mut inbox = Inbox::new(connect(&base, &format!("{A}/ws/key")).await);
    inbox.send(json!({"action": "subscribe", "key": "report.txt"})).await;
    let initial = inbox.recv_matching(|m| has_type(m, "Initial")).await.expect("Initial");
    assert_eq!(initial["managed"], true, "a keyed recipe with arguments is managed: {initial}");
    assert!(initial.get("subscription_id").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aow07_cancel_before_asset_exists_is_not_lost() {
    // Queued: `subscribe` and `cancel` in one burst. Whichever branch runs — the asset already
    // set, or `cancel_requested` applied when the task gets it — the outcome is the same. Which
    // branch ran is not observable, so coverage of the `cancel_requested` branch is probabilistic.
    // The inline case is pending a design decision (an early `Subscribed` acknowledgement; see
    // the design's decision log).
    let fixtures = Fixtures::default();
    let mut inbox = open_inbox::<Queued>(&fixtures, "/ws/q").await;
    let id = subscribe_overridden(&mut inbox, "gated", json!({"tag": "q"})).await;
    inbox.send(json!({"action": "cancel", "subscription_id": id})).await;
    let cancelled = inbox
        .recv_matching(|m| for_id(&id)(m) && status_of(m) == Some("Cancelled"))
        .await;
    assert!(cancelled.is_some(), "queued: the cancel was not lost");
    fixtures.open_gate();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(fixtures.count(), 0, "queued: the cancelled command did not complete");

    // Inline: the evaluation runs inside the subscription task, so stopping the task stops it.
    // (Here through `unsubscribe_all`, which the client can always send; a cancel by id is not
    // reachable yet, because the id arrives only with `Initial`, after the evaluation.)
    let fixtures = Fixtures::default();
    let mut inbox = open_inbox::<Inline>(&fixtures, "/ws/q").await;
    inbox.send(json!({"action": "subscribe", "query": "gated", "arguments": {"tag": "i"}})).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    inbox.send(json!({"action": "unsubscribe_all"})).await;
    inbox.recv_matching(|m| has_type(m, "UnsubscribedAll")).await.expect("UnsubscribedAll");
    fixtures.open_gate();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(fixtures.count(), 0, "inline: stopping the task stopped the evaluation");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aow08_unsubscribe_by_id_stops_notifications() {
    let fixtures = Fixtures::default();
    let mut inbox = open_inbox::<Queued>(&fixtures, "/ws/q").await;
    let id = subscribe_overridden(&mut inbox, "gated", json!({"tag": "u"})).await;
    inbox.send(json!({"action": "unsubscribe", "subscription_id": id})).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    fixtures.open_gate();
    let later = inbox.drain(Duration::from_millis(300)).await;
    assert!(!later.iter().any(|m| for_id(&id)(m)), "no notification after unsubscribing: {later:?}");
    assert!(eventually(|| fixtures.count() == 1).await, "unsubscribing does not cancel");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aow09_disconnect_does_not_cancel() {
    // Queued manager: the evaluation runs on the job queue and outlives the socket. (On the
    // inline manager it runs inside the subscription task, which a disconnect stops — AOW07.)
    let fixtures = Fixtures::default();
    let mut inbox = open_inbox::<Queued>(&fixtures, "/ws/q").await;
    subscribe_overridden(&mut inbox, "gated", json!({"tag": "d"})).await;
    drop(inbox);
    tokio::time::sleep(Duration::from_millis(100)).await;
    fixtures.open_gate();
    assert!(eventually(|| fixtures.count() == 1).await, "the evaluation outlives the socket");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aow10_arguments_on_ws_key_rejected() {
    let fixtures = Fixtures::default();
    let mut inbox = open_inbox::<Queued>(&fixtures, "/ws/key").await;
    inbox
        .send(json!({"action": "subscribe", "key": "a.txt", "arguments": {"x": 1}}))
        .await;
    let error = inbox.recv_matching(|m| has_type(m, "Error")).await.expect("Error");
    assert_eq!(error["error"]["type"], "ParameterError");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aow11_volatile_subscribe_without_arguments_gets_id() {
    let fixtures = Fixtures::default();
    let mut inbox = open_inbox::<Queued>(&fixtures, "/ws/q").await;
    inbox.send(json!({"action": "subscribe", "query": "gated_volatile"})).await;
    let initial = inbox.recv_matching(|m| has_type(m, "Initial")).await.expect("Initial");
    assert_eq!(initial["managed"], false, "{initial}");
    let id = id_of(&initial).expect("an unmanaged subscription shows its id");
    inbox.send(json!({"action": "cancel", "subscription_id": id})).await;
    let cancelled = inbox
        .recv_matching(|m| for_id(&id)(m) && status_of(m) == Some("Cancelled"))
        .await;
    assert!(cancelled.is_some(), "cancellable by its id");
    fixtures.open_gate();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aow12_resubscribe_same_address_replaces() {
    let fixtures = Fixtures::default();
    let mut inbox = open_inbox::<Queued>(&fixtures, "/ws/q").await;
    for _ in 0..2 {
        inbox.send(json!({"action": "subscribe", "query": "gated"})).await;
        inbox.recv_matching(|m| has_type(m, "Initial")).await.expect("Initial");
    }
    fixtures.open_gate();
    let messages = inbox.drain(Duration::from_millis(500)).await;
    let finished = messages.iter().filter(|m| has_type(m, "JobFinished")).count();
    assert_eq!(finished, 1, "one stream of notifications remains: {messages:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aow13_override_subscriptions_count_toward_limit() {
    let fixtures = Fixtures::default();
    let limits = WebSocketLimits {
        max_message_size: 64 * 1024,
        max_subscriptions: 2,
    };
    let base = server::<Queued>(&fixtures, &[], limits).await;
    let mut inbox = Inbox::new(connect(&base, &format!("{A}/ws/q")).await);
    subscribe_overridden(&mut inbox, "gated", json!({"tag": "1"})).await;
    subscribe_overridden(&mut inbox, "gated", json!({"tag": "2"})).await;
    inbox
        .send(json!({"action": "subscribe", "query": "gated", "arguments": {"tag": "3"}}))
        .await;
    let error = inbox.recv_matching(|m| has_type(m, "Error")).await.expect("Error");
    assert!(error["error"]["message"].as_str().unwrap_or("").contains("limit"), "{error}");
    fixtures.open_gate();
}
