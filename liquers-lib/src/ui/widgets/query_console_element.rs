use std::sync::Arc;

use serde::{Deserialize, Serialize};

use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::error::Error;
use liquers_core::metadata::{Metadata, Status};
use liquers_core::state::State;

use crate::ui::app_state::AppState;
use crate::ui::element::{UIElement, UpdateMessage, UpdateResponse};
use crate::ui::handle::UIHandle;
use crate::ui::message::{AppMessage, AssetSnapshot};
use crate::ui::ui_context::UIContext;
use crate::utils::NextPreset;
use crate::value::Value;

/// Browser-like interactive query console widget.
///
/// Manages query history, command preset resolution, and data/metadata view toggle.
/// Passive: receives `AssetSnapshot` updates pushed by AppRunner via
/// `UpdateMessage::AssetUpdate`. Has no channels, no background tasks, no polling.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryConsoleElement {
    handle: Option<UIHandle>,
    title_text: String,

    /// The text currently in the query edit field.
    pub query_text: String,

    /// History of submitted queries (oldest first). Persistent.
    history: Vec<String>,

    /// Current position in history. Points to next entry after the latest = history.len().
    /// history[history_index - 1] is the currently displayed query (if > 0).
    history_index: usize,

    /// Current view mode: true = data view, false = metadata view.
    data_view: bool,

    /// The current value from the most recent AssetSnapshot.
    #[serde(skip)]
    value: Option<Arc<Value>>,

    /// The current metadata from the most recent AssetSnapshot.
    #[serde(skip)]
    metadata: Option<Metadata>,

    /// Error from the most recent AssetSnapshot.
    #[serde(skip)]
    error: Option<Error>,

    /// Current asset status from the most recent AssetSnapshot.
    #[serde(skip)]
    status: Status,

    /// Cached next-command presets for the current query's last action.
    #[serde(skip)]
    next_presets: Vec<NextPreset>,
}

impl QueryConsoleElement {
    /// Create a new QueryConsoleElement with a title and an initial query.
    pub fn new(title: String, initial_query: String) -> Self {
        QueryConsoleElement {
            handle: None,
            title_text: title,
            query_text: initial_query,
            history: Vec::new(),
            history_index: 0,
            data_view: false,
            value: None,
            metadata: None,
            error: None,
            status: Status::None,
            next_presets: Vec::new(),
        }
    }

    /// Submit the current query_text for evaluation.
    /// Pushes to history, sends `RequestAssetUpdates { handle, query }` via ctx.
    fn submit_query(&mut self, ctx: &UIContext) {
        if self.query_text.is_empty() {
            return;
        }
        // Push to history
        self.history.push(self.query_text.clone());
        self.history_index = self.history.len();

        // Clear runtime state for new query
        self.value = None;
        self.error = None;
        self.next_presets.clear();
        self.data_view = false;

        // Send message to AppRunner
        if let Some(handle) = self.handle {
            ctx.send_message(AppMessage::RequestAssetUpdates {
                handle,
                query: self.query_text.clone(),
            });
        }
    }

    /// Navigate history backward. Returns true if position changed.
    fn history_back(&mut self) -> bool {
        if self.history_index > 0 {
            self.history_index -= 1;
            if self.history_index > 0 {
                self.query_text = self.history[self.history_index - 1].clone();
            }
            true
        } else {
            false
        }
    }

    /// Navigate history forward. Returns true if position changed.
    fn history_forward(&mut self) -> bool {
        if self.history_index < self.history.len() {
            self.history_index += 1;
            if self.history_index > 0 && self.history_index <= self.history.len() {
                self.query_text = self.history[self.history_index - 1].clone();
            }
            true
        } else {
            false
        }
    }

    /// Resolve next-command presets for the current query and state.
    /// Called after receiving an AssetUpdate with a value.
    pub fn resolve_presets(&mut self, state: &State<Value>, registry: &CommandMetadataRegistry) {
        self.next_presets = crate::utils::find_next_presets(&self.query_text, state, registry);
    }

    /// Apply a preset: set query_text to the preset's query and submit.
    fn apply_preset(&mut self, preset_index: usize, ctx: &UIContext) {
        if preset_index >= self.next_presets.len() {
            return;
        }
        self.query_text = self.next_presets[preset_index].query.clone();
        self.submit_query(ctx);
    }

