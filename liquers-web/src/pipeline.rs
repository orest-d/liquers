//! `PIPELINE` — pipelines from JavaScript: evaluate, convert, edit.
//!
//! A pipeline is the structured form of a query (`specs/reference/PIPELINES.md`). In JavaScript
//! it is a **plain object or array, its JSON form** — there is no `Pipeline` class. That form is
//! the serialized contract core already reads and writes ([`Pipeline::from_json`] /
//! [`Pipeline::to_json`]), it keeps argument values as JSON (a map argument stays a map), and a
//! page builds or inspects it with ordinary object literals. Representability, escaping and
//! argument checks are core's; nothing here reimplements them.
//!
//! # Main components
//!
//! - [`evaluate_pipeline`] (`liquers.evaluatePipeline`, `Environment.evaluatePipeline`) —
//!   evaluates a pipeline as `evaluate` evaluates a query: the same result conversion and the
//!   same structured `LiquersError` on rejection (`PIPELINE01`, `PIPELINE02`).
//! - [`pipeline_to_query`] / [`query_to_pipeline`] (`liquers.pipelineToQuery`,
//!   `liquers.queryToPipeline`) — the two conversions. A pipeline with no query form is refused
//!   with a `parameter_error` naming the representability rule (`PIPELINE03`).
//! - [`LiquersPipelineEditor`] / [`LiquersQueryEditor`] (`PipelineEditor`, `QueryEditor`) —
//!   wrappers of core's editors, checking every edit against a snapshot of the environment's
//!   command metadata and refusing a bad edit with the pipeline unchanged (`PIPELINE05`).
//! - Pipeline *commands* are not here: they are a declaration with `pipeline` in place of `run`,
//!   registered by `registerCommands` (`crate::command::spec`, `PIPELINE04`).
//!
//! # Where it is used
//!
//! The module-level functions in `lib.rs` and the `Environment` methods in
//! [`crate::environment`]; [`pipeline_from_js`] also reads a declaration's `pipeline`.
//!
//! # Related concepts
//!
//! JavaScript values become JSON by the value bridge's structural rules
//! (`crate::bridge::js_value_to_json`): an integral number inside ±2^53 is a JSON integer, and a
//! `Map`, a class instance or an opaque value is refused, so a placeholder value or argument can
//! never be a live JavaScript object. A pipeline goes back to JavaScript as plain objects
//! (`crate::bridge::json_to_js`), and a command's `Value::Pipeline` result reaches a page the
//! same way. The guide section is `LANGUAGE-INTEGRATION_GUIDE.md` §PIPELINE.
//!
//! # Examples
//!
//! Run by `pipeline01_evaluating_a_pipeline_equals_its_query` in `tests/pipeline_PIPELINE.rs`:
//!
//! ```javascript
//! await liquers.init();
//! liquers.registerCommand({ name: "hello", run: () => "hello" });
//! const p = liquers.queryToPipeline("hello/to_text");
//! // { elements: [{ action: "hello", pos: {…} }, { action: "to_text", pos: {…} }] }
//! const same = await liquers.evaluatePipeline(p) === await liquers.evaluate("hello/to_text");
//! liquers.pipelineToQuery(p);                       // "hello/to_text"
//!
//! const editor = new liquers.PipelineEditor();
//! editor.action("hello");
//! editor.action("no_such_command");                 // throws; editor.pipeline() is unchanged
//! ```

use std::sync::Arc;

use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::{Error, ErrorType};
use liquers_core::pipeline::{
    ActionEditor, ArgRef, Pipeline, PipelineEditor, PipelineElement, QueryEditor, Read,
};
use wasm_bindgen::prelude::*;

use crate::bridge::{js_value_to_json, json_to_js, value_to_js_in, JsValueBridge};
use crate::error::liquers_error_to_js;

