//! egui rendering of the query, action, resource and key editors (`crate::ui::editor`).
//!
//! [`show_editor`] draws an [`EditorTree`] in a mode, a density and a theme, and returns the
//! [`EditorCommand`]s the user's input produced this frame plus a requested mode change; the
//! element applies them to its core, so egui and the browser share every rule. Typing stays in the
//! [`EditorView`] buffers until a field is committed (Enter or focus loss); sliders, checkboxes and
//! selectors commit on change.
//!
//! # Main components
//!
//! - [`show_editor`]: the mode switch and the toolbar ("Update immediately", Update, Revert), the
//!   highlighted query text, then the [`editor_layout`] of the mode. Simple mode: step chips, one
//!   card for the last action and "Continue with" chips. Advanced mode: an outline beside the key
//!   part and query part cards, link arguments nested under their field, Collapse all / Expand
//!   all, and the filename field.
//! - Cards: a themed frame; a header of [`header_parts`] (chevron, icon, label, dimmed
//!   `namespace/name`, marks, position, menu); the [`toolbar_items`] when expanded, with presets as
//!   chips (at most three, the active one lit, the rest under "+N"); and a form grid of
//!   [`form_rows`] (labels, editors, per-field actions, help and the refused edit's message).
//! - The keyboard model of [`editor_keymap`], and `Ctrl+J` between the query text and the field
//!   at the cursor (the jump of [`EditorView`]).
//! - Variadic items and `ns` lists reorder by drag and drop (`Ui::dnd_drag_source`).
//!
//! # Where it is used
//!
//! - `EditorElement::show_in_egui` (`crate::ui::editor`) and the query console's Parameters panel.
//!
//! # Related concepts
//!
//! - [`crate::egui::theme`]: colours, density spacing and icon drawing.
//! - `crate::ui::web::editor`: the browser rendering of the same tree and layout.
//!
//! Design: `specs/design/query-editor-ergonomics/` (AC-1 to AC-10).
//!
//! # Examples
//!
//! ```
//! # use liquers_lib::egui::editor::show_editor;
//! # use liquers_lib::ui::editor::{EditorMode, EditorTree, EditorView};
//! # use liquers_lib::ui::theme::{Density, ThemeState};
//! let tree = EditorTree { text: "-R/data.csv".into(), text_editable: true, ..Default::default() };
//! let mut view = EditorView::default();
//! let theme = ThemeState::default();
//! let ctx = egui::Context::default();
//! let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
//!     let out = show_editor(ui, &tree, &mut view, EditorMode::Simple, Density::Comfortable, &theme, None);
//!     assert!(out.commands.is_empty() && out.mode.is_none());
//! });
//! output.textures_delta.clear();
//! ```

use egui::text::{CCursor, CCursorRange};
use egui::{Color32, Id, RichText, Sense, Stroke};
use liquers_core::command_metadata::ArgumentGUIInfo;

use crate::egui::theme::{color, draw_icon, spacing_style};
use crate::ui::editor::{
    editor_keymap, editor_layout, field_id, form_rows, header_parts, shortcut_text, toolbar_items, CardGroupKind,
    CardMenu, EditorCommand, EditorField, EditorKey, EditorMode, EditorNode, EditorTree, EditorView, FieldValue,
    FocusTarget, HeaderPart, KeyScope, NewElement, NodePath, StepChip, ToolItem, RESOURCE_HEADERS,
};
use crate::ui::theme::{Density, Palette, Spacing, ThemeState};
use crate::ui::ui_context::UIContext;

/// A Phosphor glyph by name, or `fallback` when the name is unknown.
fn ph(name: &str, fallback: &'static str) -> &'static str {
    crate::egui::theme::phosphor_glyph(name).unwrap_or(fallback)
}

/// What one frame of an editor produced.
#[derive(Debug, Clone, Default)]
pub struct EditorOutput {
    /// The edits, in order.
    pub commands: Vec<EditorCommand>,
    /// The mode the user switched to (the mode switch, `Ctrl+B`, a step chip, "Open in Advanced").
    pub mode: Option<EditorMode>,
}

/// The most chips a card's toolbar shows before "+N" (AC-7).
const PRESET_CHIPS: usize = 3;

/// Everything a frame draws with, and what it collects.
struct Cx<'a> {
    tree: &'a EditorTree,
    mode: EditorMode,
    theme: &'a ThemeState,
    palette: &'a Palette,
    sp: &'a Spacing,
    ui_ctx: Option<&'a UIContext>,
    out: Vec<EditorCommand>,
    mode_change: Option<EditorMode>,
    /// Widgets of this editor that can hold keyboard focus.
    ours: Vec<Id>,
    /// The id of the query text field.
    text_id: Id,
    /// This editor's id scope: every explicit widget id derives from it, so two editors showing
    /// the same path and argument never share focus or text state.
    base: Id,
    /// The cards in drawing order, for card navigation.
    cards: Vec<NodePath>,
}

impl Cx<'_> {
    fn faint(&self) -> Color32 {
        color(self.palette.text.ink_faint)
    }
    fn muted(&self) -> Color32 {
        color(self.palette.text.ink_muted)
    }
    fn danger(&self) -> Color32 {
        color(self.palette.state.danger)
    }
    fn accent(&self) -> Color32 {
        color(self.palette.state.accent)
    }
}

/// Draws `tree` in `mode` and `density` with `theme`, and returns this frame's output. `ui_ctx`
/// lets icons given by store key load; without it they fall back to their next variant.
pub fn show_editor(
    ui: &mut egui::Ui,
    tree: &EditorTree,
    view: &mut EditorView,
    mode: EditorMode,
    density: Density,
    theme: &ThemeState,
    ui_ctx: Option<&UIContext>,
) -> EditorOutput {
    crate::egui::theme::ensure_icon_font(ui.ctx());
    ui.scope(|ui| {
        spacing_style(ui.style_mut(), theme, density);
        let mut cx = Cx {
            tree,
            mode,
            theme,
            palette: theme.theme.palette(theme.mode),
            sp: theme.theme.spacing(density),
            ui_ctx,
            out: Vec::new(),
            mode_change: None,
            ours: Vec::new(),
            text_id: ui.id().with("lq-editor-text"),
            base: ui.id(),
            cards: Vec::new(),
        };
        view.resume_jump(tree, mode);
        let pending_at_start = view.focus_pending;
        body(ui, &mut cx, view);
        if pending_at_start && view.focus_pending {
            view.focus_pending = false; // its target was not drawn
        }
        handle_keys(ui, &mut cx, view);
        EditorOutput { commands: cx.out, mode: cx.mode_change }
    })
    .inner
}

fn body(ui: &mut egui::Ui, cx: &mut Cx, view: &mut EditorView) {
    toolbar(ui, cx);
    query_text(ui, cx, view);
    if let Some(path) = view.simple_offer.clone() {
        ui.horizontal(|ui| {
            let label = format!("Open in Advanced ({})", shortcut_text(EditorKey::ToggleMode).unwrap_or("Ctrl+B"));
            if ui.button(label).on_hover_text("This step is not the card Simple mode shows").clicked() {
                cx.mode_change = Some(EditorMode::Advanced);
                view.simple_offer = None;
                view.focus = Some(FocusTarget::Node(path));
                view.focus_pending = true;
            }
        });
    }
    if let Some((path, message)) = &cx.tree.message {
        if path.0.is_empty() {
            ui.colored_label(cx.danger(), message);
        }
    }
    let tree = cx.tree;
    let layout = editor_layout(tree, cx.mode);
    cx.cards = layout.groups.iter().flat_map(|g| g.cards.iter().map(|c| c.node.path().clone())).collect();
    ui.add_enabled_ui(!tree.read_only, |ui| {
        if !layout.before.is_empty() {
            chips(ui, cx, view, &layout.before);
        }
        let cards = |ui: &mut egui::Ui, cx: &mut Cx, view: &mut EditorView| {
            let mut position = 0;
            for group in &layout.groups {
                if layout.outline {
                    let title = match group.kind {
                        CardGroupKind::KeyPart => "Key part",
                        CardGroupKind::QueryPart => "Query part",
                    };
                    ui.label(RichText::new(title).small().color(cx.muted()));
                }
                for card_item in &group.cards {
                    position += 1;
                    card(ui, cx, view, card_item.node, position);
                    ui.add_space(cx.sp.card_gap);
                }
            }
        };
        if layout.outline {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(150.0);
                    outline(ui, cx, view);
                });
                ui.separator();
                ui.vertical(|ui| cards(ui, cx, view));
            });
        } else {
            cards(ui, cx, view);
        }
        if !layout.after.is_empty() {
            chips(ui, cx, view, &layout.after);
        }
        footer(ui, cx, view, &layout);
    });
    let (tree, out) = (cx.tree, &mut cx.out);
    picker(ui, tree, view, out);
    browser(ui, tree, out);
    confirmation(ui, tree, out);
}

