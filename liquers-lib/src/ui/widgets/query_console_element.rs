//! [`QueryConsoleElement`]: a browser-like query console.
//!
//! The console holds a query text with history, shows the evaluated value or its metadata, offers
//! the next presets of the last command, and opens an editor panel from its edit (pencil) button:
//! an editor (`ui::editor::EditorState`) of the whole query, drawn as a narrow column beside the
//! value, in Simple mode (the last action) or Advanced mode (every step, all collapsed but one).
//! The panel does not repeat the query text; `Ctrl+J` in the console's query field opens the
//! panel on the step under the cursor, in Advanced mode when that is not the last action.
//!
//! The console has two update modes ([`UpdateMode`]): **Apply immediately** (the default) and
//! **On demand**. Every edit made in the panel rewrites the query text at once. In Apply
//! immediately the value is recalculated 300 ms after the last such edit; On demand, only when
//! ✓ (beside the query field) or Enter is pressed. Text typed in the query field is taken only on
//! Enter or ✓, and only when it parses and plans; otherwise the error is shown and nothing is
//! evaluated. The panel's own editor always applies at once and shows no mode control or ✓ of
//! its own: the console decides when to evaluate. Undo and Redo (toolbar buttons, `Ctrl+Z`,
//! `Ctrl+Shift+Z`) step through the panel's edits.
//!
//! The value is shown through a [`ValueDisplay`]: a new value replaces the old one when it
//! arrives, with no empty frame between; an evaluation still running after 250 ms dims the old
//! value and shows a status bar with its progress and last log message; after 3 s the status bar
//! offers On demand. When the asset expires (a dependency changed), Apply immediately
//! recalculates it and On demand dims the value with "changed upstream" until ✓.
//!
//! # Main components
//!
//! - [`QueryConsoleElement`]: the element; `show_parameters`, `parameters_tree` and
//!   `apply_parameter_commands` drive the editor panel (still called the Parameters panel in
//!   the API); `update_mode`, `set_update_mode` and `check_mark_enabled` the update modes.
//!
//! # Where it is used
//!
//! - The `lui/query_console` command (`ui::commands`) creates it; egui and the browser draw it.
//!
//! # Related concepts
//!
//! - `ui::editor`: the editors; the panel reuses their core and egui renderer.
//! - `ui::widgets::value_display`: how the value is kept while a new one is evaluated.
//! - `ui::schedule_event`: the console's timers (debounce, quiet, slow) are UI events
//!   (`lq:timer`) it sends to itself.
//! - `specs/design/query-pipeline-editor/` (the Parameters panel),
//!   `specs/design/query-editor-ergonomics/` (its modes) and `specs/design/query-update-modes/`
//!   (update modes, the value display).
//!
//! # Examples
//!
//! ```
//! # use liquers_lib::ui::editor::UpdateMode;
//! # use liquers_lib::ui::QueryConsoleElement;
//! let mut console = QueryConsoleElement::new("Console".into(), "-R/a.csv/-/head-3".into());
//! assert_eq!(console.update_mode(), UpdateMode::ApplyImmediately);
//! console.set_update_mode(UpdateMode::OnDemand);
//! assert_eq!(console.update_mode(), UpdateMode::OnDemand);
//! // Nothing evaluated yet: ✓ would evaluate the text.
//! assert!(console.check_mark_enabled());
//! ```

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::error::Error;
use liquers_core::metadata::{Metadata, Status};
use liquers_core::state::State;

use liquers_core::value::ValueInterface;

use crate::ui::app_state::AppState;
use crate::ui::editor::{
    EditorCommand, EditorEffect, EditorKind, EditorMode, EditorState, EditorTree, EditorView, NodePath,
    UpdateMode,
};
use crate::ui::theme::Density;
use crate::ui::element::{UIElement, UpdateMessage, UpdateResponse};
use crate::ui::event::{UIEvent, UIEventKind};
use crate::ui::handle::UIHandle;
use crate::ui::message::AppMessage;
use crate::ui::ui_context::UIContext;
use crate::ui::widgets::value_display::{DisplayTimer, SnapshotOutcome, ValueDisplay};
use crate::utils::NextPreset;
use crate::value::Value;

/// The event Enter in a Parameters panel field sends in the browser: it edits the query and never
/// submits it, unlike the console's own `Submit` (Enter in the query input, ✓).
const PARAMETERS_SUBMIT: &str = "qc:apply";
/// ✓: applies the query text (as Enter does).
const UPDATE_EVENT: &str = "qc:update";
/// `qc:mode::<name>`: switches the update mode.
const MODE_EVENT: &str = "qc:mode::";
const UNDO_EVENT: &str = "qc:undo";
const REDO_EVENT: &str = "qc:redo";
/// Milliseconds of quiet after a panel edit before Apply immediately evaluates (AC-1).
pub const DEBOUNCE_MS: u64 = 300;
/// Milliseconds before a volatile value is asked for again.
const VOLATILE_REFRESH_MS: u64 = 500;
/// The console's update modes, in the order its control shows them.
const CONSOLE_MODES: [UpdateMode; 2] = [UpdateMode::OnDemand, UpdateMode::ApplyImmediately];

fn console_default_mode() -> UpdateMode {
    UpdateMode::ApplyImmediately
}

/// Browser-like interactive query console widget.
///
/// Manages query history, command preset resolution, and data/metadata view toggle.
/// Receives `AssetSnapshot` updates pushed by AppRunner via
/// `UpdateMessage::AssetUpdate`.
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

    /// When the value is evaluated: Apply immediately or On demand (AC-20).
    #[serde(default = "console_default_mode")]
    update_mode: UpdateMode,

    /// The shown value, its metadata and error, and the evaluation awaited.
    #[serde(skip)]
    display: ValueDisplay,

    /// Why the typed text was not applied (AC-14).
    #[serde(skip)]
    typed_error: Option<Error>,

    /// The generation of the latest debounce timer, and the query it evaluates.
    #[serde(skip)]
    debounce: u64,
    #[serde(skip)]
    debounced_query: Option<String>,

    /// The generation whose volatile refresh is scheduled.
    #[serde(skip)]
    volatile_pending: Option<u64>,

    /// Cached next-command presets for the current query's last action.
    #[serde(skip)]
    next_presets: Vec<NextPreset>,

    /// Whether the Parameters panel (an editor of the whole query) is open.
    #[serde(default)]
    show_parameters: bool,

    /// The Parameters panel's mode (AC-12): Simple, the last action, unless switched.
    #[serde(default)]
    mode: EditorMode,

    /// The Parameters panel's density; Compact when not set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    density: Option<Density>,

    /// The theme last seen through the context, for `render_web`.
    #[serde(skip)]
    theme: Option<crate::ui::theme::ThemeState>,

    /// The editor behind the Parameters panel, over the applied query text. It persists across
    /// frames (its undo history with it) and is rebuilt only when a query is applied.
    #[serde(skip)]
    parameters: Option<EditorState>,

    /// What is being typed in the Parameters panel.
    #[serde(skip)]
    #[cfg_attr(not(feature = "egui"), allow(dead_code))]
    parameters_view: EditorView,

    /// The step the browser panel opens in Advanced mode (a step chip's), else the last action.
    #[serde(skip)]
    #[cfg_attr(not(feature = "webui"), allow(dead_code))]
    open_card: Option<NodePath>,
}

/// The Parameters panel's view of an editor tree: the console draws the query text (and so its
/// filename), and its own update controls, so the panel repeats neither; it keeps "Add action…"
/// and "Add key…", so an empty query can be built in the panel.
fn panel_tree(tree: &EditorTree) -> EditorTree {
    let mut tree = tree.clone();
    tree.text_editable = false;
    tree.hide_text = true;
    tree.host_controls = true;
    tree
}

/// Checks that `text` parses and, with a registry, plans (AC-14).
fn validate_query(text: &str, ctx: &UIContext) -> Result<(), Error> {
    let query = liquers_core::parse::parse_query(text)?;
    if let Some(registry) = ctx.command_registry() {
        liquers_core::plan::PlanBuilder::new(query, registry).build()?;
    }
    Ok(())
}

/// A timer event the console sends to itself.
fn timer_event(timer: &str, generation: u64) -> UIEvent {
    UIEvent::new(UIEventKind::Named(crate::ui::TIMER_EVENT.to_string()))
        .with_field("timer", Value::from(timer.to_string()))
        .with_field("generation", Value::from(generation.to_string()))
}