/// Reads a pipeline from its JSON form: an object with `elements`, or a bare array of elements.
///
/// A string is refused rather than parsed as a query, so a page cannot pass query text where a
/// pipeline is expected and have it silently accepted; `queryToPipeline` is the explicit way.
pub fn pipeline_from_js(js: &JsValue) -> Result<Pipeline, Error> {
    if js.as_string().is_some() {
        return Err(Error::from_error(
            ErrorType::ParameterError,
            "A pipeline is an object or an array of elements, not a string; use \
             queryToPipeline() to convert query text"
                .to_string(),
        ));
    }
    Pipeline::from_json(&js_value_to_json(js)?)
}

/// A pipeline's JSON form, as plain JavaScript objects.
pub fn pipeline_to_js(pipeline: &Pipeline) -> Result<JsValue, Error> {
    json_to_js(&pipeline.to_json())
}

/// One element, read through the pipeline reader so its forms and errors are core's.
fn element_from_js(js: &JsValue) -> Result<PipelineElement, Error> {
    let json = js_value_to_json(js)?;
    let mut pipeline = Pipeline::from_json(&serde_json::Value::Array(vec![json]))?;
    pipeline
        .elements
        .pop()
        .ok_or_else(|| Error::general_error("An element was read as no element".to_string()))
}

/// Evaluates a pipeline to a JavaScript value, as [`crate::eval::evaluate_query`] does a query.
///
/// The value crosses inside `envref`, so a handle among the result resolves through the
/// environment that produced it.
pub async fn evaluate_pipeline<E>(envref: EnvRef<E>, pipeline: Pipeline) -> Result<JsValue, Error>
where
    E: Environment,
    E::Value: JsValueBridge,
{
    let state =
        liquers_core::interpreter::evaluate_pipeline(envref.clone(), &pipeline, None).await?;
    let value = state.value()?;
    value_to_js_in(&envref, value.as_ref())
}

/// [`evaluate_pipeline`] as a `Promise` that rejects with a `LiquersError`.
pub fn evaluate_pipeline_to_promise<E>(envref: EnvRef<E>, pipeline: &JsValue) -> js_sys::Promise
where
    E: Environment,
    E::Value: JsValueBridge,
{
    let pipeline = match pipeline_from_js(pipeline) {
        Ok(p) => p,
        Err(e) => return js_sys::Promise::reject(&liquers_error_to_js(e)),
    };
    wasm_bindgen_futures::future_to_promise(async move {
        evaluate_pipeline(envref, pipeline)
            .await
            .map_err(liquers_error_to_js)
    })
}

/// A pipeline's query text, with named arguments placed by `cmr`.
pub fn pipeline_to_query(
    pipeline: &JsValue,
    cmr: &CommandMetadataRegistry,
) -> Result<String, Error> {
    Ok(pipeline_from_js(pipeline)?.to_query_with(cmr)?.encode())
}

/// Query text as a pipeline's JSON form. Needs no environment.
pub fn query_to_pipeline(query: &str) -> Result<JsValue, Error> {
    pipeline_to_js(&Pipeline::from_query_text(query)?)
}

/// An argument named by position (a number) or by name (a string).
fn arg_ref(js: &JsValue) -> Result<ArgRef, Error> {
    if let Some(name) = js.as_string() {
        return Ok(ArgRef::Name(name));
    }
    match js.as_f64() {
        Some(n) if n >= 0.0 && n.fract() == 0.0 => Ok(ArgRef::Position(n as usize)),
        Some(_) | None => Err(Error::from_error(
            ErrorType::ParameterError,
            "An argument is named by its name (a string) or its position (a non-negative \
             integer)"
                .to_string(),
        )),
    }
}

/// Namespaces from a string or an array of strings.
fn namespaces(js: &JsValue) -> Result<Vec<String>, Error> {
    if let Some(ns) = js.as_string() {
        return Ok(vec![ns]);
    }
    if js_sys::Array::is_array(js) {
        return js_sys::Array::from(js)
            .iter()
            .map(|ns| {
                ns.as_string().ok_or_else(|| {
                    Error::from_error(
                        ErrorType::ParameterError,
                        "A namespace is a string".to_string(),
                    )
                })
            })
            .collect();
    }
    Err(Error::from_error(
        ErrorType::ParameterError,
        "Namespaces are a string or an array of strings".to_string(),
    ))
}