fn toolbar(ui: &mut egui::Ui, cx: &mut Cx) {
    let tree = cx.tree;
    ui.horizontal(|ui| {
        let tip = format!("Simple shows the last step; Advanced every step ({})", shortcut_text(EditorKey::ToggleMode).unwrap_or(""));
        for (mode, label) in [(EditorMode::Simple, "Simple"), (EditorMode::Advanced, "Advanced")] {
            if ui.selectable_label(cx.mode == mode, label).on_hover_text(&tip).clicked() && cx.mode != mode {
                cx.mode_change = Some(mode);
            }
        }
        ui.separator();
        let mut continuous = tree.continuous;
        if ui
            .checkbox(&mut continuous, "Update immediately")
            .on_hover_text("Set each edit into where the value lives at once")
            .changed()
        {
            cx.out.push(EditorCommand::SetContinuous(continuous));
        }
        if !tree.continuous {
            if ui.add_enabled(tree.pending, egui::Button::new("Update")).clicked() {
                cx.out.push(EditorCommand::Update);
            }
            if ui.button("Revert").clicked() {
                cx.out.push(EditorCommand::Revert);
            }
        }
        if tree.pending {
            ui.label(RichText::new("pending").italics());
        }
    });
}

/// The byte offset of character `index` of `text`.
fn byte_of_char(text: &str, index: usize) -> usize {
    text.char_indices().nth(index).map(|(b, _)| b).unwrap_or(text.len())
}

/// The character index of byte `offset` of `text`.
fn char_of_byte(text: &str, offset: usize) -> usize {
    text.char_indices().take_while(|(b, _)| *b < offset).count()
}

fn query_text(ui: &mut egui::Ui, cx: &mut Cx, view: &mut EditorView) {
    let tree = cx.tree;
    if tree.text_editable {
        let theme = cx.theme.clone();
        let text_id = cx.text_id;
        let buffer = view.text_buffer(&tree.text).clone();
        let mut typed = buffer.clone();
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _wrap: f32| {
            let job = crate::egui::widgets::query_to_layout_job_themed(buf.as_str(), None, &theme);
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let response = ui.add(
            egui::TextEdit::singleline(&mut typed)
                .id(text_id)
                .desired_width(ui.available_width())
                .layouter(&mut layouter)
                .hint_text("Enter query here…"),
        );
        cx.ours.push(text_id);
        if typed != buffer {
            *view.text_buffer(&tree.text) = typed.clone();
            if let Some((t, _)) = view.text.as_mut() {
                *t = typed.clone();
            }
        }
        if response.gained_focus() || (response.has_focus() && view.focus.is_none()) {
            view.focus = Some(FocusTarget::Text);
        }
        if view.focus == Some(FocusTarget::Text) && view.focus_pending {
            response.request_focus();
            view.focus_pending = false;
        }
        let committed = response.lost_focus() || (response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
        if committed && typed != tree.text {
            cx.out.push(EditorCommand::SetText(typed.clone()));
        }
        // Ctrl-click jumps like Ctrl+J.
        if response.clicked() && ui.input(|i| i.modifiers.command) {
            if let Some(offset) = text_cursor(ui.ctx(), text_id, &typed) {
                if let Some(command) = view.jump_from_text(tree, cx.mode, offset) {
                    cx.out.push(command);
                }
            }
        }
        // While the cursor moves, name the target.
        if response.has_focus() {
            if let Some(span) = text_cursor(ui.ctx(), text_id, &typed).and_then(|o| tree.locate(o)) {
                let hint = jump_hint(tree, span);
                ui.label(RichText::new(format!("{hint} · {}", shortcut_text(EditorKey::Jump).unwrap_or(""))).small().color(cx.faint()));
            }
        }
    } else if !tree.text.is_empty() {
        ui.label(crate::egui::widgets::query_to_layout_job_themed(tree.text.clone(), None, cx.theme));
    }
    if let Some(note) = &tree.text_note {
        ui.colored_label(cx.danger(), note);
    }
    if tree.stale {
        ui.label(RichText::new("The tree shows the last valid query.").italics());
    }
}

/// The byte offset of the query text's cursor, from its `TextEdit` state.
fn text_cursor(ctx: &egui::Context, id: Id, text: &str) -> Option<usize> {
    let state = egui::text_edit::TextEditState::load(ctx, id)?;
    let range = state.cursor.char_range()?;
    Some(byte_of_char(text, range.primary.index.0))
}

/// Selects the byte `range` of the query text and focuses it.
fn select_text(ctx: &egui::Context, id: Id, text: &str, range: std::ops::Range<usize>) {
    let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    let (a, b) = (char_of_byte(text, range.start), char_of_byte(text, range.end));
    state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
    state.store(ctx, id);
}

fn jump_hint(tree: &EditorTree, span: &crate::ui::editor::TextSpan) -> String {
    let node = crate::ui::editor::find_node(&tree.nodes, &span.path);
    let label = node.map(crate::ui::editor::node_label).unwrap_or_default();
    match &span.target {
        crate::ui::editor::SpanTarget::Element => label,
        crate::ui::editor::SpanTarget::Namespaces => "ns".to_string(),
        crate::ui::editor::SpanTarget::Argument { arg, .. } => {
            let field = node.and_then(|n| n.fields().iter().find(|f| &f.name == arg));
            let name = field.map(|f| if f.label.is_empty() { f.name.clone() } else { f.label.clone() }).unwrap_or(arg.clone());
            format!("{label} › {name}")
        }
    }
}

/// Read-only step chips; a click opens the step in Advanced mode.
fn chips(ui: &mut egui::Ui, cx: &mut Cx, view: &mut EditorView, chips: &[StepChip]) {
    ui.horizontal_wrapped(|ui| {
        for (i, chip) in chips.iter().enumerate() {
            if i > 0 {
                ui.label(RichText::new(ph("caret-right", ">")).color(cx.faint()));
            }
            let text = RichText::new(&chip.label).color(cx.muted());
            if ui.add(egui::Button::new(text).small()).on_hover_text("Open in Advanced").clicked() {
                cx.mode_change = Some(EditorMode::Advanced);
                view.focus = Some(FocusTarget::Node(chip.path.clone()));
                view.focus_pending = true;
            }
        }
    });
}

fn outline(ui: &mut egui::Ui, cx: &mut Cx, view: &mut EditorView) {
    ui.label(RichText::new("Outline").small().color(cx.muted()));
    let tree = cx.tree;
    for node in &tree.nodes {
        let focused = view.focus.as_ref().is_some_and(|f| focus_card(f) == Some(node.path()));
        let label = crate::ui::editor::node_label(node);
        if ui.selectable_label(focused, label).clicked() {
            view.focus = Some(FocusTarget::Node(node.path().clone()));
            view.focus_pending = true;
            view.collapsed.remove(&node.path().id());
        }
    }
}

/// The card a focus target is in.
fn focus_card(focus: &FocusTarget) -> Option<&NodePath> {
    match focus {
        FocusTarget::Text => None,
        FocusTarget::Node(path) | FocusTarget::Field { path, .. } => Some(path),
    }
}

fn footer(ui: &mut egui::Ui, cx: &mut Cx, view: &mut EditorView, layout: &crate::ui::editor::EditorLayout<'_>) {
    let tree = cx.tree;
    ui.horizontal_wrapped(|ui| {
        if cx.mode == EditorMode::Simple {
            if let Some(EditorNode::Action { path, next, .. }) = layout.groups.first().and_then(|g| g.cards.first()).map(|c| c.node) {
                if !next.is_empty() {
                    ui.label(RichText::new("Continue with").small().color(cx.muted()));
                    for (i, p) in next.iter().enumerate() {
                        if ui.button(&p.label).on_hover_text(&p.description).clicked() {
                            cx.out.push(EditorCommand::ApplyNext { path: path.clone(), index: i });
                        }
                    }
                }
            }
        }
        if tree.has_filename {
            let add = format!("Add action… ({})", shortcut_text(EditorKey::AddAction).unwrap_or(""));
            if ui.button(add).on_hover_text("Append a command").clicked() {
                cx.out.push(EditorCommand::SearchCommands { insert_at: None, filter: String::new() });
            }
            if cx.mode == EditorMode::Advanced {
                ui.menu_button("+ directive", |ui| {
                    if ui.button("q (quote)").clicked() {
                        cx.out.push(EditorCommand::Append(NewElement::Quote));
                    }
                    if ui.button("v (volatile)").clicked() {
                        cx.out.push(EditorCommand::Append(NewElement::Volatile));
                    }
                });
                if ui.button("Collapse all").clicked() {
                    collapse_all(cx, view, true);
                }
                if ui.button("Expand all").clicked() {
                    collapse_all(cx, view, false);
                }
            }
        }
    });
    if tree.has_filename && cx.mode == EditorMode::Advanced {
        filename(ui, tree, view, &mut cx.out);
    }
}

fn collapse_all(cx: &Cx, view: &mut EditorView, collapse: bool) {
    for path in &cx.cards {
        if collapse {
            view.collapsed.insert(path.id());
        } else {
            view.collapsed.remove(&path.id());
        }
    }
}

/// One card: frame, header, toolbar and form grid (AC-4 to AC-6).
fn card(ui: &mut egui::Ui, cx: &mut Cx, view: &mut EditorView, node: &EditorNode, position: usize) {
    let path = node.path().clone();
    let collapsed = cx.mode == EditorMode::Advanced && view.collapsed.contains(&path.id());
    let focused = view.focus.as_ref().and_then(focus_card) == Some(&path);
    let marks = crate::ui::editor::card_marks(cx.tree, node);
    let stroke = if marks.error && cx.tree.message.as_ref().is_some_and(|(p, _)| p == &path) {
        Stroke::new(1.5, cx.danger())
    } else if focused {
        Stroke::new(1.5, color(cx.palette.state.focus_ring))
    } else {
        Stroke::new(1.0, color(cx.palette.surface.line))
    };
    egui::Frame::new()
        .fill(color(cx.palette.surface.card))
        .stroke(stroke)
        .corner_radius(cx.sp.card_radius)
        .inner_margin(cx.sp.card_padding)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            card_header(ui, cx, view, node, collapsed, position);
            if collapsed {
                return;
            }
            match node {
                EditorNode::Action { excess, .. } => {
                    card_toolbar(ui, cx, view, node);
                    form_grid(ui, cx, view, node);
                    if !excess.is_empty() {
                        ui.label(RichText::new(format!("Extra parameters (not used): {}", excess.join(", "))).italics());
                    }
                }
                EditorNode::Resource { .. }
                | EditorNode::Namespaces { .. }
                | EditorNode::Directive { .. }
                | EditorNode::Value { .. }
                | EditorNode::Nested { .. }
                | EditorNode::Opaque { .. } => {
                    node_body(ui, cx, view, node);
                    if let Some((p, message)) = &cx.tree.message {
                        if p == &path {
                            ui.colored_label(cx.danger(), message);
                        }
                    }
                }
            }
        });
}

