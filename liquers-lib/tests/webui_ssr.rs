//! SSR (server-side) rendering integration tests for the webui backend.
//!
//! Run with: `cargo test -p liquers-lib --no-default-features --features webui,image-support --test webui_ssr`

#![cfg(feature = "webui")]

use std::sync::Arc;

use liquers_lib::ui::web::render_app_ssr;
use liquers_lib::ui::widgets::markdown_element::MarkdownElement;
use liquers_lib::ui::widgets::ui_spec_element::{UISpec, UISpecElement};
use liquers_lib::ui::{AppState, DirectAppState, ElementSource, StateViewElement, UIElement};
use liquers_lib::value::Value;

/// Build a UISpec root with a markdown child and render the whole tree to HTML.
fn build_tree() -> Result<DirectAppState, Box<dyn std::error::Error>> {
    let mut state = DirectAppState::new();
    let root = state.add_node(None, 0, ElementSource::None)?;
    let spec = UISpec::from_yaml("layout: vertical")?;
    let mut root_el = UISpecElement::from_spec("Dashboard".into(), spec);
    root_el.set_handle(root);
    state.set_element(root, Box::new(root_el))?;

    let child = state.add_node(Some(root), 0, ElementSource::None)?;
    let mut md = MarkdownElement::new("Doc".into(), "# Title\n\nBody".into());
    md.set_handle(child);
    state.set_element(child, Box::new(md))?;
    Ok(state)
}

#[tokio::test]
async fn ssr_renders_tree_to_html() -> Result<(), Box<dyn std::error::Error>> {
    let state = build_tree()?;
    let root = state.roots()[0];
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> = Arc::new(tokio::sync::Mutex::new(state));

    let html = render_app_ssr(&app_state).await?;

    // Root element carries its stable id, and the markdown child rendered to HTML.
    assert!(html.contains(&format!("id=\"ui-element-{}\"", root.0)), "html: {html}");
    assert!(html.contains("lq-UISpecElement"));
    assert!(html.contains("<h1>Title</h1>"), "html: {html}");
    assert!(html.contains("Body"));
    Ok(())
}

#[tokio::test]
async fn ssr_render_is_deterministic() -> Result<(), Box<dyn std::error::Error>> {
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(build_tree()?));
    let a = render_app_ssr(&app_state).await?;
    let b = render_app_ssr(&app_state).await?;
    assert_eq!(a, b, "render_web must be a pure function of state");
    Ok(())
}

#[tokio::test]
async fn ssr_escapes_hostile_text_value() -> Result<(), Box<dyn std::error::Error>> {
    // A plain Text value goes through value_to_html, which escapes it. (Markdown is a
    // separate, trusted rich format that intentionally allows raw-HTML passthrough.)
    let mut state = DirectAppState::new();
    let root = state.add_node(None, 0, ElementSource::None)?;
    let val = Arc::new(Value::from("<script>alert(1)</script>"));
    let mut sv = StateViewElement::new("X".into(), val);
    sv.set_handle(root);
    state.set_element(root, Box::new(sv))?;
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> = Arc::new(tokio::sync::Mutex::new(state));

    let html = render_app_ssr(&app_state).await?;
    assert!(!html.contains("<script>alert(1)</script>"), "raw script leaked: {html}");
    assert!(html.contains("&lt;script&gt;"), "html: {html}");
    Ok(())
}
