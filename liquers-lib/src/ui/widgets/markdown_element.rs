//! [`MarkdownElement`]: a UI element that shows Markdown text.
//!
//! egui draws it with `egui_commonmark` inside its own scroll area (so it reports
//! `scrolls_itself`); the browser converts it to HTML with `pulldown-cmark`. An asset snapshot
//! whose value is text replaces the shown text.
//!
//! # Main components
//!
//! - [`MarkdownElement`]: the element; persistent title and Markdown source.
//!
//! # Where it is used
//!
//! - The `lui/markdown` command (`ui::commands`) creates it; the query console shows it when a
//!   query evaluates to it.
//!
//! # Related concepts
//!
//! - `ui::element::UIElement`: the element trait.
//! - `liquers-markdown`: the query-aware Markdown parser (not used for display).
//!
//! # Examples
//!
//! ```
//! use liquers_lib::ui::element::UIElement;
//! use liquers_lib::ui::widgets::MarkdownElement;
//!
//! let element = MarkdownElement::new("Notes".to_string(), "# Hello".to_string());
//! assert_eq!(element.title(), "Notes");
//! assert!(element.scrolls_itself());
//! ```

use std::sync::Arc;

#[cfg(feature = "egui")]
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use serde::{Deserialize, Serialize};

use liquers_core::error::Error;
use liquers_core::metadata::Metadata;
use liquers_core::value::ValueInterface;

use crate::ui::app_state::AppState;
use crate::ui::element::{UIElement, UpdateMessage, UpdateResponse};
use crate::ui::handle::UIHandle;
use crate::ui::ui_context::UIContext;
use crate::value::Value;

/// Markdown viewer widget using `egui_commonmark`.
///
/// Renders markdown text with full CommonMark support (headings, lists,
/// code blocks, links, emphasis, etc.). The markdown source is persistent
/// across save/load; the rendering cache is rebuilt on deserialization.
#[derive(Debug, Serialize, Deserialize)]
pub struct MarkdownElement {
    handle: Option<UIHandle>,
    title_text: String,
    /// The markdown source text (persistent across save/load).
    markdown_text: String,
    /// egui_commonmark rendering cache (runtime-only, rebuilt on deserialization).
    #[cfg(feature = "egui")]
    #[serde(skip)]
    cache: CommonMarkCache,
}

impl Clone for MarkdownElement {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle,
            title_text: self.title_text.clone(),
            markdown_text: self.markdown_text.clone(),
            #[cfg(feature = "egui")]
            cache: CommonMarkCache::default(),
        }
    }
}

impl MarkdownElement {
    /// Create a new MarkdownElement with a title and markdown text.
    pub fn new(title: String, markdown_text: String) -> Self {
        Self {
            handle: None,
            title_text: title,
            markdown_text,
            #[cfg(feature = "egui")]
            cache: CommonMarkCache::default(),
        }
    }

    /// Get the markdown source text.
    pub fn markdown_text(&self) -> &str {
        &self.markdown_text
    }

    /// Set the markdown source text.
    pub fn set_markdown_text(&mut self, text: String) {
        self.markdown_text = text;
    }
}

#[typetag::serde]
impl UIElement for MarkdownElement {
    fn type_name(&self) -> &'static str {
        "MarkdownElement"
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

    fn init(&mut self, handle: UIHandle, _ctx: &UIContext) -> Result<(), Error> {
        self.set_handle(handle);
        Ok(())
    }

    fn update(&mut self, message: &UpdateMessage, _ctx: &UIContext) -> UpdateResponse {
        match message {
            UpdateMessage::AssetUpdate(snapshot) => {
                if let Some(ref value) = snapshot.value {
                    if let Ok(text) = value.try_into_string() {
                        self.markdown_text = text;
                        return UpdateResponse::NeedsRepaint;
                    }
                }
                UpdateResponse::Unchanged
            }
            UpdateMessage::AssetNotification(_) => UpdateResponse::Unchanged,
            UpdateMessage::Timer { .. } => UpdateResponse::Unchanged,
            UpdateMessage::Custom(_) => UpdateResponse::Unchanged,
            UpdateMessage::Accessor(_) | UpdateMessage::Directory(_) | UpdateMessage::NodeContext(_) => UpdateResponse::Unchanged,
            UpdateMessage::UI(_) => UpdateResponse::Unchanged,
        }
    }

