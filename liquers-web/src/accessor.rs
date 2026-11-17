//! `Accessor`: a settable value accessor from JavaScript, over the same Rust accessor.
//!
//! An accessor is a live handle on the place a value lives (a stored key, a keyed asset, a part of
//! a document or of a recipe's query). `liquers.accessor(query)` evaluates the query and wraps its
//! accessor (any other value's asset is wrapped read-only), and `liquers.memoryAccessor(value)`
//! makes a shared variable. An accessor value returned by a command crosses as an `Accessor`
//! handle, and a handle passed back crosses as the *same* Rust accessor, never a copy.
//!
//! Every method that reaches the accessor returns a `Promise`, because the trait is async. Each
//! handle keeps the token it last read or wrote, and its `set` is a compare-and-set on it; a
//! conflict rejects with a `LiquersError` whose `errorType` is `"status_conflict"` and whose
//! `token` is the current one. `subscribe(callback)` calls back from a `spawn_local` task with the
//! WebSocket's event shape (`{event: "changed", etag}`), never while a `RefCell` is borrowed.
//!
//! # Main components
//!
//! - [`LiquersAccessor`] (`Accessor`): `get`, `snapshot`, `set`, `setIf`, `info`, `log`,
//!   `describe`, `refresh`, `project`, `subscribe`.
//! - [`LiquersAccessorSnapshot`] (`AccessorSnapshot`): `value()`, `state`, `token`, `rootVersion`.
//! - [`LiquersAccessorSubscription`] (`AccessorSubscription`): `dispose()`.
//! - [`accessor_value`]: a live handle read back as its Rust accessor (`default_value.rs`).
//!
//! # Where it is used
//!
//! - `lib.rs` (`accessor`, `memoryAccessor`) and `default_value.rs` (the value bridge).
//! - `tests/accessor_ACCESSOR.rs`.
//!
//! # Related concepts
//!
//! - `liquers_core::accessor`: the contract every method calls.
//! - `ui.rs`: the handle registry and callback rules this module follows.
//!
//! # Examples
//!
//! ```javascript
//! const acc = await liquers.accessor("-R-key/config.yaml/-/ns-acc/accessor/field-title");
//! const sub = acc.subscribe(e => seen.push(e.event));
//! const { token } = await acc.snapshot();
//! await acc.setIf("Sales", token);          // seen: ["changed"] once the task has run
//! sub.dispose();
//! ```

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use futures::future::{select, Either};
use liquers_core::accessor::{
    memory_accessor, resolve_accessor, AccessorEvent, Fingerprint, PartDescription, SetOptions,
    ValueAccessor,
};
use liquers_core::context::EnvRef;
use liquers_core::error::{Error, ErrorType};
use liquers_core::metadata::Metadata;
use liquers_core::query::Query;
use liquers_core::state::State;
use wasm_bindgen::prelude::*;

use crate::asset::{status_name, LiquersState};
use crate::bridge::{js_to_value, serialize_to_js, ConversionPolicy};
use crate::default_value::WebValue;
use crate::environment::{shared_env, WebEnvironment};
use crate::error::liquers_error_to_js;
use crate::ui::is_handle_instance;

type Accessor = Arc<dyn ValueAccessor<WebValue>>;

thread_local! {
    /// Live accessor handles by id, as for UI elements: a wasm-bindgen handle cannot be borrowed
    /// back from a `JsValue`, so each carries an id (`__liquersAccessorHandle`).
    static ACCESSORS: RefCell<HashMap<u32, Accessor>> = RefCell::new(HashMap::new());
    static NEXT_ACCESSOR_ID: Cell<u32> = const { Cell::new(1) };
}

/// The accessor `js` stands for, when it is a live `Accessor` handle; `None` for anything else.
pub(crate) fn accessor_value(js: &JsValue) -> Result<Option<Accessor>, Error> {
    let marker = JsValue::from_str("__liquersAccessorHandle");
    if !is_handle_instance(js, &marker) {
        return Ok(None);
    }
    let freed = || Error::conversion_error("a freed Accessor handle", "a live Accessor handle");
    let id = js_sys::Reflect::get(js, &marker)
        .ok()
        .and_then(|id| id.as_f64())
        .ok_or_else(freed)?;
    ACCESSORS
        .with(|accessors| accessors.borrow().get(&(id as u32)).cloned())
        .map(Some)
        .ok_or_else(freed)
}

