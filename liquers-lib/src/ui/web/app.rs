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

// ─── Browser driver (wasm only) ─────────────────────────────────────────────

#[cfg(all(feature = "webui", target_arch = "wasm32"))]
mod browser {
    use std::sync::Arc;

    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    use liquers_core::context::{EnvRef, Environment};
    use liquers_core::error::Error;

    use crate::ui::action::{dispatch_action, UiAction};
    use crate::ui::app_state::AppState;
    use crate::ui::handle::UIHandle;
    use crate::ui::message::{AppMessage, AppMessageReceiver, AppMessageSender};
    use crate::ui::payload::{SimpleUIPayload, UIPayload};
    use crate::ui::runner::AppRunner;
    use crate::ui::ui_context::UIContext;
    use crate::value::Value;

    /// Owns the delegated event listeners and the root element, keeping the mount alive.
    /// Dropping it detaches the listeners.
    pub struct MountHandle {
        _root: web_sys::Element,
        _click: Closure<dyn FnMut(web_sys::Event)>,
        _keydown: Closure<dyn FnMut(web_sys::Event)>,
    }

    fn read_input_value(input_id: &str) -> Option<String> {
        let doc = web_sys::window()?.document()?;
        let el = doc.get_element_by_id(input_id)?;
        el.dyn_into::<web_sys::HtmlInputElement>().ok().map(|i| i.value())
    }

    /// Walk up from `node` to the nearest `ui-element-{n}` and parse its handle.
    fn nearest_element_handle(node: &web_sys::Element) -> Option<UIHandle> {
        let el = node.closest("[id^=\"ui-element-\"]").ok().flatten()?;
        el.id()
            .strip_prefix("ui-element-")
            .and_then(|n| n.parse::<u64>().ok())
            .map(UIHandle)
    }

    /// The single delegated handler: find the nearest `data-lq-action`, parse the `UiAction`,
    /// and dispatch it (reading the live input value for `Apply`).
    fn dispatch_dom_event(ev: &web_sys::Event, ctx: &UIContext) {
        if ev.type_() == "keydown" {
            match ev.dyn_ref::<web_sys::KeyboardEvent>() {
                Some(ke) if ke.key() == "Enter" => {}
                _ => return,
            }
        }
        let target = match ev.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
            Some(t) => t,
            None => return,
        };
        let node = match target.closest("[data-lq-action]").ok().flatten() {
            Some(n) => n,
            None => return,
        };
        let json = match node.get_attribute("data-lq-action") {
            Some(a) => a,
            None => return,
        };
        let action: UiAction = match serde_json::from_str(&json) {
            Ok(a) => a,
            Err(_) => return,
        };
        match &action {
            UiAction::Apply { handle, input_id, query } => {
                let value = read_input_value(input_id).unwrap_or_default();
                ctx.send_message(AppMessage::ApplyToInput {
                    handle: *handle,
                    input: value,
                    query: query.clone(),
                });
            }
            _ => dispatch_action(&action, ctx, nearest_element_handle(&node)),
        }
        ev.prevent_default();
    }

    /// Re-render all roots into `root` via innerHTML. Uses try_lock (single-threaded wasm:
    /// free between runner awaits).
    fn render_roots_into(root: &web_sys::Element, app_state: &Arc<tokio::sync::Mutex<dyn AppState>>) {
        if let Ok(state) = app_state.try_lock() {
            let mut html = String::new();
            for r in state.roots() {
                html.push_str(&super::super::render_element_web(r, &*state));
            }
            root.set_inner_html(&html);
        }
    }

    /// Browser entry point. Attaches the delegated listeners, drives `AppRunner::run` on a
    /// timer loop, and re-renders roots on `needs_repaint()`. Returns a `MountHandle` the
    /// caller must keep alive.
    pub async fn mount_web<E>(
        root: web_sys::Element,
        envref: EnvRef<E>,
        app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
        sender: AppMessageSender,
        receiver: AppMessageReceiver,
        initial_query: Option<String>,
    ) -> Result<MountHandle, Error>
    where
        E: Environment<Value = Value>,
        E::Payload: UIPayload + From<SimpleUIPayload>,
    {
        if let Some(q) = initial_query {
            let _ = sender.send(AppMessage::SubmitQuery { handle: None, query: q });
        }

        let ctx = UIContext::new(app_state.clone(), sender.clone());

        let click_ctx = ctx.clone();
        let click = Closure::wrap(Box::new(move |ev: web_sys::Event| {
            dispatch_dom_event(&ev, &click_ctx);
        }) as Box<dyn FnMut(web_sys::Event)>);
        let keydown_ctx = ctx.clone();
        let keydown = Closure::wrap(Box::new(move |ev: web_sys::Event| {
            dispatch_dom_event(&ev, &keydown_ctx);
        }) as Box<dyn FnMut(web_sys::Event)>);
        root.add_event_listener_with_callback("click", click.as_ref().unchecked_ref())
            .map_err(|_| Error::general_error("failed to attach click listener".to_string()))?;
        root.add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())
            .map_err(|_| Error::general_error("failed to attach keydown listener".to_string()))?;

        let loop_root = root.clone();
        let loop_state = app_state.clone();
        let mut runner = AppRunner::new(envref, receiver, sender);
        wasm_bindgen_futures::spawn_local(async move {
            let mut first = true;
            loop {
                let _ = runner.run(&loop_state).await;
                if first || runner.needs_repaint() {
                    render_roots_into(&loop_root, &loop_state);
                    first = false;
                }
                gloo_timers::future::TimeoutFuture::new(16).await;
            }
        });

        Ok(MountHandle {
            _root: root,
            _click: click,
            _keydown: keydown,
        })
    }
}

#[cfg(all(feature = "webui", target_arch = "wasm32"))]
pub use browser::{mount_web, MountHandle};
