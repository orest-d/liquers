//! `UIUSE` — showing a Liquers UI in a page.
//!
//! `mountUI(container, options)` mounts the `liquers-lib` web backend into a DOM element and
//! returns a [`LiquersUIMount`]: a page can submit queries as a control does, show a query's value
//! as a new element, subscribe to element status changes and dispose of the mount. A UI element
//! that a query returns crosses into JavaScript as a [`LiquersUIElement`] handle, and back as the
//! same `Arc`.
//!
//! **No `RefCell` borrow is held across a call into JavaScript**: subscriber lists and handles are
//! cloned out first, so a callback may dispose its subscription, or the whole mount.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use liquers_core::context::EnvRef;
use liquers_core::error::Error;
use liquers_core::metadata::Status;
use liquers_lib::ui::{
    app_message_channel, AppMessage, AppMessageSender, AppState, DirectAppState, ElementSource,
    InsertionPoint, RunnerEvent, UIContext, UIElement, UIHandle,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::environment::WebEnvironment;
use crate::error::liquers_error_to_js;

// ---------------------------------------------------------------------------
// Handles as numbers
// ---------------------------------------------------------------------------

/// A handle as a JavaScript `number`. A `u64` would export as `bigint`; handles stay far below
/// 2^53, where `f64` is exact.
pub(crate) fn handle_to_js(handle: UIHandle) -> f64 {
    handle.0 as f64
}

/// A JavaScript number read back as a handle, refusing what is not a non-negative integer.
pub(crate) fn handle_from_js(value: f64) -> Result<UIHandle, Error> {
    if value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value <= 9_007_199_254_740_991.0
    {
        Ok(UIHandle(value as u64))
    } else {
        Err(Error::general_error(format!(
            "{value} is not a UI element handle (a non-negative integer)"
        )))
    }
}

// ---------------------------------------------------------------------------
// UIElement handle
// ---------------------------------------------------------------------------

thread_local! {
    /// Live element handles by id, as in `records.rs`: a wasm-bindgen handle cannot be borrowed
    /// back from a `JsValue` without consuming it, so each handle carries an id
    /// (`__liquersUIElementHandle`) and this maps it to the `Arc` it was made from.
    static ELEMENTS: RefCell<HashMap<u32, Arc<dyn UIElement>>> = RefCell::new(HashMap::new());
    static NEXT_ELEMENT_ID: Cell<u32> = const { Cell::new(1) };
}

/// The element a live handle stands for, or `None` once it has been freed.
pub(crate) fn lookup_element(id: u32) -> Option<Arc<dyn UIElement>> {
    ELEMENTS.with(|elements| elements.borrow().get(&id).cloned())
}

/// A UI element value, as JavaScript sees it. Passing it back to Liquers (as a command's result or
/// argument) gives Rust the same element.
#[wasm_bindgen(js_name = UIElement)]
pub struct LiquersUIElement {
    value: Arc<dyn UIElement>,
    id: u32,
}

impl LiquersUIElement {
    pub fn new(value: Arc<dyn UIElement>) -> Self {
        let id = NEXT_ELEMENT_ID.with(|next| {
            let id = next.get();
            next.set(id.checked_add(1).unwrap_or(1));
            id
        });
        ELEMENTS.with(|elements| elements.borrow_mut().insert(id, value.clone()));
        LiquersUIElement { value, id }
    }

    /// The element this handle stands for.
    pub fn element(&self) -> Arc<dyn UIElement> {
        self.value.clone()
    }
}

impl Drop for LiquersUIElement {
    fn drop(&mut self) {
        let removed = ELEMENTS.with(|elements| elements.borrow_mut().remove(&self.id));
        drop(removed);
    }
}

#[wasm_bindgen(js_class = UIElement)]
impl LiquersUIElement {
    /// The element's Rust type name, for example `QueryConsoleElement`.
    #[wasm_bindgen(getter, js_name = typeName)]
    pub fn type_name(&self) -> String {
        self.value.type_name().to_string()
    }

    /// The element's title.
    #[wasm_bindgen(getter)]
    pub fn title(&self) -> String {
        self.value.title()
    }

    /// The element's web markup, rendered on its own (outside any mount).
    #[wasm_bindgen(js_name = renderHtml)]
    pub fn render_html(&self) -> String {
        let empty = DirectAppState::new();
        self.value.render_web(&empty)
    }

    /// The registry id that lets this handle cross back into Liquers as the same element. Not for
    /// pages to use; its presence is how the bridge recognizes an element handle.
    #[wasm_bindgen(getter, js_name = __liquersUIElementHandle)]
    pub fn handle_id(&self) -> u32 {
        self.id
    }
}

/// The element `js` stands for, when it is a live [`LiquersUIElement`] handle; `None` for anything
/// else. A freed handle is refused.
pub(crate) fn ui_element_value(js: &JsValue) -> Result<Option<Arc<dyn UIElement>>, Error> {
    let marker = JsValue::from_str("__liquersUIElementHandle");
    if !is_handle_instance(js, &marker) {
        return Ok(None);
    }
    let freed = || Error::conversion_error("a freed UIElement handle", "a live UIElement handle");
    let id = js_sys::Reflect::get(js, &marker)
        .ok()
        .and_then(|id| id.as_f64())
        .ok_or_else(freed)?;
    lookup_element(id as u32).map(Some).ok_or_else(freed)
}

/// Whether `js` is an instance of a handle class, recognised by a getter for `marker` on its
/// prototype, as wasm-bindgen generates it. JSON, a spread copy or a structured clone carries the
/// marker, if at all, as an own data property, so data is never taken for a handle.
pub(crate) fn is_handle_instance(js: &JsValue, marker: &JsValue) -> bool {
    if !js.is_object() {
        return false;
    }
    let object: &js_sys::Object = js.unchecked_ref();
    if js_sys::Object::has_own(object, marker) {
        return false;
    }
    let prototype = js_sys::Object::get_prototype_of(js);
    if prototype.is_null() || prototype.is_undefined() {
        return false;
    }
    let descriptor = js_sys::Object::get_own_property_descriptor(&prototype, marker);
    descriptor.is_object()
        && js_sys::Reflect::get(&descriptor, &JsValue::from_str("get"))
            .map(|getter| getter.is_function())
            .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// The mount
// ---------------------------------------------------------------------------

#[cfg(feature = "debug-handles")]
thread_local! {
    static LIVE_SUBSCRIPTIONS: Cell<usize> = const { Cell::new(0) };
}

/// How many page callbacks the live UI subscriptions hold. Test-only, like
/// [`crate::command::adapter::live_handle_count`].
#[cfg(feature = "debug-handles")]
pub fn live_subscription_count() -> usize {
    LIVE_SUBSCRIPTIONS.with(|cell| cell.get())
}

/// One subscriber: its callback, counted while it is held.
struct Subscriber {
    id: u32,
    callback: js_sys::Function,
}

impl Subscriber {
    fn new(id: u32, callback: js_sys::Function) -> Self {
        #[cfg(feature = "debug-handles")]
        LIVE_SUBSCRIPTIONS.with(|cell| cell.set(cell.get() + 1));
        Subscriber { id, callback }
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        #[cfg(feature = "debug-handles")]
        LIVE_SUBSCRIPTIONS.with(|cell| cell.set(cell.get().saturating_sub(1)));
    }
}

/// The state a mount shares with its driver's `on_tick` callback and its subscriptions.
struct MountInner {
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
    sender: AppMessageSender,
    /// `None` once disposed. Borrowed only inside `dispose` and `running`.
    handle: RefCell<Option<liquers_lib::ui::MountHandle>>,
    /// The latest status of each element, from the runner's events.
    statuses: RefCell<HashMap<UIHandle, Status>>,
    subscribers: RefCell<Vec<Rc<Subscriber>>>,
    next_subscriber: Cell<u32>,
}

impl MountInner {
    fn check_running(&self) -> Result<(), Error> {
        let running = self
            .handle
            .borrow()
            .as_ref()
            .map(|h| h.is_running())
            .unwrap_or(false);
        if running {
            Ok(())
        } else {
            Err(Error::general_error(
                "the UI mount has been disposed".to_string(),
            ))
        }
    }

    /// Records the tick's events and calls the subscribers, after the DOM was updated and with no
    /// lock held. A throwing callback is reported to `console.error` and does not stop the loop.
    fn on_tick(&self, events: Vec<RunnerEvent>) {
        if events.is_empty() {
            return;
        }
        {
            let mut statuses = self.statuses.borrow_mut();
            for event in &events {
                if let Some(handle) = event.handle {
                    statuses.insert(handle, event.status);
                }
            }
        }
        let subscribers: Vec<Rc<Subscriber>> = self.subscribers.borrow().clone();
        if subscribers.is_empty() {
            return;
        }
        for event in events {
            let js_event = status_event_to_js(&event);
            for subscriber in &subscribers {
                // A subscription disposed by an earlier callback of this tick is skipped.
                let live = self
                    .subscribers
                    .borrow()
                    .iter()
                    .any(|s| s.id == subscriber.id);
                if !live {
                    continue;
                }
                if let Err(thrown) = subscriber.callback.call1(&JsValue::NULL, &js_event) {
                    web_sys::console::error_2(
                        &JsValue::from_str("liquers: a UI status subscriber threw"),
                        &thrown,
                    );
                }
            }
        }
    }
}

fn set(object: &js_sys::Object, key: &str, value: &JsValue) {
    let _ = js_sys::Reflect::set(object, &JsValue::from_str(key), value);
}

fn status_event_to_js(event: &RunnerEvent) -> JsValue {
    let object = js_sys::Object::new();
    set(
        &object,
        "handle",
        &match event.handle {
            Some(h) => JsValue::from_f64(handle_to_js(h)),
            None => JsValue::NULL,
        },
    );
    set(
        &object,
        "status",
        &JsValue::from_str(crate::asset::status_name(event.status)),
    );
    if let Some(error) = &event.error {
        set(&object, "error", &liquers_error_to_js(error.clone()));
    }
    object.into()
}

/// What `mountUI` shows first.
enum Initial {
    None,
    Query(String),
    Spec(liquers_lib::ui::widgets::ui_spec_element::UISpec),
}

/// Reads `{ query?, spec? }`. Checked before the DOM, so the refusals are testable under Node.
fn parse_options(options: &JsValue) -> Result<Initial, Error> {
    if options.is_undefined() || options.is_null() {
        return Ok(Initial::None);
    }
    if !options.is_object() {
        return Err(Error::general_error(
            "mountUI options must be an object { query?, spec? }".to_string(),
        ));
    }
    let get = |key: &str| {
        js_sys::Reflect::get(options, &JsValue::from_str(key))
            .ok()
            .filter(|v| !v.is_undefined() && !v.is_null())
    };
    match (get("query"), get("spec")) {
        (Some(_), Some(_)) => Err(Error::general_error(
            "mountUI takes `query` or `spec`, not both".to_string(),
        )),
        (Some(query), None) => {
            let text = query.as_string().ok_or_else(|| {
                Error::general_error("mountUI `query` must be a string".to_string())
            })?;
            liquers_core::parse::parse_query(&text)?;
            Ok(Initial::Query(text))
        }
        (None, Some(spec)) => {
            use liquers_lib::ui::widgets::ui_spec_element::UISpec;
            let spec = match spec.as_string() {
                Some(yaml) => UISpec::from_yaml(&yaml)?,
                None => serde_wasm_bindgen::from_value::<UISpec>(spec).map_err(|e| {
                    Error::general_error(format!("mountUI `spec` is not a UI spec: {e}"))
                })?,
            };
            Ok(Initial::Spec(spec))
        }
        (None, None) => Ok(Initial::None),
    }
}

/// The DOM element to mount into, or a `NotSupported` refusal outside a page.
fn container_element(container: JsValue) -> Result<web_sys::Element, Error> {
    let has_document = web_sys::window().and_then(|w| w.document()).is_some();
    if !has_document {
        return Err(Error::not_supported(
            "mountUI needs a DOM element in a page with a document (not a worker or Node)"
                .to_string(),
        ));
    }
    container
        .dyn_into::<web_sys::Element>()
        .map_err(|_| Error::not_supported("mountUI needs a DOM element to mount into".to_string()))
}

/// Mounts a UI into `container` on the environment `envref`, which `current_env` re-resolves on
/// each tick when given.
pub(crate) async fn mount_ui_with(
    container: JsValue,
    options: JsValue,
    envref: EnvRef<WebEnvironment>,
    current_env: Option<Box<dyn FnMut() -> Option<EnvRef<WebEnvironment>>>>,
) -> Result<LiquersUIMount, Error> {
    let initial = parse_options(&options)?;
    let root = container_element(container)?;

    let (sender, receiver) = app_message_channel();
    let mut direct = DirectAppState::new();
    let mut spec_root = None;
    match initial {
        Initial::None => {}
        Initial::Query(query) => {
            direct.add_node(None, 0, ElementSource::Query(query))?;
        }
        Initial::Spec(spec) => {
            use liquers_lib::ui::widgets::ui_spec_element::UISpecElement;
            let handle = direct.add_node(None, 0, ElementSource::None)?;
            spec_root = Some((handle, UISpecElement::from_spec("UI".to_string(), spec)));
        }
    }
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct));
    if let Some((handle, mut element)) = spec_root {
        // `init` submits the spec's init queries, so it needs the message channel.
        let ctx = UIContext::new(app_state.clone(), sender.clone());
        element.init(handle, &ctx)?;
        let mut state = app_state
            .try_lock()
            .map_err(|_| Error::general_error("the new UI state is busy".to_string()))?;
        state.set_element(handle, Box::new(element))?;
    }

    let inner = Rc::new(MountInner {
        app_state: app_state.clone(),
        sender: sender.clone(),
        handle: RefCell::new(None),
        statuses: RefCell::new(HashMap::new()),
        subscribers: RefCell::new(Vec::new()),
        next_subscriber: Cell::new(1),
    });
    let weak: Weak<MountInner> = Rc::downgrade(&inner);
    let on_tick: Box<dyn FnMut(Vec<RunnerEvent>)> = Box::new(move |events| {
        if let Some(inner) = weak.upgrade() {
            inner.on_tick(events);
        }
    });
    let mount_options = liquers_lib::ui::MountOptions {
        envref,
        current_env,
        on_tick: Some(on_tick),
    };
    let handle =
        liquers_lib::ui::mount_web_with(root, mount_options, app_state, sender, receiver).await?;
    *inner.handle.borrow_mut() = Some(handle);
    Ok(LiquersUIMount { inner })
}

