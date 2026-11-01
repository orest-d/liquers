//! `UIDEF` — UI elements a page defines.
//!
//! `defineElement({ type, render, onEvent?, mount?, title? })` registers a [`JsElementDelegate`]
//! for `type` with `liquers-lib`'s per-thread delegate registry, and `element(type, props)` makes a
//! [`DelegatedElement`] of that type, as a `UIElement` handle that any command may return or a UI
//! may show. The element holds only its type and JSON props, so a saved tree restores and renders
//! once the type is defined again.
//!
//! Call contexts: `render` runs while the driver paints, under the UI lock, and receives only the
//! props; `onEvent` runs while the runner delivers an event, with the lock released; `mount` runs
//! after a tick, from the mount's reconciliation (see `ui.rs`). **No `RefCell` borrow is held while
//! a page function runs.**

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use liquers_core::error::{Error, ErrorType};
use liquers_lib::ui::{
    register_delegate, unregister_delegate, DelegatedElement, ElementDelegate, UIElement, UIEvent,
    UIHandle,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::error::{js_error_to_liquers, liquers_error_to_js};
use crate::ui::{handle_to_js, LiquersUIElement};

// ---------------------------------------------------------------------------
// The delegate
// ---------------------------------------------------------------------------

/// A page's element definition: its functions, called on the page's thread.
pub struct JsElementDelegate {
    kind: String,
    render: js_sys::Function,
    on_event: Option<js_sys::Function>,
    mount: Option<js_sys::Function>,
    title: Option<String>,
}

impl JsElementDelegate {
    fn new(
        kind: String,
        render: js_sys::Function,
        on_event: Option<js_sys::Function>,
        mount: Option<js_sys::Function>,
        title: Option<String>,
    ) -> Self {
        let delegate = JsElementDelegate {
            kind,
            render,
            on_event,
            mount,
            title,
        };
        #[cfg(feature = "debug-handles")]
        crate::command::adapter::count_handles(delegate.function_count(), true);
        delegate
    }

    #[cfg(feature = "debug-handles")]
    fn function_count(&self) -> usize {
        1 + usize::from(self.on_event.is_some()) + usize::from(self.mount.is_some())
    }

    /// Reports `error` to `console.error`, as every page-function failure is, and returns it.
    fn report(&self, what: &str, error: Error) -> Error {
        web_sys::console::error_1(&JsValue::from_str(&format!(
            "liquers: the {what} function of element type \"{}\" failed: {}",
            self.kind, error.message
        )));
        error
    }
}

impl Drop for JsElementDelegate {
    fn drop(&mut self) {
        #[cfg(feature = "debug-handles")]
        crate::command::adapter::count_handles(self.function_count(), false);
    }
}

impl ElementDelegate for JsElementDelegate {
    fn render(&self, element: &DelegatedElement) -> Result<String, Error> {
        let props = crate::bridge::json_to_js(&element.props)?;
        let result = self
            .render
            .call1(&JsValue::NULL, &props)
            .map_err(|thrown| {
                self.report(
                    "render",
                    js_error_to_liquers(thrown, ErrorType::ExecutionError),
                )
            })?;
        match result.as_string() {
            Some(markup) => Ok(markup),
            None => Err(self.report(
                "render",
                Error::execution_error(format!(
                    "render returned {}, not a string of markup",
                    js_type_name(&result)
                )),
            )),
        }
    }

    fn on_event(
        &self,
        element: &DelegatedElement,
        event: &UIEvent,
    ) -> Result<Option<serde_json::Value>, Error> {
        let on_event = match &self.on_event {
            Some(f) => f,
            None => return Ok(None),
        };
        let props = crate::bridge::json_to_js(&element.props)?;
        let js_event = user_event_to_js(element.handle(), event)?;
        let result = {
            let _updating = UpdatingGuard::new(element.handle());
            on_event.call2(&JsValue::NULL, &props, &js_event)
        }
        .map_err(|thrown| {
            self.report(
                "onEvent",
                js_error_to_liquers(thrown, ErrorType::ExecutionError),
            )
        })?;
        if result.is_undefined() {
            return Ok(None);
        }
        js_to_json(&result)
            .map(Some)
            .map_err(|e| self.report("onEvent", e))
    }
}

/// `{ handle, kind, fields }`: `ui-events`' `UIEvent` plus the element it reached.
fn user_event_to_js(handle: Option<UIHandle>, event: &UIEvent) -> Result<JsValue, Error> {
    let object = js_sys::Object::new();
    let set = |target: &js_sys::Object, key: &str, value: &JsValue| {
        let _ = js_sys::Reflect::set(target, &JsValue::from_str(key), value);
    };
    set(
        &object,
        "handle",
        &match handle {
            Some(h) => JsValue::from_f64(handle_to_js(h)),
            None => JsValue::NULL,
        },
    );
    set(&object, "kind", &JsValue::from_str(event.kind.name()));
    let fields = js_sys::Object::new();
    for (name, value) in &event.fields {
        set(&fields, name, &crate::bridge::value_to_js(value)?);
    }
    set(&object, "fields", &fields.into());
    Ok(object.into())
}

// ---------------------------------------------------------------------------
// The element being updated
// ---------------------------------------------------------------------------

thread_local! {
    /// The handle whose `onEvent` is running. `show` must not replace it: the runner puts the
    /// element back after the handler, which would silently undo the replacement.
    static UPDATING: Cell<Option<UIHandle>> = const { Cell::new(None) };
}

/// The element whose `onEvent` is running, if any.
pub(crate) fn element_being_updated() -> Option<UIHandle> {
    UPDATING.with(|cell| cell.get())
}

/// Records the handle being updated for the duration of an `onEvent` call, restoring the previous
/// one (a nested event) when it ends, also when the call throws.
struct UpdatingGuard {
    previous: Option<UIHandle>,
}

impl UpdatingGuard {
    fn new(handle: Option<UIHandle>) -> Self {
        let previous = UPDATING.with(|cell| cell.replace(handle));
        UpdatingGuard { previous }
    }
}

impl Drop for UpdatingGuard {
    fn drop(&mut self) {
        UPDATING.with(|cell| cell.set(self.previous));
    }
}

// ---------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------

thread_local! {
    /// The page's definitions by type. The same `Rc` is registered with `liquers-lib`; this map
    /// keeps the concrete type, for the `mount` function and the default title.
    static DEFINITIONS: RefCell<HashMap<String, Rc<JsElementDelegate>>> =
        RefCell::new(HashMap::new());
}

/// The `mount` function of `kind`, if the type is defined and has one.
pub(crate) fn mount_function(kind: &str) -> Option<js_sys::Function> {
    DEFINITIONS.with(|d| d.borrow().get(kind).and_then(|def| def.mount.clone()))
}

/// Whether some definition has a `mount` function, so a mount's reconciliation can skip its DOM
/// query when none does.
pub(crate) fn any_mount_function() -> bool {
    DEFINITIONS.with(|d| d.borrow().values().any(|def| def.mount.is_some()))
}

fn optional_function(def: &JsValue, key: &str) -> Result<Option<js_sys::Function>, Error> {
    let value = js_sys::Reflect::get(def, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED);
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    value.dyn_into::<js_sys::Function>().map(Some).map_err(|v| {
        Error::general_error(format!(
            "defineElement `{key}` must be a function, not {}",
            js_type_name(&v)
        ))
    })
}

fn define_element_inner(def: JsValue) -> Result<(), Error> {
    if !def.is_object() {
        return Err(Error::general_error(
            "defineElement takes an object { type, render, onEvent?, mount?, title? }".to_string(),
        ));
    }
    let kind = js_sys::Reflect::get(&def, &JsValue::from_str("type"))
        .ok()
        .and_then(|v| v.as_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            Error::general_error("defineElement needs `type`, a non-empty string".to_string())
        })?;
    let render = optional_function(&def, "render")?.ok_or_else(|| {
        Error::general_error(format!(
            "defineElement \"{kind}\" needs `render`, a function"
        ))
    })?;
    let on_event = optional_function(&def, "onEvent")?;
    let mount = optional_function(&def, "mount")?;
    let title_value =
        js_sys::Reflect::get(&def, &JsValue::from_str("title")).unwrap_or(JsValue::UNDEFINED);
    let title = if title_value.is_undefined() || title_value.is_null() {
        None
    } else {
        Some(title_value.as_string().ok_or_else(|| {
            Error::general_error("defineElement `title` must be a string".to_string())
        })?)
    };

    let delegate = Rc::new(JsElementDelegate::new(
        kind.clone(),
        render,
        on_event,
        mount,
        title,
    ));
    let replaced = DEFINITIONS.with(|d| d.borrow_mut().insert(kind.clone(), delegate.clone()));
    let replaced_in_lib = register_delegate(&kind, delegate);
    // Dropped here, outside the borrows: the old functions are released.
    drop(replaced);
    drop(replaced_in_lib);
    // Instances of a redefined type, and restored ones of a newly defined type, re-render.
    crate::ui::invalidate_live_mounts();
    Ok(())
}

