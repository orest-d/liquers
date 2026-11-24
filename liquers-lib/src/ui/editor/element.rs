//! [`EditorElement`]: the `UIElement` of the query, action, resource and key editors.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use liquers_core::accessor::ValueAccessor;
use liquers_core::error::Error;
use liquers_core::value::ValueInterface;

use super::command::{EditorCommand, EditorEffect};
use super::state::{EditorKind, EditorState, UpdateMode};
use super::layout::EditorMode;
use super::tree::{EditorTree, NodePath};
use super::view::EditorView;
use crate::ui::element::{UIElement, UpdateMessage, UpdateResponse};
use crate::ui::handle::UIHandle;
use crate::ui::theme::{Density, ThemeState};
use crate::ui::message::{AccessorOp, AccessorUpdate, AppMessage};
use crate::ui::ui_context::UIContext;
use crate::ui::widgets::value_display::ValueDisplay;
use crate::value::Value;

/// The event an editor sends the view it follows when its preview ends and the view should show
/// its own query again (a recipe's view: its own query reads the recipe, which a write expires).
pub const PREVIEW_END_EVENT: &str = "lq:preview-end";

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

/// The folder of the `recipes.yaml` an accessor address reads a recipe of
/// (`-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query` → `reports`), if it does.
pub fn recipe_folder(address: &str) -> Option<String> {
    let end = address.find("/recipes.yaml")?;
    let head = &address[..end];
    let folder = head
        .strip_prefix("-R-key/")
        .or_else(|| head.strip_prefix("-R/"))
        .or_else(|| head.strip_prefix("-R-stored/"))?;
    if folder.is_empty() || folder.contains("/-/") {
        return None;
    }
    Some(folder.to_owned())
}

/// An editor of a query or pipeline (a tree of its steps beside the highlighted text), of one
/// action, of a resource or of a key, bound to where its value lives. See the module docs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorElement {
    handle: Option<UIHandle>,
    title: String,
    kind: EditorKind,
    binding: EditorBinding,
    /// When an edit takes effect (query-update-modes AC-20); `None` until set or resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    update_mode: Option<UpdateMode>,
    /// "Update immediately" of an element saved before update modes: `true` loads as Apply
    /// immediately, `false` as On demand (AC-15). Read, never written.
    #[serde(default, skip_serializing)]
    continuous: Option<bool>,
    /// An element re-pointed at the edited query after each write (a memory binding has no
    /// dependents to expire, AC-13).
    #[serde(default)]
    follow: Option<UIHandle>,
    /// Simple or Advanced (AC-1, AC-2, AC-3); saved, and Simple for a new editor.
    #[serde(default)]
    mode: EditorMode,
    /// The editor's own density (AC-11); `None` follows the theme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    density: Option<Density>,
    /// The theme last seen through the context, for renderers that get no context.
    #[serde(skip)]
    theme: Option<ThemeState>,
    #[serde(skip)]
    state: Option<EditorState>,
    #[serde(skip)]
    #[cfg_attr(not(feature = "egui"), allow(dead_code))]
    view: EditorView,
    #[serde(skip)]
    bound: bool,
    /// The preview of an editor that follows no view, shown in its own area (AC-17).
    #[serde(skip)]
    display: ValueDisplay,
    /// An accessor made in code (empty `address()`), bound as it is for the session.
    #[serde(skip)]
    live: Option<Arc<dyn ValueAccessor<Value>>>,
    /// The step the browser opens in Advanced mode (a step chip's), else the last action.
    #[serde(skip)]
    #[cfg_attr(not(feature = "webui"), allow(dead_code))]
    open_card: Option<NodePath>,
}

impl EditorElement {
    /// A new editor of `kind` over `binding`.
    pub fn new(title: impl Into<String>, kind: EditorKind, binding: EditorBinding) -> Self {
        EditorElement {
            handle: None,
            title: title.into(),
            kind,
            binding,
            update_mode: None,
            continuous: None,
            follow: None,
            mode: EditorMode::Simple,
            density: None,
            theme: None,
            state: None,
            view: EditorView::default(),
            bound: false,
            display: ValueDisplay::default(),
            live: None,
            open_card: None,
        }
    }

    /// An editor of `kind` over an accessor value: its `address()` becomes the binding, or, for
    /// an accessor made in code, the accessor is used as it is for the session.
    pub fn from_accessor(
        title: impl Into<String>,
        kind: EditorKind,
        accessor: Arc<dyn ValueAccessor<Value>>,
    ) -> Self {
        let address = accessor.address().encode();
        if address.is_empty() {
            let mut element = EditorElement::new(title, kind, EditorBinding::Memory(String::new()));
            element.live = Some(accessor);
            element
        } else {
            EditorElement::new(title, kind, EditorBinding::Accessor(address))
        }
    }

    /// Starts in `mode` (builder style); see [`update_mode`](Self::update_mode).
    pub fn with_update_mode(mut self, mode: UpdateMode) -> Self {
        self.update_mode = Some(self.offered(mode));
        self
    }