fn card_header(ui: &mut egui::Ui, cx: &mut Cx, view: &mut EditorView, node: &EditorNode, collapsed: bool, position: usize) {
    let path = node.path().clone();
    let parts = header_parts(cx.tree, node, collapsed, position);
    let (label_text, hover) = match node {
        EditorNode::Action { label, doc, .. } => (label.clone(), doc.clone()),
        EditorNode::Resource { .. }
        | EditorNode::Namespaces { .. }
        | EditorNode::Directive { .. }
        | EditorNode::Value { .. }
        | EditorNode::Nested { .. }
        | EditorNode::Opaque { .. } => (crate::ui::editor::node_label(node), String::new()),
    };
    // The `ns` node shares its path with the action after it.
    let kind = if matches!(node, EditorNode::Namespaces { .. }) { "ns" } else { "node" };
    let header_id = ui.id().with(("lq-card-head", kind, path.id()));
    let expanded_two_lines = !collapsed && matches!(node, EditorNode::Action { .. });
    ui.horizontal(|ui| {
        for part in &parts {
            match part {
                HeaderPart::Chevron => {
                    if cx.mode == EditorMode::Advanced {
                        let chevron = if collapsed { ph("caret-right", ">") } else { ph("caret-down", "v") };
                        if ui.add(egui::Button::new(chevron).frame(false)).on_hover_text("Collapse or expand").clicked() {
                            toggle(view, &path);
                        }
                    }
                }
                HeaderPart::Icon => icon_tile(ui, cx, node),
                HeaderPart::Realm(_) | HeaderPart::CommandId(_) if expanded_two_lines => {}
                HeaderPart::Realm(realm) => {
                    ui.label(RichText::new(format!("[{realm}]")).small().color(cx.faint()));
                }
                HeaderPart::CommandId(id) => {
                    ui.label(RichText::new(id).monospace().small().color(cx.faint()));
                }
                HeaderPart::Label(_) => {
                    let shown = if expanded_two_lines {
                        ui.vertical(|ui| {
                            let id_line: Vec<String> = parts
                                .iter()
                                .filter_map(|p| match p {
                                    HeaderPart::Realm(r) => Some(format!("[{r}]")),
                                    HeaderPart::CommandId(id) => Some(id.clone()),
                                    HeaderPart::Chevron
                                    | HeaderPart::Icon
                                    | HeaderPart::Label(_)
                                    | HeaderPart::Modified
                                    | HeaderPart::Error
                                    | HeaderPart::Position(_)
                                    | HeaderPart::Menu => None,
                                })
                                .collect();
                            ui.label(RichText::new(id_line.join(" ")).monospace().small().color(cx.faint()));
                            header_label(ui, &label_text, header_id)
                        })
                        .inner
                    } else {
                        header_label(ui, &label_text, header_id)
                    };
                    cx.ours.push(header_id);
                    let shown = if hover.is_empty() { shown } else { shown.on_hover_text(&hover) };
                    if shown.clicked() || shown.gained_focus() {
                        view.focus = Some(FocusTarget::Node(path.clone()));
                    }
                    if shown.double_clicked() {
                        toggle(view, &path);
                    }
                    if view.focus == Some(FocusTarget::Node(path.clone())) && view.focus_pending {
                        shown.request_focus();
                        shown.scroll_to_me(None);
                        view.focus_pending = false;
                    }
                }
                HeaderPart::Modified => {
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
                    ui.painter().circle_filled(rect.center(), 3.5, color(cx.palette.state.modified));
                    response.on_hover_text("An argument differs from its default");
                }
                HeaderPart::Error => {
                    ui.label(RichText::new(ph("warning", "!")).color(cx.danger())).on_hover_text("This step has a problem");
                }
                HeaderPart::Position(n) => {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !node.path().0.is_empty() {
                            node_menu(ui, node.path(), &mut cx.out);
                        }
                        ui.label(RichText::new(format!("#{n}")).small().color(cx.faint()));
                    });
                }
                HeaderPart::Menu => {} // drawn with the position, right-aligned
            }
        }
    });
}

fn header_label(ui: &mut egui::Ui, text: &str, id: Id) -> egui::Response {
    let response = ui.add(egui::Label::new(RichText::new(text).strong()).sense(Sense::click()));
    // Give the label a stable id for keyboard focus.
    ui.interact(response.rect, id, Sense::click()).union(response)
}

fn toggle(view: &mut EditorView, path: &NodePath) {
    let id = path.id();
    if !view.collapsed.remove(&id) {
        view.collapsed.insert(id);
    }
}

fn icon_tile(ui: &mut egui::Ui, cx: &mut Cx, node: &EditorNode) {
    let theme = &cx.theme.theme;
    let size = cx.sp.icon_size;
    let (resolved, fallback);
    let icon = match node {
        EditorNode::Action { command, label, hint_icon, .. } => {
            resolved = theme.icon_for(command, label, hint_icon.as_ref(), None);
            &resolved
        }
        EditorNode::Resource { .. }
        | EditorNode::Namespaces { .. }
        | EditorNode::Directive { .. }
        | EditorNode::Value { .. }
        | EditorNode::Nested { .. }
        | EditorNode::Opaque { .. } => {
            let (name, unicode) = match node {
                EditorNode::Resource { .. } => ("file", "▤"),
                EditorNode::Namespaces { .. } => ("tag", "#"),
                EditorNode::Directive { .. } => ("quotes", "\""),
                EditorNode::Value { .. } => ("brackets-curly", "{"),
                EditorNode::Nested { .. } => ("tree-structure", "⊞"),
                EditorNode::Opaque { .. } | EditorNode::Action { .. } => ("question", "?"),
            };
            fallback = (
                crate::ui::theme::IconVariant::Set { set: Some(crate::ui::theme::PHOSPHOR.into()), name: name.into() },
                unicode,
            );
            resolved = crate::ui::theme::ResolvedIcon { variants: vec![std::borrow::Cow::Borrowed(&fallback.0)], unicode: fallback.1.into(), hue: None };
            &resolved
        }
    };
    let hue = icon.hue.and_then(|h| theme.hue(h, cx.theme.mode)).map(color).unwrap_or(cx.accent());
    egui::Frame::new()
        .fill(hue.gamma_multiply(0.22))
        .corner_radius(cx.sp.card_radius * 0.6)
        .inner_margin(2.0)
        .show(ui, |ui| {
            draw_icon(ui, icon, size * 0.75, hue, cx.ui_ctx);
        });
}

