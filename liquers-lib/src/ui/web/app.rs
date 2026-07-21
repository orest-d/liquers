//! Web backend entry points: SSR rendering (all targets) and, on wasm, the browser mount
//! + driver.
//!
//! The wasm browser driver (`mount_web`, `MountHandle`, `render_element_dom`) is implemented
//! in the M4 milestone, where it is built and exercised via `trunk` + Playwright; it only
//! compiles for `target_arch = "wasm32"`. SSR (`render_app_ssr`) works on every target and is
//! covered by native tests.

use std::sync::Arc;

use liquers_core::error::Error;

use crate::ui::app_state::AppState;

/// Server-side entry point. Locks `app_state`, renders every root via `render_element_web`,
/// and returns the concatenated HTML fragment (non-interactive; `data-lq-action` attributes
/// remain for a future hydration script). Available on all targets.
pub async fn render_app_ssr(
    app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
) -> Result<String, Error> {
    let state = app_state.lock().await;
    let mut html = String::new();
    for root in state.roots() {
        html.push_str(&super::render_element_web(root, &*state));
    }
    Ok(html)
}
