//! The Liquers UI model: elements in an application state, evaluated from queries, shown by a
//! backend (egui natively, the DOM in a browser).
//!
//! A UI is a tree of [`UIElement`]s held by an [`AppState`]. Each element has a source (usually a
//! query); the [`AppRunner`] evaluates sources into elements and reports their status. Commands of
//! the `lui` namespace build elements (`ns-lui/query_console`, `ns-lui/ui_spec`), and an element's
//! actions go back to the runner as [`AppMessage`]s through a [`UIContext`].
//!
//! # Main components
//!
//! - [`AppState`] / [`DirectAppState`]: the element tree; [`UIHandle`] names an element.
//! - [`UIElement`] and the widgets ([`QueryConsoleElement`], [`UISpecElement`]): what is shown.
//! - [`AppRunner`]: the message and evaluation loop; [`RunnerEvent`]: its status events.
//! - [`NativeDriver`], [`spawn_headless`] (not on wasm): one owner of a runner, ticked by a window
//!   or in the background.
//! - [`UIContext`], [`AppMessage`], [`UiAction`]: how commands and controls change the model.
//! - [`SimpleUIPayload`]: the payload that carries the UI context into commands.
//!
//! # Where it is used
//!
//! - `crate::egui::app::run_window` (feature `egui`): the native window.
//! - `web` (feature `webui`): the browser mount and server-side rendering.
//! - `liquers-py` (`lq.UI`) and `liquers-web`, which host a model from Python and JavaScript.
//!
//! # Related concepts
//!
//! - `specs/reference/UI_INTERFACE_FSD.md`: the model's contract.
//! - Payloads (`specs/guides/PAYLOAD_GUIDE.md`): how a command learns which element it acts for.
//!
//! # Examples
//!
//! ```
//! # use std::sync::Arc;
//! # use liquers_core::context::Environment;
//! # use liquers_lib::environment::DefaultEnvironment;
//! # use liquers_lib::ui::{app_message_channel, AppState, DirectAppState, NativeDriver, SimpleUIPayload};
//! # use liquers_lib::value::Value;
//! # let rt = tokio::runtime::Runtime::new().unwrap();
//! # rt.block_on(async {
//! let envref = DefaultEnvironment::<Value, SimpleUIPayload>::new().to_ref();
//! let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
//!     Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
//! let (sender, receiver) = app_message_channel();
//! let mut driver = NativeDriver::new(envref, app_state, receiver, sender);
//! assert!(driver.tick().await.settled); // nothing to evaluate yet
//! assert!(driver.app_state().lock().await.roots().is_empty());
//! # });
//! ```
//!
//! More: `native` (a model evaluating a query in the background), `crate::egui::app` (a window),
//! and `liquers-lib/examples/ui_query_console_app.rs`.

pub mod action;
pub mod app_state;
pub mod commands;
pub mod element;
pub mod event;
pub mod handle;
pub mod message;
pub mod payload;
pub mod resolve;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
pub mod runner;
pub mod shortcuts;
pub mod ui_context;
#[cfg(feature = "webui")]
pub mod web;
pub mod widgets;

pub use action::{dispatch_action, UiAction};
pub use app_state::{AppState, DirectAppState, Invalidation, UIChange};
pub use element::{
    AssetViewElement, AssetViewMode, ElementSource, Placeholder, StateViewElement, UIElement,
    UpdateMessage, UpdateResponse,
};
pub use event::{UIEvent, UIEventKind};
pub use handle::UIHandle;
pub use message::{
    app_message_channel, AppMessage, AppMessageReceiver, AppMessageSender, AssetSnapshot,
};
pub use payload::{AppStateRef, SimpleUIPayload, UIPayload};
pub use resolve::{
    insertion_point_to_add_args, resolve_navigation, resolve_position, InsertionPoint,
};
#[cfg(not(target_arch = "wasm32"))]
pub use native::{spawn_headless, CurrentEnv, HeadlessHandle, NativeDriver, Tick};
pub use runner::{AppRunner, ElementStatusInfo, RunnerEvent};
pub use shortcuts::{find_conflicts, validate_shortcut_strings, Key, KeyboardShortcut, Modifiers};
pub use ui_context::UIContext;
pub use widgets::QueryConsoleElement;
pub use widgets::UISpecElement;
pub use widgets::{
    delegate_for, register_delegate, unregister_delegate, DelegatedElement, ElementDelegate,
};

#[cfg(feature = "egui")]
pub use element::render_element;

#[cfg(feature = "webui")]
pub use web::{element_dom_id, render_app_ssr, render_element_web};
#[cfg(all(feature = "webui", target_arch = "wasm32"))]
pub use web::{
    mount_web, mount_web_with, run_headless, HeadlessOptions, MountHandle, MountOptions,
};

// ─── Cross-Platform Helpers ─────────────────────────────────────────────────

use liquers_core::error::Error;

/// Synchronously acquire a `tokio::sync::Mutex` for rendering.
///
/// Returns `Err` if the lock is currently held (e.g. by an async command).
/// On WASM (single-threaded), this never fails during synchronous rendering
/// because no other task can be running concurrently.
pub fn try_sync_lock<T: ?Sized>(
    mutex: &tokio::sync::Mutex<T>,
) -> Result<tokio::sync::MutexGuard<'_, T>, Error> {
    mutex
        .try_lock()
        .map_err(|_| Error::general_error("AppState lock held by async task".to_string()))
}

/// Spawn an async task on the appropriate runtime.
///
/// - **Native**: uses `tokio::spawn` (`Send + 'static` required).
/// - **WASM**: uses `wasm_bindgen_futures::spawn_local` (`'static` required, `Send` NOT required).
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_ui_task<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(future);
}

/// Spawn an async task on the appropriate runtime (WASM variant).
#[cfg(target_arch = "wasm32")]
pub fn spawn_ui_task<F>(future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}