/// Mounts a Liquers UI into a DOM element, on the global environment.
///
/// `options` is `{ query }` (a query whose value is shown as the first element) or `{ spec }` (a
/// UI spec, as YAML text or an object). Resolves to a `UIMount`.
#[wasm_bindgen(js_name = mountUI)]
pub fn mount_ui(container: JsValue, options: JsValue) -> js_sys::Promise {
    wasm_bindgen_futures::future_to_promise(async move {
        let envref = crate::environment::shared_env().map_err(liquers_error_to_js)?;
        let current: Box<dyn FnMut() -> Option<EnvRef<WebEnvironment>>> =
            Box::new(|| crate::environment::shared_env().ok());
        mount_ui_with(container, options, envref, Some(current))
            .await
            .map(JsValue::from)
            .map_err(liquers_error_to_js)
    })
}

/// A mounted UI.
#[wasm_bindgen(js_name = UIMount)]
pub struct LiquersUIMount {
    inner: Rc<MountInner>,
}

#[wasm_bindgen(js_class = UIMount)]
impl LiquersUIMount {
    /// Whether the mount is still running.
    #[wasm_bindgen(getter)]
    pub fn running(&self) -> bool {
        self.inner.check_running().is_ok()
    }

    /// Runs `query` the way a control's query action does, for the element `handle` (or as a root
    /// query). Commands place their own results; nothing is shown by `submit` itself.
    pub fn submit(&self, query: String, handle: Option<f64>) -> Result<(), JsValue> {
        self.submit_inner(query, handle)
            .map_err(liquers_error_to_js)
    }