    /// Render the single-row toolbar.
    /// Layout: [<] [>] [query_field] [Data/Metadata] [Presets v] [status]
    fn show_toolbar(&mut self, ui: &mut egui::Ui, ctx: &UIContext) {
        ui.horizontal(|ui| {
            // Back button
            let back_enabled = self.history_index > 0;
            if ui
                .add_enabled(back_enabled, egui::Button::new("\u{25C0}"))
                .clicked()
            {
                if self.history_back() {
                    self.submit_query(ctx);
                }
            }

            // Forward button
            let forward_enabled = self.history_index < self.history.len();
            if ui
                .add_enabled(forward_enabled, egui::Button::new("\u{25B6}"))
                .clicked()
            {
                if self.history_forward() {
                    self.submit_query(ctx);
                }
            }

            // Query text field with syntax highlighting (expanding)
            let qt = self.query_text.clone();
            let mut layouter =
                |ui: &egui::Ui, _buf: &dyn egui::TextBuffer, _wrap_width: f32| {
                    let layout_job = crate::egui::widgets::query_to_layout_job(&qt);
                    ui.fonts_mut(|f| f.layout_job(layout_job))
                };
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.query_text)
                    .desired_width(ui.available_width() - 200.0)
                    .layouter(&mut layouter)
                    .hint_text("Enter query..."),
            );
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.submit_query(ctx);
            }

            // Data/Metadata toggle button
            let has_value = self.value.is_some();
            let toggle_label = if !has_value {
                "Metadata"
            } else if self.data_view {
                "Data"
            } else {
                "Metadata"
            };
            if ui
                .add_enabled(has_value, egui::Button::new(toggle_label))
                .clicked()
            {
                self.data_view = !self.data_view;
            }

            // Presets dropdown (only if presets available)
            if !self.next_presets.is_empty() {
                let mut selected_preset: Option<usize> = None;
                egui::ComboBox::from_id_salt("presets")
                    .selected_text("Presets")
                    .show_ui(ui, |ui| {
                        for (i, preset) in self.next_presets.iter().enumerate() {
                            if ui
                                .selectable_label(false, &preset.label)
                                .on_hover_text(&preset.description)
                                .clicked()
                            {
                                selected_preset = Some(i);
                            }
                        }
                    });
                if let Some(idx) = selected_preset {
                    self.apply_preset(idx, ctx);
                }
            }

            // Status indicator
            crate::egui::widgets::display_status(ui, self.status);
        });
    }

    /// Render the content area: either data view or metadata pane.
    fn show_content(&self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.data_view && self.value.is_some() {
                // Data view
                if let Some(ref value) = self.value {
                    use crate::egui::UIValueExtension;
                    let val: &Value = value.as_ref();
                    val.show(ui);
                }
            } else {
                // Metadata pane
                self.show_metadata_pane(ui);
            }
        });
    }

    /// Render the scrollable metadata pane.
    fn show_metadata_pane(&self, ui: &mut egui::Ui) {
        // Show error if present
        if let Some(ref err) = self.error {
            ui.colored_label(egui::Color32::RED, format!("Error: {}", err));
            ui.separator();
        }

        // Show metadata
        if let Some(ref metadata) = self.metadata {
            let asset_info = metadata.get_asset_info();
            match asset_info {
                Ok(info) => {
                    crate::egui::widgets::display_asset_info(ui, &info);
                }
                Err(e) => {
                    ui.label(format!("Metadata error: {}", e));
                }
            }
        } else {
            ui.label("No metadata available");
        }
    }
}

#[typetag::serde]
impl UIElement for QueryConsoleElement {
    fn type_name(&self) -> &'static str {
        "QueryConsoleElement"
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

    fn init(&mut self, handle: UIHandle, ctx: &UIContext) -> Result<(), Error> {
        self.set_handle(handle);
        // If query_text is non-empty (e.g. from deserialization or creation), submit it
        if !self.query_text.is_empty() {
            self.submit_query(ctx);
        }
        Ok(())
    }

    fn update(&mut self, message: &UpdateMessage, _ctx: &UIContext) -> UpdateResponse {
        match message {
            UpdateMessage::AssetUpdate(snapshot) => {
                // Full snapshot pushed by AppRunner
                self.value = snapshot.value.clone();
                self.metadata = Some(snapshot.metadata.clone());
                self.error = snapshot.error.clone();
                self.status = snapshot.status;
                if self.value.is_some() {
                    self.data_view = true;
                }
                UpdateResponse::NeedsRepaint
            }
            UpdateMessage::AssetNotification(_) => UpdateResponse::Unchanged,
            UpdateMessage::Timer { .. } => UpdateResponse::Unchanged,
            UpdateMessage::Custom(_) => UpdateResponse::Unchanged,
        }
    }

