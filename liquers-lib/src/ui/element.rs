use std::sync::Arc;

use serde::{Deserialize, Serialize};

use liquers_core::error::Error;
use liquers_core::metadata::AssetInfo;

use super::handle::UIHandle;

// ─── ElementSource ──────────────────────────────────────────────────────────

/// Describes how an element was generated. Serializable.
/// Stored per node in AppState alongside the UIElement.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ElementSource {
    /// Element was created directly (e.g. manually constructed).
    /// No generating query — cannot be re-evaluated.
    None,

    /// Query text to evaluate (produces a UIElement or a Value to display).
    Query(String),

    /// Parameterized query with metadata.
    Recipe(liquers_core::recipes::Recipe),
}

// ─── UpdateMessage / UpdateResponse ─────────────────────────────────────────

/// Framework-agnostic update messages delivered to elements.
pub enum UpdateMessage {
    /// Asset notification from the evaluation system.
    AssetNotification(liquers_core::assets::AssetNotificationMessage),
    /// Periodic timer tick.
    Timer { elapsed_ms: u64 },
    /// Custom application-defined message.
    Custom(Box<dyn std::any::Any + Send>),
}

/// Element's response to an update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateResponse {
    /// No visual change — framework may skip repaint.
    Unchanged,
    /// Element state changed — framework should repaint.
    NeedsRepaint,
}

// ─── UIElement Trait ────────────────────────────────────────────────────────

/// Core trait for all UI elements. Serializable via typetag.
///
/// Different implementations represent different kinds of elements
/// (windows, panels, display wrappers, asset status indicators, etc.).
/// Stored in AppState as `Box<dyn UIElement>`.
#[typetag::serde]
pub trait UIElement: Send + Sync + std::fmt::Debug {
    /// Returns the type name identifying this element kind.
    fn type_name(&self) -> &'static str;

    /// Per-instance handle, None until init is called.
    fn handle(&self) -> Option<UIHandle>;

    /// Set the handle. Called by init. Must not be called more than once.
    fn set_handle(&mut self, handle: UIHandle);

    /// True if init has been called (handle is Some).
    fn is_initialised(&self) -> bool {
        self.handle().is_some()
    }

    /// Human-readable title. Defaults to type_name().
    fn title(&self) -> String {
        self.type_name().to_string()
    }

    /// Override the title.
    fn set_title(&mut self, title: String);

    /// Clone this element into a new boxed trait object.
    fn clone_boxed(&self) -> Box<dyn UIElement>;

    /// Called once after the element is registered in AppState.
    /// Default: stores the handle via set_handle.
    fn init(&mut self, handle: UIHandle, _app_state: &dyn super::app_state::AppState) -> Result<(), Error> {
        self.set_handle(handle);
        Ok(())
    }

    /// React to a framework-agnostic update message.
    /// Default: no-op.
    fn update(&mut self, _message: &UpdateMessage) -> UpdateResponse {
        UpdateResponse::Unchanged
    }

    /// Render in egui. The caller does NOT hold the AppState lock.
    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        _app_state: Arc<tokio::sync::Mutex<dyn super::app_state::AppState>>,
    ) -> egui::Response {
        ui.label(self.title())
    }
}

impl Clone for Box<dyn UIElement> {
    fn clone(&self) -> Self {
        self.clone_boxed()
    }
}

// ─── Placeholder ────────────────────────────────────────────────────────────

/// Minimal serializable element. Used as a default/stub when
/// the real element is not yet available (e.g. pending evaluation).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Placeholder {
    handle: Option<UIHandle>,
    title_text: String,
}

impl Placeholder {
    pub fn new() -> Self {
        Self {
            handle: None,
            title_text: "Placeholder".to_string(),
        }
    }

    pub fn with_title(mut self, title: String) -> Self {
        self.title_text = title;
        self
    }
}

impl Default for Placeholder {
    fn default() -> Self {
        Self::new()
    }
}

#[typetag::serde]
impl UIElement for Placeholder {
    fn type_name(&self) -> &'static str {
        "Placeholder"
    }

    fn handle(&self) -> Option<UIHandle> {
        self.handle
    }

    fn set_handle(&mut self, handle: UIHandle) {
        self.handle = Some(handle);
    }

    fn title(&self) -> String {
        self.title_text.clone()
    }

    fn set_title(&mut self, title: String) {
        self.title_text = title;
    }

    fn clone_boxed(&self) -> Box<dyn UIElement> {
        Box::new(self.clone())
    }
}

// ─── AssetViewElement ───────────────────────────────────────────────────────

/// General-purpose viewer for evaluated values.
/// Covers the full asset lifecycle: progress → value/error.
///
/// Replaces both the progress indicator and the result display
/// in a single element.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetViewElement {
    handle: Option<UIHandle>,
    title_text: String,
    type_identifier: String,

    /// The wrapped Value. Skipped during serialization.
    #[serde(skip)]
    value: Option<Arc<crate::value::Value>>,

    /// Current display mode.
    view_mode: AssetViewMode,

    /// Error message if evaluation failed.
    #[serde(skip)]
    error_message: Option<String>,

    /// Live progress info, updated by background listener.
    #[serde(skip)]
    progress_info: Option<AssetInfo>,
}