    fn get_value(&self) -> Option<Arc<Value>> {
        Some(Arc::new(Value::from(self.markdown_text.clone())))
    }

    fn get_metadata(&self) -> Option<Metadata> {
        None
    }

    #[cfg(feature = "egui")]
    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &UIContext,
        _app_state: &mut dyn AppState,
    ) -> egui::Response {
        let output = egui::ScrollArea::vertical().show(ui, |ui| {
            CommonMarkViewer::new().show(ui, &mut self.cache, &self.markdown_text);
        });
        // The scroll area already took its space; allocating the content size again below it
        // made the parent overflow and grow a second scroll bar.
        ui.interact(output.inner_rect, output.id.with("lq-markdown"), egui::Sense::hover())
    }

    fn scrolls_itself(&self) -> bool {
        true
    }

    #[cfg(feature = "webui")]
    fn render_web(&self, _app_state: &dyn AppState) -> String {
        // pulldown-cmark converts markdown → HTML, escaping text content itself.
        let parser = pulldown_cmark::Parser::new(&self.markdown_text);
        let mut body = String::new();
        pulldown_cmark::html::push_html(&mut body, parser);
        format!(
            "<div id=\"{}\" class=\"lq-element lq-MarkdownElement\">{}</div>",
            crate::ui::web::element_dom_id(self.handle()),
            body
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app_state::DirectAppState;
    use crate::ui::message::{app_message_channel, AssetSnapshot};
    use liquers_core::metadata::Status;

    fn create_test_context() -> (UIContext, crate::ui::message::AppMessageReceiver) {
        let (tx, rx) = app_message_channel();
        let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
            Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
        let ctx = UIContext::new(app_state, tx);
        (ctx, rx)
    }

    // ─── Construction ───────────────────────────────────────────────────────

    #[test]
    fn test_new() {
        let elem = MarkdownElement::new("Title".to_string(), "# Hello".to_string());
        assert_eq!(elem.title(), "Title");
        assert_eq!(elem.markdown_text(), "# Hello");
        assert!(elem.handle().is_none());
    }

    #[test]
    fn test_new_empty_text() {
        let elem = MarkdownElement::new("Empty".to_string(), String::new());
        assert_eq!(elem.markdown_text(), "");
    }

    // ─── UIElement Trait ────────────────────────────────────────────────────

    #[test]
    fn test_type_name() {
        let elem = MarkdownElement::new("T".to_string(), "text".to_string());
        assert_eq!(elem.type_name(), "MarkdownElement");
    }

    #[test]
    fn test_handle_and_set_handle() {
        let mut elem = MarkdownElement::new("T".to_string(), "text".to_string());
        assert!(elem.handle().is_none());
        elem.set_handle(UIHandle(42));
        assert_eq!(elem.handle(), Some(UIHandle(42)));
    }

    #[test]
    fn test_title_and_set_title() {
        let mut elem = MarkdownElement::new("Initial".to_string(), "text".to_string());
        assert_eq!(elem.title(), "Initial");
        elem.set_title("Updated".to_string());
        assert_eq!(elem.title(), "Updated");
    }

    #[test]
    fn test_clone_boxed() {
        let mut elem = MarkdownElement::new("Clone".to_string(), "# Heading".to_string());
        elem.set_handle(UIHandle(7));
        let boxed: Box<dyn UIElement> = Box::new(elem);
        let cloned = boxed.clone_boxed();
        assert_eq!(cloned.type_name(), "MarkdownElement");
        assert_eq!(cloned.title(), "Clone");
        assert_eq!(cloned.handle(), Some(UIHandle(7)));
    }

    /// A long document scrolls inside the element: it takes no more than the space it is given,
    /// so a host does not grow a second scroll bar around it.
    #[cfg(feature = "egui")]
    #[test]
    fn long_markdown_stays_within_its_space() {
        let text = (0..200).map(|i| format!("Line {i}\n")).collect::<Vec<_>>().join("\n");
        let mut elem = MarkdownElement::new("T".to_string(), text);
        assert!(elem.scrolls_itself());
        let (ctx, _rx) = create_test_context();
        let mut state = crate::ui::app_state::DirectAppState::new();
        let egui_ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))),
            ..Default::default()
        };
        let mut used = 0.0;
        let mut output = egui_ctx.run_ui(input, |ui| {
            let top = ui.cursor().top();
            elem.show_in_egui(ui, &ctx, &mut state);
            used = ui.cursor().top() - top;
        });
        output.textures_delta.clear();
        assert!(used <= 310.0, "the element took {used} px of a 300 px window");
    }

    #[test]
    fn test_is_initialised() {
        let mut elem = MarkdownElement::new("T".to_string(), "text".to_string());
        assert!(!elem.is_initialised());
        let (ctx, _rx) = create_test_context();
        elem.init(UIHandle(1), &ctx).expect("init");
        assert!(elem.is_initialised());
    }

    #[test]
    fn test_get_value_returns_markdown_text() {
        let elem = MarkdownElement::new("T".to_string(), "# Hello".to_string());
        let value = elem.get_value().expect("should have value");
        assert_eq!(value.try_into_string().expect("string"), "# Hello");
    }

    #[test]
    fn test_get_metadata_returns_none() {
        let elem = MarkdownElement::new("T".to_string(), "text".to_string());
        assert!(elem.get_metadata().is_none());
    }

    // ─── Update ─────────────────────────────────────────────────────────────

    #[test]
    fn test_asset_update_changes_text() {
        let mut elem = MarkdownElement::new("T".to_string(), "old".to_string());
        let (ctx, _rx) = create_test_context();
        let snapshot = AssetSnapshot {
            value: Some(Arc::new(Value::from("# New content"))),
            metadata: liquers_core::metadata::Metadata::new(),
            error: None,
            status: Status::Ready,
            query: None,
            preview: false,
        };
        let response = elem.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);
        assert_eq!(response, UpdateResponse::NeedsRepaint);
        assert_eq!(elem.markdown_text(), "# New content");
    }

    #[test]
    fn test_asset_update_no_value_unchanged() {
        let mut elem = MarkdownElement::new("T".to_string(), "original".to_string());
        let (ctx, _rx) = create_test_context();
        let snapshot = AssetSnapshot {
            value: None,
            metadata: liquers_core::metadata::Metadata::new(),
            error: None,
            status: Status::Submitted,
            query: None,
            preview: false,
        };
        let response = elem.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);
        assert_eq!(response, UpdateResponse::Unchanged);
        assert_eq!(elem.markdown_text(), "original");
    }

    #[test]
    fn test_timer_update_unchanged() {
        let mut elem = MarkdownElement::new("T".to_string(), "text".to_string());
        let (ctx, _rx) = create_test_context();
        assert_eq!(
            elem.update(&UpdateMessage::Timer { elapsed_ms: 100 }, &ctx),
            UpdateResponse::Unchanged
        );
    }

    // ─── Serialization ──────────────────────────────────────────────────────

    #[test]
    fn test_serialization_roundtrip() {
        let mut elem = MarkdownElement::new("Doc".to_string(), "# Title\n\nBody".to_string());
        elem.set_handle(UIHandle(55));

        let boxed: Box<dyn UIElement> = Box::new(elem);
        let json = serde_json::to_string(&boxed).expect("serialize");
        assert!(json.contains("MarkdownElement"));

        let restored: Box<dyn UIElement> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.type_name(), "MarkdownElement");
        assert_eq!(restored.handle(), Some(UIHandle(55)));
        assert_eq!(restored.title(), "Doc");
        // markdown_text is persistent
        let value = restored.get_value().expect("value preserved");
        assert_eq!(value.try_into_string().expect("string"), "# Title\n\nBody");
    }

    #[test]
    fn test_typetag_roundtrip() {
        let elem = MarkdownElement::new("T".to_string(), "**bold**".to_string());
        let boxed: Box<dyn UIElement> = Box::new(elem);
        let json = serde_json::to_string(&boxed).expect("serialize");
        let restored: Box<dyn UIElement> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.type_name(), "MarkdownElement");
    }
}