fn conversion(e: impl std::fmt::Display, what: &str) -> Error {
    Error::from_error(ErrorType::ConversionError, format!("Could not convert {what}: {e}"))
}

fn to_js<T: serde::Serialize>(value: &T, what: &str) -> Result<JsValue, JsValue> {
    serialize_to_js(value).map_err(|e| liquers_error_to_js(conversion(e, what)))
}

fn set(object: &js_sys::Object, key: &str, value: &JsValue) {
    let _ = js_sys::Reflect::set(object, &JsValue::from_str(key), value);
}

fn optional_string(value: Option<String>) -> JsValue {
    value.map(|s| JsValue::from_str(&s)).unwrap_or(JsValue::NULL)
}

/// An accessor handle, as JavaScript sees it.
#[wasm_bindgen(js_name = Accessor)]
pub struct LiquersAccessor {
    inner: Accessor,
    /// The environment it came from, for `project`; the global one when unknown.
    env: Option<EnvRef<WebEnvironment>>,
    /// The token this handle last read or wrote.
    token: Rc<Cell<Option<Fingerprint>>>,
    id: u32,
}

impl LiquersAccessor {
    /// A handle over `inner`. Spawns the accessor's driver, when it has one no handle has taken.
    pub fn new(inner: Accessor, env: Option<EnvRef<WebEnvironment>>) -> Self {
        if let Some(driver) = inner.driver() {
            wasm_bindgen_futures::spawn_local(driver);
        }
        let id = NEXT_ACCESSOR_ID.with(|next| {
            let id = next.get();
            next.set(id.checked_add(1).unwrap_or(1));
            id
        });
        ACCESSORS.with(|accessors| accessors.borrow_mut().insert(id, inner.clone()));
        LiquersAccessor {
            inner,
            env,
            token: Rc::new(Cell::new(None)),
            id,
        }
    }

    fn environment(&self) -> Result<EnvRef<WebEnvironment>, Error> {
        match &self.env {
            Some(env) => Ok(env.clone()),
            None => shared_env(),
        }
    }

    fn set_promise(&self, value: JsValue, expected: Option<Option<Fingerprint>>) -> js_sys::Promise {
        let value: WebValue = match js_to_value(&value, ConversionPolicy::Structural) {
            Ok(value) => value,
            Err(e) => return js_sys::Promise::reject(&liquers_error_to_js(e)),
        };
        let inner = self.inner.clone();
        let token = self.token.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            let result = match expected {
                Some(expected) => inner.set_if(value, expected, SetOptions::default()).await,
                None => inner.set(value, SetOptions::default()).await,
            };
            match result {
                Ok(snapshot) => {
                    token.set(Some(snapshot.fingerprint));
                    Ok(JsValue::from_str(&snapshot.fingerprint.token()))
                }
                Err(e) => Err(liquers_error_to_js(e)),
            }
        })
    }
}

impl Drop for LiquersAccessor {
    fn drop(&mut self) {
        let removed = ACCESSORS.with(|accessors| accessors.borrow_mut().remove(&self.id));
        drop(removed);
    }
}

/// `info()`'s plain object.
fn info_to_js(info: &liquers_core::accessor::AccessorInfo) -> Result<JsValue, JsValue> {
    let object = js_sys::Object::new();
    set(&object, "status", &JsValue::from_str(status_name(info.status)));
    set(&object, "message", &JsValue::from_str(&info.message));
    set(&object, "primaryProgress", &to_js(&info.primary_progress, "progress")?);
    set(&object, "secondaryProgress", &to_js(&info.secondary_progress, "progress")?);
    set(&object, "logLen", &JsValue::from_f64(info.log_len as f64));
    set(&object, "token", &optional_string(info.fingerprint.map(|f| f.token())));
    Ok(object.into())
}