fn card_toolbar(ui: &mut egui::Ui, cx: &mut Cx, view: &mut EditorView, node: &EditorNode) {
    let EditorNode::Action { path, presets, next, filename, fields, .. } = node else {
        return;
    };
    let items = toolbar_items(node, cx.mode);
    ui.horizontal_wrapped(|ui| {
        for item in &items {
            match item {
                ToolItem::Reset => {
                    let tip = format!("Reset every argument to its default ({})", shortcut_text(EditorKey::Reset).unwrap_or(""));
                    if ui.small_button(format!("{} Reset", ph("arrow-counter-clockwise", ""))).on_hover_text(tip).clicked() {
                        reset_all(&mut cx.out, path, fields);
                    }
                }
                ToolItem::Presets => {
                    for (i, p) in presets.iter().enumerate().take(PRESET_CHIPS) {
                        let text = RichText::new(format!("{} {}", i + 1, p.label));
                        let tip = format!("{} (Alt+{})", p.description, i + 1);
                        if ui.selectable_label(p.active, text).on_hover_text(tip).clicked() {
                            cx.out.push(EditorCommand::ApplyPreset { path: path.clone(), index: i });
                        }
                    }
                    if presets.len() > PRESET_CHIPS {
                        ui.menu_button(format!("+{}", presets.len() - PRESET_CHIPS), |ui| {
                            for (i, p) in presets.iter().enumerate().skip(PRESET_CHIPS) {
                                if ui.selectable_label(p.active, format!("{} {}", i + 1, p.label)).clicked() {
                                    cx.out.push(EditorCommand::ApplyPreset { path: path.clone(), index: i });
                                }
                            }
                        });
                    }
                }
                ToolItem::Next => {
                    ui.menu_button(format!("Next {}", ph("caret-down", "")), |ui| {
                        for (i, p) in next.iter().enumerate() {
                            if ui.button(&p.label).on_hover_text(&p.description).clicked() {
                                cx.out.push(EditorCommand::ApplyNext { path: path.clone(), index: i });
                            }
                        }
                    });
                }
                ToolItem::Filename => {
                    if let Some(name) = filename {
                        if ui.small_button("Filename").on_hover_text(name).clicked() {
                            cx.out.push(EditorCommand::FilenameFrom { path: path.clone() });
                        }
                    }
                }
                ToolItem::MoveUp => {
                    if ui.small_button(ph("arrow-up", "Up")).on_hover_text("Move up (Alt+Up)").clicked() {
                        move_card(&mut cx.out, path, -1);
                    }
                }
                ToolItem::MoveDown => {
                    if ui.small_button(ph("arrow-down", "Down")).on_hover_text("Move down (Alt+Down)").clicked() {
                        move_card(&mut cx.out, path, 1);
                    }
                }
                ToolItem::Delete => {
                    let danger = cx.danger();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(RichText::new(format!("{} Delete", ph("trash", ""))).color(danger)).on_hover_text("Delete this step").clicked() {
                            cx.out.push(EditorCommand::Remove { path: path.clone() });
                        }
                    });
                }
            }
        }
    });
    // A list opened from the keyboard.
    if let Some((menu_path, menu)) = view.open_menu.clone() {
        if &menu_path == path {
            let entries: Vec<(usize, String, bool)> = match &menu {
                CardMenu::Presets => presets.iter().enumerate().map(|(i, p)| (i, p.label.clone(), p.active)).collect(),
                CardMenu::Next => next.iter().enumerate().map(|(i, p)| (i, p.label.clone(), false)).collect(),
                CardMenu::Argument(arg) => fields
                    .iter()
                    .find(|f| &f.name == arg)
                    .map(|f| f.presets.iter().enumerate().map(|(i, p)| (i, p.label.clone(), p.active)).collect())
                    .unwrap_or_default(),
            };
            egui::Frame::new().stroke(Stroke::new(1.0, color(cx.palette.surface.line_strong))).inner_margin(4.0).show(ui, |ui| {
                for (i, label, active) in entries {
                    if ui.selectable_label(active, label).clicked() {
                        cx.out.push(match &menu {
                            CardMenu::Presets => EditorCommand::ApplyPreset { path: path.clone(), index: i },
                            CardMenu::Next => EditorCommand::ApplyNext { path: path.clone(), index: i },
                            CardMenu::Argument(arg) => {
                                EditorCommand::ApplyArgumentPreset { path: path.clone(), arg: arg.clone(), index: i }
                            }
                        });
                        view.open_menu = None;
                    }
                }
                if ui.small_button("Close").clicked() {
                    view.open_menu = None;
                }
            });
        }
    }
}

fn reset_all(out: &mut Vec<EditorCommand>, path: &NodePath, fields: &[EditorField]) {
    for field in fields {
        let default = matches!(&field.value, FieldValue::Text(t) if field.default_text.as_deref() == Some(t.as_str()));
        if !default && !matches!(field.value, FieldValue::Unset) {
            out.push(EditorCommand::Reset { path: path.clone(), arg: field.name.clone() });
        }
    }
}

fn move_card(out: &mut Vec<EditorCommand>, path: &NodePath, by: isize) {
    if let Some(at) = path.last_element() {
        let to = at as isize + by;
        if to >= 1 {
            out.push(EditorCommand::Move { path: path.clone(), to: to as usize });
        }
    }
}

/// The form grid: labels right-aligned in a fixed column, the editors, and a narrow column of
/// per-field actions that stay faint until the row is hovered or focused (AC-6).
fn form_grid(ui: &mut egui::Ui, cx: &mut Cx, view: &mut EditorView, node: &EditorNode) {
    let path = node.path().clone();
    let rows = form_rows(cx.tree, node, view.last_field.as_deref());
    egui::Grid::new(("lq-form", path.id())).num_columns(3).spacing([8.0, cx.sp.row_gap]).show(ui, |ui| {
        for row in rows {
            let field = row.field;
            ui.allocate_ui_with_layout(
                egui::vec2(cx.sp.label_width, cx.sp.control_height),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.set_min_width(cx.sp.label_width);
                    let text = if row.required { format!("{} *", row.label) } else { row.label.clone() };
                    ui.label(RichText::new(text).color(cx.muted())).on_hover_text(field.hover());
                },
            );
            let mut row_active = false;
            ui.vertical(|ui| {
                row_active = field_editor(ui, cx, view, &path, field);
                if !row.help.is_empty() {
                    ui.add(egui::Label::new(RichText::new(&row.help).small().color(cx.faint())).extend());
                }
                if let Some(message) = &row.message {
                    ui.colored_label(cx.danger(), message);
                }
            });
            field_actions(ui, cx, &path, field, row_active);
            ui.end_row();
        }
    });
}

/// The editor of one field; returns whether it is hovered or focused.
fn field_editor(ui: &mut egui::Ui, cx: &mut Cx, view: &mut EditorView, path: &NodePath, field: &EditorField) -> bool {
    let target = FocusTarget::Field { path: path.clone(), arg: field.name.clone(), item: None };
    let response = match &field.value {
        FieldValue::Link { query, nodes: inner, is_key } => {
            let id = field_id(path, &field.name, None);
            let buffer = view.buffer(&id, query);
            let response = ui.add(egui::TextEdit::singleline(buffer).desired_width(ui.available_width().min(320.0)));
            if response.lost_focus() && *buffer != *query {
                cx.out.push(EditorCommand::SetLink { path: path.clone(), arg: field.name.clone(), query: buffer.clone() });
                view.last_field = Some(id);
            }
            if cx.mode == EditorMode::Advanced {
                // The link's own tree, nested under the field with a guide line.
                let guide = color(cx.palette.surface.line_strong);
                let inner_resp = ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(if *is_key { "key link" } else { "link" }).small().color(cx.faint()));
                        for (i, n) in inner.iter().enumerate() {
                            card(ui, cx, view, n, i + 1);
                        }
                    });
                });
                let r = inner_resp.response.rect;
                ui.painter().add(egui::Shape::dashed_line(&[r.left_top(), r.left_bottom()], Stroke::new(1.0, guide), 4.0, 3.0));
            }
            Some(response)
        }
        FieldValue::Items(items) => {
            items_ui(ui, cx, view, path, field, items);
            None
        }
        FieldValue::Other(text) => Some(ui.label(RichText::new(text).italics())),
        FieldValue::Unset | FieldValue::Text(_) => {
            let text = match &field.value {
                FieldValue::Text(t) => t.clone(),
                FieldValue::Unset | FieldValue::Link { .. } | FieldValue::Items(_) | FieldValue::Other(_) => String::new(),
            };
            if field.multiple {
                items_ui(ui, cx, view, path, field, &[]);
                None
            } else {
                let (response, command) = widget(ui, cx.base, path, field, &text, None, view);
                if let Some(command) = command {
                    cx.out.push(command);
                    view.last_field = Some(field_id(path, &field.name, None));
                }
                response
            }
        }
    };
    let Some(response) = response else {
        return ui.ui_contains_pointer();
    };
    cx.ours.push(response.id);
    if response.gained_focus() {
        view.focus = Some(target.clone());
    }
    if view.focus.as_ref() == Some(&target) && view.focus_pending {
        response.request_focus();
        response.scroll_to_me(None);
        view.focus_pending = false;
    }
    response.hovered() || response.has_focus()
}

fn field_actions(ui: &mut egui::Ui, cx: &mut Cx, path: &NodePath, field: &EditorField, active: bool) {
    let tint = if active { cx.muted() } else { cx.faint().gamma_multiply(0.6) };
    ui.horizontal(|ui| {
        if !field.presets.is_empty() {
            ui.menu_button(RichText::new(ph("caret-down", "v")).color(tint), |ui| {
                for (i, p) in field.presets.iter().enumerate() {
                    if ui.selectable_label(p.active, &p.label).on_hover_text(&p.description).clicked() {
                        cx.out.push(EditorCommand::ApplyArgumentPreset { path: path.clone(), arg: field.name.clone(), index: i });
                    }
                }
            });
        }
        let is_link = matches!(field.value, FieldValue::Link { .. });
        let switch = RichText::new(if is_link { ph("text-t", "=") } else { ph("link", "~") }).color(tint);
        let tip = format!("Switch between a value and a link ({})", shortcut_text(EditorKey::ToggleLink).unwrap_or(""));
        if ui.add(egui::Button::new(switch).frame(false)).on_hover_text(tip).clicked() {
            cx.out.push(if is_link {
                EditorCommand::ToValue { path: path.clone(), arg: field.name.clone() }
            } else {
                EditorCommand::ToLink { path: path.clone(), arg: field.name.clone() }
            });
        }
        if ui.add(egui::Button::new(RichText::new(ph("arrow-counter-clockwise", "R")).color(tint)).frame(false)).on_hover_text("Reset to the default").clicked() {
            cx.out.push(EditorCommand::Reset { path: path.clone(), arg: field.name.clone() });
        }
    });
}