/// Defines a UI element type: `{ type, render, onEvent?, mount?, title? }`.
///
/// `render(props)` returns the element's markup. `onEvent(props, event)` returns the new props
/// (or `undefined` to keep them). `mount(node, props)` runs once the element's node is in the
/// page, and may return a cleanup function. Defining a type again replaces it; its instances
/// re-render.
#[wasm_bindgen(js_name = defineElement)]
pub fn define_element(
    #[wasm_bindgen(unchecked_param_type = "ElementDefinition")] def: JsValue,
) -> Result<(), JsValue> {
    define_element_inner(def).map_err(liquers_error_to_js)
}

/// Removes the definition of `type`, releasing its functions. Its instances render a placeholder
/// until it is defined again. Returns whether the type was defined.
#[wasm_bindgen(js_name = undefineElement)]
pub fn undefine_element(#[wasm_bindgen(js_name = "type")] kind: String) -> bool {
    let removed = DEFINITIONS.with(|d| d.borrow_mut().remove(&kind));
    let removed_in_lib = unregister_delegate(&kind);
    let was_defined = removed.is_some();
    drop(removed);
    drop(removed_in_lib);
    if was_defined {
        crate::ui::invalidate_live_mounts();
    }
    was_defined
}

fn element_inner(
    kind: String,
    props: JsValue,
    title: Option<String>,
) -> Result<LiquersUIElement, Error> {
    if kind.is_empty() {
        return Err(Error::general_error(
            "element needs a type, a non-empty string".to_string(),
        ));
    }
    let props = if props.is_undefined() || props.is_null() {
        serde_json::Value::Object(serde_json::Map::new())
    } else {
        js_to_json(&props)?
    };
    let title = title
        .or_else(|| DEFINITIONS.with(|d| d.borrow().get(&kind).and_then(|def| def.title.clone())));
    let mut element = DelegatedElement::new(kind, props);
    if let Some(title) = title {
        element = element.with_title(title);
    }
    let element: Arc<dyn UIElement> = Arc::new(element);
    Ok(LiquersUIElement::new(element))
}