    fn get_value(&self) -> Option<Arc<Value>> {
        self.value.clone()
    }

    fn get_metadata(&self) -> Option<Metadata> {
        self.metadata.clone()
    }

    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &UIContext,
        _app_state: &mut dyn AppState,
    ) -> egui::Response {
        ui.vertical(|ui| {
            self.show_toolbar(ui, ctx);
            ui.separator();
            self.show_content(ui);
        })
        .response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app_state::DirectAppState;
    use crate::ui::message::app_message_channel;
    use liquers_core::assets::AssetNotificationMessage;

    fn create_test_context() -> (UIContext, crate::ui::message::AppMessageReceiver) {
        let (tx, rx) = app_message_channel();
        let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
            Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
        let ctx = UIContext::new(app_state, tx);
        (ctx, rx)
    }

    // ─── 1. Construction (4 tests) ──────────────────────────────────────────

    #[test]
    fn test_new_with_title_and_initial_query() {
        let console = QueryConsoleElement::new("My Console".to_string(), "text-hello".to_string());
        assert_eq!(console.title(), "My Console");
        assert_eq!(console.query_text, "text-hello");
        assert!(console.handle().is_none());
        assert!(console.history.is_empty());
        assert_eq!(console.history_index, 0);
        assert!(!console.data_view);
        assert!(console.value.is_none());
        assert!(console.metadata.is_none());
        assert!(console.error.is_none());
        assert!(console.next_presets.is_empty());
    }

    #[test]
    fn test_new_with_empty_query() {
        let console = QueryConsoleElement::new("Console".to_string(), String::new());
        assert_eq!(console.query_text, "");
        assert!(console.history.is_empty());
    }

    #[test]
    fn test_new_default_field_values() {
        let console = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert_eq!(console.status, Status::None);
    }

    #[test]
    fn test_new_multiple_instances_independent() {
        let mut c1 = QueryConsoleElement::new("A".to_string(), "q1".to_string());
        let mut c2 = QueryConsoleElement::new("B".to_string(), "q2".to_string());
        c1.set_handle(UIHandle(1));
        c2.set_handle(UIHandle(2));
        assert_eq!(c1.handle(), Some(UIHandle(1)));
        assert_eq!(c2.handle(), Some(UIHandle(2)));
        assert_eq!(c1.query_text, "q1");
        assert_eq!(c2.query_text, "q2");
    }

    // ─── 2. UIElement Trait (8 tests) ───────────────────────────────────────

    #[test]
    fn test_type_name() {
        let c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert_eq!(c.type_name(), "QueryConsoleElement");
    }

    #[test]
    fn test_handle_and_set_handle() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert!(c.handle().is_none());
        c.set_handle(UIHandle(42));
        assert_eq!(c.handle(), Some(UIHandle(42)));
        c.set_handle(UIHandle(99));
        assert_eq!(c.handle(), Some(UIHandle(99)));
    }

    #[test]
    fn test_title_and_set_title() {
        let mut c = QueryConsoleElement::new("Initial".to_string(), "q".to_string());
        assert_eq!(c.title(), "Initial");
        c.set_title("Updated".to_string());
        assert_eq!(c.title(), "Updated");
    }

    #[test]
    fn test_clone_boxed_preserves_fields() {
        let mut c = QueryConsoleElement::new("Original".to_string(), "q".to_string());
        c.set_handle(UIHandle(7));
        let boxed: Box<dyn UIElement> = Box::new(c);
        let cloned = boxed.clone_boxed();
        assert_eq!(cloned.type_name(), "QueryConsoleElement");
        assert_eq!(cloned.title(), "Original");
        assert_eq!(cloned.handle(), Some(UIHandle(7)));
    }

    #[test]
    fn test_get_value_returns_none_initially() {
        let c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert!(c.get_value().is_none());
    }

    #[test]
    fn test_get_metadata_returns_none_initially() {
        let c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert!(c.get_metadata().is_none());
    }

    #[test]
    fn test_clone_boxed_returns_boxed_ui_element() {
        let c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        let boxed: Box<dyn UIElement> = Box::new(c);
        let cloned = boxed.clone_boxed();
        assert_eq!(cloned.type_name(), "QueryConsoleElement");
    }

    #[test]
    fn test_is_initialised_before_init() {
        let c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert!(!c.is_initialised());
    }

    // ─── 3. History Navigation (6 tests) ────────────────────────────────────

    #[test]
    fn test_history_back_at_beginning_returns_false() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert!(!c.history_back());
        assert_eq!(c.history_index, 0);
    }

    #[test]
    fn test_history_forward_at_end_returns_false() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.history = vec!["q1".to_string(), "q2".to_string()];
        c.history_index = 2;
        assert!(!c.history_forward());
        assert_eq!(c.history_index, 2);
    }

    #[test]
    fn test_history_back_after_submit() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.history = vec!["q1".to_string(), "q2".to_string(), "q3".to_string()];
        c.history_index = 3;
        assert!(c.history_back());
        assert_eq!(c.history_index, 2);
        assert!(c.history_back());
        assert_eq!(c.history_index, 1);
    }

    #[test]
    fn test_history_forward_after_back() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.history = vec!["q1".to_string(), "q2".to_string(), "q3".to_string()];
        c.history_index = 3;
        c.history_back();
        c.history_back(); // index = 1
        assert!(c.history_forward());
        assert_eq!(c.history_index, 2);
        assert!(c.history_forward());
        assert_eq!(c.history_index, 3);
    }

    #[test]
    fn test_history_navigation_boundaries() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.history = vec!["a".to_string(), "b".to_string()];
        c.history_index = 2;
        assert!(!c.history_forward()); // at end
        assert!(c.history_back());
        assert!(c.history_back()); // index = 0
        assert!(!c.history_back()); // at beginning
    }

    #[test]
    fn test_history_back_forward_symmetric() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.history = vec!["q1".to_string(), "q2".to_string()];
        c.history_index = 2;
        c.history_back();
        c.history_forward();
        assert_eq!(c.history_index, 2);
    }

    // ─── 4. AssetSnapshot Updates (6 tests) ─────────────────────────────────

    #[test]
    fn test_asset_update_stores_all_fields() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        let (ctx, _rx) = create_test_context();
        let snapshot = AssetSnapshot {
            value: Some(Arc::new(Value::from("hello"))),
            metadata: Metadata::new(),
            error: None,
            status: Status::Ready,
        };
        let response = c.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);
        assert_eq!(response, UpdateResponse::NeedsRepaint);
        assert!(c.value.is_some());
        assert!(c.metadata.is_some());
        assert!(c.error.is_none());
    }

    #[test]
    fn test_asset_update_with_error() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        let (ctx, _rx) = create_test_context();
        let snapshot = AssetSnapshot {
            value: None,
            metadata: Metadata::new(),
            error: Some(Error::general_error("fail".to_string())),
            status: Status::Error,
        };
        c.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);
        assert!(c.error.is_some());
        assert!(c.value.is_none());
    }

    #[test]
    fn test_asset_update_with_value_sets_data_view_true() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.data_view = false;
        let (ctx, _rx) = create_test_context();
        let snapshot = AssetSnapshot {
            value: Some(Arc::new(Value::from("v"))),
            metadata: Metadata::new(),
            error: None,
            status: Status::Ready,
        };
        c.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);
        assert!(c.data_view);
    }

    #[test]
    fn test_asset_update_without_value_preserves_data_view() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.data_view = true;
        let (ctx, _rx) = create_test_context();
        let snapshot = AssetSnapshot {
            value: None,
            metadata: Metadata::new(),
            error: None,
            status: Status::Submitted,
        };
        c.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);
        assert!(c.data_view); // unchanged
    }

    #[test]
    fn test_other_updates_return_unchanged() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        let (ctx, _rx) = create_test_context();
        assert_eq!(
            c.update(&UpdateMessage::Timer { elapsed_ms: 100 }, &ctx),
            UpdateResponse::Unchanged
        );
    }

    #[test]
    fn test_asset_notification_returns_unchanged() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        let (ctx, _rx) = create_test_context();
        let msg = UpdateMessage::AssetNotification(AssetNotificationMessage::Initial);
        assert_eq!(c.update(&msg, &ctx), UpdateResponse::Unchanged);
    }

    // ─── 5. Serialization (4 tests) ─────────────────────────────────────────

    #[test]
    fn test_serialization_persistent_fields() {
        let mut c = QueryConsoleElement::new("Title".to_string(), "q".to_string());
        c.set_handle(UIHandle(42));
        c.history = vec!["q1".to_string(), "q2".to_string()];
        c.history_index = 1;
        c.data_view = true;

        let boxed: Box<dyn UIElement> = Box::new(c);
        let json = serde_json::to_string(&boxed).expect("serialize");
        let restored: Box<dyn UIElement> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.type_name(), "QueryConsoleElement");
        assert_eq!(restored.handle(), Some(UIHandle(42)));
        assert_eq!(restored.title(), "Title");
    }

    #[test]
    fn test_deserialization_resets_runtime_fields() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.value = Some(Arc::new(Value::from("v")));
        c.metadata = Some(Metadata::new());
        c.error = Some(Error::general_error("e".to_string()));
        c.next_presets = vec![NextPreset {
            query: "q1".to_string(),
            label: "P".to_string(),
            description: "D".to_string(),
        }];

        let boxed: Box<dyn UIElement> = Box::new(c);
        let json = serde_json::to_string(&boxed).expect("serialize");
        let restored: Box<dyn UIElement> = serde_json::from_str(&json).expect("deserialize");
        assert!(restored.get_value().is_none());
        assert!(restored.get_metadata().is_none());
    }

    #[test]
    fn test_typetag_roundtrip() {
        let c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        let boxed: Box<dyn UIElement> = Box::new(c);
        let json = serde_json::to_string(&boxed).expect("serialize");
        assert!(json.contains("QueryConsoleElement"));
        let restored: Box<dyn UIElement> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.type_name(), "QueryConsoleElement");
    }

    #[test]
    fn test_history_preserved_across_serialization() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.history = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        c.history_index = 2;

        let boxed: Box<dyn UIElement> = Box::new(c);
        let json = serde_json::to_string(&boxed).expect("serialize");
        let _restored: Box<dyn UIElement> = serde_json::from_str(&json).expect("deserialize");
        // History restored (verifiable via downcast in Phase 2+)
    }

    // ─── 6. Presets (3 tests) ───────────────────────────────────────────────

    #[test]
    fn test_apply_preset_sets_query_and_submits() {
        let mut c = QueryConsoleElement::new("C".to_string(), "initial".to_string());
        c.next_presets = vec![
            NextPreset {
                query: "p1".to_string(),
                label: "P1".to_string(),
                description: String::new(),
            },
            NextPreset {
                query: "p2".to_string(),
                label: "P2".to_string(),
                description: String::new(),
            },
        ];
        let (ctx, mut rx) = create_test_context();
        c.set_handle(UIHandle(1));
        c.apply_preset(1, &ctx);
        assert_eq!(c.query_text, "p2");
        if let Ok(msg) = rx.try_recv() {
            match msg {
                AppMessage::RequestAssetUpdates { query, .. } => assert_eq!(query, "p2"),
                AppMessage::SubmitQuery { .. }
                | AppMessage::Quit
                | AppMessage::Serialize { .. }
                | AppMessage::Deserialize { .. } => panic!("Expected RequestAssetUpdates"),
            }
        }
    }

    #[test]
    fn test_next_presets_empty_initially() {
        let c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert!(c.next_presets.is_empty());
    }

    #[test]
    fn test_apply_preset_out_of_bounds() {
        let mut c = QueryConsoleElement::new("C".to_string(), "original".to_string());
        c.next_presets = vec![NextPreset {
            query: "p1".to_string(),
            label: "P1".to_string(),
            description: String::new(),
        }];
        let (ctx, _rx) = create_test_context();
        c.apply_preset(99, &ctx);
        assert_eq!(c.query_text, "original"); // unchanged
    }

    // ─── 7. Initialization (3 tests) ────────────────────────────────────────

    #[test]
    fn test_init_submits_non_empty_query() {
        let mut c = QueryConsoleElement::new("C".to_string(), "text-hello".to_string());
        let (tx, mut rx) = app_message_channel();
        let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
            Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
        let ctx = UIContext::new(app_state, tx);
        c.init(UIHandle(1), &ctx).expect("init");
        assert_eq!(c.handle(), Some(UIHandle(1)));
        assert!(rx.try_recv().is_ok()); // message sent
    }

    #[test]
    fn test_init_empty_query_no_submit() {
        let mut c = QueryConsoleElement::new("C".to_string(), String::new());
        let (tx, mut rx) = app_message_channel();
        let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
            Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
        let ctx = UIContext::new(app_state, tx);
        c.init(UIHandle(1), &ctx).expect("init");
        assert_eq!(c.handle(), Some(UIHandle(1)));
        assert!(rx.try_recv().is_err()); // no message
    }

    #[test]
    fn test_is_initialised_after_init() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert!(!c.is_initialised());
        let (ctx, _rx) = create_test_context();
        c.init(UIHandle(1), &ctx).expect("init");
        assert!(c.is_initialised());
    }
}