/// The body of a card that is not an action.
fn node_body(ui: &mut egui::Ui, cx: &mut Cx, view: &mut EditorView, node: &EditorNode) {
    let out = &mut cx.out;
    match node {
        EditorNode::Resource { path, header, key, note } => {
            ui.horizontal(|ui| {
                if let Some(header) = header {
                    let mut chosen = header.clone();
                    egui::ComboBox::from_id_salt(("header", path.id()))
                        .selected_text(if chosen.is_empty() { "(asset)".to_string() } else { chosen.clone() })
                        .width(80.0)
                        .show_ui(ui, |ui| {
                            for h in RESOURCE_HEADERS {
                                ui.selectable_value(&mut chosen, h.to_string(), if h.is_empty() { "(asset)" } else { h });
                            }
                        });
                    if &chosen != header {
                        out.push(EditorCommand::SetHeader { path: path.clone(), header: chosen });
                    }
                }
                let id = format!("key:{}", path.id());
                let buffer = view.buffer(&id, key);
                let response = ui.add(egui::TextEdit::singleline(buffer).desired_width(ui.available_width().min(260.0)));
                cx.ours.push(response.id);
                if response.lost_focus() && *buffer != *key {
                    out.push(EditorCommand::SetKey { path: path.clone(), key: buffer.clone() });
                }
                if ui.button(ph("folder-open", "...")).on_hover_text("Browse the store").clicked() {
                    let folder = key.rsplit_once('/').map(|(f, _)| f.to_string()).unwrap_or_default();
                    out.push(EditorCommand::Browse { path: path.clone(), folder });
                }
            });
            if let Some(note) = note {
                ui.label(RichText::new(note).italics());
            }
        }
        EditorNode::Namespaces { path, list, unused } => {
            ui.horizontal(|ui| {
                ui.label("ns").on_hover_text("The namespaces the following commands are looked up in");
                let mut moved: Option<(usize, usize)> = None;
                for (i, ns) in list.iter().enumerate() {
                    let id = cx.base.with(("ns", path.id(), i));
                    let response = ui
                        .dnd_drag_source(id, i, |ui| {
                            ui.label(RichText::new(ns).monospace());
                        })
                        .response;
                    if let Some(from) = response.dnd_release_payload::<usize>() {
                        moved = Some((*from, i));
                    }
                    if ui.small_button(ph("x", "x")).on_hover_text("Remove this namespace").clicked() {
                        let mut new = list.clone();
                        new.remove(i);
                        out.push(EditorCommand::SetNamespaces { path: path.clone(), namespaces: new });
                    }
                }
                if let Some((from, to)) = moved {
                    let mut new = list.clone();
                    let item = new.remove(from);
                    new.insert(to, item);
                    out.push(EditorCommand::SetNamespaces { path: path.clone(), namespaces: new });
                }
                ui.menu_button("+", |ui| {
                    for ns in unused {
                        if ui.button(ns).clicked() {
                            let mut new = list.clone();
                            new.push(ns.clone());
                            out.push(EditorCommand::SetNamespaces { path: path.clone(), namespaces: new });
                        }
                    }
                });
            });
        }
        EditorNode::Directive { name, .. } => {
            ui.label(RichText::new(if name == "q" { "Quote: pass the query so far as a value" } else { "Volatile: never cache" }).small());
        }
        EditorNode::Value { json, .. } => {
            ui.label(RichText::new(json).monospace());
        }
        EditorNode::Nested { nodes: inner, .. } => {
            ui.add_enabled_ui(false, |ui| {
                for (i, n) in inner.iter().enumerate() {
                    card(ui, cx, view, n, i + 1);
                }
            });
        }
        EditorNode::Opaque { reason, text, .. } => {
            ui.label(RichText::new(text).monospace());
            ui.label(RichText::new(reason).italics());
        }
        EditorNode::Action { .. } => {}
    }
}

fn node_menu(ui: &mut egui::Ui, path: &NodePath, out: &mut Vec<EditorCommand>) {
    ui.menu_button(ph("dots-three", "..."), |ui| {
        if ui.button("Insert action before").clicked() {
            out.push(EditorCommand::SearchCommands { insert_at: Some(path.clone()), filter: String::new() });
        }
        if ui.button("Insert resource before").clicked() {
            out.push(EditorCommand::Insert { path: path.clone(), element: NewElement::Resource("data".into()) });
        }
        if let Some(at) = path.last_element() {
            if at > 0 && ui.button("Move up").clicked() {
                out.push(EditorCommand::Move { path: path.clone(), to: at - 1 });
            }
            if ui.button("Move down").clicked() {
                out.push(EditorCommand::Move { path: path.clone(), to: at + 1 });
            }
        }
        if ui.button("Delete").clicked() {
            out.push(EditorCommand::Remove { path: path.clone() });
        }
    });
}

fn items_ui(ui: &mut egui::Ui, cx: &mut Cx, view: &mut EditorView, path: &NodePath, field: &EditorField, items: &[FieldValue]) {
    ui.horizontal_wrapped(|ui| {
        let mut moved: Option<(usize, usize)> = None;
        for (k, item) in items.iter().enumerate() {
            let id = cx.base.with(("item", path.id(), &field.name, k));
            let target = FocusTarget::Field { path: path.clone(), arg: field.name.clone(), item: Some(k) };
            let response = ui
                .dnd_drag_source(id, k, |ui| match item {
                    FieldValue::Text(text) => {
                        let (response, command) = widget(ui, cx.base, path, field, text, Some(k), view);
                        if let Some(command) = command {
                            cx.out.push(command);
                            view.last_field = Some(field_id(path, &field.name, Some(k)));
                        }
                        if let Some(r) = response {
                            cx.ours.push(r.id);
                            if r.gained_focus() {
                                view.focus = Some(target.clone());
                            }
                            if view.focus.as_ref() == Some(&target) && view.focus_pending {
                                r.request_focus();
                                view.focus_pending = false;
                            }
                        }
                    }
                    FieldValue::Link { query, .. } => {
                        ui.label(RichText::new(query).monospace());
                    }
                    FieldValue::Unset | FieldValue::Items(_) | FieldValue::Other(_) => {
                        ui.label("…");
                    }
                })
                .response;
            if let Some(from) = response.dnd_release_payload::<usize>() {
                moved = Some((*from, k));
            }
            if ui.small_button(ph("x", "x")).clicked() {
                cx.out.push(EditorCommand::RemoveItem { path: path.clone(), arg: field.name.clone(), at: k });
            }
        }
        if let Some((from, to)) = moved {
            if from != to {
                cx.out.push(EditorCommand::MoveItem { path: path.clone(), arg: field.name.clone(), from, to });
            }
        }
        let id = format!("{}:new", field_id(path, &field.name, None));
        let buffer = view.buffer(&id, "");
        let response = ui.add(egui::TextEdit::singleline(buffer).desired_width(60.0).hint_text("+"));
        cx.ours.push(response.id);
        if response.lost_focus() && !buffer.is_empty() {
            cx.out.push(EditorCommand::InsertItem {
                path: path.clone(),
                arg: field.name.clone(),
                at: items.len(),
                text: std::mem::take(buffer),
            });
        }
    });
}

/// The widget of one value (AC-2): its response, and a command when the value was committed.
fn widget(
    ui: &mut egui::Ui,
    base: Id,
    path: &NodePath,
    field: &EditorField,
    text: &str,
    item: Option<usize>,
    view: &mut EditorView,
) -> (Option<egui::Response>, Option<EditorCommand>) {
    let commit = |value: String| -> EditorCommand {
        match item {
            Some(at) => EditorCommand::SetItem { path: path.clone(), arg: field.name.clone(), at, text: value },
            None => EditorCommand::SetField { path: path.clone(), arg: field.name.clone(), text: value },
        }
    };
    let id = field_id(path, &field.name, item);
    let shown = if text.is_empty() { field.default_text.clone().unwrap_or_default() } else { text.to_string() };
    match &field.widget {
        ArgumentGUIInfo::Checkbox => {
            let mut on = matches!(shown.as_str(), "true" | "t" | "yes" | "y" | "1");
            let r = ui.checkbox(&mut on, "");
            let c = r.changed().then(|| commit(if on { "t" } else { "f" }.to_string()));
            (Some(r), c)
        }
        ArgumentGUIInfo::RadioBoolean { true_label, false_label } => {
            let on = matches!(shown.as_str(), "true" | "t" | "yes" | "y" | "1");
            let mut chosen = on;
            let r = ui.radio_value(&mut chosen, true, true_label);
            ui.radio_value(&mut chosen, false, false_label);
            (Some(r), (chosen != on).then(|| commit(if chosen { "t" } else { "f" }.to_string())))
        }
        ArgumentGUIInfo::IntegerSlider { min, max, .. } | ArgumentGUIInfo::IntegerRange { min, max } => {
            let mut n: i64 = shown.parse().unwrap_or(*min);
            let r = ui.add(egui::Slider::new(&mut n, *min..=*max));
            let c = r.changed().then(|| commit(n.to_string()));
            (Some(r), c)
        }
        ArgumentGUIInfo::FloatSlider { min, max, .. } => {
            let mut x: f64 = shown.parse().unwrap_or(*min);
            let r = ui.add(egui::Slider::new(&mut x, *min..=*max));
            let c = r.changed().then(|| commit(x.to_string()));
            (Some(r), c)
        }
        ArgumentGUIInfo::EnumSelector | ArgumentGUIInfo::HorizontalRadioEnum | ArgumentGUIInfo::VerticalRadioEnum
            if !field.alternatives.is_empty() =>
        {
            let mut chosen = shown.clone();
            let r = match &field.widget {
                ArgumentGUIInfo::HorizontalRadioEnum => {
                    ui.horizontal(|ui| {
                        for a in &field.alternatives {
                            ui.radio_value(&mut chosen, a.clone(), a);
                        }
                    })
                    .response
                }
                ArgumentGUIInfo::VerticalRadioEnum => {
                    ui.vertical(|ui| {
                        for a in &field.alternatives {
                            ui.radio_value(&mut chosen, a.clone(), a);
                        }
                    })
                    .response
                }
                _ => {
                    egui::ComboBox::from_id_salt(("enum", &id))
                        .selected_text(chosen.clone())
                        .show_ui(ui, |ui| {
                            for a in &field.alternatives {
                                ui.selectable_value(&mut chosen, a.clone(), a);
                            }
                        })
                        .response
                }
            };
            (Some(r), (chosen != shown).then(|| commit(chosen)))
        }
        ArgumentGUIInfo::ColorString => {
            let mut c = parse_color(&shown).unwrap_or(Color32::WHITE);
            let r = ui.color_edit_button_srgba(&mut c);
            let cmd = r.changed().then(|| commit(color_text(c)));
            (Some(r), cmd)
        }
        ArgumentGUIInfo::Hide => (None, None),
        ArgumentGUIInfo::TextArea(w, h) | ArgumentGUIInfo::CodeArea(w, h, _) => {
            let buffer = view.buffer(&id, text);
            let r = ui.add(
                egui::TextEdit::multiline(buffer)
                    .desired_width(*w as f32 * 8.0)
                    .desired_rows(*h)
                    .code_editor(),
            );
            let c = (r.lost_focus() && *buffer != text).then(|| commit(buffer.clone()));
            (Some(r), c)
        }
        ArgumentGUIInfo::TextField(_)
        | ArgumentGUIInfo::CodeField(_, _)
        | ArgumentGUIInfo::IntegerField
        | ArgumentGUIInfo::FloatField
        | ArgumentGUIInfo::DateField(_)
        | ArgumentGUIInfo::EnumSelector
        | ArgumentGUIInfo::HorizontalRadioEnum
        | ArgumentGUIInfo::VerticalRadioEnum
        | ArgumentGUIInfo::NodeSelector
        | ArgumentGUIInfo::None => {
            let width = match &field.widget {
                ArgumentGUIInfo::TextField(w) | ArgumentGUIInfo::CodeField(w, _) | ArgumentGUIInfo::DateField(w) => {
                    (*w).clamp(12, 60) as f32 * 7.0
                }
                _ => 120.0,
            };
            let buffer = view.buffer(&id, text);
            let mut edit = egui::TextEdit::singleline(buffer).desired_width(width).id(base.with(("lq-field", &id)));
            if let Some(d) = &field.default_text {
                edit = edit.hint_text(d.as_str());
            }
            let inner = ui.horizontal(|ui| {
                let r = ui.add(edit);
                if item.is_none() {
                    if let Some(d) = &field.default_text {
                        ui.label(RichText::new(format!("default {d}")).small().weak());
                    }
                }
                r
            });
            let r = inner.inner;
            let c = (r.lost_focus() && *buffer != text).then(|| commit(buffer.clone()));
            (Some(r), c)
        }
    }
}