fn read_mode(read: Option<String>) -> Result<Read, Error> {
    match read {
        None => Ok(Read::Asset),
        Some(text) => Read::from_instruction(&text).ok_or_else(|| {
            Error::from_error(
                ErrorType::ParameterError,
                format!(
                    "Unknown resource read {text:?}; expected asset, binary, metadata, recipe, \
                     directory, stored_binary, stored_metadata or store_directory"
                ),
            )
        }),
    }
}

/// What both editor classes do, so their JavaScript methods are written once
/// ([`editor_methods!`]). `QueryEditor` refuses more edits than `PipelineEditor`; the trait does
/// not care which.
trait Editing {
    fn view(&self) -> &PipelineEditor;
    fn push_element(&mut self, element: PipelineElement) -> Result<(), Error>;
    fn insert_element(&mut self, at: usize, element: PipelineElement) -> Result<(), Error>;
    fn replace_element(&mut self, at: usize, element: PipelineElement) -> Result<(), Error>;
    fn remove_element(&mut self, at: usize) -> Result<(), Error>;
    fn move_element(&mut self, from: usize, to: usize) -> Result<(), Error>;
    fn realm(&mut self, realm: &str) -> Result<(), Error>;
    fn filename(&mut self, name: Option<&str>) -> Result<(), Error>;
    fn add_resource(&mut self, key: &str, read: Read) -> Result<(), Error>;
    fn add_action(&mut self, name: &str) -> Result<ActionEditor<'_>, Error>;
    fn action_at(&mut self, at: usize) -> Result<ActionEditor<'_>, Error>;
    fn query_text(&self) -> Result<String, Error>;
}

impl Editing for PipelineEditor {
    fn view(&self) -> &PipelineEditor {
        self
    }
    fn push_element(&mut self, element: PipelineElement) -> Result<(), Error> {
        self.push(element).map(|_| ())
    }
    fn insert_element(&mut self, at: usize, element: PipelineElement) -> Result<(), Error> {
        self.insert(at, element).map(|_| ())
    }
    fn replace_element(&mut self, at: usize, element: PipelineElement) -> Result<(), Error> {
        self.replace(at, element).map(|_| ())
    }
    fn remove_element(&mut self, at: usize) -> Result<(), Error> {
        self.remove(at).map(|_| ())
    }
    fn move_element(&mut self, from: usize, to: usize) -> Result<(), Error> {
        self.move_to(from, to).map(|_| ())
    }
    fn realm(&mut self, realm: &str) -> Result<(), Error> {
        self.set_realm(realm).map(|_| ())
    }
    fn filename(&mut self, name: Option<&str>) -> Result<(), Error> {
        self.set_filename(name).map(|_| ())
    }
    fn add_resource(&mut self, key: &str, read: Read) -> Result<(), Error> {
        self.resource(key, read).map(|_| ())
    }
    fn add_action(&mut self, name: &str) -> Result<ActionEditor<'_>, Error> {
        self.action(name)
    }
    fn action_at(&mut self, at: usize) -> Result<ActionEditor<'_>, Error> {
        self.edit_action(at)
    }
    fn query_text(&self) -> Result<String, Error> {
        liquers_core::query::Query::try_from(self).map(|q| q.encode())
    }
}

