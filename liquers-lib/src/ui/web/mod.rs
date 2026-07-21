//! Web-framework-independent rendering backend for `liquers_lib::ui`.
//!
//! String-first: elements define `UIElement::render_web(&self) -> String` (the shared
//! source of truth for SSR and browser). The browser `show_in_web` default writes that
//! string into the live DOM. Interactivity is a serializable `UiAction` in `data-lq-action`
//! attributes dispatched by a single delegated listener. See `specs/webui/`.

pub mod app;
pub mod dataframe;
pub mod html;
pub mod widgets;

pub use app::render_app_ssr;
pub use html::{escape_html, value_to_html};

use crate::ui::app_state::AppState;
use crate::ui::handle::UIHandle;

/// Stable DOM id for an element: `ui-element-{n}`, or `ui-element-unset` before init.
/// Used as the CSS/query hook and as the anchor event delegation walks up to.
pub fn element_dom_id(handle: Option<UIHandle>) -> String {
    match handle {
        Some(h) => format!("ui-element-{}", h.0),
        None => "ui-element-unset".to_string(),
    }
}

/// SSR helper: render one element (by handle) to HTML from an immutable AppState borrow.
/// Returns a small placeholder for a pending (element=None) or missing node. Needs no lock
/// and no extract-replace because `render_web` is immutable.
pub fn render_element_web(handle: UIHandle, app_state: &dyn AppState) -> String {
    match app_state.get_element(handle) {
        Ok(Some(el)) => el.render_web(app_state),
        Ok(None) => format!(
            "<div id=\"{}\" class=\"lq-pending\">Loading…</div>",
            element_dom_id(Some(handle))
        ),
        Err(_) => format!(
            "<div class=\"lq-missing\">Element {} not found</div>",
            handle.0
        ),
    }
}