/// The keyboard model (AC-9, AC-10): active while focus is in the editor.
fn handle_keys(ui: &mut egui::Ui, cx: &mut Cx, view: &mut EditorView) {
    let Some(focus) = view.focus.clone() else {
        return;
    };
    let focused = ui.ctx().memory(|m| m.focused());
    if focused.is_some_and(|id| !cx.ours.contains(&id)) {
        return; // keyboard focus is elsewhere in the app
    }
    let in_text = focused.is_some_and(|id| id == cx.text_id);
    let scope_ok = |scope: KeyScope| match scope {
        KeyScope::Anywhere => true,
        KeyScope::Header => matches!(focus, FocusTarget::Node(_)) && !in_text,
        KeyScope::Field => matches!(focus, FocusTarget::Field { .. }),
    };
    // Most specific first: egui ignores extra Shift and Alt when matching.
    let mut map: Vec<_> = editor_keymap().iter().filter(|(_, _, scope)| scope_ok(*scope)).collect();
    map.sort_by_key(|(s, _, _)| std::cmp::Reverse(s.modifiers.alt as u8 + s.modifiers.shift as u8 + s.modifiers.ctrl as u8));
    let mut pressed = None;
    ui.input_mut(|i| {
        for (shortcut, key, _) in &map {
            if i.consume_shortcut(&shortcut.to_egui()) {
                pressed = Some(*key);
                break;
            }
        }
    });
    if let Some(key) = pressed {
        run_key(ui, cx, view, &focus, key);
    }
}

fn set_focus(view: &mut EditorView, target: FocusTarget) {
    view.focus = Some(target);
    view.focus_pending = true;
}

/// Focuses the card `pick(current index, card count)` chooses.
fn go_card(cx: &Cx, view: &mut EditorView, pick: impl Fn(Option<usize>, usize) -> usize) {
    let n = cx.cards.len();
    if n == 0 {
        return;
    }
    let at = view.focus.as_ref().and_then(focus_card).and_then(|c| cx.cards.iter().position(|p| p == c));
    let to = pick(at, n).min(n - 1);
    set_focus(view, FocusTarget::Node(cx.cards[to].clone()));
}

fn run_key(ui: &mut egui::Ui, cx: &mut Cx, view: &mut EditorView, focus: &FocusTarget, key: EditorKey) {
    let card = focus_card(focus).cloned().or_else(|| cx.cards.last().cloned());
    let at = card.as_ref().and_then(|c| cx.cards.iter().position(|p| p == c));
    let node = card.as_ref().and_then(|c| crate::ui::editor::find_node(&cx.tree.nodes, c));
    match key {
        EditorKey::ToggleMode => cx.mode_change = Some(cx.mode.toggled()),
        EditorKey::FocusQuery | EditorKey::Leave => set_focus(view, FocusTarget::Text),
        EditorKey::Jump => match focus {
            FocusTarget::Text => {
                let typed = view.text.as_ref().map(|(t, _)| t.clone()).unwrap_or_else(|| cx.tree.text.clone());
                if let Some(offset) = text_cursor(ui.ctx(), cx.text_id, &typed) {
                    if let Some(command) = view.jump_from_text(cx.tree, cx.mode, offset) {
                        cx.out.push(command);
                    }
                    if let Some(c) = view.focus.as_ref().and_then(focus_card).cloned() {
                        view.collapsed.remove(&c.id());
                    }
                }
            }
            FocusTarget::Node(_) | FocusTarget::Field { .. } => {
                if let Some(range) = view.jump_to_text(cx.tree) {
                    select_text(ui.ctx(), cx.text_id, &cx.tree.text, range);
                }
            }
        },
        EditorKey::NextRegion | EditorKey::PrevRegion => match focus {
            FocusTarget::Text => {
                if let Some(first) = cx.cards.first().cloned() {
                    set_focus(view, FocusTarget::Node(first));
                }
            }
            FocusTarget::Node(_) | FocusTarget::Field { .. } => set_focus(view, FocusTarget::Text),
        },
        EditorKey::FirstCard => go_card(cx, view, |_, _| 0),
        EditorKey::LastCard => go_card(cx, view, |_, n| n - 1),
        EditorKey::NextCard => go_card(cx, view, |at, n| at.map_or(0, |i| (i + 1).min(n - 1))),
        EditorKey::PrevCard => go_card(cx, view, |at, _| at.map_or(0, |i| i.saturating_sub(1))),
        EditorKey::Enter => {
            if let (Some(c), Some(node)) = (card.clone(), node) {
                view.collapsed.remove(&c.id());
                if let Some(field) = node.fields().first() {
                    set_focus(view, FocusTarget::Field { path: c, arg: field.name.clone(), item: None });
                }
            }
        }
        EditorKey::ToggleCard => {
            if let Some(c) = &card {
                toggle(view, c);
            }
        }
        EditorKey::Collapse => {
            if let Some(c) = &card {
                view.collapsed.insert(c.id());
            }
        }
        EditorKey::Expand => {
            if let Some(c) = &card {
                view.collapsed.remove(&c.id());
            }
        }
        EditorKey::CollapseAll => collapse_all(cx, view, true),
        EditorKey::ExpandAll => collapse_all(cx, view, false),
        EditorKey::Reset => {
            if let (Some(c), Some(node)) = (&card, node) {
                reset_all(&mut cx.out, c, node.fields());
            }
        }
        EditorKey::Presets => {
            if let Some(c) = card {
                view.open_menu = Some((c, CardMenu::Presets));
            }
        }
        EditorKey::Next => {
            if let Some(c) = card {
                view.open_menu = Some((c, CardMenu::Next));
            }
        }
        EditorKey::ApplyPreset(n) => {
            if let (Some(c), Some(EditorNode::Action { presets, .. })) = (card, node) {
                let index = usize::from(n.max(1)) - 1;
                if index < presets.len() {
                    cx.out.push(EditorCommand::ApplyPreset { path: c, index });
                }
            }
        }
        EditorKey::AddAction => {
            let insert_at = at.and_then(|i| cx.cards.get(i + 1)).cloned();
            cx.out.push(EditorCommand::SearchCommands { insert_at, filter: String::new() });
        }
        EditorKey::MoveUp | EditorKey::MoveDown => {
            if cx.mode == EditorMode::Advanced {
                if let Some(c) = &card {
                    move_card(&mut cx.out, c, if key == EditorKey::MoveUp { -1 } else { 1 });
                }
            }
        }
        EditorKey::DeleteCard => {
            if let Some(c) = card {
                if !c.0.is_empty() {
                    cx.out.push(EditorCommand::Remove { path: c });
                }
            }
        }
        EditorKey::ToggleLink => {
            if let (FocusTarget::Field { path, arg, .. }, Some(node)) = (focus, node) {
                let is_link = node.fields().iter().any(|f| &f.name == arg && matches!(f.value, FieldValue::Link { .. }));
                cx.out.push(if is_link {
                    EditorCommand::ToValue { path: path.clone(), arg: arg.clone() }
                } else {
                    EditorCommand::ToLink { path: path.clone(), arg: arg.clone() }
                });
            }
        }
        EditorKey::Complete => {
            if let FocusTarget::Field { path, arg, .. } = focus {
                view.open_menu = Some((path.clone(), CardMenu::Argument(arg.clone())));
            }
        }
        EditorKey::Update => {
            if !cx.tree.continuous {
                cx.out.push(EditorCommand::Update);
            }
        }
    }
}