impl Editing for QueryEditor {
    fn view(&self) -> &PipelineEditor {
        self.editor()
    }
    fn push_element(&mut self, element: PipelineElement) -> Result<(), Error> {
        self.push(element).map(|_| ())
    }
    fn insert_element(&mut self, at: usize, element: PipelineElement) -> Result<(), Error> {
        self.insert(at, element).map(|_| ())
    }
    fn replace_element(&mut self, at: usize, element: PipelineElement) -> Result<(), Error> {
        self.replace(at, element).map(|_| ())
    }
    fn remove_element(&mut self, at: usize) -> Result<(), Error> {
        self.remove(at).map(|_| ())
    }
    fn move_element(&mut self, from: usize, to: usize) -> Result<(), Error> {
        self.move_to(from, to).map(|_| ())
    }
    fn realm(&mut self, realm: &str) -> Result<(), Error> {
        self.set_realm(realm).map(|_| ())
    }
    fn filename(&mut self, name: Option<&str>) -> Result<(), Error> {
        self.set_filename(name).map(|_| ())
    }
    fn add_resource(&mut self, key: &str, read: Read) -> Result<(), Error> {
        self.resource(key, read).map(|_| ())
    }
    fn add_action(&mut self, name: &str) -> Result<ActionEditor<'_>, Error> {
        self.action(name)
    }
    fn action_at(&mut self, at: usize) -> Result<ActionEditor<'_>, Error> {
        self.edit_action(at)
    }
    fn query_text(&self) -> Result<String, Error> {
        Ok(self.query().encode())
    }
}

fn js_err<T>(result: Result<T, Error>) -> Result<T, JsValue> {
    result.map_err(liquers_error_to_js)
}

