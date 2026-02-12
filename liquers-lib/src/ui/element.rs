use std::sync::Arc;

use serde::{Deserialize, Serialize};

use liquers_core::error::Error;
use liquers_core::metadata::AssetInfo;

use super::handle::UIHandle;
use super::ui_context::UIContext;

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
    ///
    /// `UIContext` provides access to AppState (via `ctx.app_state()`) and
    /// the message channel for submitting queries.
    fn init(&mut self, handle: UIHandle, _ctx: &UIContext) -> Result<(), Error> {
        self.set_handle(handle);
        Ok(())
    }

    /// React to a framework-agnostic update message.
    /// Default: no-op.
    fn update(&mut self, _message: &UpdateMessage, _ctx: &UIContext) -> UpdateResponse {
        UpdateResponse::Unchanged
    }

    /// Get the wrapped value, if any.
    /// Default: None (element doesn't hold a displayable value).
    fn get_value(&self) -> Option<Arc<crate::value::Value>> {
        None
    }

    /// Get the associated metadata, if any.
    /// Default: None.
    fn get_metadata(&self) -> Option<liquers_core::metadata::Metadata> {
        None
    }

    /// Render in egui.
    ///
    /// The caller holds the AppState lock and has extracted this element via
    /// `take_element`. The `app_state` parameter allows container elements to
    /// recursively render children using the extract-render-replace pattern.
    ///
    /// The `UIContext` provides a message channel for submitting async work.
    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &UIContext,
        _app_state: &mut dyn super::app_state::AppState,
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
/// When created via `from_asset_ref`, a background task monitors the asset
/// and updates shared state (value, info) via `Arc<RwLock>`. The element
/// itself is non-generic and serializable via typetag.
///
/// For simple wrapping (no live updates), use `new_value`, `new_progress`,
/// or `new_error`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetViewElement {
    handle: Option<UIHandle>,
    title_text: String,
    type_identifier: String,

    /// Current display mode.
    view_mode: AssetViewMode,

    /// The wrapped Value. Shared with background task when created via from_asset_ref.
    #[serde(skip)]
    value: Arc<std::sync::RwLock<Option<Arc<crate::value::Value>>>>,

    /// Error from evaluation failure.
    #[serde(skip)]
    error: Arc<std::sync::RwLock<Option<liquers_core::error::Error>>>,

    /// Live progress info, updated by background task.
    #[serde(skip)]
    progress_info: Arc<std::sync::RwLock<Option<AssetInfo>>>,

    /// Notification receiver for asset updates (serde skip).
    /// When Some, show_in_egui checks for changes on each frame.
    #[serde(skip)]
    notification_rx: Option<tokio::sync::watch::Receiver<liquers_core::assets::AssetNotificationMessage>>,
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
            value: Arc::new(std::sync::RwLock::new(None)),
            view_mode: AssetViewMode::Progress,
            error: Arc::new(std::sync::RwLock::new(None)),
            progress_info: Arc::new(std::sync::RwLock::new(None)),
            notification_rx: None,
        }
    }

    /// Create in Value mode with a pre-evaluated value.
    pub fn new_value(title: String, value: Arc<crate::value::Value>) -> Self {
        Self {
            handle: None,
            title_text: title,
            type_identifier: String::new(),
            value: Arc::new(std::sync::RwLock::new(Some(value))),
            view_mode: AssetViewMode::Value,
            error: Arc::new(std::sync::RwLock::new(None)),
            progress_info: Arc::new(std::sync::RwLock::new(None)),
            notification_rx: None,
        }
    }

    /// Create in Error mode from an evaluation error.
    pub fn new_error(error: liquers_core::error::Error) -> Self {
        Self {
            handle: None,
            title_text: "Error".to_string(),
            type_identifier: String::new(),
            value: Arc::new(std::sync::RwLock::new(None)),
            view_mode: AssetViewMode::Error,
            error: Arc::new(std::sync::RwLock::new(Some(error))),
            progress_info: Arc::new(std::sync::RwLock::new(None)),
            notification_rx: None,
        }
    }

    /// Async constructor. Captures a generic `AssetRef<E>` in a background task.
    ///
    /// Stores non-generic shared state (value, info, error) via `Arc<RwLock>`,
    /// allowing the element itself to remain non-generic and serializable.
    pub async fn from_asset_ref<E: liquers_core::context::Environment<Value = crate::value::Value>>(
        title: String,
        asset_ref: liquers_core::assets::AssetRef<E>,
    ) -> Self {
        // Get initial state
        let initial_state = asset_ref.poll_state().await;
        let initial_info = asset_ref.get_asset_info().await.ok();
        let notification_rx = asset_ref.subscribe_to_notifications().await;

        let initial_value = initial_state.map(|s| Arc::new((*s.data).clone()));
        let initial_mode = if initial_value.is_some() {
            AssetViewMode::Value
        } else {
            AssetViewMode::Progress
        };

        let value = Arc::new(std::sync::RwLock::new(initial_value));
        let info = Arc::new(std::sync::RwLock::new(initial_info));
        let error: Arc<std::sync::RwLock<Option<liquers_core::error::Error>>> =
            Arc::new(std::sync::RwLock::new(None));

        // Spawn background task — owns the generic AssetRef, updates shared non-generic state
        let value_clone = value.clone();
        let info_clone = info.clone();
        let error_clone = error.clone();
        let mut rx = asset_ref.subscribe_to_notifications().await;
        tokio::spawn(async move {
            loop {
                match rx.changed().await {
                    Ok(()) => {
                        if let Some(state) = asset_ref.poll_state().await {
                            if let Ok(mut v) = value_clone.write() {
                                *v = Some(Arc::new((*state.data).clone()));
                            }
                        }
                        if let Ok(asset_info) = asset_ref.get_asset_info().await {
                            if let Ok(mut i) = info_clone.write() {
                                *i = Some(asset_info);
                            }
                        }
                        // Check for error in notification
                        let notif = rx.borrow().clone();
                        if let liquers_core::assets::AssetNotificationMessage::ErrorOccurred(e) = notif {
                            if let Ok(mut err) = error_clone.write() {
                                *err = Some(e);
                            }
                        }
                    }
                    Err(_) => break, // Sender dropped
                }
            }
        });

        Self {
            handle: None,
            title_text: title,
            type_identifier: String::new(),
            value,
            view_mode: initial_mode,
            error,
            progress_info: info,
            notification_rx: Some(notification_rx),
        }
    }

    /// Set the value and switch to Value mode.
    pub fn set_value(&mut self, value: Arc<crate::value::Value>) {
        if let Ok(mut v) = self.value.write() {
            *v = Some(value);
        }
        self.view_mode = AssetViewMode::Value;
        if let Ok(mut e) = self.error.write() {
            *e = None;
        }
    }

    /// Set an error and switch to Error mode.
    pub fn set_error(&mut self, error: liquers_core::error::Error) {
        if let Ok(mut e) = self.error.write() {
            *e = Some(error);
        }
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

    /// Get the wrapped value, if any (reads from shared state).
    pub fn value(&self) -> Option<Arc<crate::value::Value>> {
        self.value.read().ok().and_then(|v| v.clone())
    }

    /// Get the progress info, if any (reads from shared state).
    pub fn progress_info(&self) -> Option<AssetInfo> {
        self.progress_info.read().ok().and_then(|i| i.clone())
    }

    /// Get the error, if any (reads from shared state).
    pub fn error(&self) -> Option<liquers_core::error::Error> {
        self.error.read().ok().and_then(|e| e.clone())
    }

    /// Get the error message, if any.
    pub fn error_message(&self) -> Option<String> {
        self.error().map(|e| e.to_string())
    }

    /// Check notification_rx for changes and update view_mode accordingly.
    /// Call this before rendering to ensure the display is up to date.
    fn sync_from_notifications(&mut self) {
        if let Some(rx) = &mut self.notification_rx {
            if rx.has_changed().unwrap_or(false) {
                let _ = rx.borrow_and_update();
                // Check if we have a value now
                if self.value.read().ok().map_or(false, |v| v.is_some()) {
                    if self.view_mode == AssetViewMode::Progress {
                        self.view_mode = AssetViewMode::Value;
                    }
                }
                // Check if there's an error
                if self.error.read().ok().map_or(false, |e| e.is_some()) {
                    if self.view_mode == AssetViewMode::Progress {
                        self.view_mode = AssetViewMode::Error;
                    }
                }
            }
        }
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

    fn get_value(&self) -> Option<Arc<crate::value::Value>> {
        self.value()
    }

    fn get_metadata(&self) -> Option<liquers_core::metadata::Metadata> {
        self.progress_info()
            .map(|info| liquers_core::metadata::Metadata::from(info))
    }

    fn update(&mut self, message: &UpdateMessage, _ctx: &UIContext) -> UpdateResponse {
        // Sync from background task notifications first
        self.sync_from_notifications();

        match message {
            UpdateMessage::AssetNotification(notif) => {
                use liquers_core::assets::AssetNotificationMessage;
                match notif {
                    AssetNotificationMessage::ErrorOccurred(err) => {
                        self.set_error(err.clone());
                    }
                    AssetNotificationMessage::JobFinished => {
                        // Value mode transition happens via sync_from_notifications
                    }
                    AssetNotificationMessage::PrimaryProgressUpdated(_progress) => {
                        // Progress is tracked via progress_info in shared state
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
        _ctx: &UIContext,
        _app_state: &mut dyn super::app_state::AppState,
    ) -> egui::Response {
        // Sync from background task before rendering
        self.sync_from_notifications();

        match &self.view_mode {
            AssetViewMode::Progress => {
                if let Some(info) = self.progress_info() {
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
                if let Some(value) = self.value() {
                    use crate::egui::UIValueExtension;
                    let val: &crate::value::Value = value.as_ref();
                    val.show(ui);
                    ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover())
                } else {
                    ui.label("No value available")
                }
            }
            AssetViewMode::Metadata => {
                if let Some(info) = self.progress_info() {
                    crate::egui::widgets::display_asset_info(ui, &info)
                } else {
                    ui.label("No metadata available")
                }
            }
            AssetViewMode::Error => {
                if let Some(err) = self.error() {
                    ui.colored_label(egui::Color32::RED, err.to_string())
                } else {
                    ui.label("Unknown error")
                }
            }
        }
    }
}

// ─── StateViewElement ──────────────────────────────────────────────────────

/// Wraps a non-UI Value for display. Used when `insert_state` receives a
/// plain value (not an `ExtValue::UIElement`).
///
/// Unlike `AssetViewElement` (which tracks an asset lifecycle),
/// `StateViewElement` simply holds a snapshot of a value and its metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateViewElement {
    handle: Option<UIHandle>,
    title_text: String,

    /// The wrapped Value. Skipped during serialization.
    #[serde(skip)]
    value: Option<Arc<crate::value::Value>>,

    /// The associated metadata. Skipped during serialization.
    #[serde(skip)]
    metadata: Option<liquers_core::metadata::Metadata>,
}

impl StateViewElement {
    /// Create a new StateViewElement wrapping the given value.
    pub fn new(title: String, value: Arc<crate::value::Value>) -> Self {
        Self {
            handle: None,
            title_text: title,
            value: Some(value),
            metadata: None,
        }
    }

    /// Create with associated metadata.
    pub fn with_metadata(mut self, metadata: liquers_core::metadata::Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Create from a State, extracting title and metadata.
    pub fn from_state(state: &liquers_core::state::State<crate::value::Value>) -> Self {
        let title = {
            let t = state.metadata.title().to_string();
            if t.is_empty() { "View".to_string() } else { t }
        };
        let mut elem = Self::new(title, Arc::new((*state.data).clone()));
        elem.metadata = Some((*state.metadata).clone());
        elem
    }
}

#[typetag::serde]
impl UIElement for StateViewElement {
    fn type_name(&self) -> &'static str {
        "StateViewElement"
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

    fn get_value(&self) -> Option<Arc<crate::value::Value>> {
        self.value.clone()
    }

    fn get_metadata(&self) -> Option<liquers_core::metadata::Metadata> {
        self.metadata.clone()
    }

    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &UIContext,
        _app_state: &mut dyn super::app_state::AppState,
    ) -> egui::Response {
        if let Some(value) = &self.value {
            use crate::egui::UIValueExtension;
            let val: &crate::value::Value = value.as_ref();
            val.show(ui);
            ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover())
        } else {
            ui.label("No value available (deserialized)")
        }
    }
}

// ─── Rendering Helper ───────────────────────────────────────────────────────

/// Extract-render-replace pattern for rendering an element.
///
/// Uses `try_sync_lock` instead of `blocking_lock` for WASM compatibility.
/// If the lock is held by an async task (rare on native, impossible on WASM),
/// a placeholder is shown and a repaint is requested for the next frame.
///
/// The AppState lock is held for the entire render cycle (take → show → put).
/// This allows container elements to recursively render children via the
/// `app_state` parameter passed to `show_in_egui`. The lock blocks async tasks
/// during rendering, which is acceptable for egui's synchronous render loop.
pub fn render_element(
    ui: &mut egui::Ui,
    handle: UIHandle,
    ctx: &UIContext,
) {
    // 1. Acquire lock for the entire render cycle.
    let mut state = match super::try_sync_lock(ctx.app_state()) {
        Ok(guard) => guard,
        Err(_) => {
            // Lock held by async task — show placeholder, repaint next frame.
            ui.label(format!("Loading {:?}...", handle));
            ui.ctx().request_repaint();
            return;
        }
    };

    // 2. Extract element.
    match state.take_element(handle) {
        Ok(mut element) => {
            // 3. Render with access to AppState (element is extracted, no aliasing).
            element.show_in_egui(ui, ctx, &mut *state);

            // 4. Put element back.
            let _ = state.put_element(handle, element);
        }
        Err(_) => {
            // Element missing — show placeholder.
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
        e.set_error(Error::general_error("Something failed".to_string()));
        assert_eq!(e.view_mode(), &AssetViewMode::Error);
        assert!(e.error().is_some());
        assert!(e.error_message().is_some());
    }

    #[test]
    fn test_asset_view_element_new_error() {
        let e = AssetViewElement::new_error(Error::general_error("eval failed".to_string()));
        assert_eq!(e.view_mode(), &AssetViewMode::Error);
        assert!(e.error().is_some());
        assert_eq!(e.title(), "Error");
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
    fn test_placeholder_get_value_is_none() {
        let p = Placeholder::new();
        assert!(p.get_value().is_none());
        assert!(p.get_metadata().is_none());
    }

    #[test]
    fn test_asset_view_get_value() {
        let val = Arc::new(crate::value::Value::from("hello"));
        let e = AssetViewElement::new_value("Test".to_string(), val.clone());
        assert!(e.get_value().is_some());
    }

    #[test]
    fn test_asset_view_get_value_none_in_progress() {
        let e = AssetViewElement::new_progress("Loading".to_string());
        assert!(e.get_value().is_none());
    }

    #[test]
    fn test_asset_view_get_metadata_none_by_default() {
        let e = AssetViewElement::new_progress("Loading".to_string());
        assert!(e.get_metadata().is_none());
    }

    #[test]
    fn test_state_view_element_basics() {
        let val = Arc::new(crate::value::Value::from("hello state"));
        let e = StateViewElement::new("State Title".to_string(), val.clone());
        assert_eq!(e.type_name(), "StateViewElement");
        assert_eq!(e.title(), "State Title");
        assert!(e.handle().is_none());
        assert!(e.get_value().is_some());
        assert!(e.get_metadata().is_none());
    }

    #[test]
    fn test_state_view_element_with_metadata() {
        let val = Arc::new(crate::value::Value::from("test"));
        let metadata = liquers_core::metadata::Metadata::new();
        let e = StateViewElement::new("M".to_string(), val).with_metadata(metadata);
        assert!(e.get_metadata().is_some());
    }

    #[test]
    fn test_state_view_element_clone_boxed() {
        let val = Arc::new(crate::value::Value::from("clone me"));
        let e = StateViewElement::new("Clone".to_string(), val);
        let boxed: Box<dyn UIElement> = Box::new(e);
        let cloned = boxed.clone_boxed();
        assert_eq!(cloned.type_name(), "StateViewElement");
        assert_eq!(cloned.title(), "Clone");
    }

    #[test]
    fn test_state_view_element_serialization_loses_value() {
        let val = Arc::new(crate::value::Value::from("serialize"));
        let mut e = StateViewElement::new("Ser".to_string(), val);
        e.set_handle(UIHandle(99));

        let boxed: Box<dyn UIElement> = Box::new(e);
        let json = serde_json::to_string(&boxed).expect("serialize");
        let restored: Box<dyn UIElement> = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.type_name(), "StateViewElement");
        assert_eq!(restored.handle(), Some(UIHandle(99)));
        // Value is lost after serialization (serde skip)
        assert!(restored.get_value().is_none());
    }

    #[test]
    fn test_update_response_unchanged_default() {
        let (tx, _rx) = super::super::message::app_message_channel();
        let app_state: Arc<tokio::sync::Mutex<dyn super::super::app_state::AppState>> =
            Arc::new(tokio::sync::Mutex::new(super::super::app_state::DirectAppState::new()));
        let ctx = UIContext::new(app_state, tx);

        let mut p = Placeholder::new();
        let msg = UpdateMessage::Timer { elapsed_ms: 100 };
        assert_eq!(p.update(&msg, &ctx), UpdateResponse::Unchanged);
    }
}