fn parse_color(text: &str) -> Option<Color32> {
    let hex = text.trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    match hex.len() {
        6 => Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

fn color_text(c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    format!("{r:02X}{g:02X}{b:02X}{a:02X}")
}

fn filename(ui: &mut egui::Ui, tree: &EditorTree, view: &mut EditorView, out: &mut Vec<EditorCommand>) {
    ui.horizontal(|ui| {
        ui.label("filename");
        let buffer = view.buffer("filename", &tree.filename);
        let response = ui.add(egui::TextEdit::singleline(buffer).desired_width(160.0));
        if response.lost_focus() && *buffer != tree.filename {
            out.push(EditorCommand::SetFilename(buffer.clone()));
        }
        if let Some(default) = &tree.filename_default {
            if ui.button("Default").on_hover_text(default).clicked() {
                out.push(EditorCommand::DefaultFilename);
            }
        }
    });
}

fn picker(ui: &mut egui::Ui, tree: &EditorTree, view: &mut EditorView, out: &mut Vec<EditorCommand>) {
    let Some(picker) = &tree.picker else {
        return;
    };
    let mut open = true;
    egui::Window::new("Add a command")
        .id(egui::Id::new(("picker", ui.id())))
        .open(&mut open)
        .show(ui.ctx(), |ui| {
            let buffer = view.buffer("picker", &picker.filter);
            if ui.text_edit_singleline(buffer).changed() {
                out.push(EditorCommand::SearchCommands { insert_at: picker.insert_at.clone(), filter: buffer.clone() });
            }
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for ns in &picker.catalog {
                    let name = if ns.namespace.is_empty() { "(root)" } else { ns.namespace.as_str() };
                    egui::CollapsingHeader::new(name)
                        .default_open(!picker.filter.is_empty())
                        .show(ui, |ui| {
                            for c in &ns.commands {
                                if ui.button(&c.key.name).on_hover_text(format!("{}\n{}", c.label, c.doc)).clicked() {
                                    out.push(EditorCommand::PickCommand(c.key.clone()));
                                }
                            }
                        });
                }
            });
        });
    if !open {
        out.push(EditorCommand::ClosePicker);
    }
}

fn browser(ui: &mut egui::Ui, tree: &EditorTree, out: &mut Vec<EditorCommand>) {
    let Some(browser) = &tree.browser else {
        return;
    };
    let mut open = true;
    egui::Window::new("Choose a key")
        .id(egui::Id::new(("browser", ui.id())))
        .open(&mut open)
        .show(ui.ctx(), |ui| match &browser.listing {
            None => {
                ui.label("Listing…");
            }
            Some(listing) => {
                ui.label(RichText::new(format!("/{}", listing.folder)).monospace());
                if let Some(error) = &listing.error {
                    ui.colored_label(Color32::from_rgb(220, 80, 80), error);
                }
                if !listing.folder.is_empty() && ui.button("..").clicked() {
                    let parent = listing.folder.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default();
                    out.push(EditorCommand::Browse { path: browser.path.clone(), folder: parent });
                }
                for entry in &listing.entries {
                    let key = if listing.folder.is_empty() { entry.name.clone() } else { format!("{}/{}", listing.folder, entry.name) };
                    if entry.is_dir {
                        if ui.button(format!("📁 {}", entry.name)).clicked() {
                            out.push(EditorCommand::Browse { path: browser.path.clone(), folder: key });
                        }
                    } else if ui.button(&entry.name).clicked() {
                        out.push(EditorCommand::SetKey { path: browser.path.clone(), key });
                    }
                }
            }
        });
    if !open {
        out.push(EditorCommand::CloseBrowser);
    }
}