/// Display mode for AssetViewElement.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AssetViewMode {
    /// Show evaluation progress (spinner, progress bar).
    Progress,
    /// Show the value (text, image, dataframe, etc.).
    Value,
    /// Show metadata (log, status, query, etc.).
    Metadata,
    /// Show error details.
    Error,
}

impl AssetViewElement {
    /// Create in Progress mode (evaluation starting).
    pub fn new_progress(title: String) -> Self {
        Self {
            handle: None,
            title_text: title,
            type_identifier: String::new(),
            value: None,
            view_mode: AssetViewMode::Progress,
            error_message: None,
            progress_info: None,
        }
    }

    /// Create in Value mode with a pre-evaluated value.
    pub fn new_value(title: String, value: Arc<crate::value::Value>) -> Self {
        Self {
            handle: None,
            title_text: title,
            type_identifier: String::new(),
            value: Some(value),
            view_mode: AssetViewMode::Value,
            error_message: None,
            progress_info: None,
        }
    }

    /// Set the value and switch to Value mode.
    pub fn set_value(&mut self, value: Arc<crate::value::Value>) {
        self.value = Some(value);
        self.view_mode = AssetViewMode::Value;
        self.error_message = None;
    }

    /// Set an error message and switch to Error mode.
    pub fn set_error(&mut self, message: String) {
        self.error_message = Some(message);
        self.view_mode = AssetViewMode::Error;
    }

    /// Get the current view mode.
    pub fn view_mode(&self) -> &AssetViewMode {
        &self.view_mode
    }

    /// Set the view mode explicitly.
    pub fn set_view_mode(&mut self, mode: AssetViewMode) {
        self.view_mode = mode;
    }

    /// Get the wrapped value, if any.
    pub fn value(&self) -> Option<&Arc<crate::value::Value>> {
        self.value.as_ref()
    }

    /// Get the progress info, if any.
    pub fn progress_info(&self) -> Option<&AssetInfo> {
        self.progress_info.as_ref()
    }

    /// Get the error message, if any.
    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }
}

#[typetag::serde]
impl UIElement for AssetViewElement {
    fn type_name(&self) -> &'static str {
        "AssetViewElement"
    }

    fn handle(&self) -> Option<UIHandle> {
        self.handle
    }

    fn set_handle(&mut self, handle: UIHandle) {
        self.handle = Some(handle);
    }

    fn title(&self) -> String {
        self.title_text.clone()
    }

    fn set_title(&mut self, title: String) {
        self.title_text = title;
    }

    fn clone_boxed(&self) -> Box<dyn UIElement> {
        Box::new(self.clone())
    }

    fn update(&mut self, message: &UpdateMessage) -> UpdateResponse {
        match message {
            UpdateMessage::AssetNotification(notif) => {
                use liquers_core::assets::AssetNotificationMessage;
                match notif {
                    AssetNotificationMessage::ErrorOccurred(err) => {
                        self.set_error(err.message.clone());
                    }
                    AssetNotificationMessage::JobFinished => {
                        // Value mode transition happens externally via set_value
                    }
                    AssetNotificationMessage::PrimaryProgressUpdated(_progress) => {
                        // Progress is tracked via progress_info set externally
                    }
                    AssetNotificationMessage::Initial
                    | AssetNotificationMessage::JobSubmitted
                    | AssetNotificationMessage::JobStarted
                    | AssetNotificationMessage::StatusChanged(_)
                    | AssetNotificationMessage::ValueProduced
                    | AssetNotificationMessage::LogMessage
                    | AssetNotificationMessage::SecondaryProgressUpdated(_) => {}
                }
                UpdateResponse::NeedsRepaint
            }
            UpdateMessage::Timer { .. } => UpdateResponse::Unchanged,
            UpdateMessage::Custom(_) => UpdateResponse::Unchanged,
        }
    }

    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        _app_state: Arc<tokio::sync::Mutex<dyn super::app_state::AppState>>,
    ) -> egui::Response {
        match &self.view_mode {
            AssetViewMode::Progress => {
                if let Some(info) = &self.progress_info {
                    ui.vertical(|ui| {
                        crate::egui::widgets::display_progress(ui, &info.progress);
                        crate::egui::widgets::display_status(ui, info.status);
                    })
                    .response
                } else {
                    ui.vertical(|ui| {
                        ui.spinner();
                        ui.label(format!("Evaluating: {}", self.title_text));
                    })
                    .response
                }
            }
            AssetViewMode::Value => {
                if let Some(value) = &self.value {
                    use crate::egui::UIValueExtension;
                    // Clone the Arc so we can call show
                    let val: &crate::value::Value = value.as_ref();
                    val.show(ui);
                    ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover())
                } else {
                    ui.label("No value available")
                }
            }
            AssetViewMode::Metadata => {
                if let Some(info) = &self.progress_info {
                    crate::egui::widgets::display_asset_info(ui, info)
                } else {
                    ui.label("No metadata available")
                }
            }
            AssetViewMode::Error => {
                if let Some(msg) = &self.error_message {
                    ui.colored_label(egui::Color32::RED, msg)
                } else {
                    ui.label("Unknown error")
                }
            }
        }
    }
}

