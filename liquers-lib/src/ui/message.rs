use super::handle::UIHandle;

/// Messages sent from the render loop (or any synchronous context) to the
/// async processing loop. The sender side (`AppMessageSender`) is cheap to
/// clone and its `send()` is synchronous and non-blocking, making it safe to
/// call from `show_in_egui`.
#[derive(Debug, Clone)]
pub enum AppMessage {
    /// Submit a query for evaluation, with results bound to the given handle.
    SubmitQuery { handle: UIHandle, query: String },
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
