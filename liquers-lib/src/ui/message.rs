use std::sync::Arc;

use liquers_core::error::Error;
use liquers_core::metadata::{Metadata, Status};

use super::handle::UIHandle;
use crate::value::Value;

/// Full non-generic snapshot of a monitored asset.
/// Pushed by AppRunner via `UpdateMessage::AssetUpdate`.
#[derive(Clone, Debug)]
pub struct AssetSnapshot {
    /// The current value (if evaluation has completed successfully).
    pub value: Option<Arc<Value>>,
    /// Full metadata, always available via `AssetRef::get_metadata()`.
    /// During evaluation: contains status, progress, logs as they arrive.
    /// After completion: contains the full metadata from the evaluated State.
    pub metadata: Metadata,
    /// Error from evaluation failure.
    pub error: Option<Error>,
    /// Current asset status.
    pub status: Status,
}

/// Messages sent from the render loop (or any synchronous context) to the
/// async processing loop. The sender side (`AppMessageSender`) is cheap to
/// clone and its `send()` is synchronous and non-blocking, making it safe to
/// call from `show_in_egui`.
#[derive(Debug, Clone)]
pub enum AppMessage {
    /// Submit a query for evaluation, with results bound to the given handle.
    SubmitQuery { handle: Option<UIHandle>, query: String },
    /// Request AppRunner to evaluate a query and push `AssetSnapshot` updates
    /// to the element at the given handle. AppRunner monitors the asset
    /// lifecycle and delivers `UpdateMessage::AssetUpdate` on each notification
    /// change. Monitoring auto-stops when the element is removed from AppState.
    RequestAssetUpdates { handle: UIHandle, query: String },
    /// Request application quit.
    Quit,
    /// Save application state to disk.
    Serialize { path: String },
    /// Load application state from disk.
    Deserialize { path: String },
}

pub type AppMessageSender = tokio::sync::mpsc::UnboundedSender<AppMessage>;
pub type AppMessageReceiver = tokio::sync::mpsc::UnboundedReceiver<AppMessage>;

/// Create a new application message channel.
pub fn app_message_channel() -> (AppMessageSender, AppMessageReceiver) {
    tokio::sync::mpsc::unbounded_channel()
}