    /// Shows `query`'s value as a new root element, or instead of the element `handle`. Returns the
    /// element's handle. Refused from inside a render, which holds the UI lock.
    pub fn show(&self, query: String, handle: Option<f64>) -> Result<f64, JsValue> {
        self.show_inner(query, handle).map_err(liquers_error_to_js)
    }

    /// Subscribes to element status changes. The only kind is `"status"`; the callback receives
    /// `{ handle, status, error? }`, after the page was updated.
    pub fn on(
        &self,
        kind: String,
        callback: js_sys::Function,
    ) -> Result<LiquersUISubscription, JsValue> {
        self.on_inner(kind, callback).map_err(liquers_error_to_js)
    }

    /// The element tree, depth first: `{ handle, parent, typeName, title, status }` for each node.
    pub fn elements(&self) -> Result<js_sys::Array, JsValue> {
        self.elements_inner().map_err(liquers_error_to_js)
    }

    /// Stops the mount: its listeners are removed, its container emptied, its subscriptions
    /// dropped. Idempotent.
    pub fn dispose(&self) {
        let handle = self.inner.handle.borrow_mut().take();
        if let Some(handle) = handle {
            handle.stop();
        }
        let dropped: Vec<Rc<Subscriber>> = self.inner.subscribers.borrow_mut().drain(..).collect();
        drop(dropped);
    }
}