fn confirmation(ui: &mut egui::Ui, tree: &EditorTree, out: &mut Vec<EditorCommand>) {
    let Some(c) = &tree.confirmation else {
        return;
    };
    egui::Window::new("Change the namespaces?")
        .id(egui::Id::new(("confirm", ui.id())))
        .collapsible(false)
        .show(ui.ctx(), |ui| {
            ui.label("This changes which command these actions run:");
            for line in &c.changes {
                ui.label(line);
            }
            ui.horizontal(|ui| {
                if ui.button("Confirm").clicked() {
                    out.push(EditorCommand::ConfirmNamespaces);
                }
                if ui.button("Cancel").clicked() {
                    out.push(EditorCommand::CancelNamespaces);
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use liquers_core::command_metadata::{
        ArgumentInfo, CommandMetadata, CommandMetadataRegistry, CommandPreset,
    };
    use liquers_core::value::ValueInterface;

    use crate::ui::editor::{EditorKind, EditorState};
    use crate::value::Value;

    const P: &str = "-R/data/sales.csv/-/ns-pl/gt-amount-1000/sort-amount-f/head-20";

    fn registry() -> CommandMetadataRegistry {
        let mut cmr = CommandMetadataRegistry::new();
        let mut is_in = CommandMetadata::new("is_in");
        is_in
            .with_namespace("pl")
            .with_argument(ArgumentInfo::string_argument("column"))
            .with_argument(ArgumentInfo::string_argument("values").set_multiple());
        cmr.add_command(&is_in);
        let mut gt = CommandMetadata::new("gt");
        gt.with_namespace("pl")
            .with_label("Greater than")
            .with_argument(ArgumentInfo::string_argument("column").with_description("The column to compare"))
            .with_argument(ArgumentInfo::string_argument("value"));
        cmr.add_command(&gt);
        let mut sort = CommandMetadata::new("sort");
        sort.with_namespace("pl")
            .with_label("Sort by column")
            .with_argument(ArgumentInfo::string_argument("column"))
            .with_argument(ArgumentInfo::boolean_argument("ascending").with_default(true));
        cmr.add_command(&sort);
        let mut head = CommandMetadata::new("head");
        head.with_namespace("pl").with_label("Get first rows").with_doc("Return first N rows");
        head.with_argument(ArgumentInfo::integer_argument("n", false).with_default(5).with_description("Number of rows"));
        for (q, label) in [("head-5", "Top 5"), ("head-20", "Top 20"), ("head-100", "Top 100"), ("head-1000", "Top 1000")] {
            head.presets.push(CommandPreset::new(q, label, "").expect("preset"));
        }
        head.next.push(CommandPreset::new("sort", "Sort", "").expect("next"));
        cmr.add_command(&head);
        cmr
    }

    fn tree_of(query: &str) -> EditorTree {
        let mut state = EditorState::new(EditorKind::Query, Arc::new(registry()));
        state.on_snapshot(&Value::from_query(&liquers_core::parse::parse_query(query).expect("query")), None).expect("snapshot");
        state.tree().clone()
    }

    fn frame(
        ctx: &egui::Context,
        tree: &EditorTree,
        view: &mut EditorView,
        mode: EditorMode,
        events: Vec<egui::Event>,
    ) -> EditorOutput {
        let theme = ThemeState::default();
        let input = egui::RawInput { events, ..Default::default() };
        let mut result = EditorOutput::default();
        let mut output = ctx.run_ui(input, |ui| {
            result = show_editor(ui, tree, view, mode, Density::Comfortable, &theme, None);
        });
        output.textures_delta.clear();
        result
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    fn head_path(tree: &EditorTree) -> NodePath {
        tree.nodes[tree.last_action().expect("head")].path().clone()
    }

    /// query-pipeline-editor AC-11, AC-3: every node kind draws headless in both modes and
    /// produces no command without input.
    #[test]
    fn show_editor_draws_every_node_kind() {
        let tree = tree_of("-R/data/sales.csv/-/ns-pl/is_in-region-EU-US/v/head-~X~-R/config/limit.txt~E/unknown-1");
        assert!(tree.nodes.len() >= 5, "{:?}", tree.nodes);
        let mut view = EditorView::default();
        let ctx = egui::Context::default();
        for mode in [EditorMode::Simple, EditorMode::Advanced, EditorMode::Advanced] {
            let out = frame(&ctx, &tree, &mut view, mode, vec![]);
            assert!(out.commands.is_empty(), "{:?}", out.commands);
            assert!(out.mode.is_none());
        }
    }

    // Test 8 (AC-4, AC-5).
    #[test]
    fn expanded_card_offers_the_toolbar() {
        let tree = tree_of(P);
        let head = &tree.nodes[tree.last_action().expect("head")];
        assert_eq!(
            toolbar_items(head, EditorMode::Simple),
            [ToolItem::Reset, ToolItem::Presets, ToolItem::Next, ToolItem::Delete]
        );
        assert_eq!(
            toolbar_items(head, EditorMode::Advanced),
            [ToolItem::Reset, ToolItem::Presets, ToolItem::Next, ToolItem::MoveUp, ToolItem::MoveDown, ToolItem::Delete]
        );
        let gt = tree.nodes.iter().find(|n| crate::ui::editor::node_label(n) == "Greater than").expect("gt");
        let collapsed = header_parts(&tree, gt, true, 2);
        assert_eq!(collapsed[..4], [
            HeaderPart::Chevron,
            HeaderPart::Icon,
            HeaderPart::Label("Greater than".into()),
            HeaderPart::CommandId("pl/gt".into()),
        ]);
        let expanded = header_parts(&tree, gt, false, 2);
        assert_eq!(expanded[2..4], [HeaderPart::CommandId("pl/gt".into()), HeaderPart::Label("Greater than".into())]);
        assert!(expanded.contains(&HeaderPart::Position(2)) && expanded.ends_with(&[HeaderPart::Menu]));

        // Drawn: Reset clicked through the keyboard resets the modified argument.
        let ctx = egui::Context::default();
        let mut view = EditorView { focus: Some(FocusTarget::Node(head_path(&tree))), ..EditorView::default() };
        frame(&ctx, &tree, &mut view, EditorMode::Simple, vec![]);
        let out = frame(&ctx, &tree, &mut view, EditorMode::Simple, vec![key(egui::Key::R, egui::Modifiers::ALT)]);
        assert_eq!(out.commands, [EditorCommand::Reset { path: head_path(&tree), arg: "n".into() }]);
    }

    // Test 9 (AC-6).
    #[test]
    fn form_grid_aligns_labels_and_shows_refusal() {
        let mut state = EditorState::new(EditorKind::Query, Arc::new(registry()));
        state.on_snapshot(&Value::from_query(&liquers_core::parse::parse_query(P).expect("q")), None).expect("snapshot");
        let gt = state.tree().nodes.iter().find(|n| crate::ui::editor::node_label(n) == "Greater than").expect("gt").clone();
        let rows = form_rows(state.tree(), &gt, None);
        let labels: Vec<_> = rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["column", "value"]);
        assert!(rows.iter().all(|r| r.required));
        assert_eq!(rows[0].help, "The column to compare");

        let head = head_path(state.tree());
        let last = field_id(&head, "n", None);
        assert!(state.apply(EditorCommand::SetField { path: head.clone(), arg: "n".into(), text: "abc".into() }).is_err());
        let node = crate::ui::editor::find_node(&state.tree().nodes, &head).expect("head").clone();
        let rows = form_rows(state.tree(), &node, Some(&last));
        assert!(rows[0].message.is_some(), "the message is under the field");
        assert!(!rows[0].required, "n has a default");

        let ctx = egui::Context::default();
        let mut view = EditorView::default();
        let tree = state.tree().clone();
        for mode in [EditorMode::Simple, EditorMode::Advanced] {
            assert!(frame(&ctx, &tree, &mut view, mode, vec![]).commands.is_empty());
        }
    }

    // Test 13 (AC-3, AC-7, AC-9).
    #[test]
    fn shortcuts_move_between_cards_and_toggle_mode() {
        let tree = tree_of(P);
        let ctx = egui::Context::default();
        let head = head_path(&tree);
        let mut view = EditorView { focus: Some(FocusTarget::Node(head.clone())), ..EditorView::default() };
        frame(&ctx, &tree, &mut view, EditorMode::Advanced, vec![]);

        let out = frame(&ctx, &tree, &mut view, EditorMode::Advanced, vec![key(egui::Key::B, egui::Modifiers::COMMAND)]);
        assert_eq!(out.mode, Some(EditorMode::Simple));

        frame(&ctx, &tree, &mut view, EditorMode::Advanced, vec![key(egui::Key::PageUp, egui::Modifiers::NONE)]);
        let FocusTarget::Node(prev) = view.focus.clone().expect("focus") else { panic!("{:?}", view.focus) };
        assert_ne!(prev, head, "PageUp moves to the previous card");
        frame(&ctx, &tree, &mut view, EditorMode::Advanced, vec![key(egui::Key::PageDown, egui::Modifiers::NONE)]);
        assert_eq!(view.focus, Some(FocusTarget::Node(head.clone())));

        frame(&ctx, &tree, &mut view, EditorMode::Advanced, vec![key(egui::Key::F6, egui::Modifiers::NONE)]);
        assert_eq!(view.focus, Some(FocusTarget::Text));
        view.focus = Some(FocusTarget::Node(head.clone()));
        frame(&ctx, &tree, &mut view, EditorMode::Advanced, vec![]);

        let collapse_all = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        frame(&ctx, &tree, &mut view, EditorMode::Advanced, vec![key(egui::Key::OpenBracket, collapse_all)]);
        assert!(view.collapsed.contains(&head.id()) && view.collapsed.len() >= 4, "{:?}", view.collapsed);
        frame(&ctx, &tree, &mut view, EditorMode::Advanced, vec![key(egui::Key::CloseBracket, collapse_all)]);
        assert!(view.collapsed.is_empty());

        let out = frame(&ctx, &tree, &mut view, EditorMode::Advanced, vec![key(egui::Key::K, egui::Modifiers::COMMAND)]);
        assert_eq!(out.commands, [EditorCommand::SearchCommands { insert_at: None, filter: String::new() }]);

        let out = frame(&ctx, &tree, &mut view, EditorMode::Advanced, vec![key(egui::Key::Num1, egui::Modifiers::ALT)]);
        assert_eq!(out.commands, [EditorCommand::ApplyPreset { path: head.clone(), index: 0 }]);

        // Focus elsewhere in the app: the editor's keys are not taken.
        let other = egui::Id::new("elsewhere");
        ctx.memory_mut(|m| m.request_focus(other));
        let out = frame(&ctx, &tree, &mut view, EditorMode::Advanced, vec![key(egui::Key::B, egui::Modifiers::COMMAND)]);
        assert!(out.mode.is_none());
    }

    // Test 13a (AC-10).
    #[test]
    fn ctrl_j_jumps_both_ways() {
        // A query is ASCII, but a typed buffer need not be: the cursor's char index is converted.
        assert_eq!(byte_of_char("ä-b", 1), 2);
        assert_eq!(char_of_byte("ä-b", 2), 1);
        let query = P;
        let tree = tree_of(query);
        assert!(!tree.spans.is_empty());
        let ctx = egui::Context::default();
        let mut view = EditorView { focus: Some(FocusTarget::Text), focus_pending: true, ..EditorView::default() };
        frame(&ctx, &tree, &mut view, EditorMode::Simple, vec![]);
        // The query text took the pending focus.
        let byte = query.rfind("20").expect("20") + 1;
        let chars = query[..byte].chars().count();
        let mut found = None;
        ctx.memory(|m| {
            if let Some(id) = m.focused() {
                found = Some(id);
            }
        });
        let id = found.expect("the query text has focus");
        let mut state = egui::text_edit::TextEditState::load(&ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(chars))));
        state.store(&ctx, id);

        frame(&ctx, &tree, &mut view, EditorMode::Simple, vec![key(egui::Key::J, egui::Modifiers::COMMAND)]);
        let head = head_path(&tree);
        assert_eq!(view.focus, Some(FocusTarget::Field { path: head.clone(), arg: "n".into(), item: None }));
        frame(&ctx, &tree, &mut view, EditorMode::Simple, vec![]);
        assert_ne!(ctx.memory(|m| m.focused()), Some(id), "the field took focus");

        frame(&ctx, &tree, &mut view, EditorMode::Simple, vec![key(egui::Key::J, egui::Modifiers::COMMAND)]);
        assert_eq!(view.focus, Some(FocusTarget::Text));
        let state = egui::text_edit::TextEditState::load(&ctx, id).expect("state");
        let range = state.cursor.char_range().expect("selection");
        let (a, b) = (range.primary.index.0.min(range.secondary.index.0), range.primary.index.0.max(range.secondary.index.0));
        let selected: String = query.chars().skip(a).take(b - a).collect();
        assert_eq!(selected, "20");
    }

    // Codex review of PR #278: two editors showing the same argument do not share its widget.
    #[test]
    fn two_editors_do_not_share_field_state() {
        let tree = tree_of(P);
        let head = head_path(&tree);
        let ctx = egui::Context::default();
        let theme = ThemeState::default();
        let field = FocusTarget::Field { path: head.clone(), arg: "n".into(), item: None };
        let mut a = EditorView { focus: Some(field), focus_pending: true, ..EditorView::default() };
        let mut b = EditorView::default();
        let both = |events: Vec<egui::Event>, a: &mut EditorView, b: &mut EditorView| {
            let input = egui::RawInput { events, ..Default::default() };
            let mut outs = (EditorOutput::default(), EditorOutput::default());
            let mut output = ctx.run_ui(input, |ui| {
                ui.push_id("a", |ui| outs.0 = show_editor(ui, &tree, a, EditorMode::Simple, Density::Comfortable, &theme, None));
                ui.push_id("b", |ui| outs.1 = show_editor(ui, &tree, b, EditorMode::Simple, Density::Comfortable, &theme, None));
            });
            output.textures_delta.clear();
            outs
        };
        both(vec![], &mut a, &mut b);
        both(vec![], &mut a, &mut b);
        let (out_a, out_b) = both(
            vec![egui::Event::Text("7".into()), key(egui::Key::Enter, egui::Modifiers::NONE)],
            &mut a,
            &mut b,
        );
        assert!(
            out_a.commands.iter().any(|c| matches!(c, EditorCommand::SetField { arg, .. } if arg == "n")),
            "{:?}",
            out_a.commands
        );
        assert!(out_b.commands.is_empty(), "the other editor's field took the typing: {:?}", out_b.commands);
    }
}
