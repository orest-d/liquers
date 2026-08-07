//! Browser/JavaScript integration of Liquers, compiled to WebAssembly.
//!
//! This crate is the `wasm32` half of the language integration described in
//! `specs/LANGUAGE-INTEGRATION_GUIDE.md`. A page constructs an environment, evaluates queries as
//! `Promise`s, and registers commands written in JavaScript.
//!
//! # Target
//!
//! **This crate only functions on `wasm32`.** `JsValue` is `!Send`/`!Sync` on every target, and on
//! native the `MaybeSend`/`MaybeSync` markers resolve to `Send`/`Sync`, so the bridge types cannot
//! exist there. Rather than fail to compile natively — which would break
//! `cargo check --workspace` for everyone — the functional body is `wasm32`-gated and a native
//! build produces an intentionally empty crate. The workspace's `default-members` also excludes
//! this crate, so the native test loop never builds it.
//!
//! Build it with:
//!
//! ```text
//! cargo check -p liquers-web --target wasm32-unknown-unknown
//! wasm-pack test --headless --chrome liquers-web
//! ```
//!
//! # Architecture
//!
//! There is no new `Environment` and no new `CommandExecutor`.
//! `liquers_lib::environment::DefaultEnvironment` is already generic over the value type and
//! already selects the inline asset manager on `wasm32`, and the executor closure aliases already
//! drop `Send`/`Sync` there — so a JavaScript command is an ordinary registered async command
//! whose closure owns a `js_sys::Function`. This crate contributes a value bridge, a
//! `#[wasm_bindgen]` object/eval/command surface, and a `Promise` bridge.
//!
//! See `specs/liquers-web/` for the full design.

#![cfg(target_arch = "wasm32")]

pub mod bridge;
pub mod default_value;
pub mod value;

pub use bridge::{ConversionPolicy, JsValueBridge};
pub use value::{JsOpaque, ORIGIN_JAVASCRIPT};

// The `#[wasm_bindgen]` surface (objects, errors, environment, commands, evaluation) is added by
// milestones M3-M4 of `specs/liquers-web/phase4-implementation.md`.