    /// The modes this editor offers: Live preview needs a whole query to evaluate, so an action,
    /// resource or key editor offers On demand and Apply immediately.
    pub fn update_modes(&self) -> &'static [UpdateMode] {
        match self.kind {
            EditorKind::Query => &UpdateMode::ALL,
            EditorKind::Action | EditorKind::Resource | EditorKind::Key => {
                &[UpdateMode::OnDemand, UpdateMode::ApplyImmediately]
            }
        }
    }

    /// `mode`, or On demand where Live preview is not offered.
    fn offered(&self, mode: UpdateMode) -> UpdateMode {
        if self.update_modes().contains(&mode) {
            mode
        } else {
            UpdateMode::OnDemand
        }
    }

    /// When an edit takes effect: the mode set, else the legacy "Update immediately" of an old
    /// saved element, else Live preview (Apply immediately where Live preview is not offered).
    pub fn update_mode(&self) -> UpdateMode {
        if let Some(mode) = self.update_mode {
            return mode;
        }
        match self.continuous {
            Some(true) => UpdateMode::ApplyImmediately,
            Some(false) => UpdateMode::OnDemand,
            None => match self.kind {
                EditorKind::Query => UpdateMode::LivePreview,
                EditorKind::Action | EditorKind::Resource | EditorKind::Key => UpdateMode::ApplyImmediately,
            },
        }
    }

    /// The preview shown in the editor's own area, when it follows no view.
    pub fn display(&self) -> &ValueDisplay {
        &self.display
    }

    /// Re-points `handle` at the edited query after each write (builder style).
    pub fn with_follow(mut self, handle: UIHandle) -> Self {
        self.follow = Some(handle);
        self
    }

    /// Starts in `mode` (builder style).
    pub fn with_mode(mut self, mode: EditorMode) -> Self {
        self.mode = mode;
        self
    }

    /// Uses `density` instead of the theme's (builder style).
    pub fn with_density(mut self, density: Density) -> Self {
        self.density = Some(density);
        self
    }

    /// Simple or Advanced.
    pub fn mode(&self) -> EditorMode {
        self.mode
    }

    /// Switches the mode; every edit is kept (AC-3).
    pub fn set_mode(&mut self, mode: EditorMode) {
        self.mode = mode;
    }

    /// The editor's own density, if it has one.
    pub fn density(&self) -> Option<Density> {
        self.density
    }

    /// Sets or clears the editor's own density.
    pub fn set_density(&mut self, density: Option<Density>) {
        self.density = density;
    }

    /// The density to draw with: the editor's own, else the theme's.
    pub fn effective_density(&self, theme: &ThemeState) -> Density {
        self.density.unwrap_or(theme.density)
    }

    /// The theme last seen through the context (the built-in one before any).
    pub fn theme_state(&self) -> ThemeState {
        self.theme.clone().unwrap_or_default()
    }

    pub fn kind(&self) -> EditorKind {
        self.kind
    }
    pub fn binding(&self) -> &EditorBinding {
        &self.binding
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
        self.theme = Some(ctx.theme());
        if self.state.is_none() {
            if let Some(registry) = ctx.command_registry() {
                let mode = self.update_mode();
                self.update_mode = Some(mode);
                self.continuous = None;
                self.state = Some(
                    EditorState::new(self.kind, registry.clone())
                        .with_update_modes(self.update_modes())
                        .with_update_mode(mode),
                );
            }
        }
        if !self.bound && self.state.is_some() {
            if let Some(handle) = self.handle {
                self.bound = true;
                let op = match self.live.take() {
                    Some(accessor) => AccessorOp::Adopt(accessor),
                    None => AccessorOp::Watch,
                };
                ctx.send_message(AppMessage::Accessor { handle, binding: self.binding.clone(), op });
                // A recipe of a `recipes.yaml`: its folder's nodes, for Node arguments.
                if let EditorBinding::Accessor(address) = &self.binding {
                    if let Some(folder) = recipe_folder(address) {
                        ctx.send_message(AppMessage::NodeContext { handle, folder });
                    }
                }
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
            self.update_mode = Some(state.update_mode());
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
            EditorEffect::RenameData { folder, editor } => {
                ctx.send_message(AppMessage::RenameData { handle, folder, editor })
            }
            EditorEffect::Preview(value) => {
                // In place, in the followed view, else in the editor's own area (AC-17); a
                // recipe's preview runs in the recipe's folder (AC-18).
                let target = self.follow.unwrap_or(handle);
                let cwd = self.preview_cwd();
                ctx.send_message(AppMessage::Preview { handle: target, query: value_text(&value), cwd });
            }
            EditorEffect::ClearPreview => self.end_preview(ctx),
        }
    }

    /// The preview is dropped (Revert, Undo to the applied value) or applied (a write): the
    /// view shows the applied value again (AC-17).
    fn end_preview(&mut self, ctx: &UIContext) {
        let Some(follow) = self.follow else {
            self.display.clear();
            return;
        };
        match self.preview_cwd() {
            // A recipe's view asks for its own query again: the recipe's text alone would not
            // resolve its folder's keys.
            Some(_) => ctx.send_message(AppMessage::UIEvent {
                handle: follow,
                event: crate::ui::event::UIEvent::new(crate::ui::event::UIEventKind::Named(
                    PREVIEW_END_EVENT.to_string(),
                )),
            }),
            None => {
                if let (Some(value), EditorKind::Query) = (self.state.as_ref().and_then(EditorState::value), self.kind) {
                    ctx.send_message(AppMessage::RequestAssetUpdates { handle: follow, query: value_text(&value) });
                }
            }
        }
    }

    /// The folder of the recipe this editor is bound to, if it is.
    fn preview_cwd(&self) -> Option<String> {
        match &self.binding {
            EditorBinding::Accessor(address) => recipe_folder(address),
            EditorBinding::Memory(_) => None,
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
        // The followed view shows the applied query: a recipe's view by expiry (the write
        // expires what depends on the recipe) after leaving a preview; any other is re-pointed.
        self.end_preview(ctx);
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
                    if let EditorBinding::Memory(_) = &self.binding {
                        // The fallback after a reload, for a memory or code-made binding.
                        if let Some(v) = state.value() {
                            self.binding = EditorBinding::Memory(value_text(&v));
                        }
                    }
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
            // `after_write` has re-pointed the view: a `ClearPreview` of the write is done.
            if let EditorEffect::ClearPreview = effect {
                return;
            }
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
            UpdateMessage::NodeContext(context) => {
                if let Some(state) = &mut self.state {
                    state.set_node_context(Some(context.clone()));
                }
                UpdateResponse::NeedsRepaint
            }
            UpdateMessage::UI(event) => {
                if let Some(mode) = EditorView::mode_event(event) {
                    // Commit what was typed but not yet submitted, then switch (AC-3).
                    let drafts = match self.tree() {
                        Some(tree) => EditorView::draft_commands(event, tree),
                        None => vec![],
                    };
                    self.apply_commands(drafts, ctx);
                    // A step chip names the step Advanced mode opens.
                    self.open_card = EditorView::mode_event_path(event);
                    self.mode = mode;
                    return UpdateResponse::NeedsRepaint;
                }
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
            UpdateMessage::AssetUpdate(snapshot) => {
                // The preview of an editor that follows no view (AC-17).
                match self.display.on_snapshot(snapshot) {
                    crate::ui::widgets::SnapshotOutcome::Ignored => UpdateResponse::Unchanged,
                    crate::ui::widgets::SnapshotOutcome::Progress
                    | crate::ui::widgets::SnapshotOutcome::Settled => UpdateResponse::NeedsRepaint,
                }
            }
            UpdateMessage::AssetNotification(_) | UpdateMessage::Timer { .. } | UpdateMessage::Custom(_) => {
                UpdateResponse::Unchanged
            }
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
        let theme = ctx.theme();
        let density = self.effective_density(&theme);
        let mode = self.mode;
        let view = &mut self.view;
        let inner = ui.push_id(id, |ui| {
            crate::egui::editor::show_editor(ui, &tree, view, mode, density, &theme, Some(ctx))
        });
        if let Some(mode) = inner.inner.mode {
            self.mode = mode;
        }
        self.apply_commands(inner.inner.commands, ctx);
        // The preview of an editor that follows no view, framed in the preview colour (AC-22).
        if let Some(value) = self.display.value().filter(|_| self.display.is_preview()).cloned() {
            let colour = crate::egui::theme::color(theme.theme.palette(theme.mode).state.preview);
            egui::Frame::new()
                .stroke(egui::Stroke::new(2.0, colour))
                .inner_margin(egui::Margin::same(4))
                .show(ui, |ui| {
                    if self.display.is_dimmed() {
                        ui.multiply_opacity(0.45);
                    }
                    use crate::egui::UIValueExtension;
                    value.as_ref().show(ui);
                });
        }
        inner.response
    }

    #[cfg(feature = "webui")]
    fn render_web(&self, app_state: &dyn crate::ui::app_state::AppState) -> String {
        let dom_id = crate::ui::web::element_dom_id(self.handle);
        // The preview of an editor that follows no view (AC-17, AC-22).
        let preview = match self.display.value().filter(|_| self.display.is_preview()) {
            Some(value) => format!(
                "<div class=\"lq-ed-own-preview lq-value-preview{}\">{}</div>",
                if self.display.is_dimmed() { " lq-value-stale" } else { "" },
                crate::ui::web::html::value_to_html(value.as_ref(), app_state)
            ),
            None => String::new(),
        };
        match self.tree() {
            Some(tree) => format!(
                "<div id=\"{dom_id}\" class=\"lq-element lq-EditorElement\">{}{preview}</div>",
                {
                    let theme = self.theme_state();
                    let density = self.effective_density(&theme);
                    crate::ui::web::editor::editor_html_open(
                        tree,
                        &self.title,
                        self.mode,
                        density,
                        &theme.theme,
                        self.open_card.as_ref(),
                    )
                }
            ),
            None => format!(
                "<div id=\"{dom_id}\" class=\"lq-element lq-EditorElement\">{}</div>",
                crate::ui::web::html::escape_html(&self.title)
            ),
        }
    }
}