// ─── Rendering Helper ───────────────────────────────────────────────────────

/// Extract-render-replace pattern for rendering an element.
/// The caller does NOT hold the AppState lock.
pub fn render_element(
    ui: &mut egui::Ui,
    handle: UIHandle,
    app_state: &Arc<tokio::sync::Mutex<dyn super::app_state::AppState>>,
) {
    // 1. Extract element from AppState (blocking_lock is safe here:
    //    egui render loop runs on the main thread, outside the tokio runtime)
    let element = {
        let mut state = app_state.blocking_lock();
        state.take_element(handle)
    };

    match element {
        Ok(mut element) => {
            // 2. Render (element can lock AppState if it needs to read children, etc.)
            element.show_in_egui(ui, app_state.clone());

            // 3. Put element back
            let mut state = app_state.blocking_lock();
            let _ = state.put_element(handle, element);
        }
        Err(_) => {
            // Element missing — show placeholder
            ui.label(format!("Element {:?} not found", handle));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placeholder_defaults() {
        let p = Placeholder::new();
        assert_eq!(p.type_name(), "Placeholder");
        assert_eq!(p.title(), "Placeholder");
        assert!(p.handle().is_none());
        assert!(!p.is_initialised());
    }

    #[test]
    fn test_placeholder_with_title() {
        let p = Placeholder::new().with_title("My Panel".to_string());
        assert_eq!(p.title(), "My Panel");
    }

    #[test]
    fn test_placeholder_set_handle() {
        let mut p = Placeholder::new();
        p.set_handle(UIHandle(42));
        assert_eq!(p.handle(), Some(UIHandle(42)));
        assert!(p.is_initialised());
    }

    #[test]
    fn test_placeholder_clone_boxed() {
        let p = Placeholder::new().with_title("Test".to_string());
        let boxed: Box<dyn UIElement> = Box::new(p);
        let cloned = boxed.clone_boxed();
        assert_eq!(cloned.type_name(), "Placeholder");
        assert_eq!(cloned.title(), "Test");
    }

    #[test]
    fn test_placeholder_serialization_roundtrip() {
        let mut p = Placeholder::new().with_title("Saved".to_string());
        p.set_handle(UIHandle(7));

        let boxed: Box<dyn UIElement> = Box::new(p);
        let json = serde_json::to_string(&boxed).expect("serialize");
        let restored: Box<dyn UIElement> = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.type_name(), "Placeholder");
        assert_eq!(restored.title(), "Saved");
        assert_eq!(restored.handle(), Some(UIHandle(7)));
    }

    #[test]
    fn test_asset_view_element_progress_mode() {
        let e = AssetViewElement::new_progress("Loading".to_string());
        assert_eq!(e.view_mode(), &AssetViewMode::Progress);
        assert!(e.value().is_none());
        assert!(e.error_message().is_none());
    }

    #[test]
    fn test_asset_view_element_set_value() {
        let mut e = AssetViewElement::new_progress("Test".to_string());
        let val = Arc::new(crate::value::Value::from("hello"));
        e.set_value(val.clone());
        assert_eq!(e.view_mode(), &AssetViewMode::Value);
        assert!(e.value().is_some());
    }

    #[test]
    fn test_asset_view_element_set_error() {
        let mut e = AssetViewElement::new_progress("Test".to_string());
        e.set_error("Something failed".to_string());
        assert_eq!(e.view_mode(), &AssetViewMode::Error);
        assert_eq!(e.error_message(), Some("Something failed"));
    }

    #[test]
    fn test_asset_view_serialization_loses_value() {
        let val = Arc::new(crate::value::Value::from("hello"));
        let mut e = AssetViewElement::new_value("Test".to_string(), val);
        e.set_handle(UIHandle(3));

        let boxed: Box<dyn UIElement> = Box::new(e);
        let json = serde_json::to_string(&boxed).expect("serialize");
        let restored: Box<dyn UIElement> = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.type_name(), "AssetViewElement");
        assert_eq!(restored.handle(), Some(UIHandle(3)));
        // Value is lost after serialization (serde skip)
    }

    #[test]
    fn test_element_source_serialization() {
        let sources = vec![
            ElementSource::None,
            ElementSource::Query("/-/hello".to_string()),
        ];
        for source in sources {
            let json = serde_json::to_string(&source).expect("serialize");
            let _restored: ElementSource = serde_json::from_str(&json).expect("deserialize");
        }
    }

    #[test]
    fn test_update_response_unchanged_default() {
        let mut p = Placeholder::new();
        let msg = UpdateMessage::Timer { elapsed_ms: 100 };
        assert_eq!(p.update(&msg), UpdateResponse::Unchanged);
    }
}
