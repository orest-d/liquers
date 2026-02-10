use std::sync::Arc;

use super::app_state::AppState;
use super::handle::UIHandle;
use super::message::{AppMessage, AppMessageSender};

/// Context provided to `show_in_egui` and other rendering code.
///
/// Bundles the shared `AppState` and a message sender so that UI elements can
/// both read state (via `try_sync_lock`) and submit asynchronous work
/// (via `send_message` / `submit_query`) without needing access to the
/// tokio runtime or `EnvRef`.
#[derive(Clone)]
pub struct UIContext {
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
    sender: AppMessageSender,
}

impl UIContext {
    pub fn new(
        app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
        sender: AppMessageSender,
    ) -> Self {
        Self { app_state, sender }
    }

    /// Access the shared AppState mutex.
    pub fn app_state(&self) -> &Arc<tokio::sync::Mutex<dyn AppState>> {
        &self.app_state
    }

    /// Submit a query for evaluation, binding results to the given handle.
    pub fn submit_query(&self, handle: UIHandle, query: impl Into<String>) {
        let _ = self.sender.send(AppMessage::SubmitQuery {
            handle,
            query: query.into(),
        });
    }

    /// Send an arbitrary application message.
    pub fn send_message(&self, message: AppMessage) {
        let _ = self.sender.send(message);
    }

    /// Request application quit.
    pub fn request_quit(&self) {
        let _ = self.sender.send(AppMessage::Quit);
    }

    /// Request re-evaluation of all pending nodes.
    pub fn evaluate_pending(&self) {
        let _ = self.sender.send(AppMessage::EvaluatePending);
    }
}