/// The JavaScript methods both editor classes share. A macro, because `#[wasm_bindgen]` needs
/// one inherent `impl` per class.
macro_rules! editor_methods {
    ($class:ident, $js_class:literal) => {
        #[wasm_bindgen(js_class = $js_class)]
        impl $class {
            /// The number of elements.
            #[wasm_bindgen(getter)]
            pub fn length(&self) -> usize {
                self.inner.view().pipeline().elements.len()
            }

            /// The pipeline's JSON form: plain objects, a copy.
            #[wasm_bindgen(unchecked_return_type = "LiquersPipeline")]
            pub fn pipeline(&self) -> Result<JsValue, JsValue> {
                js_err(pipeline_to_js(self.inner.view().pipeline()))
            }

            /// The query text.
            pub fn query(&self) -> Result<String, JsValue> {
                js_err(self.inner.query_text())
            }

            /// Checks the whole pipeline against the registry.
            pub fn validate(&self) -> Result<(), JsValue> {
                js_err(self.inner.view().validate())
            }

            /// Appends an element, in its JSON form.
            pub fn push(
                &mut self,
                #[wasm_bindgen(unchecked_param_type = "LiquersPipelineElement")] element: JsValue,
            ) -> Result<(), JsValue> {
                let element = js_err(element_from_js(&element))?;
                js_err(self.inner.push_element(element))
            }

            /// Inserts an element before `at`.
            pub fn insert(
                &mut self,
                at: usize,
                #[wasm_bindgen(unchecked_param_type = "LiquersPipelineElement")] element: JsValue,
            ) -> Result<(), JsValue> {
                let element = js_err(element_from_js(&element))?;
                js_err(self.inner.insert_element(at, element))
            }

            /// Replaces the element at `at`.
            pub fn replace(
                &mut self,
                at: usize,
                #[wasm_bindgen(unchecked_param_type = "LiquersPipelineElement")] element: JsValue,
            ) -> Result<(), JsValue> {
                let element = js_err(element_from_js(&element))?;
                js_err(self.inner.replace_element(at, element))
            }

            /// Removes the element at `at`.
            pub fn remove(&mut self, at: usize) -> Result<(), JsValue> {
                js_err(self.inner.remove_element(at))
            }

            /// Moves the element at `from` to `to`.
            #[wasm_bindgen(js_name = moveTo)]
            pub fn move_to(&mut self, from: usize, to: usize) -> Result<(), JsValue> {
                js_err(self.inner.move_element(from, to))
            }

            /// Sets the realm; `""` is the root realm.
            #[wasm_bindgen(js_name = setRealm)]
            pub fn set_realm(&mut self, realm: &str) -> Result<(), JsValue> {
                js_err(self.inner.realm(realm))
            }

            /// Sets the filename, or clears it with `null`.
            #[wasm_bindgen(js_name = setFilename)]
            pub fn set_filename(&mut self, name: Option<String>) -> Result<(), JsValue> {
                js_err(self.inner.filename(name.as_deref()))
            }

            /// Appends a resource read. `read` is `asset` (the default), `binary`, `metadata`,
            /// `recipe`, `directory`, `stored_binary`, `stored_metadata` or `store_directory`.
            pub fn resource(&mut self, key: &str, read: Option<String>) -> Result<(), JsValue> {
                let read = js_err(read_mode(read))?;
                js_err(self.inner.add_resource(key, read))
            }

            /// Appends an action, optionally in namespaces, and returns its index for the
            /// argument edits. Refused, with nothing appended, when the command does not resolve
            /// in those namespaces (or the default ones).
            pub fn action(
                &mut self,
                name: &str,
                #[wasm_bindgen(unchecked_optional_param_type = "string | string[]")] ns: Option<
                    JsValue,
                >,
            ) -> Result<usize, JsValue> {
                let ns = match ns {
                    Some(ns) if !(ns.is_undefined() || ns.is_null()) => {
                        Some(js_err(namespaces(&ns))?)
                    }
                    Some(_) | None => None,
                };
                let index = {
                    let mut action = js_err(self.inner.add_action(name))?;
                    match ns {
                        Some(ns) => action.ns(ns).map(|a| a.index()),
                        None => Ok(action.index()),
                    }
                };
                // Core looks the command up lazily, so that `.ns(…)` can follow `action(…)`. Here
                // the namespaces come with the call, so the command must resolve now.
                let checked = index
                    .and_then(|index| self.inner.view().command_metadata(index).map(|_| index));
                match checked {
                    Ok(index) => Ok(index),
                    Err(e) => {
                        // The action was appended before it was refused; take it back so the
                        // refusal changes nothing. Removing the last element of a pipeline that
                        // was valid before it was appended cannot itself be refused.
                        let last = self
                            .inner
                            .view()
                            .pipeline()
                            .elements
                            .len()
                            .saturating_sub(1);
                        let _ = self.inner.remove_element(last);
                        Err(liquers_error_to_js(e))
                    }
                }
            }

            /// Sets the namespaces of the action at `at` and re-resolves its command.
            #[wasm_bindgen(js_name = setNamespace)]
            pub fn set_namespace(
                &mut self,
                at: usize,
                #[wasm_bindgen(unchecked_param_type = "string | string[]")] ns: JsValue,
            ) -> Result<(), JsValue> {
                let ns = js_err(namespaces(&ns))?;
                js_err(
                    self.inner
                        .action_at(at)
                        .and_then(|mut a| a.ns(ns).map(|_| ())),
                )
            }

            /// Sets an argument of the action at `at` to a literal JSON value. `arg` is the
            /// argument's name or its position.
            pub fn set(
                &mut self,
                at: usize,
                #[wasm_bindgen(unchecked_param_type = "string | number")] arg: JsValue,
                value: JsValue,
            ) -> Result<(), JsValue> {
                let arg = js_err(arg_ref(&arg))?;
                let value = js_err(js_value_to_json(&value))?;
                js_err(
                    self.inner
                        .action_at(at)
                        .and_then(|mut a| a.set(arg, value).map(|_| ())),
                )
            }

            /// Appends a value to the action's positional parameters (a variadic argument).
            #[wasm_bindgen(js_name = pushParameter)]
            pub fn push_parameter(&mut self, at: usize, value: JsValue) -> Result<(), JsValue> {
                let value = js_err(js_value_to_json(&value))?;
                js_err(
                    self.inner
                        .action_at(at)
                        .and_then(|mut a| a.push(value).map(|_| ())),
                )
            }

            /// Sets an argument to the result of a query, evaluated when the pipeline runs.
            pub fn link(
                &mut self,
                at: usize,
                #[wasm_bindgen(unchecked_param_type = "string | number")] arg: JsValue,
                query: &str,
            ) -> Result<(), JsValue> {
                let arg = js_err(arg_ref(&arg))?;
                js_err(
                    self.inner
                        .action_at(at)
                        .and_then(|mut a| a.link(arg, query).map(|_| ())),
                )
            }

            /// Removes an argument of the action at `at`.
            pub fn unset(
                &mut self,
                at: usize,
                #[wasm_bindgen(unchecked_param_type = "string | number")] arg: JsValue,
            ) -> Result<(), JsValue> {
                let arg = js_err(arg_ref(&arg))?;
                js_err(
                    self.inner
                        .action_at(at)
                        .and_then(|mut a| a.unset(arg).map(|_| ())),
                )
            }

            /// The names of the arguments of the action at `at` that are not set yet.
            #[wasm_bindgen(js_name = unsetArguments)]
            pub fn unset_arguments(&self, at: usize) -> Result<Vec<String>, JsValue> {
                js_err(
                    self.inner
                        .view()
                        .unset_arguments(at)
                        .map(|args| args.into_iter().map(|a| a.name.clone()).collect()),
                )
            }
        }
    };
}

