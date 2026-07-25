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
    use crate::ui::app_state::{AppState, Invalidation, UIChange};
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

    /// Re-render all roots into `root` via innerHTML — the whole-tree path.
    fn render_all_roots(root: &web_sys::Element, state: &dyn AppState) {
        let mut html = String::new();
        for r in state.roots() {
            html.push_str(&super::super::render_element_web(r, state));
        }
        root.set_inner_html(&html);
    }

    /// The focused element and, for a text field, its selection — captured before markup is
    /// replaced so it can be put back afterwards.
    struct FocusSnapshot {
        element_id: String,
        selection: Option<(u32, u32)>,
    }

    fn capture_focus(doc: &web_sys::Document) -> Option<FocusSnapshot> {
        let active = doc.active_element()?;
        let element_id = active.id();
        if element_id.is_empty() {
            return None; // nothing to find it by after the replacement
        }
        let selection = active
            .dyn_ref::<web_sys::HtmlInputElement>()
            .and_then(|input| match (input.selection_start(), input.selection_end()) {
                (Ok(Some(start)), Ok(Some(end))) => Some((start, end)),
                _ => None,
            });
        Some(FocusSnapshot {
            element_id,
            selection,
        })
    }

    fn restore_focus(doc: &web_sys::Document, snapshot: &FocusSnapshot) {
        let element = match doc.get_element_by_id(&snapshot.element_id) {
            Some(el) => el,
            None => return, // the field is gone; nothing to restore
        };
        if let Some(html_el) = element.dyn_ref::<web_sys::HtmlElement>() {
            let _ = html_el.focus();
        }
        if let (Some(input), Some((start, end))) =
            (element.dyn_ref::<web_sys::HtmlInputElement>(), snapshot.selection)
        {
            let _ = input.set_selection_range(start, end);
        }
    }

    /// Apply one recorded change to the DOM. Returns false to request the whole-tree fallback.
    ///
    /// Each change re-reads the current model rather than trusting the record, which is what makes
    /// a stale entry harmless — an element inserted and removed within the same batch is simply
    /// skipped.
    fn apply_change(root: &web_sys::Element, change: &UIChange, state: &dyn AppState) -> bool {
        let doc = match web_sys::window().and_then(|w| w.document()) {
            Some(d) => d,
            None => return false,
        };
        match change {
            UIChange::Inserted {
                parent,
                handle,
                index,
            } => insert_element_node(root, &doc, *parent, *handle, *index, state),
            UIChange::Removed { parent, handle } => {
                remove_element_node(&doc, *parent, *handle, state)
            }
            UIChange::Replaced { handle } => replace_element_markup(&doc, *handle, state),
        }
    }

    /// The node an element's children are rendered into: the mount root for a root element, else
    /// the node the parent marked with `data-lq-children="{parent}"`. `None` means the parent did
    /// not declare one, so its markup may depend on its child set and only a re-render is safe.
    fn children_container(
        root: &web_sys::Element,
        doc: &web_sys::Document,
        parent: Option<UIHandle>,
    ) -> Option<web_sys::Element> {
        match parent {
            None => Some(root.clone()),
            Some(p) => doc
                .query_selector(&format!("[data-lq-children=\"{}\"]", p.0))
                .ok()
                .flatten(),
        }
    }

    /// Insert one rendered element node, without disturbing its siblings.
    fn insert_element_node(
        root: &web_sys::Element,
        doc: &web_sys::Document,
        parent: Option<UIHandle>,
        handle: UIHandle,
        index: usize,
        state: &dyn AppState,
    ) -> bool {
        if !state.node_exists(handle) {
            return true; // inserted and removed again before the renderer looked
        }
        let dom_id = super::super::element_dom_id(Some(handle));
        if doc.get_element_by_id(&dom_id).is_some() {
            // Already materialized. One batch can insert an ancestor and its descendant — a
            // command adds a container, whose `init` then adds a child, all before the next tick.
            // Rendering the ancestor draws its whole subtree from the *final* model, so the
            // descendant is already in the DOM by the time its own record is applied; inserting
            // again would duplicate the id and leave a ghost node that a later removal misses.
            return true;
        }
        let container = match children_container(root, doc, parent) {
            Some(c) => c,
            // The parent's markup depends on its child set (or it has never been rendered):
            // re-render it instead.
            None => return fall_back_to_parent(doc, parent, state),
        };

        let markup = super::super::render_element_web(handle, state);
        let holder = match doc.create_element("div") {
            Ok(h) => h,
            Err(_) => return false,
        };
        holder.set_inner_html(&markup);
        let node = match holder.first_element_child() {
            Some(n) => n,
            // `render_element_web` always produces exactly one element; an empty result would mean
            // the renderer changed shape, and only a full render can be trusted then.
            None => return false,
        };

        let children = container.children();
        let result = match children.item(index as u32) {
            Some(before) => container.insert_before(&node, Some(&before)),
            // Index past the end (or an empty container): append.
            None => container.append_child(&node),
        };
        result.is_ok()
    }

    /// Remove one element's node. Siblings and the parent's own markup are untouched, which is
    /// what the parent asserted by declaring a child container.
    fn remove_element_node(
        doc: &web_sys::Document,
        parent: Option<UIHandle>,
        handle: UIHandle,
        state: &dyn AppState,
    ) -> bool {
        let dom_id = super::super::element_dom_id(Some(handle));
        match doc.get_element_by_id(&dom_id) {
            Some(node) => {
                // Only a declared container guarantees the parent's markup is child-independent.
                if parent.is_some()
                    && doc
                        .query_selector(&format!(
                            "[data-lq-children=\"{}\"]",
                            parent.map(|p| p.0).unwrap_or_default()
                        ))
                        .ok()
                        .flatten()
                        .is_none()
                {
                    return fall_back_to_parent(doc, parent, state);
                }
                node.remove();
                true
            }
            // Already absent — nothing to do.
            None => true,
        }
    }

    /// Re-render the parent element; `None` (a root) can only be handled by a whole-tree render.
    fn fall_back_to_parent(
        doc: &web_sys::Document,
        parent: Option<UIHandle>,
        state: &dyn AppState,
    ) -> bool {
        match parent {
            Some(p) => replace_element_markup(doc, p, state),
            None => false,
        }
    }

    /// Replace `#ui-element-{handle}`'s markup in place. False requests the whole-tree fallback.
    fn replace_element_markup(
        doc: &web_sys::Document,
        handle: UIHandle,
        state: &dyn AppState,
    ) -> bool {
        if !state.node_exists(handle) {
            return true; // removed since it was recorded — the parent's record covers it
        }
        let dom_id = super::super::element_dom_id(Some(handle));
        match doc.get_element_by_id(&dom_id) {
            Some(node) => {
                node.set_outer_html(&super::super::render_element_web(handle, state));
                true
            }
            // Never rendered: only a full render can place it.
            None => false,
        }
    }

    /// Apply an invalidation to the DOM, preserving focus and caret across the replacements.
    fn apply_invalidation(
        root: &web_sys::Element,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
        invalidation: &Invalidation,
    ) {
        let state = match app_state.try_lock() {
            Ok(s) => s,
            // Records stay pending; the next tick applies them.
            Err(_) => return,
        };
        let doc = web_sys::window().and_then(|w| w.document());
        let focus = doc.as_ref().and_then(capture_focus);

        match invalidation {
            Invalidation::None => return,
            Invalidation::All => render_all_roots(root, &*state),
            Invalidation::Changes(changes) => {
                // A change that cannot be applied in place abandons the incremental path: a
                // half-updated page is worse than a redundant full render.
                for change in changes {
                    if !apply_change(root, change, &*state) {
                        render_all_roots(root, &*state);
                        break;
                    }
                }
            }
        }

        if let (Some(doc), Some(focus)) = (doc.as_ref(), focus.as_ref()) {
            restore_focus(doc, focus);
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
            loop {
                let _ = runner.run(&loop_state).await;
                // Rendering follows what the model says changed. `DirectAppState` starts at
                // `All`, so the first paint needs no special case; when the lock is busy the
                // records stay pending and are applied on a later tick.
                let invalidation = match loop_state.try_lock() {
                    Ok(mut state) => state.take_invalidation(),
                    Err(_) => Invalidation::None,
                };
                apply_invalidation(&loop_root, &loop_state, &invalidation);
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
