//! `_entry_format` — the encoding of a `DataEntry` envelope — on every route that reads or writes
//! one, replacing `format`. Design: `specs/design/axum-query-named-arguments/` (Phase 3,
//! EFO01–EFO06).

mod common;

use axum::{http::StatusCode, Router};
use common::{env_with, get, request, Fixtures};
use liquers_axum::{AssetsApiBuilder, RecipesApiBuilder, StoreApiBuilder};
use liquers_core::{
    context::{EnvRef, SimpleEnvironment},
    environment_builder::Queued,
    value::Value,
};
use serde_json::json;

type Env = SimpleEnvironment<Value>;

async fn app() -> (Router, EnvRef<Env>) {
    let envref = env_with::<Queued>(&Fixtures::default(), &[("make_text/made.txt", json!({}))]).await;
    let app = Router::new()
        .merge(AssetsApiBuilder::<Env>::new("/api/assets").build())
        .merge(StoreApiBuilder::<Env>::new("/api/store").build())
        .merge(RecipesApiBuilder::<Env>::new("/api/recipes").build())
        .with_state(envref.clone());
    (app, envref)
}

/// A `Text` value at `docs/a.txt`, written through the Assets API so it carries its type.
async fn write_text(app: &Router) {
    let answer = request(
        app,
        "POST",
        "/api/assets/key/data/docs/a.txt?type_identifier=Text&data_format=txt",
        Some("hello"),
        &[],
    )
    .await;
    assert_eq!(answer.status, StatusCode::CREATED, "{}", answer.text());
}

fn content_type(answer: &common::Answer) -> String {
    answer
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

#[tokio::test]
async fn efo01_q_entry_entry_format_then_accept_then_cbor() {
    let (app, _) = app().await;
    let answer = get(&app, "/api/assets/q/entry/make_text?_entry_format=json").await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(content_type(&answer), "application/json");
    assert!(answer.json()["metadata"].is_object());

    let answer = request(&app, "GET", "/api/assets/q/entry/make_text", None, &[("accept", "application/json")]).await;
    assert_eq!(content_type(&answer), "application/json", "Accept is the fallback");

    let answer = get(&app, "/api/assets/q/entry/make_text").await;
    assert_eq!(content_type(&answer), "application/cbor", "CBOR is the default");
}

#[tokio::test]
async fn efo02_old_format_on_key_entry_is_400() {
    let (app, _) = app().await;
    write_text(&app).await;
    for uri in [
        "/api/assets/key/entry/docs/a.txt?format=json",
        "/api/assets/key/recover/docs/a.txt?format=json",
        "/api/store/entry/docs/a.txt?format=json",
        "/api/recipes/entry/made.txt?format=json",
    ] {
        let answer = get(&app, uri).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(answer.json()["error"]["type"], "ParameterError", "{uri}");
        assert!(answer.text().contains("_entry_format"), "{uri}: {}", answer.text());
    }
}

#[tokio::test]
async fn efo03_store_and_recipes_entry_accept_entry_format() {
    let (app, _) = app().await;
    write_text(&app).await;
    for uri in [
        "/api/store/entry/docs/a.txt?_entry_format=json",
        "/api/assets/key/entry/docs/a.txt?_entry_format=json",
        "/api/assets/key/recover/docs/a.txt?_entry_format=json",
        "/api/recipes/entry/made.txt?_entry_format=json",
    ] {
        let answer = get(&app, uri).await;
        assert_eq!(answer.status, StatusCode::OK, "{uri}: {}", answer.text());
        assert_eq!(content_type(&answer), "application/json", "{uri}");
    }
}

#[tokio::test]
async fn efo04_unknown_entry_format_value_400() {
    let (app, _) = app().await;
    for uri in [
        "/api/assets/q/entry/make_text?_entry_format=xml",
        "/api/recipes/entry/made.txt?_entry_format=xml",
    ] {
        let answer = get(&app, uri).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(answer.json()["error"]["type"], "ParameterError", "{uri}");
    }
}

#[tokio::test]
async fn efo05_post_key_entry_decodes_by_entry_format_then_content_type() {
    let (app, _) = app().await;
    let entry = json!({
        "metadata": {"type_identifier": "Text", "data_format": "txt"},
        "data": "aGk=",
    })
    .to_string();
    let answer = request(&app, "POST", "/api/assets/key/entry/notes/a.txt?_entry_format=json", Some(&entry), &[]).await;
    assert_eq!(answer.status, StatusCode::CREATED, "{}", answer.text());
    // No option: the JSON Content-Type `request` sends selects the decoding.
    let answer = request(&app, "POST", "/api/assets/key/entry/notes/b.txt", Some(&entry), &[]).await;
    assert_eq!(answer.status, StatusCode::CREATED, "{}", answer.text());
    let answer = request(&app, "POST", "/api/assets/key/entry/notes/c.txt?format=json", Some(&entry), &[]).await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn efo06_format_on_q_entry_is_an_override() {
    let (app, _) = app().await;
    // `format` is not an option any more: on `q/entry` it is an argument override like any
    // other name, and `echo_args` has no `format` argument.
    let answer = get(&app, "/api/assets/q/entry/echo_args?format=json").await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{}", answer.text());
    assert_eq!(answer.json()["error"]["type"], "ParameterError");
    assert!(answer.text().contains("'format'"), "{}", answer.text());
}