/// A UI element of a defined `type` with `props` (JSON data). Return it from a command, or pass it
/// to anything that takes a UI element. The type need not be defined yet: until it is, the element
/// renders a placeholder.
#[wasm_bindgen(js_name = element)]
pub fn element(
    #[wasm_bindgen(js_name = "type")] kind: String,
    #[wasm_bindgen(unchecked_param_type = "any")] props: Option<JsValue>,
    title: Option<String>,
) -> Result<LiquersUIElement, JsValue> {
    element_inner(kind, props.unwrap_or(JsValue::UNDEFINED), title).map_err(liquers_error_to_js)
}

// ---------------------------------------------------------------------------
// JSON data
// ---------------------------------------------------------------------------

/// What `value` is, for an error message: `function`, `class instance (Date)`, `a cycle`…
fn js_type_name(value: &JsValue) -> String {
    if value.is_null() {
        return "null".to_string();
    }
    if js_sys::Array::is_array(value) {
        return "an array".to_string();
    }
    let type_of = value.js_typeof().as_string().unwrap_or_default();
    if type_of == "object" {
        let constructor = js_sys::Reflect::get(value, &JsValue::from_str("constructor"))
            .ok()
            .and_then(|c| js_sys::Reflect::get(&c, &JsValue::from_str("name")).ok())
            .and_then(|n| n.as_string())
            .unwrap_or_default();
        if constructor.is_empty() || constructor == "Object" {
            return "an object".to_string();
        }
        return format!("a {constructor} instance");
    }
    format!("a {type_of}")
}