/// A pipeline editor (`PipelineEditor`): every edit is checked against the command metadata
/// the editor was opened with, and a refused edit throws and leaves the pipeline unchanged.
#[wasm_bindgen(js_name = PipelineEditor)]
pub struct LiquersPipelineEditor {
    inner: PipelineEditor,
}

/// A query editor (`QueryEditor`): a pipeline editor that also refuses any edit after which the
/// pipeline has no query form, so `query()` never throws.
#[wasm_bindgen(js_name = QueryEditor)]
pub struct LiquersQueryEditor {
    inner: QueryEditor,
}

editor_methods!(LiquersPipelineEditor, "PipelineEditor");
editor_methods!(LiquersQueryEditor, "QueryEditor");

#[wasm_bindgen(js_class = PipelineEditor)]
impl LiquersPipelineEditor {
    /// Opens an editor on the global environment's commands: empty, or over `pipeline`.
    /// Throws before `init()`. `Environment.pipelineEditor` opens one on another environment.
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(unchecked_optional_param_type = "LiquersPipeline")] pipeline: Option<
            JsValue,
        >,
    ) -> Result<LiquersPipelineEditor, JsValue> {
        let cmr = js_err(crate::environment::current_metadata_registry())?;
        js_err(Self::open(&pipeline.unwrap_or(JsValue::UNDEFINED), cmr))
    }
}

impl LiquersPipelineEditor {
    /// An editor over `pipeline` (empty when it is `undefined` or `null`), checked by `cmr`.
    pub fn open(
        pipeline: &JsValue,
        cmr: Arc<CommandMetadataRegistry>,
    ) -> Result<LiquersPipelineEditor, Error> {
        let inner = if pipeline.is_undefined() || pipeline.is_null() {
            PipelineEditor::new(cmr)
        } else {
            PipelineEditor::from_pipeline(pipeline_from_js(pipeline)?, cmr)?
        };
        Ok(LiquersPipelineEditor { inner })
    }
}

#[wasm_bindgen(js_class = QueryEditor)]
impl LiquersQueryEditor {
    /// Opens an editor on the global environment's commands: empty, or over `query`.
    /// Throws before `init()`. `Environment.queryEditor` opens one on another environment.
    #[wasm_bindgen(constructor)]
    pub fn new(query: Option<String>) -> Result<LiquersQueryEditor, JsValue> {
        let cmr = js_err(crate::environment::current_metadata_registry())?;
        js_err(Self::open(query.as_deref(), cmr))
    }
}

impl LiquersQueryEditor {
    /// An editor over `query` (empty when `None`), checked by `cmr`.
    pub fn open(
        query: Option<&str>,
        cmr: Arc<CommandMetadataRegistry>,
    ) -> Result<LiquersQueryEditor, Error> {
        let inner = match query {
            None => QueryEditor::new(cmr),
            Some(text) => QueryEditor::from_query(&liquers_core::parse::parse_query(text)?, cmr)?,
        };
        Ok(LiquersQueryEditor { inner })
    }
}
