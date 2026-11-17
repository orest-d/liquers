//! [`EditorElement`]: the `UIElement` of the query, action, resource and key editors.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use liquers_core::error::Error;
use liquers_core::value::ValueInterface;

use super::command::{EditorCommand, EditorEffect};
use super::state::{EditorKind, EditorState};
use super::tree::EditorTree;
use super::view::EditorView;
use crate::ui::element::{UIElement, UpdateMessage, UpdateResponse};
use crate::ui::handle::UIHandle;
use crate::ui::message::{AccessorOp, AccessorUpdate, AppMessage};
use crate::ui::ui_context::UIContext;
use crate::value::Value;

/// Where the edited value lives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EditorBinding {
    /// An accessor's query, e.g. `-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query`:
    /// the runner evaluates it and takes the accessor.
    Accessor(String),
    /// The value itself, as text (a query, a pipeline's JSON or a key); the runner keeps it in a
    /// memory accessor for the session, and the element keeps the latest text here.
    Memory(String),
}

fn yes() -> bool {
    true
}

/// An editor of a query or pipeline (a tree of its steps beside the highlighted text), of one
/// action, of a resource or of a key, bound to where its value lives. See the module docs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorElement {
    handle: Option<UIHandle>,
    title: String,
    kind: EditorKind,
    binding: EditorBinding,
    /// "Update immediately" (AC-26): each committed edit is set into the accessor at once;
    /// otherwise edits wait for Update.
    #[serde(default = "yes")]
    continuous: bool,
    /// An element re-pointed at the edited query after each write (a memory binding has no
    /// dependents to expire, AC-13).
    #[serde(default)]
    follow: Option<UIHandle>,
    #[serde(skip)]
    state: Option<EditorState>,
    #[serde(skip)]
    #[cfg_attr(not(feature = "egui"), allow(dead_code))]
    view: EditorView,
    #[serde(skip)]
    bound: bool,
}

impl EditorElement {
    /// A new editor of `kind` over `binding`.
    pub fn new(title: impl Into<String>, kind: EditorKind, binding: EditorBinding) -> Self {
        EditorElement {
            handle: None,
            title: title.into(),
            kind,
            binding,
            continuous: true,
            follow: None,
            state: None,
            view: EditorView::default(),
            bound: false,
        }
    }

    /// Sets "Update immediately" (builder style).
    pub fn with_continuous(mut self, continuous: bool) -> Self {
        self.continuous = continuous;
        self
    }

    /// Re-points `handle` at the edited query after each write (builder style).
    pub fn with_follow(mut self, handle: UIHandle) -> Self {
        self.follow = Some(handle);
        self
    }

    pub fn kind(&self) -> EditorKind {
        self.kind
    }
    pub fn binding(&self) -> &EditorBinding {
        &self.binding
    }
    pub fn continuous(&self) -> bool {
        self.continuous
    }
    /// The core, once the element has a registry.
    pub fn state(&self) -> Option<&EditorState> {
        self.state.as_ref()
    }
    /// The tree to draw, once the element has a registry.
    pub fn tree(&self) -> Option<&EditorTree> {
        self.state.as_ref().map(EditorState::tree)
    }

    /// Builds the core from the context's registry and binds the accessor, once.
    fn ensure_state(&mut self, ctx: &UIContext) {
        if self.state.is_none() {
            if let Some(registry) = ctx.command_registry() {
                self.state = Some(EditorState::new(self.kind, registry.clone()).with_continuous(self.continuous));
            }
        }
        if !self.bound && self.state.is_some() {
            if let Some(handle) = self.handle {
                self.bound = true;
                ctx.send_message(AppMessage::Accessor {
                    handle,
                    binding: self.binding.clone(),
                    op: AccessorOp::Watch,
                });
            }
        }
    }

    /// Applies commands from a renderer and carries out their effects. Returns whether anything
    /// was applied.
    pub fn apply_commands(&mut self, commands: Vec<EditorCommand>, ctx: &UIContext) -> bool {
        if commands.is_empty() {
            return false;
        }
        self.ensure_state(ctx);
        let mut effects = Vec::new();
        if let Some(state) = &mut self.state {
            for command in commands {
                // A refused edit is shown in the tree's message; the others still apply.
                if let Ok(effect) = state.apply(command) {
                    effects.push(effect);
                }
            }
            self.continuous = state.continuous();
        }
        for effect in effects {
            self.run_effect(effect, ctx);
        }
        true
    }

    fn run_effect(&mut self, effect: EditorEffect, ctx: &UIContext) {
        let Some(handle) = self.handle else {
            return;
        };
        match effect {
            EditorEffect::None => {}
            EditorEffect::Write { value, expected } => ctx.send_message(AppMessage::Accessor {
                handle,
                binding: self.binding.clone(),
                op: AccessorOp::SetIf { value, expected },
            }),
            EditorEffect::Reload => ctx.send_message(AppMessage::Accessor {
                handle,
                binding: self.binding.clone(),
                op: AccessorOp::Get,
            }),
            EditorEffect::ListDirectory(folder) => {
                ctx.send_message(AppMessage::ListDirectory { handle, folder })
            }
        }
    }