impl LiquersUIMount {
    /// The shared UI state, for integrations that extend the mount.
    pub fn app_state(&self) -> Arc<tokio::sync::Mutex<dyn AppState>> {
        self.inner.app_state.clone()
    }

    /// The message channel of the mount, for integrations that extend it.
    pub fn sender(&self) -> AppMessageSender {
        self.inner.sender.clone()
    }

    fn submit_inner(&self, query: String, handle: Option<f64>) -> Result<(), Error> {
        self.inner.check_running()?;
        liquers_core::parse::parse_query(&query)?;
        let handle = handle.map(handle_from_js).transpose()?;
        self.inner
            .sender
            .send(AppMessage::SubmitQuery { handle, query })
            .map_err(|_| Error::general_error("the UI mount has been disposed".to_string()))
    }

    fn show_inner(&self, query: String, handle: Option<f64>) -> Result<f64, Error> {
        self.inner.check_running()?;
        liquers_core::parse::parse_query(&query)?;
        let handle = handle.map(handle_from_js).transpose()?;
        let mut state = self.inner.app_state.try_lock().map_err(|_| {
            Error::general_error(
                "the UI is busy rendering; call show() outside a render callback".to_string(),
            )
        })?;
        let point = match handle {
            Some(h) => {
                if !state.node_exists(h) {
                    return Err(Error::general_error(format!("no UI element {}", h.0)));
                }
                InsertionPoint::Instead(h)
            }
            None => InsertionPoint::Root,
        };
        let placed = state.insert_source(&point, ElementSource::Query(query))?;
        Ok(handle_to_js(placed))
    }