/// The id of a console's query field, so `Ctrl+J` can read and set its cursor.
#[cfg(feature = "egui")]
fn query_field_id(handle: Option<UIHandle>) -> egui::Id {
    egui::Id::new(("lq-console-query", handle.map(|h| h.0)))
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
            update_mode: console_default_mode(),
            display: ValueDisplay::default(),
            typed_error: None,
            debounce: 0,
            debounced_query: None,
            volatile_pending: None,
            next_presets: Vec::new(),
            show_parameters: false,
            mode: EditorMode::Simple,
            density: None,
            theme: None,
            parameters: None,
            parameters_view: EditorView::default(),
            open_card: None,
        }
    }

    /// Whether the Parameters panel is open.
    pub fn show_parameters(&self) -> bool {
        self.show_parameters
    }

    /// Opens or closes the Parameters panel. Opening it in Advanced mode collapses every step
    /// but one again.
    pub fn set_show_parameters(&mut self, on: bool) {
        if on && !self.show_parameters {
            self.parameters_view.shown_mode = None;
            self.open_card = None;
        }
        self.show_parameters = on;
    }

    /// When the value is evaluated. A console has two modes; Live preview (an editor's) reads as
    /// Apply immediately.
    pub fn update_mode(&self) -> UpdateMode {
        match self.update_mode {
            UpdateMode::OnDemand => UpdateMode::OnDemand,
            UpdateMode::LivePreview | UpdateMode::ApplyImmediately => UpdateMode::ApplyImmediately,
        }
    }

    /// Sets the update mode (Live preview is taken as Apply immediately). Switching drops a
    /// pending debounced evaluation; edits not yet evaluated wait for ✓.
    pub fn set_update_mode(&mut self, mode: UpdateMode) {
        self.update_mode = match mode {
            UpdateMode::OnDemand => UpdateMode::OnDemand,
            UpdateMode::LivePreview | UpdateMode::ApplyImmediately => UpdateMode::ApplyImmediately,
        };
        self.debounced_query = None;
    }

    /// Whether ✓ has something to apply: the text differs from the query last evaluated, or that
    /// query's asset changed upstream (AC-13, AC-19).
    pub fn check_mark_enabled(&self) -> bool {
        self.display.is_changed_upstream() || self.display.requested() != Some(self.query_text.as_str())
    }

    /// The shown value and the evaluation awaited.
    pub fn display(&self) -> &ValueDisplay {
        &self.display
    }

    /// Why the typed text was not applied, if it was not (AC-14).
    pub fn typed_error(&self) -> Option<&Error> {
        self.typed_error.as_ref()
    }

    /// Whether Undo and Redo have anything to step to in the panel.
    pub fn can_undo(&self) -> bool {
        self.parameters.as_ref().is_some_and(EditorState::can_undo)
    }

    /// See [`can_undo`](Self::can_undo).
    pub fn can_redo(&self) -> bool {
        self.parameters.as_ref().is_some_and(EditorState::can_redo)
    }

    /// `Ctrl+J` in the query text with the cursor at byte `offset`: opens the panel on the step
    /// under the cursor. A step other than the one Simple mode shows (normally the last action)
    /// opens Advanced mode with that step's card the only one expanded. Returns whether the
    /// cursor was on a step.
    pub fn jump_to_editor(&mut self, offset: usize, ctx: &UIContext) -> bool {
        self.set_show_parameters(true);
        let Some(tree) = self.parameters_tree(ctx) else {
            return false;
        };
        let Some(path) = tree.locate(offset).map(|s| s.path.clone()) else {
            return false;
        };
        let view = &mut self.parameters_view;
        view.text = None;
        view.jump_from_text(&tree, self.mode, offset);
        if let Some(mode) = view.mode_request.take() {
            self.mode = mode;
        }
        if self.mode == EditorMode::Advanced {
            // Opened by the jump: only the jumped-to step is expanded.
            view.shown_mode = None;
            view.enter_mode(&tree, EditorMode::Advanced);
            self.open_card = path.0.first().cloned().map(|s| NodePath(vec![s]));
        }
        true
    }

    /// The Parameters panel's mode.
    pub fn mode(&self) -> EditorMode {
        self.mode
    }

    /// Switches the Parameters panel's mode; nothing is submitted (AC-12).
    pub fn set_mode(&mut self, mode: EditorMode) {
        self.mode = mode;
    }

    /// The Parameters panel's density: its own, else Compact.
    pub fn density(&self) -> Density {
        self.density.unwrap_or(Density::Compact)
    }

    /// Sets or clears the Parameters panel's own density.
    pub fn set_density(&mut self, density: Option<Density>) {
        self.density = density;
    }

    /// The Parameters panel's tree: the applied query's tree without its own text field (the
    /// console has one), filename or update controls; Simple mode shows its last action as the
    /// card. The editor is built from the query text the first time and kept after that, so
    /// typing in the query field does not reset it (AC-15). `None` when the text does not parse
    /// or there is no registry.
    pub fn parameters_tree(&mut self, ctx: &UIContext) -> Option<EditorTree> {
        if self.parameters.is_none() {
            self.parameters = Self::build_editor(&self.query_text, ctx);
        }
        Some(panel_tree(self.parameters.as_ref()?.tree()))
    }

    /// A panel editor over `text`: it always applies at once, and the console owns its controls.
    fn build_editor(text: &str, ctx: &UIContext) -> Option<EditorState> {
        let registry = ctx.command_registry()?.clone();
        let query = liquers_core::parse::parse_query(text).ok()?;
        let mut state = EditorState::new(EditorKind::Query, registry)
            .with_update_modes(&CONSOLE_MODES)
            .with_update_mode(UpdateMode::ApplyImmediately)
            .with_host_controls(true);
        state.on_snapshot(&Value::from_query(&query), None).ok()?;
        Some(state)
    }

    /// Points the panel's editor at an applied text, keeping its undo history.
    fn sync_editor(&mut self, ctx: &UIContext) {
        let text = self.query_text.clone();
        let current = self
            .parameters
            .as_ref()
            .and_then(EditorState::value)
            .and_then(|v| v.try_into_query().ok())
            .map(|q| q.encode());
        let wanted = liquers_core::parse::parse_query(&text).ok().map(|q| q.encode());
        if current.is_some() && current == wanted {
            return;
        }
        let Some(state) = &mut self.parameters else {
            self.parameters = Self::build_editor(&text, ctx);
            return;
        };
        match state.apply(EditorCommand::SetText(text.clone())) {
            Ok(EditorEffect::Write { .. }) => {
                state.on_written(liquers_core::accessor::Fingerprint {
                    anchor: 0,
                    content: liquers_core::metadata::Version::new(0),
                });
            }
            Ok(EditorEffect::None)
            | Ok(EditorEffect::Reload)
            | Ok(EditorEffect::ListDirectory(_))
            | Ok(EditorEffect::RenameData { .. })
            | Ok(EditorEffect::Preview(_))
            | Ok(EditorEffect::ClearPreview)
            | Err(_) => self.parameters = Self::build_editor(&text, ctx),
        }
    }

    /// Applies edits made in the Parameters panel: each accepted edit rewrites the query text at
    /// once. In Apply immediately the text is evaluated after [`DEBOUNCE_MS`] of quiet (AC-1);
    /// On demand it waits for ✓ (AC-8). Returns whether the text changed.
    pub fn apply_parameter_commands(&mut self, commands: Vec<EditorCommand>, ctx: &UIContext) -> bool {
        if commands.is_empty() || self.parameters_tree(ctx).is_none() {
            return false;
        }
        let Some(state) = &mut self.parameters else {
            return false;
        };
        let mut changed = false;
        let mut folders: Vec<String> = Vec::new();
        for command in commands {
            match state.apply(command) {
                Ok(EditorEffect::Write { value, .. }) => {
                    if let Ok(q) = value.try_into_query() {
                        self.query_text = q.encode();
                        changed = true;
                    }
                    state.on_written(liquers_core::accessor::Fingerprint {
                        anchor: 0,
                        content: liquers_core::metadata::Version::new(0),
                    });
                }
                Ok(EditorEffect::ListDirectory(folder)) => folders.push(folder),
                // The console writes nothing but its text: no rename, no reload, and its editor
                // never previews (it always applies at once).
                Ok(EditorEffect::None)
                | Ok(EditorEffect::Reload)
                | Ok(EditorEffect::RenameData { .. })
                | Ok(EditorEffect::Preview(_))
                | Ok(EditorEffect::ClearPreview)
                | Err(_) => {}
            }
        }
        if let Some(handle) = self.handle {
            for folder in folders {
                ctx.send_message(AppMessage::ListDirectory { handle, folder });
            }
        }
        if changed {
            self.typed_error = None;
            match self.update_mode() {
                UpdateMode::ApplyImmediately => self.schedule_debounce(ctx),
                UpdateMode::OnDemand | UpdateMode::LivePreview => {}
            }
        }
        changed
    }

    /// Evaluates the query text after [`DEBOUNCE_MS`], unless another edit comes first.
    fn schedule_debounce(&mut self, ctx: &UIContext) {
        self.debounce += 1;
        self.debounced_query = Some(self.query_text.clone());
        if let Some(handle) = self.handle {
            crate::ui::schedule_event(
                ctx,
                handle,
                Duration::from_millis(DEBOUNCE_MS),
                timer_event("debounce", self.debounce),
            );
        }
    }

    /// Enter or ✓: applies the query text when it parses and plans; otherwise records why and
    /// evaluates nothing (AC-14). Returns whether it was applied.
    fn apply_typed(&mut self, ctx: &UIContext) -> bool {
        // An explicit Enter or ✓ supersedes a debounced panel edit, applied or not.
        self.debounced_query = None;
        if let Err(error) = validate_query(&self.query_text, ctx) {
            self.typed_error = Some(error);
            return false;
        }
        self.submit_query(ctx);
        true
    }

    /// Submit the current query_text for evaluation, recording it in the history.
    fn submit_query(&mut self, ctx: &UIContext) {
        if self.query_text.is_empty() {
            return;
        }
        if self.history.last() != Some(&self.query_text) {
            self.history.push(self.query_text.clone());
        }
        self.history_index = self.history.len();
        self.evaluate(ctx);
    }

    /// Evaluates the query text: the shown value stays until the new one arrives (AC-3), and the
    /// quiet and slow timers start (AC-4, AC-21).
    fn evaluate(&mut self, ctx: &UIContext) {
        if self.query_text.is_empty() {
            return;
        }
        self.typed_error = None;
        self.debounced_query = None;
        self.next_presets.clear();
        self.sync_editor(ctx);
        let query = self.query_text.clone();
        self.request(&query, ctx);
    }

    /// Asks the runner for `query` and starts the display's timers.
    fn request(&mut self, query: &str, ctx: &UIContext) {
        let generation = self.display.request(query, false);
        if let Some(handle) = self.handle {
            ctx.send_message(AppMessage::RequestAssetUpdates { handle, query: query.to_string() });
            for timer in [DisplayTimer::Quiet, DisplayTimer::Slow] {
                crate::ui::schedule_event(
                    ctx,
                    handle,
                    Duration::from_millis(timer.after_ms()),
                    timer_event(timer.name(), generation),
                );
            }
        }
    }

    /// A timer the console set: a debounce, a display timer or a volatile refresh.
    fn on_timer_event(&mut self, event: &UIEvent, ctx: &UIContext) -> UpdateResponse {
        let text = |name: &str| event.field(name).and_then(|v| v.try_into_string().ok());
        let Some(timer) = text("timer") else {
            return UpdateResponse::Unchanged;
        };
        let Some(generation) = text("generation").and_then(|g| g.parse::<u64>().ok()) else {
            return UpdateResponse::Unchanged;
        };
        if timer == "debounce" {
            if generation != self.debounce {
                return UpdateResponse::Unchanged;
            }
            let Some(query) = self.debounced_query.take() else {
                return UpdateResponse::Unchanged;
            };
            match self.update_mode() {
                UpdateMode::ApplyImmediately => {
                    self.query_text = query;
                    self.submit_query(ctx);
                    UpdateResponse::NeedsRepaint
                }
                UpdateMode::OnDemand | UpdateMode::LivePreview => UpdateResponse::Unchanged,
            }
        } else if timer == "volatile" {
            if self.volatile_pending == Some(generation) {
                self.volatile_pending = None;
            }
            if generation != self.display.generation() {
                return UpdateResponse::Unchanged;
            }
            let query = self.display.requested().unwrap_or(&self.query_text).to_string();
            if let (Some(handle), false) = (self.handle, query.is_empty()) {
                ctx.send_message(AppMessage::RequestAssetUpdates { handle, query });
            }
            UpdateResponse::Unchanged
        } else if let Some(timer) = DisplayTimer::from_name(&timer) {
            // Repaint only when the display changed: a re-render in the browser replaces the
            // whole console, so a no-op timer must not cause one.
            let before = (self.display.phase(), self.display.status_line());
            self.display.on_timer(generation, timer);
            if (self.display.phase(), self.display.status_line()) == before {
                UpdateResponse::Unchanged
            } else {
                UpdateResponse::NeedsRepaint
            }
        } else {
            UpdateResponse::Unchanged
        }
    }

    /// Handle a backend-neutral user event. Shared by every backend: the web driver reaches it
    /// through `update(UpdateMessage::UI)`, egui calls it inline from `show_in_egui`.
    ///
    /// `Submit` (Enter) and `qc:update` (✓) take the query from field `query` when present (the
    /// browser input), keep `query_text` otherwise (egui edits it in place), then apply it if
    /// it is valid.
    fn handle_ui_event(&mut self, event: &UIEvent, ctx: &UIContext) -> UpdateResponse {
        match &event.kind {
            UIEventKind::Submit => self.on_apply_event(event, ctx),
            UIEventKind::Named(name) if name == UPDATE_EVENT => self.on_apply_event(event, ctx),
            UIEventKind::Named(name) if name == crate::ui::TIMER_EVENT => self.on_timer_event(event, ctx),
            UIEventKind::Named(name) if name == crate::ui::editor::PREVIEW_END_EVENT => {
                // The editor this console follows dropped or applied its preview: show the
                // console's own query again (AC-17).
                if self.display.requested_preview() && !self.query_text.is_empty() {
                    self.evaluate(ctx);
                    UpdateResponse::NeedsRepaint
                } else {
                    UpdateResponse::Unchanged
                }
            }
            UIEventKind::Named(name) if name.starts_with(MODE_EVENT) => {
                match UpdateMode::from_name(&name[MODE_EVENT.len()..]) {
                    Ok(mode) => {
                        self.set_update_mode(mode);
                        UpdateResponse::NeedsRepaint
                    }
                    Err(_) => UpdateResponse::Unchanged,
                }
            }
            UIEventKind::Named(name) if name == UNDO_EVENT || name == REDO_EVENT => {
                let command = if name == UNDO_EVENT { EditorCommand::Undo } else { EditorCommand::Redo };
                if self.apply_parameter_commands(vec![command], ctx) {
                    UpdateResponse::NeedsRepaint
                } else {
                    UpdateResponse::Unchanged
                }
            }
            UIEventKind::Named(name) if name == "qc:params" => {
                self.set_show_parameters(!self.show_parameters);
                if self.show_parameters {
                    let _ = self.parameters_tree(ctx);
                }
                UpdateResponse::NeedsRepaint
            }
            UIEventKind::Named(name) if name == PARAMETERS_SUBMIT => {
                // Enter in a Parameters field edits the query; it never submits (AC-12).
                self.parameter_drafts(event, ctx);
                UpdateResponse::NeedsRepaint
            }
            UIEventKind::Named(_) | UIEventKind::Change(_) => {
                if let Some(mode) = EditorView::mode_event(event) {
                    // Keep what was typed in the panel, then switch (AC-3). A step chip names the
                    // step Advanced mode opens.
                    self.parameter_drafts(event, ctx);
                    self.open_card = EditorView::mode_event_path(event);
                    self.mode = mode;
                    return UpdateResponse::NeedsRepaint;
                }
                self.parameter_event(event, ctx)
            }
        }
    }

    /// Enter or ✓, with the typed text when the event carries it.
    fn on_apply_event(&mut self, event: &UIEvent, ctx: &UIContext) -> UpdateResponse {
        if let Some(text) = event.field("query").and_then(|v| v.try_into_string().ok()) {
            self.query_text = text;
        }
        self.apply_typed(ctx);
        UpdateResponse::NeedsRepaint
    }

    /// A browser event of the Parameters panel, decoded and applied to the query text.
    fn parameter_event(&mut self, event: &UIEvent, ctx: &UIContext) -> UpdateResponse {
        let Some(tree) = self.open_parameters(ctx) else {
            return UpdateResponse::Unchanged;
        };
        self.apply_panel(EditorView::from_ui_event(event, &tree), ctx)
    }

    /// The field values an event carries from the Parameters panel, applied as edits.
    fn parameter_drafts(&mut self, event: &UIEvent, ctx: &UIContext) -> UpdateResponse {
        let Some(tree) = self.open_parameters(ctx) else {
            return UpdateResponse::Unchanged;
        };
        self.apply_panel(EditorView::draft_commands(event, &tree), ctx)
    }

    fn open_parameters(&mut self, ctx: &UIContext) -> Option<EditorTree> {
        if !self.show_parameters {
            return None;
        }
        self.parameters_tree(ctx)
    }

    /// Applies panel edits; the panel never sets the console's text directly, nor its mode.
    fn apply_panel(&mut self, commands: Vec<EditorCommand>, ctx: &UIContext) -> UpdateResponse {
        let commands: Vec<EditorCommand> = commands
            .into_iter()
            .filter(|c| !matches!(c, EditorCommand::SetText(_) | EditorCommand::SetUpdateMode(_)))
            .collect();
        if commands.is_empty() {
            return UpdateResponse::Unchanged;
        }
        self.apply_parameter_commands(commands, ctx);
        UpdateResponse::NeedsRepaint
    }

    /// For volatile assets, schedule a delayed refresh to get a new version.
    fn schedule_volatile_refresh(&mut self, ctx: &UIContext) {
        // One refresh per request: a new request (a new generation) schedules its own.
        let generation = self.display.generation();
        if self.volatile_pending == Some(generation) {
            return;
        }
        let Some(handle) = self.handle else {
            return;
        };
        self.volatile_pending = Some(generation);
        crate::ui::schedule_event(
            ctx,
            handle,
            Duration::from_millis(VOLATILE_REFRESH_MS),
            timer_event("volatile", generation),
        );
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

    /// Returns the error location available to the egui query layouter, if any: the typed
    /// text's, else the evaluation's.
    fn error_position(&self) -> Option<&liquers_core::query::Position> {
        self.typed_error.as_ref().or(self.display.error()).map(|error| &error.position)
    }

    /// Render the single-row toolbar.
    /// Layout: [<] [>] [query_field] [✓] [mode] [undo redo] [Data/Metadata] [✎] [Presets v] [status]
    #[cfg(feature = "egui")]
    fn show_toolbar(&mut self, ui: &mut egui::Ui, ctx: &UIContext) {
        let theme = ctx.theme();
        let palette = theme.theme.palette(theme.mode);
        let accent = crate::egui::theme::color(palette.state.accent);
        let faint = crate::egui::theme::color(palette.text.ink_faint);
        ui.horizontal(|ui| {
            // Back button
            let back_enabled = self.history_index > 0;
            if ui
                .add_enabled(back_enabled, egui::Button::new("\u{25C0}"))
                .clicked()
            {
                if self.history_back() {
                    self.evaluate(ctx);
                }
            }

            // Forward button
            let forward_enabled = self.history_index < self.history.len();
            if ui
                .add_enabled(forward_enabled, egui::Button::new("\u{25B6}"))
                .clicked()
            {
                if self.history_forward() {
                    self.evaluate(ctx);
                }
            }

            // Query text field with syntax highlighting (expanding)
            let qt = self.query_text.clone();
            // The layouter may outlive this borrow of `self`, so retain the small position value.
            let error_position = self.error_position().cloned();
            let mut layouter = |ui: &egui::Ui, _buf: &dyn egui::TextBuffer, _wrap_width: f32| {
                let layout_job = crate::egui::widgets::query_to_layout_job_themed(
                    &qt,
                    error_position.as_ref(),
                    &theme,
                );
                ui.fonts_mut(|f| f.layout_job(layout_job))
            };
            let field_id = query_field_id(self.handle);
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.query_text)
                    .id(field_id)
                    .desired_width(ui.available_width() - 300.0)
                    .layouter(&mut layouter)
                    .hint_text("Enter query..."),
            );
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.handle_ui_event(&UIEvent::new(UIEventKind::Submit), ctx);
            }
            // Ctrl+J: edit the step under the cursor.
            let jump = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::J);
            if response.has_focus() && ui.input_mut(|i| i.consume_shortcut(&jump)) {
                let cursor = egui::text_edit::TextEditState::load(ui.ctx(), field_id)
                    .and_then(|s| s.cursor.char_range())
                    .map(|r| r.primary.index.0);
                if let Some(chars) = cursor {
                    let offset = self.query_text.char_indices().nth(chars).map(|(b, _)| b).unwrap_or(self.query_text.len());
                    self.jump_to_editor(offset, ctx);
                }
            }

            // ✓ beside the field (AC-13): in Apply immediately with nothing to apply, a lightning
            // mark instead.
            crate::egui::theme::ensure_icon_font(ui.ctx());
            let glyph = |name: &str, fallback: &'static str| {
                crate::egui::theme::phosphor_glyph(name).unwrap_or(fallback)
            };
            let pending = self.check_mark_enabled();
            match (self.update_mode(), pending) {
                (UpdateMode::ApplyImmediately, false) => {
                    ui.label(egui::RichText::new(glyph("lightning", "\u{26A1}")).color(faint))
                        .on_hover_text("Each edit is applied at once");
                }
                (UpdateMode::ApplyImmediately, true)
                | (UpdateMode::OnDemand, _)
                | (UpdateMode::LivePreview, _) => {
                    let mut text = egui::RichText::new(glyph("check", "\u{2713}"));
                    if pending {
                        text = text.color(accent);
                    }
                    if ui
                        .add_enabled(pending, egui::Button::new(text).frame(false))
                        .on_hover_text("Apply the query (Enter)")
                        .clicked()
                    {
                        self.handle_ui_event(&UIEvent::new(UIEventKind::Named(UPDATE_EVENT.into())), ctx);
                    }
                }
            }

            // The mode control: one icon per mode (AC-22).
            for mode in CONSOLE_MODES {
                let on = self.update_mode() == mode;
                if ui
                    .selectable_label(on, glyph(mode.icon(), mode.label()))
                    .on_hover_text(mode.label())
                    .clicked()
                    && !on
                {
                    self.set_update_mode(mode);
                }
            }
            // Undo and Redo of the panel's edits (AC-10).
            let (can_undo, can_redo) = (self.can_undo(), self.can_redo());
            let undo = ui
                .add_enabled(can_undo, egui::Button::new(glyph("arrow-u-up-left", "\u{21B6}")).frame(false))
                .on_hover_text("Undo (Ctrl+Z)")
                .clicked();
            let redo = ui
                .add_enabled(can_redo, egui::Button::new(glyph("arrow-u-up-right", "\u{21B7}")).frame(false))
                .on_hover_text("Redo (Ctrl+Shift+Z)")
                .clicked();
            if undo {
                self.apply_parameter_commands(vec![EditorCommand::Undo], ctx);
            }
            if redo {
                self.apply_parameter_commands(vec![EditorCommand::Redo], ctx);
            }

            // Data/Metadata toggle button
            let has_value = self.display.value().is_some();
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

            // The editor panel toggle: an icon button.
            if ui
                .selectable_label(self.show_parameters, glyph("pencil-simple", "\u{270E}"))
                .on_hover_text("Edit the query (Ctrl+J on a step)")
                .clicked()
            {
                self.set_show_parameters(!self.show_parameters);
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
            crate::egui::widgets::display_status(ui, self.display.status());
        });
        // Undo / Redo keys while no text field has focus (the panel's fields handle their own).
        if ui.ctx().memory(|m| m.focused().is_none()) {
            let undo = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
            let redo = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z);
            if ui.input_mut(|i| i.consume_shortcut(&redo)) {
                self.apply_parameter_commands(vec![EditorCommand::Redo], ctx);
            } else if ui.input_mut(|i| i.consume_shortcut(&undo)) {
                self.apply_parameter_commands(vec![EditorCommand::Undo], ctx);
            }
        }
        // The typed text's error (AC-14), then the status bar (AC-4, AC-21, AC-22).
        if let Some(error) = &self.typed_error {
            let danger = crate::egui::theme::color(palette.state.danger);
            ui.label(egui::RichText::new(format!("Not applied: {error}")).small().color(danger));
        }
        if let Some(line) = self.display.status_line() {
            ui.horizontal(|ui| {
                let mut text = egui::RichText::new(line.text()).small();
                if line.preview {
                    text = text.color(crate::egui::theme::color(palette.state.preview));
                }
                ui.label(text);
                if line.suggest_on_demand && self.update_mode() == UpdateMode::ApplyImmediately {
                    if ui.small_button("Switch to On demand").clicked() {
                        self.set_update_mode(UpdateMode::OnDemand);
                    }
                }
            });
        }
    }

    /// The editor panel's contents: the editor over the query text, or why there is none.
    #[cfg(feature = "egui")]
    fn show_editor_panel(&mut self, ui: &mut egui::Ui, ctx: &UIContext) {
        let Some(tree) = self.parameters_tree(ctx) else {
            ui.label("No editor: the query does not parse, or there is no registry.");
            return;
        };
        let mut view = std::mem::take(&mut self.parameters_view);
        let theme = ctx.theme();
        let (mode, density) = (self.mode, self.density());
        let output = ui
            .push_id("lq-console-parameters", |ui| {
                crate::egui::editor::show_editor(ui, &tree, &mut view, mode, density, &theme, Some(ctx))
            })
            .inner;
        self.parameters_view = view;
        if let Some(mode) = output.mode {
            self.mode = mode;
        }
        // Ctrl+J in a field: back to the console's query field, the field's text selected.
        if let Some(range) = output.select_text {
            let id = query_field_id(self.handle);
            let text = &self.query_text;
            let char_of = |b: usize| text.char_indices().take_while(|(i, _)| *i < b).count();
            let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
            state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(char_of(range.start)),
                egui::text::CCursor::new(char_of(range.end)),
            )));
            state.store(ui.ctx(), id);
            ui.ctx().memory_mut(|m| m.request_focus(id));
        }
        self.apply_panel(output.commands, ctx);
    }

    /// Render the content area: either data view or metadata pane. A stale value is dimmed, a
    /// preview framed in the theme's preview colour (AC-4, AC-22).
    #[cfg(feature = "egui")]
    fn show_content(&mut self, ui: &mut egui::Ui, ctx: &UIContext, app_state: &mut dyn AppState) {
        let theme = ctx.theme();
        let preview = crate::egui::theme::color(theme.theme.palette(theme.mode).state.preview);
        let frame = if self.display.is_preview() {
            egui::Frame::new().stroke(egui::Stroke::new(2.0, preview)).inner_margin(egui::Margin::same(4))
        } else {
            egui::Frame::new()
        };
        let dimmed = self.display.is_dimmed();
        frame.show(ui, |ui| {
            if dimmed {
                ui.multiply_opacity(0.45);
            }
            self.show_value(ui, ctx, app_state);
        });
    }

    #[cfg(feature = "egui")]
    fn show_value(&mut self, ui: &mut egui::Ui, ctx: &UIContext, app_state: &mut dyn AppState) {
        let data_view = self.data_view && self.display.value().is_some();
        // An element that scrolls itself gets the space as it is: wrapping it in another scroll
        // area shows two scroll bars.
        if data_view {
            if let Some(element) = self.display.ui_element_mut().filter(|e| e.scrolls_itself()) {
                element.show_in_egui(ui, ctx, app_state);
                return;
            }
        }
        egui::ScrollArea::vertical().id_salt("lq-console-value-scroll").show(ui, |ui| {
            if data_view {
                // If value is a UIElement, render it with show_in_egui
                if let Some(element) = self.display.ui_element_mut() {
                    element.show_in_egui(ui, ctx, app_state);
                } else if let Some(value) = self.display.value() {
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
    #[cfg(feature = "egui")]
    fn show_metadata_pane(&self, ui: &mut egui::Ui) {
        // Show error if present
        if let Some(err) = self.display.error() {
            ui.colored_label(egui::Color32::RED, format!("Error: {}", err));
            ui.separator();
        }

        // Show metadata
        if let Some(metadata) = self.display.metadata() {
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

    /// A snapshot pushed by the runner.
    fn on_asset_update(&mut self, snapshot: &crate::ui::message::AssetSnapshot, ctx: &UIContext) -> UpdateResponse {
        let before = self.display.requested().map(str::to_string);
        let outcome = self.display.on_snapshot(snapshot);
        match outcome {
            SnapshotOutcome::Ignored => return UpdateResponse::Unchanged,
            SnapshotOutcome::Progress | SnapshotOutcome::Settled => {}
        }
        // Re-pointed from outside (an editor this console follows): an applied query becomes the
        // console's text; a preview leaves the text, so ✓ or Enter returns to it (AC-17).
        let now = self.display.requested().map(str::to_string);
        if now != before && !self.display.requested_preview() {
            if let Some(query) = now {
                self.query_text = query;
                self.typed_error = None;
                self.debounced_query = None;
                self.sync_editor(ctx);
            }
        }
        if snapshot.value.is_some() {
            self.data_view = true;
        }
        let is_volatile = snapshot.status == Status::Volatile || snapshot.metadata.is_volatile();
        if is_volatile && snapshot.value.is_some() {
            self.schedule_volatile_refresh(ctx);
        }
        if snapshot.status == Status::Expired && snapshot.value.is_none() {
            // A dependency changed: recalculate, or wait for ✓ (AC-19).
            match self.update_mode() {
                UpdateMode::ApplyImmediately => {
                    let query = self.display.requested().map(str::to_string).unwrap_or_else(|| self.query_text.clone());
                    if !query.is_empty() {
                        self.request(&query, ctx);
                    }
                }
                UpdateMode::OnDemand | UpdateMode::LivePreview => self.display.mark_changed_upstream(),
            }
        }
        UpdateResponse::NeedsRepaint
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

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn init(&mut self, handle: UIHandle, ctx: &UIContext) -> Result<(), Error> {
        self.set_handle(handle);
        // If query_text is non-empty (e.g. from deserialization or creation), submit it
        if !self.query_text.is_empty() {
            self.submit_query(ctx);
        }
        Ok(())
    }

    fn update(&mut self, message: &UpdateMessage, ctx: &UIContext) -> UpdateResponse {
        self.theme = Some(ctx.theme());
        match message {
            UpdateMessage::AssetUpdate(snapshot) => self.on_asset_update(snapshot, ctx),
            UpdateMessage::Directory(listing) => {
                // "Browse…" in the panel (AC-16).
                match &mut self.parameters {
                    Some(state) => {
                        state.on_directory(listing.clone());
                        UpdateResponse::NeedsRepaint
                    }
                    None => UpdateResponse::Unchanged,
                }
            }
            UpdateMessage::AssetNotification(_) => UpdateResponse::Unchanged,
            UpdateMessage::Timer { .. } => UpdateResponse::Unchanged,
            UpdateMessage::Custom(_) => UpdateResponse::Unchanged,
            UpdateMessage::Accessor(_) | UpdateMessage::NodeContext(_) => UpdateResponse::Unchanged,
            UpdateMessage::UI(event) => self.handle_ui_event(event, ctx),
        }
    }

    fn get_value(&self) -> Option<Arc<Value>> {
        self.display.value().cloned()
    }

    fn get_metadata(&self) -> Option<Metadata> {
        self.display.metadata().cloned()
    }

    #[cfg(feature = "egui")]
    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &UIContext,
        app_state: &mut dyn AppState,
    ) -> egui::Response {
        ui.vertical(|ui| {
            self.show_toolbar(ui, ctx);
            ui.separator();
            if self.show_parameters {
                // The editor is a narrow, resizable column beside the value.
                let panel_id = ui.id().with(("lq-console-editor", self.handle.map(|h| h.0)));
                egui::Panel::right(panel_id)
                    .resizable(true)
                    .default_size(320.0)
                    .size_range(220.0..=640.0)
                    .frame(egui::Frame::new().inner_margin(egui::Margin { left: 6, right: 0, top: 0, bottom: 0 }))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().id_salt("lq-console-editor-scroll").show(ui, |ui| {
                            self.show_editor_panel(ui, ctx);
                        });
                    });
            }
            self.show_content(ui, ctx, app_state);
        })
        .response
    }

    #[cfg(feature = "webui")]
    fn render_web(&self, app_state: &dyn AppState) -> String {
        use crate::ui::web::editor::ph_html;
        use crate::ui::web::html::{escape_html, event_attr, field_attr, value_to_html};
        use crate::ui::web::{element_dom_id, widgets};

        let dom_id = element_dom_id(self.handle());
        let hid = self.handle().map(|h| h.0).unwrap_or(0);
        let input_id = format!("qc-input-{}", hid);
        let named = |name: &str| event_attr(&UIEventKind::Named(name.to_string()));
        let icon_button = |icon: &str, fallback: &str, event: &str, title: &str, class: &str, enabled: bool| {
            format!(
                "<button type=\"button\" class=\"{}\" title=\"{}\"{}{}>{}</button>",
                escape_html(class),
                escape_html(title),
                if enabled { "" } else { " disabled" },
                named(event),
                ph_html(icon, fallback)
            )
        };

        // Toolbar: the query input is field `query`; Enter in it sends a `Submit` UI event and ✓
        // a `qc:update`, both carrying the typed text (AC-12, AC-13).
        let submit = event_attr(&UIEventKind::Submit);
        // The browser cannot know about text typed since the last render, so ✓ is always there
        // (accented while there is something to apply); the mode control shows the lightning.
        let pending = self.check_mark_enabled();
        let check = icon_button(
            "check",
            "\u{2713}",
            UPDATE_EVENT,
            "Apply the query (Enter)",
            if pending { "lq-go lq-qc-check lq-ed-pending-check" } else { "lq-go lq-qc-check" },
            true,
        );
        let mut modes = String::from("<span class=\"lq-qc-modes\">");
        for mode in CONSOLE_MODES {
            let class = if mode == self.update_mode() { "lq-qc-mode lq-mode-on" } else { "lq-qc-mode" };
            modes.push_str(&icon_button(
                mode.icon(),
                mode.label(),
                &format!("{MODE_EVENT}{}", mode.name()),
                mode.label(),
                class,
                true,
            ));
        }
        modes.push_str("</span>");
        let undo = icon_button("arrow-u-up-left", "\u{21B6}", UNDO_EVENT, "Undo", "lq-qc-undo", self.can_undo());
        let redo = icon_button("arrow-u-up-right", "\u{21B7}", REDO_EVENT, "Redo", "lq-qc-redo", self.can_redo());
        let toolbar = format!(
            "<div class=\"lq-qc-toolbar\"><input id=\"{}\" class=\"lq-query-input\" value=\"{}\"{}{}/>{}{}{}{}<button type=\"button\" class=\"lq-qc-params{}\" title=\"Edit the query\" aria-label=\"Edit the query\"{}>\u{270E}</button>{}</div>",
            input_id,
            escape_html(&self.query_text),
            field_attr("query"),
            submit,
            check,
            modes,
            undo,
            redo,
            if self.show_parameters { " lq-mode-on" } else { "" },
            named("qc:params"),
            widgets::status_html(self.display.status())
        );
        // The typed text's error and the status bar (AC-4, AC-14, AC-21, AC-22).
        let mut status = String::new();
        if let Some(error) = &self.typed_error {
            status.push_str(&format!(
                "<div class=\"lq-status-bar lq-qc-typed-error\">Not applied: {}</div>",
                escape_html(&error.to_string())
            ));
        }
        if let Some(line) = self.display.status_line() {
            let switch = if line.suggest_on_demand && self.update_mode() == UpdateMode::ApplyImmediately {
                format!(
                    "<button type=\"button\" class=\"lq-qc-switch\"{}>Switch to On demand</button>",
                    named(&format!("{MODE_EVENT}{}", UpdateMode::OnDemand.name()))
                )
            } else {
                String::new()
            };
            status.push_str(&format!(
                "<div class=\"lq-status-bar{}\"><span>{}</span>{}</div>",
                if line.preview { " lq-ed-preview" } else { "" },
                escape_html(&line.text()),
                switch
            ));
        }
        // The Parameters panel (AC-12), from the editor the last event built.
        let parameters = match (&self.parameters, self.show_parameters) {
            // Enter in a panel field sends `qc:apply`, so it is never taken for the console's
            // own Submit (the browser sends every field of the element with either).
            (Some(state), true) => crate::ui::web::editor::editor_html_open(
                &panel_tree(state.tree()),
                "Edit the query",
                self.mode,
                self.density(),
                &self.theme.clone().unwrap_or_default().theme,
                self.open_card.as_ref(),
            )
            .replace(&submit, &named(PARAMETERS_SUBMIT)),
            (Some(_), false) | (None, true) | (None, false) => String::new(),
        };

        // Content: data view (value) or metadata pane.
        let content = if self.data_view {
            match self.display.value() {
                Some(v) => value_to_html(v.as_ref(), app_state),
                None => String::new(),
            }
        } else {
            match self.display.metadata() {
                Some(m) => match m.get_asset_info() {
                    Ok(info) => widgets::asset_info_html(&info),
                    Err(_) => "<div>No metadata available</div>".to_string(),
                },
                None => "<div>No metadata available</div>".to_string(),
            }
        };
        let error = match self.display.error() {
            Some(e) => widgets::error_html(e),
            None => String::new(),
        };
        let mut content_class = String::from("lq-qc-content");
        if self.display.is_dimmed() {
            content_class.push_str(" lq-value-stale");
        }
        if self.display.is_preview() {
            content_class.push_str(" lq-value-preview");
        }

        // The editor is a narrow column beside the value.
        let parameters = if parameters.is_empty() {
            parameters
        } else {
            format!("<div class=\"lq-qc-editor\">{parameters}</div>")
        };
        format!(
            "<div id=\"{}\" class=\"lq-element lq-QueryConsoleElement\">{}{}<div class=\"lq-qc-body\"><div class=\"{}\">{}{}</div>{}</div></div>",
            dom_id, toolbar, status, content_class, error, content, parameters
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app_state::DirectAppState;
    use crate::ui::message::{app_message_channel, AssetSnapshot};
    use liquers_core::assets::AssetNotificationMessage;
    use crate::ui::editor::FocusTarget;

    fn create_test_context() -> (UIContext, crate::ui::message::AppMessageReceiver) {
        let (tx, rx) = app_message_channel();
        let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
            Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
        let ctx = UIContext::new(app_state, tx);
        (ctx, rx)
    }

    // ─── UI events (ui-events) ──────────────────────────────────────────────

    fn submit_event(query: &str) -> UIEvent {
        UIEvent::new(UIEventKind::Submit).with_field("query", Value::from(query.to_string()))
    }

    fn requested_query(rx: &mut crate::ui::message::AppMessageReceiver) -> Option<String> {
        match rx.try_recv() {
            Ok(AppMessage::RequestAssetUpdates { query, .. }) => Some(query),
            Ok(AppMessage::SubmitQuery { .. })
            | Ok(AppMessage::ApplyToInput { .. })
            | Ok(AppMessage::UIEvent { .. })
            | Ok(AppMessage::Quit)
            | Ok(AppMessage::Serialize { .. })
            | Ok(AppMessage::Deserialize { .. })
            | Ok(AppMessage::Accessor { .. })
            | Ok(AppMessage::ListDirectory { .. })
            | Ok(AppMessage::NodeContext { .. })
            | Ok(AppMessage::RenameData { .. })
            | Ok(AppMessage::Evaluate { .. })
            | Ok(AppMessage::SaveAs { .. })
            | Ok(AppMessage::LoadIcon { .. })
            | Ok(AppMessage::Preview { .. })
            | Err(_) => None,
        }
    }

    #[test]
    fn test_ui_submit_sets_query_history_and_requests_updates() {
        let mut c = QueryConsoleElement::new("C".to_string(), "hello".to_string());
        let (ctx, mut rx) = create_test_context();
        c.set_handle(UIHandle(1));
        let response = c.update(&UpdateMessage::UI(submit_event("about")), &ctx);
        assert_eq!(response, UpdateResponse::NeedsRepaint);
        assert_eq!(c.query_text, "about");
        assert_eq!(c.history.last().map(String::as_str), Some("about"));
        assert_eq!(requested_query(&mut rx).as_deref(), Some("about"));
    }

    #[test]
    fn test_ui_submit_without_field_resubmits_current_query() {
        let mut c = QueryConsoleElement::new("C".to_string(), "hello".to_string());
        let (ctx, mut rx) = create_test_context();
        c.set_handle(UIHandle(1));
        let response = c.update(&UpdateMessage::UI(UIEvent::new(UIEventKind::Submit)), &ctx);
        assert_eq!(response, UpdateResponse::NeedsRepaint);
        assert_eq!(c.query_text, "hello");
        assert_eq!(requested_query(&mut rx).as_deref(), Some("hello"));
    }

    #[test]
    fn test_ui_named_event_is_unchanged() {
        let mut c = QueryConsoleElement::new("C".to_string(), "hello".to_string());
        let (ctx, mut rx) = create_test_context();
        c.set_handle(UIHandle(1));
        let event = UIEvent::new(UIEventKind::Named("reset".to_string()));
        assert_eq!(
            c.update(&UpdateMessage::UI(event), &ctx),
            UpdateResponse::Unchanged
        );
        assert_eq!(requested_query(&mut rx), None);
        assert!(c.history.is_empty());
    }

    /// AC-8: egui edits `query_text` in place and sends a field-less `Submit`; the web sends the
    /// typed text as field `query`. Both end in the same state and request.
    #[test]
    fn egui_and_web_submit_share_outcome() {
        let (ctx, mut rx) = create_test_context();
        let mut web = QueryConsoleElement::new("C".to_string(), "hello".to_string());
        web.set_handle(UIHandle(1));
        web.update(&UpdateMessage::UI(submit_event("about")), &ctx);

        let mut egui_like = QueryConsoleElement::new("C".to_string(), "hello".to_string());
        egui_like.set_handle(UIHandle(1));
        egui_like.query_text = "about".to_string();
        egui_like.handle_ui_event(&UIEvent::new(UIEventKind::Submit), &ctx);

        assert_eq!(web.query_text, egui_like.query_text);
        assert_eq!(web.history, egui_like.history);
        assert_eq!(requested_query(&mut rx), requested_query(&mut rx));
    }

    #[test]
    fn test_expired_after_ui_submit_requests_submitted_query() {
        let mut c = QueryConsoleElement::new("C".to_string(), "hello".to_string());
        let (ctx, mut rx) = create_test_context();
        c.set_handle(UIHandle(1));
        c.update(&UpdateMessage::UI(submit_event("about")), &ctx);
        let _ = requested_query(&mut rx);
        let snapshot = AssetSnapshot {
            value: None,
            metadata: Metadata::new(),
            error: None,
            status: Status::Expired,
            query: None,
            preview: false,
        };
        c.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);
        assert_eq!(requested_query(&mut rx).as_deref(), Some("about"));
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
        assert!(console.display.value().is_none());
        assert!(console.display.metadata().is_none());
        assert!(console.display.error().is_none());
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
        assert_eq!(console.display.status(), Status::None);
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
            query: None,
            preview: false,
        };
        let response = c.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);
        assert_eq!(response, UpdateResponse::NeedsRepaint);
        assert!(c.display.value().is_some());
        assert!(c.display.metadata().is_some());
        assert!(c.display.error().is_none());
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
            query: None,
            preview: false,
        };
        c.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);
        assert!(c.display.error().is_some());
        assert!(c.display.value().is_none());
    }

    #[test]
    fn test_asset_update_exposes_error_position_for_egui_layout() {
        let mut c = QueryConsoleElement::new("C".to_string(), "broken".to_string());
        let (ctx, _rx) = create_test_context();
        let position = liquers_core::query::Position::new(0, 1, 1);
        let snapshot = AssetSnapshot {
            value: None,
            metadata: Metadata::new(),
            error: Some(Error::general_error("fail".to_string()).with_position(&position)),
            status: Status::Error,
            query: None,
            preview: false,
        };

        c.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);

        assert_eq!(c.error_position(), Some(&position));
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
            query: None,
            preview: false,
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
            query: None,
            preview: false,
        };
        c.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);
        assert!(c.data_view); // unchanged
    }

    #[test]
    fn test_asset_update_expired_resubmits_without_invalidating_value() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.set_handle(UIHandle(7));
        c.display.set_value("q", Some(Arc::new(Value::from("keep"))));
        let (ctx, mut rx) = create_test_context();
        let snapshot = AssetSnapshot {
            value: None,
            metadata: Metadata::new(),
            error: None,
            status: Status::Expired,
            query: None,
            preview: false,
        };
        c.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);
        assert!(c.display.value().is_some());
        assert_eq!(c.display.status(), Status::Expired);

        match rx.try_recv() {
            Ok(AppMessage::RequestAssetUpdates { handle, query }) => {
                assert_eq!(handle, UIHandle(7));
                assert_eq!(query, "q");
            }
            other => panic!("Expected RequestAssetUpdates, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_volatile_asset_schedules_delayed_refresh() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.set_handle(UIHandle(9));
        let (ctx, mut rx) = create_test_context();
        let snapshot = AssetSnapshot {
            value: Some(Arc::new(Value::from("v1"))),
            metadata: Metadata::new(),
            error: None,
            status: Status::Volatile,
            query: None,
            preview: false,
        };
        c.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);

        assert!(rx.try_recv().is_err(), "refresh should be delayed");

        let msg = tokio::time::timeout(std::time::Duration::from_millis(900), rx.recv())
            .await
            .expect("expected delayed refresh message")
            .expect("channel closed unexpectedly");

        // The refresh is a timer event the console sent itself; delivered, it asks again.
        let AppMessage::UIEvent { handle, event } = msg else {
            panic!("Expected the refresh timer, got {msg:?}");
        };
        assert_eq!(handle, UIHandle(9));
        c.update(&UpdateMessage::UI(event), &ctx);
        match rx.try_recv() {
            Ok(AppMessage::RequestAssetUpdates { handle, query }) => {
                assert_eq!(handle, UIHandle(9));
                assert_eq!(query, "q");
            }
            other => panic!("Expected RequestAssetUpdates, got {other:?}"),
        }
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
        c.display.set_value("q", Some(Arc::new(Value::from("v"))));
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

    /// query-pipeline-editor AC-8: the Parameters panel edits the last action of the text and
    /// does not submit.
    #[test]
    fn parameters_panel_edits_the_last_action_without_submitting() {
        use liquers_core::command_metadata::{ArgumentInfo, CommandMetadata};
        let mut cmr = CommandMetadataRegistry::new();
        let mut head = CommandMetadata::new("head");
        head.with_argument(ArgumentInfo::integer_argument("n", false).with_default(5));
        cmr.add_command(&head);
        let (ctx, mut rx) = create_test_context();
        let ctx = ctx.with_command_registry(Arc::new(cmr));
        let mut c = QueryConsoleElement::new("C".to_string(), "-R/a.csv/-/head-3".to_string());
        c.set_handle(UIHandle(1));
        c.set_show_parameters(true);
        let tree = c.parameters_tree(&ctx).expect("tree");
        assert!(!tree.text_editable && tree.hide_text && tree.has_filename);
        let path = tree.nodes[tree.last_action().expect("head")].path().clone();
        let changed = c.apply_parameter_commands(
            vec![EditorCommand::SetField { path: path.clone(), arg: "n".into(), text: "7".into() }],
            &ctx,
        );
        assert!(changed);
        assert_eq!(c.query_text, "-R/a.csv/-/head-7");
        assert!(rx.try_recv().is_err(), "nothing is submitted");
        // A second edit after the first is written applies too.
        c.apply_parameter_commands(
            vec![EditorCommand::Reset { path, arg: "n".into() }],
            &ctx,
        );
        assert_eq!(c.query_text, "-R/a.csv/-/head");
    }

    // Test 18 (AC-12).
    #[test]
    fn parameters_panel_switches_mode_without_submit() {
        use liquers_core::command_metadata::{ArgumentInfo, CommandMetadata};
        let mut cmr = CommandMetadataRegistry::new();
        let mut head = CommandMetadata::new("head");
        head.with_argument(ArgumentInfo::integer_argument("n", false).with_default(5));
        cmr.add_command(&head);
        let (ctx, mut rx) = create_test_context();
        let ctx = ctx.with_command_registry(Arc::new(cmr));
        let mut c = QueryConsoleElement::new("C".to_string(), "-R/a.csv/-/head-3".to_string());
        c.set_handle(UIHandle(1));
        assert_eq!(c.mode(), EditorMode::Simple);
        assert_eq!(c.density(), Density::Compact);

        let toggle = UIEvent::new(UIEventKind::Named("qc:params".into()));
        assert_eq!(c.update(&UpdateMessage::UI(toggle), &ctx), UpdateResponse::NeedsRepaint);
        assert!(c.show_parameters());
        let advanced = UIEvent::new(UIEventKind::Named("ed:mode::advanced".into()));
        assert_eq!(c.update(&UpdateMessage::UI(advanced), &ctx), UpdateResponse::NeedsRepaint);
        assert_eq!(c.mode(), EditorMode::Advanced);
        assert!(rx.try_recv().is_err(), "switching the mode submits nothing");

        // Enter in a Parameters field edits the query and submits nothing.
        let tree = c.parameters_tree(&ctx).expect("tree");
        let path = tree.nodes[tree.last_action().expect("head")].path().clone();
        let field = crate::ui::editor::field_id(&path, "n", None);
        let enter = UIEvent::new(UIEventKind::Named(PARAMETERS_SUBMIT.into()))
            .with_field("query", Value::from("-R/a.csv/-/head-3".to_string()))
            .with_field(&field, Value::from("9".to_string()));
        c.update(&UpdateMessage::UI(enter.clone()), &ctx);
        assert_eq!(c.query_text, "-R/a.csv/-/head-9");
        assert!(rx.try_recv().is_err(), "nothing is submitted");
        // Codex review of PR #278: Enter in an unchanged field submits nothing either.
        c.update(&UpdateMessage::UI(enter), &ctx);
        assert!(rx.try_recv().is_err(), "an unchanged field submits nothing");

        // A mode switch keeps a value typed in the panel.
        let simple = UIEvent::new(UIEventKind::Named("ed:mode::simple".into()))
            .with_field(&field, Value::from("11".to_string()));
        c.update(&UpdateMessage::UI(simple), &ctx);
        assert_eq!((c.mode(), c.query_text.as_str()), (EditorMode::Simple, "-R/a.csv/-/head-11"));
        c.set_mode(EditorMode::Advanced);

        // Go submits the query input even while a panel field holds an uncommitted edit.
        let go = UIEvent::new(UIEventKind::Submit)
            .with_field("query", Value::from("-R/a.csv/-/head-4".to_string()))
            .with_field(&field, Value::from("12".to_string()));
        c.update(&UpdateMessage::UI(go), &ctx);
        assert_eq!(c.query_text, "-R/a.csv/-/head-4");
        assert!(rx.try_recv().is_ok(), "Go submits");

        #[cfg(feature = "webui")]
        {
            let html = c.render_web(&DirectAppState::new());
            assert!(html.contains("data-lq-mode=\"advanced\"") && html.contains("data-lq-density=\"compact\""), "{html}");
            assert!(html.contains("data-lq-event=\"qc:apply\""), "panel fields send qc:apply: {html}");
            let panel = &html[html.find("lq-editor").expect("panel")..];
            assert!(!panel.contains("data-lq-event=\"submit\""), "the panel sends no console Submit");
        }
        let json = serde_json::to_value(&c).expect("json");
        let back: QueryConsoleElement = serde_json::from_value(json).expect("back");
        assert_eq!(back.mode(), EditorMode::Advanced);
    }

    /// An empty query can be built in the panel: it offers "Add key…" and "Add action…".
    #[test]
    fn empty_query_offers_a_key_and_an_action() {
        use liquers_core::command_metadata::{CommandKey, CommandMetadata};
        use crate::ui::editor::NewElement;
        let mut cmr = CommandMetadataRegistry::new();
        cmr.add_command(&CommandMetadata::new("head"));
        let (ctx, _rx) = create_test_context();
        let ctx = ctx.with_command_registry(Arc::new(cmr));
        let mut c = QueryConsoleElement::new("C".to_string(), String::new());
        c.set_handle(UIHandle(1));
        c.set_show_parameters(true);
        let tree = c.parameters_tree(&ctx).expect("an empty query has a tree");
        assert!(tree.nodes.is_empty() && tree.has_filename);
        assert!(crate::ui::editor::offers_key(&tree));

        // "Add key…" in the browser decodes to a resource inserted first.
        let add_key = UIEvent::new(UIEventKind::Named("ed:addkey::".into()));
        let commands = EditorView::from_ui_event(&add_key, &tree);
        assert_eq!(
            commands,
            [EditorCommand::Insert { path: NodePath::element(0), element: NewElement::Resource(String::new()) }]
        );
        assert!(c.apply_parameter_commands(commands, &ctx));
        assert_eq!(c.query_text, "-R");
        let tree = c.parameters_tree(&ctx).expect("tree");
        assert!(!crate::ui::editor::offers_key(&tree), "a query with a key offers no second one");
        let path = tree.nodes[0].path().clone();
        assert!(c.apply_parameter_commands(vec![EditorCommand::SetKey { path, key: "data".into() }], &ctx));
        assert!(c.apply_parameter_commands(
            vec![EditorCommand::Append(NewElement::Action(CommandKey::new_name("head")))],
            &ctx
        ));
        assert_eq!(c.query_text, "-R/data/-/head");

        #[cfg(feature = "webui")]
        {
            let mut c = QueryConsoleElement::new("C".to_string(), String::new());
            c.set_handle(UIHandle(1));
            let toggle = UIEvent::new(UIEventKind::Named("qc:params".into()));
            c.update(&UpdateMessage::UI(toggle), &ctx);
            let html = c.render_web(&DirectAppState::new());
            assert!(html.contains("data-lq-event=\"ed:addkey::\""), "{html}");
            assert!(html.contains("Add action"), "{html}");
            assert!(!html.contains("lq-ed-filename"), "the console shows the filename in its query: {html}");
        }
    }

    /// The panel does not repeat the query; `Ctrl+J` on a step other than the last opens
    /// Advanced mode with only that step expanded, and on the last one keeps Simple mode.
    #[test]
    fn ctrl_j_opens_the_step_under_the_cursor() {
        use liquers_core::command_metadata::{ArgumentInfo, CommandMetadata};
        let mut cmr = CommandMetadataRegistry::new();
        let mut head = CommandMetadata::new("head");
        head.with_argument(ArgumentInfo::integer_argument("n", false).with_default(5));
        cmr.add_command(&head);
        let mut tail = CommandMetadata::new("tail");
        tail.with_argument(ArgumentInfo::integer_argument("n", false).with_default(5));
        cmr.add_command(&tail);
        let (ctx, _rx) = create_test_context();
        let ctx = ctx.with_command_registry(Arc::new(cmr));
        let text = "-R/a.csv/-/head-3/tail-2";
        let mut c = QueryConsoleElement::new("C".to_string(), text.to_string());
        c.set_handle(UIHandle(1));
        let tree = c.parameters_tree(&ctx).expect("tree");
        assert!(tree.hide_text, "the console shows the query; the panel does not repeat it");

        // The last action: the panel opens in Simple mode, on its field.
        assert!(c.jump_to_editor(text.rfind('2').expect("2"), &ctx));
        assert!(c.show_parameters());
        assert_eq!(c.mode(), EditorMode::Simple);
        assert!(matches!(&c.parameters_view.focus, Some(FocusTarget::Field { arg, .. }) if arg == "n"));

        // Another action: Advanced mode, that step open and the rest collapsed.
        assert!(c.jump_to_editor(text.find('3').expect("3"), &ctx));
        assert_eq!(c.mode(), EditorMode::Advanced);
        let tree = c.parameters_tree(&ctx).expect("tree");
        let open: Vec<String> = tree
            .nodes
            .iter()
            .map(|n| n.path().id())
            .filter(|id| !c.parameters_view.collapsed.contains(id))
            .collect();
        assert_eq!(open, [NodePath::element(1).id()]);
        assert_eq!(c.open_card, Some(NodePath::element(1)));

        #[cfg(feature = "webui")]
        {
            let html = c.render_web(&DirectAppState::new());
            assert!(html.contains("lq-qc-editor") && html.contains("lq-qc-body"), "{html}");
            assert!(!html.contains(">Parameters<"), "an icon button, not a Parameters label: {html}");
            assert_eq!(html.matches("<details open class=\"lq-card\"").count(), 1, "{html}");
            let panel = &html[html.find("lq-editor").expect("panel")..];
            assert!(!panel.contains("lq-query"), "the panel does not repeat the query: {panel}");
        }
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
                | AppMessage::ApplyToInput { .. }
                | AppMessage::UIEvent { .. }
                | AppMessage::Quit
                | AppMessage::Serialize { .. }
                | AppMessage::Deserialize { .. }
                | AppMessage::Accessor { .. }
                | AppMessage::ListDirectory { .. }
                | AppMessage::NodeContext { .. }
                | AppMessage::RenameData { .. }
                | AppMessage::Evaluate { .. }
                | AppMessage::SaveAs { .. }
                | AppMessage::LoadIcon { .. }
                | AppMessage::Preview { .. } => panic!("Expected RequestAssetUpdates"),
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

    // ─── 8. Update modes (query-update-modes) ───────────────────────────────

    fn head_registry() -> CommandMetadataRegistry {
        use liquers_core::command_metadata::{ArgumentInfo, CommandMetadata};
        let mut cmr = CommandMetadataRegistry::new();
        let mut head = CommandMetadata::new("head");
        head.with_argument(ArgumentInfo::integer_argument("n", false).with_default(5));
        cmr.add_command(&head);
        cmr
    }

    /// A console on `-R/a.csv/-/head-3`, its first evaluation received and settled.
    fn settled_console(mode: UpdateMode) -> (QueryConsoleElement, UIContext, crate::ui::message::AppMessageReceiver) {
        let (ctx, mut rx) = create_test_context();
        let ctx = ctx.with_command_registry(Arc::new(head_registry()));
        let mut c = QueryConsoleElement::new("C".to_string(), "-R/a.csv/-/head-3".to_string());
        c.set_update_mode(mode);
        c.init(UIHandle(1), &ctx).expect("init");
        assert_eq!(requested_query(&mut rx).as_deref(), Some("-R/a.csv/-/head-3"));
        c.update(&UpdateMessage::AssetUpdate(ready("-R/a.csv/-/head-3", "three")), &ctx);
        (c, ctx, rx)
    }

    fn ready(query: &str, value: &str) -> AssetSnapshot {
        AssetSnapshot {
            value: Some(Arc::new(Value::from(value))),
            metadata: Metadata::new(),
            error: None,
            status: Status::Ready,
            query: Some(query.to_string()),
            preview: false,
        }
    }

    fn set_n(c: &mut QueryConsoleElement, ctx: &UIContext, n: &str) -> bool {
        let tree = c.parameters_tree(ctx).expect("tree");
        let path = tree.nodes[tree.last_action().expect("head")].path().clone();
        c.apply_parameter_commands(vec![EditorCommand::SetField { path, arg: "n".into(), text: n.into() }], ctx)
    }

    fn named(name: &str) -> UIEvent {
        UIEvent::new(UIEventKind::Named(name.to_string()))
    }

    fn debounce_timer(generation: u64) -> UpdateMessage {
        UpdateMessage::UI(timer_event("debounce", generation))
    }

    /// AC-1: a burst of panel edits evaluates once, after the debounce, and the old value stays
    /// on screen until the new one arrives (AC-3).
    #[test]
    fn panel_edit_evaluates_after_the_debounce() {
        let (mut c, ctx, mut rx) = settled_console(UpdateMode::ApplyImmediately);
        assert!(set_n(&mut c, &ctx, "7"));
        assert!(set_n(&mut c, &ctx, "8"));
        assert_eq!(c.query_text, "-R/a.csv/-/head-8");
        assert_eq!(requested_query(&mut rx), None, "nothing before the debounce");
        // The first edit's timer is stale.
        c.update(&debounce_timer(1), &ctx);
        assert_eq!(requested_query(&mut rx), None);
        c.update(&debounce_timer(2), &ctx);
        assert_eq!(requested_query(&mut rx).as_deref(), Some("-R/a.csv/-/head-8"));
        assert_eq!(requested_query(&mut rx), None, "one request for the burst");
        assert_eq!(c.display.value().and_then(|v| v.try_into_string().ok()).as_deref(), Some("three"));
        // A snapshot of the old query, arriving late, is ignored (AC-6).
        c.update(&UpdateMessage::AssetUpdate(ready("-R/a.csv/-/head-3", "late")), &ctx);
        assert_eq!(c.display.value().and_then(|v| v.try_into_string().ok()).as_deref(), Some("three"));
        c.update(&UpdateMessage::AssetUpdate(ready("-R/a.csv/-/head-8", "eight")), &ctx);
        assert_eq!(c.display.value().and_then(|v| v.try_into_string().ok()).as_deref(), Some("eight"));
        assert!(c.display.status_line().is_none(), "a fast result shows no status bar");
    }

    /// AC-8, AC-13: On demand, a panel edit rewrites the text and waits for ✓.
    #[test]
    fn on_demand_waits_for_the_check_mark() {
        let (mut c, ctx, mut rx) = settled_console(UpdateMode::OnDemand);
        assert!(!c.check_mark_enabled());
        assert!(set_n(&mut c, &ctx, "7"));
        assert_eq!(c.query_text, "-R/a.csv/-/head-7");
        assert!(c.check_mark_enabled());
        c.update(&debounce_timer(c.debounce), &ctx);
        assert_eq!(requested_query(&mut rx), None, "On demand evaluates nothing by itself");
        c.update(&UpdateMessage::UI(named(UPDATE_EVENT)), &ctx);
        assert_eq!(requested_query(&mut rx).as_deref(), Some("-R/a.csv/-/head-7"));
        assert!(!c.check_mark_enabled());
        // The mode control switches; a console has no Live preview.
        c.update(&UpdateMessage::UI(named("qc:mode::immediate")), &ctx);
        assert_eq!(c.update_mode(), UpdateMode::ApplyImmediately);
        c.set_update_mode(UpdateMode::LivePreview);
        assert_eq!(c.update_mode(), UpdateMode::ApplyImmediately);
        let json = serde_json::to_value(&c).expect("json");
        assert_eq!(json["update_mode"], "immediate");
        let mut back: QueryConsoleElement = serde_json::from_value(json).expect("back");
        back.set_update_mode(UpdateMode::OnDemand);
        let back: QueryConsoleElement = serde_json::from_value(serde_json::to_value(&back).expect("json")).expect("back");
        assert_eq!(back.update_mode(), UpdateMode::OnDemand);
    }

    /// AC-13: the panel shows no update controls of its own; the console has them.
    #[test]
    fn panel_shows_no_controls_of_its_own() {
        let (mut c, ctx, _rx) = settled_console(UpdateMode::OnDemand);
        c.set_show_parameters(true);
        let tree = c.parameters_tree(&ctx).expect("tree");
        assert!(tree.host_controls && tree.hide_text && !tree.text_editable);
        assert_eq!(tree.update_mode, UpdateMode::ApplyImmediately, "the panel's editor never holds edits");
        #[cfg(feature = "webui")]
        {
            let html = c.render_web(&DirectAppState::new());
            let panel = &html[html.find("lq-editor").expect("panel")..];
            assert!(!panel.contains("ed:updatemode") && !panel.contains("lq-ed-check"), "{panel}");
            assert_eq!(html.matches("data-lq-event=\"qc:update\"").count(), 1, "one ✓: {html}");
            assert_eq!(html.matches("lq-qc-modes").count(), 1, "one mode control: {html}");
        }
    }

    /// AC-14: Enter applies the typed text only when it parses and plans.
    #[test]
    fn enter_applies_only_valid_text() {
        let (mut c, ctx, mut rx) = settled_console(UpdateMode::ApplyImmediately);
        c.update(&UpdateMessage::UI(submit_event("-R/a.csv/-/nosuch")), &ctx);
        assert_eq!(requested_query(&mut rx), None);
        assert!(c.typed_error().is_some());
        assert_eq!(c.display.value().and_then(|v| v.try_into_string().ok()).as_deref(), Some("three"));
        c.update(&UpdateMessage::UI(submit_event("-R/a.csv/-/head-~")), &ctx);
        assert_eq!(requested_query(&mut rx), None, "a parse error applies nothing");
        assert!(c.typed_error().is_some());
        // Typed text is never applied by the debounce either.
        c.update(&debounce_timer(c.debounce), &ctx);
        assert_eq!(requested_query(&mut rx), None);
        c.update(&UpdateMessage::UI(submit_event("-R/a.csv/-/head-4")), &ctx);
        assert_eq!(requested_query(&mut rx).as_deref(), Some("-R/a.csv/-/head-4"));
        assert!(c.typed_error().is_none());
        // The panel follows the applied text.
        let tree = c.parameters_tree(&ctx).expect("tree");
        assert_eq!(tree.text, "-R/a.csv/-/head-4");
    }

    /// Review of PR #285: a rejected Enter cancels a pending debounced panel edit; a no-op
    /// display timer repaints nothing; a volatile refresh of a new request is not blocked by
    /// one scheduled for the previous request.
    #[test]
    fn typed_enter_cancels_the_debounce_and_noop_timers_do_not_repaint() {
        let (mut c, ctx, mut rx) = settled_console(UpdateMode::ApplyImmediately);
        assert!(set_n(&mut c, &ctx, "7"));
        c.update(&UpdateMessage::UI(submit_event("-R/a.csv/-/nosuch")), &ctx);
        c.update(&debounce_timer(c.debounce), &ctx);
        assert_eq!(requested_query(&mut rx), None);
        assert_eq!(c.query_text, "-R/a.csv/-/nosuch", "the rejected text stays");
        assert!(c.typed_error().is_some());

        let generation = c.display.generation();
        let quiet = UpdateMessage::UI(timer_event("quiet", generation));
        assert_eq!(c.update(&quiet, &ctx), UpdateResponse::Unchanged, "settled: nothing to dim");

        c.volatile_pending = Some(generation);
        c.update(&UpdateMessage::UI(submit_event("-R/a.csv/-/head-4")), &ctx);
        assert_ne!(c.volatile_pending, Some(c.display.generation()));
    }

    /// AC-15: the probe of QUERY-CONSOLE-EDITOR-LOSES-EDITS-WHEN-UPDATE-IMMEDIATELY-IS-OFF: an
    /// edit not yet evaluated survives the next frames, and a second edit builds on it.
    #[test]
    fn pending_panel_edits_survive_a_frame() {
        let (mut c, ctx, _rx) = settled_console(UpdateMode::OnDemand);
        c.set_show_parameters(true);
        assert!(set_n(&mut c, &ctx, "7"));
        // A frame: the panel's tree is asked for again.
        let tree = c.parameters_tree(&ctx).expect("tree");
        assert_eq!(tree.text, "-R/a.csv/-/head-7");
        assert_eq!(c.update_mode(), UpdateMode::OnDemand, "the mode is not reset");
        // Typing in the query field (not yet entered) does not reset the panel either.
        c.query_text.push_str("/x");
        assert_eq!(c.parameters_tree(&ctx).expect("tree").text, "-R/a.csv/-/head-7");
        assert!(set_n(&mut c, &ctx, "9"));
        assert_eq!(c.query_text, "-R/a.csv/-/head-9", "a GUI edit replaces unentered typing");
    }

    /// AC-10: Undo and Redo in the panel rewrite the text.
    #[test]
    fn undo_in_the_panel_rewrites_the_text() {
        let (mut c, ctx, mut rx) = settled_console(UpdateMode::ApplyImmediately);
        assert!(!c.can_undo());
        assert!(set_n(&mut c, &ctx, "7"));
        assert!(set_n(&mut c, &ctx, "8"));
        assert!(c.can_undo());
        c.update(&UpdateMessage::UI(named(UNDO_EVENT)), &ctx);
        assert_eq!(c.query_text, "-R/a.csv/-/head-7");
        assert!(c.can_redo());
        c.update(&UpdateMessage::UI(named(REDO_EVENT)), &ctx);
        assert_eq!(c.query_text, "-R/a.csv/-/head-8");
        // An undo is an edit like any other: it is evaluated after the debounce.
        c.update(&UpdateMessage::UI(named(UNDO_EVENT)), &ctx);
        c.update(&debounce_timer(c.debounce), &ctx);
        assert_eq!(requested_query(&mut rx).as_deref(), Some("-R/a.csv/-/head-7"));
        // Undo history survives the evaluation.
        assert!(c.can_undo());
    }

    /// AC-16 (QUERY-CONSOLE-EDITOR-CANNOT-BROWSE-THE-STORE): Browse… lists the store folder and
    /// the listing reaches the panel.
    #[test]
    fn browse_lists_the_store() {
        let (mut c, ctx, mut rx) = settled_console(UpdateMode::ApplyImmediately);
        let _ = c.parameters_tree(&ctx);
        c.apply_parameter_commands(vec![EditorCommand::Browse { path: NodePath::element(0), folder: "/data/".into() }], &ctx);
        match rx.try_recv() {
            Ok(AppMessage::ListDirectory { handle, folder }) => {
                assert_eq!((handle, folder.as_str()), (UIHandle(1), "data"));
            }
            other => panic!("Expected ListDirectory, got {other:?}"),
        }
        let listing = crate::ui::editor::DirectoryListing { folder: "data".into(), entries: vec![], error: Some("empty".into()) };
        assert_eq!(c.update(&UpdateMessage::Directory(listing), &ctx), UpdateResponse::NeedsRepaint);
        let browser = c.parameters_tree(&ctx).expect("tree").browser.expect("browser");
        assert_eq!(browser.listing.and_then(|l| l.error).as_deref(), Some("empty"));
    }

    /// AC-19: an expired asset is recalculated in Apply immediately and marked changed upstream
    /// On demand.
    #[test]
    fn expiry_follows_the_mode() {
        let expired = AssetSnapshot {
            value: None,
            metadata: Metadata::new(),
            error: None,
            status: Status::Expired,
            query: Some("-R/a.csv/-/head-3".to_string()),
            preview: false,
        };
        let (mut c, ctx, mut rx) = settled_console(UpdateMode::ApplyImmediately);
        c.update(&UpdateMessage::AssetUpdate(expired.clone()), &ctx);
        assert_eq!(requested_query(&mut rx).as_deref(), Some("-R/a.csv/-/head-3"));
        assert!(!c.display.is_changed_upstream());

        let (mut c, ctx, mut rx) = settled_console(UpdateMode::OnDemand);
        c.update(&UpdateMessage::AssetUpdate(expired), &ctx);
        assert_eq!(requested_query(&mut rx), None);
        assert!(c.display.is_changed_upstream() && c.display.is_dimmed());
        assert!(c.check_mark_enabled(), "✓ recalculates");
        c.update(&UpdateMessage::UI(named(UPDATE_EVENT)), &ctx);
        assert_eq!(requested_query(&mut rx).as_deref(), Some("-R/a.csv/-/head-3"));
    }

    /// AC-4, AC-12, AC-21, AC-22: the browser shows ✓, the mode control, Undo, the status bar and
    /// a dimmed value.
    #[test]
    fn render_web_shows_mode_status_and_dimming() {
        let (mut c, ctx, _rx) = settled_console(UpdateMode::ApplyImmediately);
        let generation = c.display.generation();
        assert!(set_n(&mut c, &ctx, "7"));
        c.update(&debounce_timer(c.debounce), &ctx);
        let generation = c.display.generation().max(generation);
        c.update(&UpdateMessage::UI(timer_event("quiet", generation)), &ctx);
        assert!(c.display.is_dimmed());
        c.update(&UpdateMessage::UI(timer_event("slow", generation)), &ctx);
        assert!(c.display.status_line().is_some_and(|l| l.suggest_on_demand));
        #[cfg(feature = "webui")]
        {
            let html = c.render_web(&DirectAppState::new());
            for needle in [
                "data-lq-event=\"qc:mode::on_demand\"",
                "data-lq-event=\"qc:mode::immediate\"",
                "data-lq-event=\"qc:undo\"",
                "lq-status-bar",
                "Switch to On demand",
                "lq-value-stale",
            ] {
                assert!(html.contains(needle), "{needle}: {html}");
            }
            assert!(!html.contains("lq-value-preview"), "{html}");
            // Settled: no status bar, no dimming; ✓ becomes the lightning mark.
            c.update(&UpdateMessage::AssetUpdate(ready("-R/a.csv/-/head-7", "seven")), &ctx);
            let html = c.render_web(&DirectAppState::new());
            assert!(!html.contains("lq-status-bar") && !html.contains("lq-value-stale"), "{html}");
            assert!(html.contains("class=\"lq-go lq-qc-check\""), "nothing pending: a plain ✓: {html}");
            // A preview is framed (AC-22).
            let mut preview = ready("-R/a.csv/-/head-7", "p");
            preview.preview = true;
            c.display.request("-R/a.csv/-/head-7", true);
            c.update(&UpdateMessage::AssetUpdate(preview), &ctx);
            let html = c.render_web(&DirectAppState::new());
            assert!(html.contains("lq-value-preview"), "{html}");
        }
    }
}