/// `describe()`'s plain object.
fn part_to_js(part: &PartDescription) -> Result<JsValue, JsValue> {
    let object = js_sys::Object::new();
    set(&object, "step", &JsValue::from_str(&part.step));
    set(&object, "typeIdentifier", &JsValue::from_str(&part.type_identifier));
    set(&object, "argument", &to_js(&part.argument, "an argument")?);
    set(
        &object,
        "command",
        &optional_string(part.command.as_ref().map(|c| c.to_string())),
    );
    set(&object, "overridden", &JsValue::from_bool(part.overridden));
    set(&object, "readOnly", &JsValue::from_bool(part.read_only));
    Ok(object.into())
}

/// An event in the WebSocket's shape (`{event: "changed", etag: …}`), with a status named as
/// `LiquersAsset.status` names it (`"source"`), not as the WebSocket's JSON does.
fn event_to_js(event: &AccessorEvent) -> JsValue {
    let js = crate::bridge::json_to_js(&event.to_json()).unwrap_or(JsValue::NULL);
    match event {
        AccessorEvent::Status(status) if js.is_object() => {
            let _ = js_sys::Reflect::set(
                &js,
                &JsValue::from_str("status"),
                &JsValue::from_str(status_name(*status)),
            );
        }
        AccessorEvent::Status(_)
        | AccessorEvent::Initial
        | AccessorEvent::PrimaryProgress(_)
        | AccessorEvent::SecondaryProgress(_)
        | AccessorEvent::Log(_)
        | AccessorEvent::Changed(_)
        | AccessorEvent::Invalidated
        | AccessorEvent::Removed => {}
    }
    js
}

#[wasm_bindgen(js_class = Accessor)]
impl LiquersAccessor {
    /// The query that names this accessor (empty for one made in code).
    #[wasm_bindgen(getter)]
    pub fn address(&self) -> String {
        self.inner.address().encode()
    }

    /// `{ write, notify }`: whether it can be written, and whether its source notifies changes.
    #[wasm_bindgen(getter, unchecked_return_type = "AccessorCapabilities")]
    pub fn capabilities(&self) -> JsValue {
        let capabilities = self.inner.capabilities();
        let object = js_sys::Object::new();
        set(&object, "write", &JsValue::from_bool(capabilities.write));
        set(&object, "notify", &JsValue::from_bool(capabilities.notify));
        object.into()
    }

