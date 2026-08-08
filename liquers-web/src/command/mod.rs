//! `COMMAND` — registering JavaScript callables as Liquers commands.

pub mod adapter;
pub mod spec;

pub use adapter::{call_js_command, register_js_command};
pub use spec::{IsAsync, JsCommandSpec, StateMode, RESERVED_NAMESPACE};