fn not_json(what: &str, path: &str) -> Error {
    let at = if path.is_empty() {
        String::new()
    } else {
        format!(" at {path}")
    };
    Error::from_error(
        ErrorType::ConversionError,
        format!("element props must be JSON data; found {what}{at}"),
    )
}

/// Reads JSON data from JavaScript, refusing what JSON cannot hold: functions, symbols, bigints,
/// class instances (a `Date`, a `Map`, a handle) and cycles. `undefined` in an object is left out,
/// as `JSON.stringify` does.
pub(crate) fn js_to_json(value: &JsValue) -> Result<serde_json::Value, Error> {
    let plain = js_sys::Object::get_prototype_of(&js_sys::Object::new().into());
    let mut path: Vec<JsValue> = Vec::new();
    to_json(value, &plain.into(), &mut path, "")
}

fn to_json(
    value: &JsValue,
    plain: &JsValue,
    path: &mut Vec<JsValue>,
    at: &str,
) -> Result<serde_json::Value, Error> {
    if value.is_null() || value.is_undefined() {
        return Ok(serde_json::Value::Null);
    }
    if let Some(b) = value.as_bool() {
        return Ok(serde_json::Value::Bool(b));
    }
    if let Some(n) = value.as_f64() {
        if !n.is_finite() {
            return Err(not_json(&format!("the number {n}"), at));
        }
        if n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0 {
            return Ok(serde_json::Value::from(n as i64));
        }
        return serde_json::Number::from_f64(n)
            .map(serde_json::Value::Number)
            .ok_or_else(|| not_json(&format!("the number {n}"), at));
    }
    if let Some(s) = value.as_string() {
        return Ok(serde_json::Value::String(s));
    }
    if !value.is_object() {
        return Err(not_json(&js_type_name(value), at));
    }
    if path.iter().any(|seen| seen == value) {
        return Err(not_json("a cycle", at));
    }
    if js_sys::Array::is_array(value) {
        let array: &js_sys::Array = value.unchecked_ref();
        path.push(value.clone());
        let mut items = Vec::with_capacity(array.length() as usize);
        for (i, item) in array.iter().enumerate() {
            items.push(to_json(&item, plain, path, &format!("{at}[{i}]"))?);
        }
        path.pop();
        return Ok(serde_json::Value::Array(items));
    }
    let prototype: JsValue = js_sys::Object::get_prototype_of(value).into();
    if !(prototype.is_null() || &prototype == plain) {
        return Err(not_json(&js_type_name(value), at));
    }
    let object: &js_sys::Object = value.unchecked_ref();
    path.push(value.clone());
    let mut map = serde_json::Map::new();
    for key in js_sys::Object::keys(object).iter() {
        let name = key.as_string().unwrap_or_default();
        let item = js_sys::Reflect::get(value, &key).unwrap_or(JsValue::UNDEFINED);
        if item.is_undefined() {
            continue;
        }
        let item_at = if at.is_empty() {
            name.clone()
        } else {
            format!("{at}.{name}")
        };
        map.insert(name, to_json(&item, plain, path, &item_at)?);
    }
    path.pop();
    Ok(serde_json::Value::Object(map))
}
