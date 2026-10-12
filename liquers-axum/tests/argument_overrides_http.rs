//! Argument overrides over HTTP: the Query API and the Assets API's `q/data`, `q/entry` and
//! `q/submit`, with managed reporting. Design: `specs/design/axum-query-named-arguments/`
//! (Phase 3, AOH01–AOH21).

mod common;

use axum::{http::StatusCode, Router};
use base64::prelude::*;
use common::{env_with, eventually, get, post, request, Answer, Fixtures};
use liquers_axum::{AssetsApiBuilder, QueryApiBuilder};
use liquers_core::{
    context::{EnvRef, Environment, GenericEnvironment},
    environment_builder::{AssetManagerKind, Inline, Queued},
    value::Value,
};
use serde_json::{json, Value as JsonValue};

const A: &str = "/api/assets";
const B: &str = "/q";

fn build_app<K: AssetManagerKind>(envref: EnvRef<GenericEnvironment<Value, (), K>>) -> Router
where
    GenericEnvironment<Value, (), K>: Environment<Value = Value>,
{
    Router::new()
        .merge(QueryApiBuilder::<GenericEnvironment<Value, (), K>>::new("/q/").build())
        .merge(AssetsApiBuilder::<GenericEnvironment<Value, (), K>>::new(A).build())
        .with_state(envref)
}

async fn app_with(fixtures: &Fixtures, recipes: &[(&str, JsonValue)]) -> Router {
    build_app::<Queued>(env_with::<Queued>(fixtures, recipes).await)
}

async fn app() -> Router {
    app_with(&Fixtures::default(), &[]).await
}

/// The `data` of a JSON `DataEntry` answer, as text.
fn entry_data(answer: &Answer) -> String {
    let encoded = answer.json()["data"].as_str().unwrap_or("").to_string();
    String::from_utf8(BASE64_STANDARD.decode(encoded).unwrap_or_default()).unwrap_or_default()
}

fn error_type(answer: &Answer) -> String {
    answer.json()["error"]["type"].as_str().unwrap_or("").to_string()
}

