//! The execution context a JavaScript command receives (`COMMAND07`).
//!
//! A command declares an argument named `context` (or names a parameter `context`), and `run`
//! receives a [`LiquersContext`] in that position. The class is concrete for JavaScript, but
//! [`call_js_command`](super::call_js_command) is generic over the environment, so the class holds
//! a type-erased [`ContextOps`]. It is valid only during its call: the adapter clears the slot when
//! `run` (or its promise) settles, and every method afterwards throws, as `lq.Context` does in
//! `liquers-py`.
//!
//! # Main components
//!
//! - [`LiquersContext`] (`Context`): `info`, `warning`, `error`, `evaluate`, `isCancelled`,
//!   `element`, and the template calls `render`, `expand`, `validate` (`TEMPLATE06`, `10`, `11`).
//! - `ContextOps`: the type-erased operations behind it; `as_any` lets the template calls recover
//!   the concrete `Context<WebEnvironment>` that `crate::template` renders through.
//! - [`JsPayload`]: what a payload tells a command (the UI element handle).
//!
//! # Where it is used
//!
//! Built by `crate::command::adapter::call_js_command` for a command whose declaration places a
//! `context`.
//!
//! # Related concepts
//!
//! `liquers_core::context::Context`; `specs/reference/JAVASCRIPT_INTEGRATION.md` §4 "Context";
//! `COMMAND07` in `tests/commands_COMMAND.rs`.
//!
//! # Examples
//!
//! Run by `template06_in_command_render_records_dependencies` in `tests/template_TEMPLATE.rs`:
//!
//! ```javascript
//! liquers.registerCommand({
//!   name: "report", async: true,
//!   run: async (context) => context.render(liquers.Query.parse("-R/templates/count.md")),
//! });
//! ```

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
    /// The context itself, for the template calls, which need the concrete value type
    /// (`crate::template` renders through `liquers_lib`'s `ContextHost`).
    fn as_any(&self) -> &dyn std::any::Any;
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
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// What a template call inside a command needs: the command's own context, with the web
/// environment's value type. A downstream environment with another value type gets an error
/// naming the reason, rather than a render that silently bypasses the command's asset.
fn web_context(ops: &Rc<dyn ContextOps>) -> Result<Context<crate::environment::WebEnvironment>, Error> {
    ops.as_any()
        .downcast_ref::<Context<crate::environment::WebEnvironment>>()
        .cloned()
        .ok_or_else(|| {
            Error::from_error(
                liquers_core::error::ErrorType::NotSupported,
                "Templates render inside a command only in the default web environment, whose \
                 value type ContextHost reads"
                    .to_string(),
            )
        })
}

/// A template call inside a command: the command's context, the parsed arguments, then `run`.
fn template_call<F, Fut>(slot: Result<Rc<dyn ContextOps>, Error>, call: F) -> js_sys::Promise
where
    F: FnOnce(Context<crate::environment::WebEnvironment>) -> Fut + 'static,
    Fut: Future<Output = Result<JsValue, Error>> + 'static,
{
    let context = slot.and_then(|ops| web_context(&ops));
    wasm_bindgen_futures::future_to_promise(async move {
        let context = context.map_err(liquers_error_to_js)?;
        call(context).await.map_err(liquers_error_to_js)
    })
}

/// Reads the template arguments shared by `render` and `expand`. `full` is refused: inside a
/// command, recovered errors are warnings in the command's own log.
fn command_options(options: &JsValue) -> Result<crate::template::TemplateOptions, Error> {
    let options = crate::template::options_from_js(options)?;
    if options.full {
        return Err(Error::from_error(
            liquers_core::error::ErrorType::ParameterError,
            "Template options: `full` is for top-level calls; inside a command the warnings are \
             in the command's log"
                .to_string(),
        ));
    }
    Ok(options)
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

    /// Renders a template inside this command (`TEMPLATE06`): `state` is the bottom frame and
    /// `.`, `data` (an object) names above it. Its queries and partials become dependencies of
    /// the command's result; recovered errors are warnings in the command's log.
    #[wasm_bindgen(unchecked_return_type = "Promise<string>")]
    pub fn render(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string | Query")] template: JsValue,
        state: Option<JsValue>,
        #[wasm_bindgen(unchecked_optional_param_type = "Record<string, unknown> | null")]
        data: Option<JsValue>,
        #[wasm_bindgen(unchecked_optional_param_type = "LiquersTemplateOptions")] options: Option<
            JsValue,
        >,
    ) -> js_sys::Promise {
        let parsed = (|| {
            Ok::<_, Error>((
                crate::template::source_from_js(&template)?,
                crate::template::value_from_js(&state.unwrap_or(JsValue::UNDEFINED))?,
                crate::template::value_from_js(&data.unwrap_or(JsValue::UNDEFINED))?,
                command_options(&options.unwrap_or(JsValue::UNDEFINED))?,
            ))
        })();
        template_call(self.ops(), move |context| async move {
            let (source, state, data, options) = parsed?;
            let frames = crate::template::Frames::Render { state, data };
            let text = crate::template::render_in(&context, &source, frames, &options).await?;
            Ok(JsValue::from_str(&text))
        })
    }

    /// Renders a template whose only frame (and `.`) is `data`, inside this command.
    #[wasm_bindgen(unchecked_return_type = "Promise<string>")]
    pub fn expand(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string | Query")] template: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "Record<string, unknown> | null")]
        data: Option<JsValue>,
        #[wasm_bindgen(unchecked_optional_param_type = "LiquersTemplateOptions")] options: Option<
            JsValue,
        >,
    ) -> js_sys::Promise {
        let parsed = (|| {
            Ok::<_, Error>((
                crate::template::source_from_js(&template)?,
                crate::template::value_from_js(&data.unwrap_or(JsValue::UNDEFINED))?,
                command_options(&options.unwrap_or(JsValue::UNDEFINED))?,
            ))
        })();
        template_call(self.ops(), move |context| async move {
            let (source, data, options) = parsed?;
            let frames = crate::template::Frames::Expand { data };
            let text = crate::template::render_in(&context, &source, frames, &options).await?;
            Ok(JsValue::from_str(&text))
        })
    }

    /// A template's positioned diagnostics; runs nothing.
    #[wasm_bindgen(unchecked_return_type = "Promise<LiquersTemplateDiagnostic[]>")]
    pub fn validate(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string | Query")] template: JsValue,
    ) -> js_sys::Promise {
        let source = crate::template::source_from_js(&template);
        template_call(self.ops(), move |context| async move {
            crate::template::validate(&context, &source?).await
        })
    }
}