    /// Resolves with the current value, converted as `evaluate` converts it.
    #[wasm_bindgen(unchecked_return_type = "Promise<any>")]
    pub fn get(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        let token = self.token.clone();
        let env = self.env.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            let snapshot = inner.get().await.map_err(liquers_error_to_js)?;
            token.set(Some(snapshot.fingerprint));
            let value = snapshot.state.value().map_err(liquers_error_to_js)?;
            match &env {
                Some(env) => crate::bridge::value_to_js_in(env, value.as_ref()),
                None => crate::bridge::value_to_js(value.as_ref()),
            }
            .map_err(liquers_error_to_js)
        })
    }

    /// Resolves with an `AccessorSnapshot`: the value, its state and its token.
    #[wasm_bindgen(unchecked_return_type = "Promise<AccessorSnapshot>")]
    pub fn snapshot(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        let token = self.token.clone();
        let env = self.env.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            let snapshot = inner.get().await.map_err(liquers_error_to_js)?;
            token.set(Some(snapshot.fingerprint));
            let state = LiquersState::from_state(snapshot.state);
            let state = match env {
                Some(env) => state.in_env(env),
                None => state,
            };
            Ok(LiquersAccessorSnapshot {
                state: Some(state),
                token: snapshot.fingerprint.token(),
                root_version: snapshot.root_version.to_string(),
            }
            .into())
        })
    }

    /// Sets the value, checked against the token this handle last saw; resolves with the new
    /// token.
    #[wasm_bindgen(unchecked_return_type = "Promise<string>")]
    pub fn set(&self, value: JsValue) -> js_sys::Promise {
        let expected = self.token.get().map(Some);
        self.set_promise(value, expected)
    }

    /// Sets the value if its token is `token`; `null` creates a value that must not exist yet.
    /// Resolves with the new token; a mismatch rejects with `errorType === "status_conflict"`.
    #[wasm_bindgen(js_name = setIf, unchecked_return_type = "Promise<string>")]
    pub fn set_if(&self, value: JsValue, token: Option<String>) -> js_sys::Promise {
        let expected = match token.as_deref().map(Fingerprint::from_token).transpose() {
            Ok(expected) => expected,
            Err(e) => return js_sys::Promise::reject(&liquers_error_to_js(e)),
        };
        self.set_promise(value, Some(expected))
    }

    /// Resolves with `{ status, message, primaryProgress, secondaryProgress, logLen, token }`,
    /// without reading the value.
    #[wasm_bindgen(unchecked_return_type = "Promise<AccessorInfo>")]
    pub fn info(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            let info = inner.info().await.map_err(liquers_error_to_js)?;
            info_to_js(&info)
        })
    }

    /// Resolves with the log entries from index `since` (default 0).
    #[wasm_bindgen(unchecked_return_type = "Promise<any[]>")]
    pub fn log(&self, since: Option<u32>) -> js_sys::Promise {
        let inner = self.inner.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            let log = inner
                .log(since.unwrap_or(0) as usize)
                .await
                .map_err(liquers_error_to_js)?;
            to_js(&log, "the log")
        })
    }

    /// Resolves with what the part is: `{ step, typeIdentifier, argument, command, overridden,
    /// readOnly }`.
    #[wasm_bindgen(unchecked_return_type = "Promise<PartDescription>")]
    pub fn describe(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            let part = inner.describe().await.map_err(liquers_error_to_js)?;
            part_to_js(&part)
        })
    }

    /// Pulls the source's pending changes; this is how events arrive from a source that cannot
    /// notify (the store).
    #[wasm_bindgen(unchecked_return_type = "Promise<void>")]
    pub fn refresh(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            inner.refresh().await.map_err(liquers_error_to_js)?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Resolves with the accessor of a part: this accessor's address with `actions` appended
    /// (`"field-title"`, `"recipe-0/query"`), evaluated in its environment.
    #[wasm_bindgen(unchecked_return_type = "Promise<Accessor>")]
    pub fn project(&self, actions: &str) -> js_sys::Promise {
        let prepared = (|| -> Result<(EnvRef<WebEnvironment>, Query), Error> {
            let base = self.inner.address().encode();
            if base.is_empty() {
                return Err(Error::general_error(
                    "an accessor with no address cannot be projected by query".to_string(),
                ));
            }
            let query = liquers_core::parse::parse_query(&format!(
                "{base}/{}",
                actions.trim_matches('/')
            ))?;
            Ok((self.environment()?, query))
        })();
        let (envref, query) = match prepared {
            Ok(prepared) => prepared,
            Err(e) => return js_sys::Promise::reject(&liquers_error_to_js(e)),
        };
        wasm_bindgen_futures::future_to_promise(async move {
            let inner = resolve_accessor(&envref, &query)
                .await
                .map_err(liquers_error_to_js)?;
            Ok(LiquersAccessor::new(inner, Some(envref)).into())
        })
    }

    /// Calls `callback(event)` for every event from now on, from a task of its own; returns the
    /// `AccessorSubscription` whose `dispose()` stops it.
    pub fn subscribe(
        &self,
        #[wasm_bindgen(unchecked_param_type = "(event: AccessorEventObject) => void")]
        callback: js_sys::Function,
    ) -> LiquersAccessorSubscription {
        let (stop_tx, stop_rx) = futures::channel::oneshot::channel::<()>();
        let mut events = self.inner.subscribe();
        events.borrow_and_update();
        wasm_bindgen_futures::spawn_local(async move {
            let mut stop = stop_rx;
            loop {
                let changed = {
                    let changed = Box::pin(events.changed());
                    match select(changed, &mut stop).await {
                        Either::Left((Ok(()), _)) => true,
                        Either::Left((Err(_), _)) | Either::Right(_) => false,
                    }
                };
                match changed {
                    true => {
                        let event = events.borrow_and_update().clone();
                        let js_event = event_to_js(&event);
                        if let Err(thrown) = callback.call1(&JsValue::NULL, &js_event) {
                            web_sys::console::error_2(
                                &JsValue::from_str("liquers: an accessor subscriber threw"),
                                &thrown,
                            );
                        }
                    }
                    false => break,
                }
            }
        });
        LiquersAccessorSubscription {
            stop: RefCell::new(Some(stop_tx)),
        }
    }

    /// The registry id that lets this handle cross back into Liquers as the same accessor. Not
    /// for pages to use; its presence is how the bridge recognizes an accessor handle.
    #[wasm_bindgen(getter, js_name = __liquersAccessorHandle)]
    pub fn handle_id(&self) -> u32 {
        self.id
    }
}

