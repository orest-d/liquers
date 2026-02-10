pub mod app_state;
pub mod commands;
pub mod element;
pub mod handle;
pub mod message;
pub mod payload;
pub mod resolve;
pub mod ui_context;

pub use app_state::{AppState, DirectAppState, NodeData};
pub use element::{
    AssetViewElement, AssetViewMode, ElementSource, Placeholder, UIElement, UpdateMessage,
    UpdateResponse,
};
pub use handle::UIHandle;
pub use message::{AppMessage, AppMessageReceiver, AppMessageSender, app_message_channel};
pub use payload::{AppStateRef, SimpleUIPayload, UIPayload};
pub use resolve::{
    insertion_point_to_add_args, resolve_navigation, resolve_position, InsertionPoint,
};
pub use ui_context::UIContext;

pub use element::render_element;

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
