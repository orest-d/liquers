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
    SubmitQuery {
        handle: Option<UIHandle>,
        query: String,
    },
    /// Request AppRunner to evaluate a query and push `AssetSnapshot` updates
    /// to the element at the given handle. AppRunner monitors the asset
    /// lifecycle and delivers `UpdateMessage::AssetUpdate` on each notification
    /// change, including expiration transitions. Monitoring auto-stops when the
    /// element is removed from AppState.
    RequestAssetUpdates { handle: UIHandle, query: String },
    /// Apply `query` to `input` (used as the input state), binding results to `handle`.
    /// Sent when a `UiAction::Apply` fires: the backend reads the live input value and
    /// forwards it here, where AppRunner runs `apply_immediately(query, state(input))`.
    ApplyToInput {
        handle: UIHandle,
        input: String,
        query: String,
    },
    /// Deliver a user interaction to the element at `handle` as `UpdateMessage::UI`.
    /// Sent by a backend driver (e.g. the web driver on Enter or a `data-lq-event` click).
    UIEvent {
        handle: UIHandle,
        event: super::event::UIEvent,
    },
    /// Bind, read or write the accessor of an editor element (`ui::editor`). The runner keeps
    /// one accessor per handle: `Watch` binds it (evaluating an accessor binding, or making a
    /// memory accessor) and delivers its value and every change; `Get` reads it again; `SetIf`
    /// writes. Results come back as `UpdateMessage::Accessor`.
    Accessor {
        handle: UIHandle,
        binding: crate::ui::editor::EditorBinding,
        op: AccessorOp,
    },
    /// List the store directory `folder` for an editor's file selector; the result comes back
    /// as `UpdateMessage::Directory`.
    ListDirectory { handle: UIHandle, folder: String },
    /// Request application quit.
    Quit,
    /// Save application state to disk.
    Serialize { path: String },
    /// Load application state from disk.
    Deserialize { path: String },
}

/// What an [`AppMessage::Accessor`] asks.
#[derive(Debug, Clone)]
pub enum AccessorOp {
    /// Bind the accessor and deliver its value and its changes.
    Watch,
    /// Like `Watch`, with an accessor made in code (empty `address()`): it is used as it is,
    /// for the session; after a reload the element falls back to its `Memory` text.
    Adopt(std::sync::Arc<dyn liquers_core::accessor::ValueAccessor<Value>>),
    /// Read the accessor again.
    Get,
    /// Set `value` if the accessor's fingerprint is still `expected`.
    SetIf {
        value: Value,
        expected: Option<liquers_core::accessor::Fingerprint>,
    },
}

/// The runner's answer to an [`AppMessage::Accessor`], delivered as `UpdateMessage::Accessor`.
#[derive(Debug, Clone)]
pub enum AccessorUpdate {
    /// The accessor is bound; `address` is its query (empty for one made in code).
    Bound { address: String, writable: bool },
    /// The accessor's current value: on `Watch`, `Get`, and after a change elsewhere.
    Snapshot {
        value: Arc<Value>,
        fingerprint: Option<liquers_core::accessor::Fingerprint>,
    },
    /// A `SetIf` succeeded.
    Written {
        fingerprint: liquers_core::accessor::Fingerprint,
    },
    /// A `SetIf` was refused because the value changed meanwhile: its live value.
    Conflict {
        value: Arc<Value>,
        fingerprint: Option<liquers_core::accessor::Fingerprint>,
    },
    /// Binding, reading or writing failed.
    Failed(Error),
}

pub type AppMessageSender = tokio::sync::mpsc::UnboundedSender<AppMessage>;
pub type AppMessageReceiver = tokio::sync::mpsc::UnboundedReceiver<AppMessage>;

/// Create a new application message channel.
pub fn app_message_channel() -> (AppMessageSender, AppMessageReceiver) {
    tokio::sync::mpsc::unbounded_channel()
}