/// A read: the value, its state and its token.
#[wasm_bindgen(js_name = AccessorSnapshot)]
pub struct LiquersAccessorSnapshot {
    state: Option<LiquersState>,
    token: String,
    root_version: String,
}

#[wasm_bindgen(js_class = AccessorSnapshot)]
impl LiquersAccessorSnapshot {
    /// The value, converted as `evaluate` converts it.
    #[wasm_bindgen(unchecked_return_type = "any")]
    pub fn value(&self) -> Result<JsValue, JsValue> {
        match &self.state {
            Some(state) => state.to_js(),
            None => Ok(JsValue::NULL),
        }
    }

    /// The part's `State` (its metadata carries the root's status). Each read takes it: a second
    /// read is `undefined`.
    #[wasm_bindgen(getter)]
    pub fn state(&mut self) -> Option<LiquersState> {
        self.state.take()
    }

    /// The token, for `setIf`.
    #[wasm_bindgen(getter)]
    pub fn token(&self) -> String {
        self.token.clone()
    }

    /// The version of the root value the part was read from.
    #[wasm_bindgen(getter, js_name = rootVersion)]
    pub fn root_version(&self) -> String {
        self.root_version.clone()
    }
}

/// A callback registered with `Accessor.subscribe`.
#[wasm_bindgen(js_name = AccessorSubscription)]
pub struct LiquersAccessorSubscription {
    stop: RefCell<Option<futures::channel::oneshot::Sender<()>>>,
}

#[wasm_bindgen(js_class = AccessorSubscription)]
impl LiquersAccessorSubscription {
    /// Stops the callback; a second call does nothing.
    pub fn dispose(&self) {
        let stop = self.stop.borrow_mut().take();
        if let Some(stop) = stop {
            let _ = stop.send(());
        }
    }
}

/// `liquers.accessor` and `Environment.accessor`: the accessor a query names.
pub fn accessor_promise(envref: EnvRef<WebEnvironment>, query: &str) -> js_sys::Promise {
    let parsed = match liquers_core::parse::parse_query(query) {
        Ok(q) => q,
        Err(e) => return js_sys::Promise::reject(&liquers_error_to_js(e)),
    };
    wasm_bindgen_futures::future_to_promise(async move {
        let inner = resolve_accessor(&envref, &parsed)
            .await
            .map_err(liquers_error_to_js)?;
        Ok(LiquersAccessor::new(inner, Some(envref)).into())
    })
}

/// `liquers.memoryAccessor`: a new variable holding `value` (none when omitted), named `address`.
pub fn memory_accessor_handle(
    envref: Option<EnvRef<WebEnvironment>>,
    value: Option<JsValue>,
    address: Option<String>,
) -> Result<LiquersAccessor, JsValue> {
    let state = match value {
        Some(value) if !value.is_undefined() => {
            let value: WebValue =
                js_to_value(&value, ConversionPolicy::Structural).map_err(liquers_error_to_js)?;
            Some(State::from_value_and_metadata(value, Arc::new(Metadata::new())))
        }
        Some(_) | None => None,
    };
    let address = match address {
        Some(address) => liquers_core::parse::parse_query(&address).map_err(liquers_error_to_js)?,
        None => Query::new(),
    };
    Ok(LiquersAccessor::new(memory_accessor(state, address), envref))
}
