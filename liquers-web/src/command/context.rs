//! The execution context a JavaScript command receives (`COMMAND07`).
//!
//! A command declares an argument named `context` (or names a parameter `context`), and `run`
//! receives a [`LiquersContext`] in that position. The class is concrete for JavaScript, but
//! [`call_js_command`](super::call_js_command) is generic over the environment, so the class holds
//! a type-erased [`ContextOps`]. It is valid only during its call: the adapter clears the slot when
//! `run` (or its promise) settles, and every method afterwards throws, as `lq.Context` does in
//! `liquers-py`.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use wasm_bindgen::prelude::*;

use crate::bridge::{value_to_js_in, JsValueBridge};
use crate::error::liquers_error_to_js;

/// A future that is not `Send`, as every future that touches JavaScript is on wasm.
pub(crate) type LocalBoxFuture<T> = Pin<Box<dyn Future<Output = T> + 'static>>;

/// What a payload tells a JavaScript command. Only the UI element handle is exposed.
pub trait JsPayload {
    /// The handle of the UI element that triggered the evaluation, if any.
    fn ui_handle(&self) -> Option<u64>;
}

impl JsPayload for () {
    fn ui_handle(&self) -> Option<u64> {
        None
    }
}

impl JsPayload for liquers_lib::ui::SimpleUIPayload {
    fn ui_handle(&self) -> Option<u64> {
        use liquers_lib::ui::UIPayload;
        self.handle().map(|h| h.0)
    }
}

/// The operations a JavaScript `Context` forwards to a core `Context<E>`, with `E` erased.
pub(crate) trait ContextOps {
    fn info(&self, message: &str) -> Result<(), Error>;
    fn warning(&self, message: &str) -> Result<(), Error>;
    fn error(&self, message: &str) -> Result<(), Error>;
    fn is_cancelled(&self) -> bool;
    fn element(&self) -> Option<u64>;
    fn evaluate(&self, query: String) -> LocalBoxFuture<Result<JsValue, Error>>;
}

impl<E> ContextOps for Context<E>
where
    E: Environment,
    E::Value: JsValueBridge,
    E::Payload: JsPayload,
{
    fn info(&self, message: &str) -> Result<(), Error> {
        Context::info(self, message)
    }
    fn warning(&self, message: &str) -> Result<(), Error> {
        Context::warning(self, message)
    }
    fn error(&self, message: &str) -> Result<(), Error> {
        Context::error(self, message)
    }
    fn is_cancelled(&self) -> bool {
        Context::is_cancelled(self)
    }
    fn element(&self) -> Option<u64> {
        self.get_payload_clone().and_then(|p| p.ui_handle())
    }
    fn evaluate(&self, query: String) -> LocalBoxFuture<Result<JsValue, Error>> {
        let context = self.clone();
        Box::pin(async move {
            let query = liquers_core::parse::parse_query(&query)?;
            let asset = Context::evaluate(&context, &query).await?;
            let state = context.wait_for_dependency(&asset).await?;
            let value = state.value()?;
            value_to_js_in(&context.get_envref(), value.as_ref())
        })
    }
}

/// The slot a [`LiquersContext`] reads through. Emptied when the call ends.
pub(crate) type ContextSlot = Rc<RefCell<Option<Rc<dyn ContextOps>>>>;

/// Owns a context slot for the duration of one command call and empties it on drop, on every
/// path out of the call — a returned value, a thrown error or a refused Promise.
pub(crate) struct ContextGuard {
    slot: ContextSlot,
}

impl ContextGuard {
    pub(crate) fn new(ops: Rc<dyn ContextOps>) -> (ContextGuard, LiquersContext) {
        let slot: ContextSlot = Rc::new(RefCell::new(Some(ops)));
        (ContextGuard { slot: slot.clone() }, LiquersContext { slot })
    }
}

impl Drop for ContextGuard {
    fn drop(&mut self) {
        self.slot.borrow_mut().take();
    }
}

const INVALID_CONTEXT: &str =
    "the context is used after its command returned; it is valid only during the call";

/// The execution context of a JavaScript command. Valid only during the call that received it.
#[wasm_bindgen(js_name = Context)]
pub struct LiquersContext {
    slot: ContextSlot,
}

impl LiquersContext {
    /// The live operations, cloned out of the slot so no borrow is held while they run.
    fn ops(&self) -> Result<Rc<dyn ContextOps>, Error> {
        self.slot
            .borrow()
            .clone()
            .ok_or_else(|| Error::general_error(INVALID_CONTEXT.to_string()))
    }

    fn log(&self, f: impl Fn(&dyn ContextOps) -> Result<(), Error>) -> Result<(), JsValue> {
        let ops = self.ops().map_err(liquers_error_to_js)?;
        f(ops.as_ref()).map_err(liquers_error_to_js)
    }
}

#[wasm_bindgen(js_class = Context)]
impl LiquersContext {
    /// Adds an info entry to the asset's log.
    pub fn info(&self, message: &str) -> Result<(), JsValue> {
        self.log(|ops| ops.info(message))
    }

    /// Adds a warning entry to the asset's log.
    pub fn warning(&self, message: &str) -> Result<(), JsValue> {
        self.log(|ops| ops.warning(message))
    }

    /// Adds an error entry to the asset's log. The evaluation fails once `run` returns.
    pub fn error(&self, message: &str) -> Result<(), JsValue> {
        self.log(|ops| ops.error(message))
    }

    /// Whether the evaluation was cancelled. Always `false` in the browser today.
    #[wasm_bindgen(js_name = isCancelled)]
    pub fn is_cancelled(&self) -> Result<bool, JsValue> {
        let ops = self.ops().map_err(liquers_error_to_js)?;
        Ok(ops.is_cancelled())
    }

    /// The handle of the UI element that triggered the command, or `null` outside a UI.
    ///
    /// A `number`, not a `bigint`: handles stay far below 2^53.
    #[wasm_bindgen(getter, unchecked_return_type = "number | null")]
    pub fn element(&self) -> Result<JsValue, JsValue> {
        let ops = self.ops().map_err(liquers_error_to_js)?;
        Ok(match ops.element() {
            Some(h) => JsValue::from_f64(h as f64),
            None => JsValue::NULL,
        })
    }

    /// Evaluates a query as a dependency of this command and resolves to its value.
    pub fn evaluate(&self, query: String) -> js_sys::Promise {
        let ops = self.ops();
        wasm_bindgen_futures::future_to_promise(async move {
            let ops = ops.map_err(liquers_error_to_js)?;
            ops.evaluate(query).await.map_err(liquers_error_to_js)
        })
    }
}