    fn on_inner(
        &self,
        kind: String,
        callback: js_sys::Function,
    ) -> Result<LiquersUISubscription, Error> {
        self.inner.check_running()?;
        if kind != "status" {
            return Err(Error::general_error(format!(
                "unknown UI event kind {kind:?}; the only kind is \"status\""
            )));
        }
        let id = self.inner.next_subscriber.get();
        self.inner.next_subscriber.set(id.wrapping_add(1));
        self.inner
            .subscribers
            .borrow_mut()
            .push(Rc::new(Subscriber::new(id, callback)));
        Ok(LiquersUISubscription {
            mount: Rc::downgrade(&self.inner),
            id,
        })
    }

    fn elements_inner(&self) -> Result<js_sys::Array, Error> {
        let state = self.inner.app_state.try_lock().map_err(|_| {
            Error::general_error(
                "the UI is busy rendering; call elements() outside a render callback".to_string(),
            )
        })?;
        let statuses = self.inner.statuses.borrow().clone();
        let out = js_sys::Array::new();
        let mut stack: Vec<UIHandle> = state.roots().into_iter().rev().collect();
        while let Some(handle) = stack.pop() {
            let element = state.get_element(handle).ok().flatten();
            let status = match statuses.get(&handle) {
                Some(s) => crate::asset::status_name(*s),
                None => match element {
                    Some(_) => "ready",
                    None => "none",
                },
            };
            let object = js_sys::Object::new();
            set(&object, "handle", &JsValue::from_f64(handle_to_js(handle)));
            set(
                &object,
                "parent",
                &match state.parent(handle).ok().flatten() {
                    Some(p) => JsValue::from_f64(handle_to_js(p)),
                    None => JsValue::NULL,
                },
            );
            set(
                &object,
                "typeName",
                &match element {
                    Some(e) => JsValue::from_str(e.type_name()),
                    None => JsValue::NULL,
                },
            );
            set(
                &object,
                "title",
                &JsValue::from_str(&element.map(|e| e.title()).unwrap_or_default()),
            );
            set(&object, "status", &JsValue::from_str(status));
            out.push(&object.into());
            let children = state.children(handle).unwrap_or_default();
            stack.extend(children.into_iter().rev());
        }
        Ok(out)
    }
}

/// A status subscription. `dispose()` releases its callback.
#[wasm_bindgen(js_name = UISubscription)]
pub struct LiquersUISubscription {
    mount: Weak<MountInner>,
    id: u32,
}

#[wasm_bindgen(js_class = UISubscription)]
impl LiquersUISubscription {
    /// Stops the callback. Idempotent; a no-op after the mount was disposed.
    pub fn dispose(&self) {
        if let Some(inner) = self.mount.upgrade() {
            let removed: Vec<Rc<Subscriber>> = {
                let mut subscribers = inner.subscribers.borrow_mut();
                let (gone, kept): (Vec<_>, Vec<_>) =
                    subscribers.drain(..).partition(|s| s.id == self.id);
                *subscribers = kept;
                gone
            };
            drop(removed);
        }
    }
}