/// The log messages of a metadata JSON object.
fn log_messages(metadata: &JsonValue) -> Vec<String> {
    metadata["log"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|e| e["message"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn aoh01_query_api_get_query_string() {
    let app = app().await;
    let answer = get(&app, &format!("{B}/echo_args?query=a%20b&limit=20&fields=x&fields=y")).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(answer.text(), "query=a b;rank=true;case_sensitive=false;limit=20;fields=x,y");
    assert_eq!(answer.managed().as_deref(), Some("false"));
}

#[tokio::test]
async fn aoh03_query_api_post_json_body() {
    let app = app().await;
    let answer = post(&app, &format!("{B}/echo_args"), r#"{"query": "expiration safety", "limit": 20}"#).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(answer.text(), "query=expiration safety;rank=true;case_sensitive=false;limit=20;fields=");
    assert_eq!(answer.managed().as_deref(), Some("false"));
}

#[tokio::test]
async fn aoh07_override_replaces_path_value() {
    let app = app().await;
    // Empty path slots leave `query` as "" and `rank`, `case_sensitive` at their defaults; the
    // path gives `limit`; the override gives `query`.
    let answer = get(&app, &format!("{B}/echo_args----20?query=a")).await;
    assert_eq!(answer.text(), "query=a;rank=true;case_sensitive=false;limit=20;fields=");
    // An override also replaces a value the path did give.
    let answer = get(&app, &format!("{B}/echo_args-hello-f-f-3?query=a&limit=4")).await;
    assert_eq!(answer.text(), "query=a;rank=false;case_sensitive=false;limit=4;fields=");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aoh10_different_overrides_not_shared() {
    let fixtures = Fixtures::default();
    let app = app_with(&fixtures, &[]).await;
    let (uri_a, uri_b) = (format!("{B}/echo_args?query=a"), format!("{B}/echo_args?query=b"));
    let (a, b) = tokio::join!(get(&app, &uri_a), get(&app, &uri_b));
    assert!(a.text().starts_with("query=a;"), "{}", a.text());
    assert!(b.text().starts_with("query=b;"), "{}", b.text());
    // The same overrides twice are evaluated twice: an unmanaged asset is never reused.
    fixtures.open_gate();
    let first = get(&app, &format!("{B}/gated?tag=t")).await;
    let second = get(&app, &format!("{B}/gated?tag=t")).await;
    assert_eq!((first.text(), second.text()), ("t:1".to_string(), "t:2".to_string()));
}

#[tokio::test]
async fn aoh02_q_data_and_q_entry_get_query_string() {
    let app = app().await;
    let answer = get(&app, &format!("{A}/q/data/echo_args?query=a%20b&limit=20&fields=x&fields=y")).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(answer.text(), "query=a b;rank=true;case_sensitive=false;limit=20;fields=x,y");
    assert_eq!(answer.managed().as_deref(), Some("false"));

    let answer = get(&app, &format!("{A}/q/entry/echo_args?query=e&_entry_format=json")).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert!(entry_data(&answer).starts_with("query=e;"));
    assert_eq!(answer.managed().as_deref(), Some("false"));
}

#[tokio::test]
async fn aoh04_q_entry_post_json_body() {
    let app = app().await;
    let answer = post(&app, &format!("{A}/q/entry/echo_args?_entry_format=json"), r#"{"query":"a","limit":3}"#).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(entry_data(&answer), "query=a;rank=true;case_sensitive=false;limit=3;fields=");
    assert_eq!(answer.managed().as_deref(), Some("false"));
}

#[tokio::test]
async fn aoh05_variadic_repeated_key_and_json_array() {
    let app = app().await;
    let answer = get(&app, &format!("{A}/q/data/echo_args?fields=a&fields=b")).await;
    assert!(answer.text().ends_with("fields=a,b"), "{}", answer.text());
    let answer = post(&app, &format!("{A}/q/data/echo_args"), r#"{"fields":["a","b"]}"#).await;
    assert!(answer.text().ends_with("fields=a,b"), "{}", answer.text());
}

#[tokio::test]
async fn aoh06_unknown_injected_or_actionless_name_400() {
    let app = app().await;
    for uri in [
        format!("{A}/q/data/echo_args?nope=1"),
        format!("{A}/q/data/echo_args?ctx=1"),
        format!("{A}/q/data/-R/notes/todo.txt?x=1"),
    ] {
        let answer = get(&app, &uri).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{uri}: {}", answer.text());
        assert_eq!(error_type(&answer), "ParameterError", "{uri}");
    }
}

#[tokio::test]
async fn aoh08_unconvertible_value_422() {
    let app = app().await;
    let answer = get(&app, &format!("{A}/q/data/echo_args?limit=abc")).await;
    assert_eq!(answer.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", answer.text());
    assert_eq!(error_type(&answer), "ConversionError");
    for body in [r#"{"limit":"abc"}"#, r#"{"limit":null}"#] {
        let answer = post(&app, &format!("{A}/q/data/echo_args"), body).await;
        assert_eq!(answer.status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {}", answer.text());
        assert_eq!(error_type(&answer), "ConversionError", "{body}");
    }
}

#[tokio::test]
async fn aoh09_no_override_forms_stay_managed() {
    let app = app().await;
    let forms = [
        request(&app, "GET", &format!("{A}/q/data/echo_args"), None, &[]).await,
        request(&app, "GET", &format!("{A}/q/data/echo_args?"), None, &[]).await,
        request(&app, "POST", &format!("{A}/q/data/echo_args"), Some(""), &[]).await,
        request(&app, "POST", &format!("{A}/q/data/echo_args"), Some("{}"), &[]).await,
        request(&app, "GET", &format!("{A}/q/entry/echo_args?_entry_format=json"), None, &[]).await,
        request(&app, "GET", &format!("{B}/echo_args"), None, &[]).await,
        request(&app, "GET", &format!("{B}/echo_args?"), None, &[]).await,
        request(&app, "POST", &format!("{B}/echo_args"), Some(""), &[]).await,
        request(&app, "POST", &format!("{B}/echo_args"), Some("{}"), &[]).await,
    ];
    for (i, answer) in forms.iter().enumerate() {
        assert_eq!(answer.status, StatusCode::OK, "form {i}: {}", answer.text());
        assert_eq!(answer.managed().as_deref(), Some("true"), "form {i}");
    }
    let info = get(&app, &format!("{A}/q/info/echo_args")).await;
    assert_eq!(info.status, StatusCode::OK, "the managed asset is found: {}", info.text());

    let submitted = post(&app, &format!("{A}/q/submit/make_text"), "").await;
    assert_eq!(submitted.json()["result"]["managed"], true);
    assert!(submitted.json()["result"].get("note").is_none(), "no note for a managed asset");
    assert_eq!(submitted.managed().as_deref(), Some("true"));
}

#[tokio::test]
async fn aoh11_malformed_body_or_duplicate_name_400() {
    let app = app().await;
    for (uri, body) in [
        (format!("{A}/q/data/echo_args"), "[1]"),
        (format!("{A}/q/data/echo_args"), "not json"),
        (format!("{A}/q/data/echo_args?query=a"), r#"{"query":"b"}"#),
    ] {
        let answer = post(&app, &uri, body).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{uri} {body}");
        assert_eq!(error_type(&answer), "ParameterError", "{uri} {body}");
    }
}

#[tokio::test]
async fn aoh12_reserved_name_400() {
    let app = app().await;
    for uri in [
        format!("{A}/q/data/echo_args?_foo=1"),
        // `_entry_format` is an option of the entry route only.
        format!("{A}/q/data/echo_args?_entry_format=json"),
        format!("{A}/q/submit/echo_args?_entry_format=json"),
        format!("{B}/echo_args?_foo=1"),
        format!("{B}/echo_args?_entry_format=json"),
    ] {
        let answer = get(&app, &uri).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(error_type(&answer), "ParameterError", "{uri}");
    }
}

#[tokio::test]
async fn aoh13_link_override() {
    let app = app().await;
    let answer = get(&app, &format!("{A}/q/data/echo_args?_link_query=make_text")).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert!(answer.text().starts_with("query=generated;"), "{}", answer.text());

    let answer = get(&app, &format!("{A}/q/data/echo_args?_link_query=-R/")).await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{}", answer.text());
    assert_eq!(error_type(&answer), "ParseError");

    let answer = get(&app, &format!("{A}/q/data/echo_args?query=a&_link_query=make_text")).await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST);
    assert_eq!(error_type(&answer), "ParameterError");
}

#[tokio::test]
async fn aoh14_body_type_mismatch_logged_url_text_not() {
    let app = app().await;
    let answer = post(&app, &format!("{A}/q/entry/echo_args?_entry_format=json"), r#"{"limit":"20"}"#).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert!(entry_data(&answer).contains("limit=20"));
    let messages = log_messages(&answer.json()["metadata"]);
    assert!(messages.iter().any(|m| m.contains("'limit'")), "a warning names limit: {messages:?}");

    let answer = get(&app, &format!("{A}/q/entry/echo_args?limit=21&_entry_format=json")).await;
    assert!(entry_data(&answer).contains("limit=21"));
    let messages = log_messages(&answer.json()["metadata"]);
    assert!(!messages.iter().any(|m| m.contains("'limit'")), "URL text is not warned about: {messages:?}");
}

#[tokio::test]
async fn aoh15_q_entry_answers_data_and_metadata() {
    let app = app().await;
    let answer = get(&app, &format!("{A}/q/entry/echo_args?query=z&_entry_format=json")).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert!(entry_data(&answer).starts_with("query=z;"));
    let metadata = &answer.json()["metadata"];
    assert!(metadata.is_object());
    assert!(metadata.get("log").is_some(), "the log travels with the value: {metadata}");
}

async fn failure_carries_metadata<K: AssetManagerKind>()
where
    GenericEnvironment<Value, (), K>: Environment<Value = Value>,
{
    let app = build_app::<K>(env_with::<K>(&Fixtures::default(), &[]).await);
    for uri in [
        format!("{A}/q/entry/fail_with?reason=x&_entry_format=json"),
        format!("{A}/q/entry/fail_with?_entry_format=json"),
    ] {
        let answer = get(&app, &uri).await;
        assert_ne!(answer.status, StatusCode::OK, "{uri}");
        let metadata = &answer.json()["error"]["metadata"];
        assert!(metadata.is_object(), "{uri}: the failure keeps its metadata: {}", answer.text());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aoh16_q_entry_failure_carries_metadata() {
    failure_carries_metadata::<Queued>().await;
    failure_carries_metadata::<Inline>().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aoh17_q_submit_with_overrides_is_unmanaged() {
    let fixtures = Fixtures::default();
    let app = app_with(&fixtures, &[]).await;
    let answer = post(&app, &format!("{A}/q/submit/gated?tag=s"), "").await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    let result = &answer.json()["result"];
    assert_eq!(result["managed"], false);
    assert!(result["note"].as_str().unwrap_or("").contains("unmanaged"));
    assert_eq!(answer.managed().as_deref(), Some("false"));

    let info = get(&app, &format!("{A}/q/info/gated")).await;
    assert_eq!(info.status, StatusCode::NOT_FOUND, "an unmanaged evaluation is in no map");

    fixtures.open_gate();
    assert!(eventually(|| fixtures.count() == 1).await, "the side effect still happens");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aoh18_q_submit_volatile_without_overrides_is_unmanaged() {
    let fixtures = Fixtures::default();
    let app = app_with(&fixtures, &[]).await;
    let answer = post(&app, &format!("{A}/q/submit/gated_volatile"), "").await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(answer.json()["result"]["managed"], false);
    assert!(answer.json()["result"]["note"].is_string());
    fixtures.open_gate();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aoh19_recipe_with_arguments_keyed_asset_managed() {
    let app = app_with(&Fixtures::default(), &[("echo_args/report.txt", json!({"limit": 7}))]).await;
    let submitted = post(&app, &format!("{A}/key/submit/report.txt"), "").await;
    assert_eq!(submitted.status, StatusCode::OK, "{}", submitted.text());
    let info = get(&app, &format!("{A}/key/info/report.txt")).await;
    assert_eq!(info.status, StatusCode::OK, "{}", info.text());

    // `-R/report.txt` is the key; a bare `report.txt` would be a query of a filename alone.
    let answer = get(&app, &format!("{A}/q/data/-R/report.txt")).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert!(answer.text().contains("limit=7"), "{}", answer.text());
    assert_eq!(answer.managed().as_deref(), Some("true"), "the keyed recipe asset is managed");
}

#[tokio::test]
async fn aoh20_post_read_routes_served_when_read_only() {
    let envref = env_with::<Queued>(&Fixtures::default(), &[]).await;
    let app = AssetsApiBuilder::<GenericEnvironment<Value, (), Queued>>::new(A)
        .read_only()
        .build()
        .with_state(envref);
    for uri in [format!("{A}/q/data/echo_args"), format!("{A}/q/entry/echo_args")] {
        let answer = post(&app, &uri, r#"{"query":"a"}"#).await;
        assert_eq!(answer.status, StatusCode::OK, "{uri}: {}", answer.text());
    }
}

#[tokio::test]
async fn aoh21_q_data_post_query_string_and_body_combined() {
    let app = app().await;
    let answer = post(&app, &format!("{A}/q/data/echo_args?limit=3"), r#"{"query":"a"}"#).await;
    assert_eq!(answer.text(), "query=a;rank=true;case_sensitive=false;limit=3;fields=");
}

