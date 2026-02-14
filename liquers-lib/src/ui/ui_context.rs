use std::sync::Arc;

use super::app_state::AppState;
use super::handle::UIHandle;
use super::message::{AppMessage, AppMessageSender};

/// Context provided to `show_in_egui` and other rendering code.
///
/// Bundles the shared `AppState`, a message sender, and the current handle
/// so that UI elements can both read state (via `try_sync_lock`) and submit
/// asynchronous work (via `send_message` / `submit_query`) without needing
/// access to the tokio runtime or `EnvRef`.
///
/// Phase 1a: UIContext is the primary holder of UI context state (app_state,
/// sender, current_handle). UIPayload provides an interface to UIContext for
/// command execution.
#[derive(Clone)]
pub struct UIContext {
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
    sender: AppMessageSender,
    current_handle: Option<UIHandle>,
}

impl UIContext {
    pub fn new(
        app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
        sender: AppMessageSender,
    ) -> Self {
        Self {
            app_state,
            sender,
            current_handle: None,
        }
    }

    /// Access the shared AppState mutex.
    pub fn app_state(&self) -> &Arc<tokio::sync::Mutex<dyn AppState>> {
        &self.app_state
    }

    /// Get the currently focused UI element handle, if any.
    pub fn handle(&self) -> Option<UIHandle> {
        self.current_handle
    }

    /// Return a new UIContext with a different current handle.
    /// Does not mutate the original (builder pattern).
    pub fn with_handle(mut self, handle: Option<UIHandle>) -> Self {
        self.current_handle = handle;
        self
    }

    /// Submit a query for evaluation, binding results to the given handle.
    pub fn submit_query(&self, handle: UIHandle, query: impl Into<String>) {
        let _ = self.sender.send(AppMessage::SubmitQuery {
            handle: Some(handle),
            query: query.into(),
        });
    }

    /// Submit a root query (no current handle).
    /// Used when there is no existing element — e.g. creating the root element
    /// from a query at startup.
    pub fn submit_root_query(&self, query: impl Into<String>) {
        let _ = self.sender.send(AppMessage::SubmitQuery {
            handle: None,
            query: query.into(),
        });
    }

    /// Send an arbitrary application message.
    pub fn send_message(&self, message: AppMessage) {
        let _ = self.sender.send(message);
    }

    /// Submit a query for the current handle (set via `with_handle`).
    /// No-op if no current handle is set.
    pub fn submit_query_current(&self, query: impl Into<String>) {
        if let Some(handle) = self.current_handle {
            self.submit_query(handle, query);
        }
    }

    /// Request application quit.
    pub fn request_quit(&self) {
        let _ = self.sender.send(AppMessage::Quit);
    }
}