    /// After a write: keep a memory binding's text, and re-point the followed view.
    fn after_write(&mut self, ctx: &UIContext) {
        let Some(value) = self.state.as_ref().and_then(EditorState::value) else {
            return;
        };
        let text = value_text(&value);
        if let EditorBinding::Memory(_) = &self.binding {
            self.binding = EditorBinding::Memory(text.clone());
        }
        if let (Some(follow), EditorKind::Query) = (self.follow, self.kind) {
            ctx.send_message(AppMessage::RequestAssetUpdates { handle: follow, query: text });
        }
    }

    fn on_accessor(&mut self, update: &AccessorUpdate, ctx: &UIContext) {
        let mut effect = EditorEffect::None;
        let mut written = false;
        if let Some(state) = &mut self.state {
            match update {
                AccessorUpdate::Bound { address, writable } => {
                    if !writable {
                        state.set_read_only(Some("The accessor cannot be written.".to_string()));
                    }
                    if address.is_empty() {
                        if let EditorBinding::Accessor(_) = &self.binding {
                            // Made in code: it works for the session but will not re-bind.
                            let text = state.value().map(|v| value_text(&v)).unwrap_or_default();
                            self.binding = EditorBinding::Memory(text);
                        }
                    }
                }
                AccessorUpdate::Snapshot { value, fingerprint } => {
                    let _ = state.on_snapshot(value, *fingerprint);
                }
                AccessorUpdate::Written { fingerprint } => {
                    effect = state.on_written(*fingerprint);
                    written = true;
                }
                AccessorUpdate::Conflict { value, fingerprint } => state.on_conflict(value, *fingerprint),
                AccessorUpdate::Failed(error) => {
                    if state.value().is_none() {
                        state.set_read_only(Some(error.to_string()));
                    } else {
                        state.on_write_failed(error);
                    }
                }
            }
        }
        if written {
            self.after_write(ctx);
        }
        self.run_effect(effect, ctx);
    }
}

/// The text form of a working-copy value: a query's encoding, a pipeline's JSON, a key.
fn value_text(value: &Value) -> String {
    match value.try_into_query() {
        Ok(q) => q.encode(),
        Err(_) => match value.try_into_pipeline() {
            Ok(p) => p.to_json().to_string(),
            Err(_) => value.try_into_string().unwrap_or_default(),
        },
    }
}

#[typetag::serde]
impl UIElement for EditorElement {
    fn type_name(&self) -> &'static str {
        "EditorElement"
    }

    fn handle(&self) -> Option<UIHandle> {
        self.handle
    }

    fn set_handle(&mut self, handle: UIHandle) {
        self.handle = Some(handle);
    }

    fn title(&self) -> String {
        self.title.clone()
    }

    fn set_title(&mut self, title: String) {
        self.title = title;
    }

    fn clone_boxed(&self) -> Box<dyn UIElement> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn init(&mut self, handle: UIHandle, ctx: &UIContext) -> Result<(), Error> {
        self.set_handle(handle);
        self.bound = false;
        self.ensure_state(ctx);
        Ok(())
    }

    fn update(&mut self, message: &UpdateMessage, ctx: &UIContext) -> UpdateResponse {
        self.ensure_state(ctx);
        match message {
            UpdateMessage::Accessor(update) => {
                self.on_accessor(update, ctx);
                UpdateResponse::NeedsRepaint
            }
            UpdateMessage::Directory(listing) => {
                if let Some(state) = &mut self.state {
                    state.on_directory(listing.clone());
                }
                UpdateResponse::NeedsRepaint
            }
            UpdateMessage::UI(event) => {
                let commands = match self.tree() {
                    Some(tree) => EditorView::from_ui_event(event, tree),
                    None => vec![],
                };
                if self.apply_commands(commands, ctx) {
                    UpdateResponse::NeedsRepaint
                } else {
                    UpdateResponse::Unchanged
                }
            }
            UpdateMessage::AssetNotification(_)
            | UpdateMessage::Timer { .. }
            | UpdateMessage::Custom(_)
            | UpdateMessage::AssetUpdate(_) => UpdateResponse::Unchanged,
        }
    }

    fn get_value(&self) -> Option<Arc<Value>> {
        self.state.as_ref().and_then(EditorState::value).map(Arc::new)
    }

    #[cfg(feature = "egui")]
    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &UIContext,
        _app_state: &mut dyn crate::ui::app_state::AppState,
    ) -> egui::Response {
        self.ensure_state(ctx);
        let id = egui::Id::new(("lq-editor", self.handle.map(|h| h.0).unwrap_or(0)));
        let Some(state) = &self.state else {
            return ui.label("Editor: no command registry");
        };
        let tree = state.tree().clone();
        let inner = ui.push_id(id, |ui| crate::egui::editor::show_editor(ui, &tree, &mut self.view));
        self.apply_commands(inner.inner, ctx);
        inner.response
    }

    #[cfg(feature = "webui")]
    fn render_web(&self, _app_state: &dyn crate::ui::app_state::AppState) -> String {
        let dom_id = crate::ui::web::element_dom_id(self.handle);
        match self.tree() {
            Some(tree) => format!(
                "<div id=\"{dom_id}\" class=\"lq-element lq-EditorElement\">{}</div>",
                crate::ui::web::editor::editor_html(tree, &self.title)
            ),
            None => format!(
                "<div id=\"{dom_id}\" class=\"lq-element lq-EditorElement\">{}</div>",
                crate::ui::web::html::escape_html(&self.title)
            ),
        }
    }
}
