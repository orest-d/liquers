//! egui rendering of the query, action, resource and key editors (`crate::ui::editor`).
//!
//! [`show_editor`] draws an [`EditorTree`] and returns the [`EditorCommand`]s the user's input
//! produced this frame; the element applies them to its core, so egui and the browser share every
//! rule. Typing stays in the [`EditorView`] buffers until a field is committed (Enter or focus
//! loss); sliders, checkboxes and selectors commit on change.
//!
//! # Main components
//!
//! - [`show_editor`]: the toolbar ("Update immediately", Update, Revert), the highlighted query
//!   text, the collapsible nodes, the filename field, and the picker, file selector and namespace
//!   confirmation windows.
//! - Variadic items and `ns` lists reorder by drag and drop (`Ui::dnd_drag_source`).
//!
//! # Where it is used
//!
//! - `EditorElement::show_in_egui` (`crate::ui::editor`).
//!
//! # Related concepts
//!
//! - [`crate::egui::widgets::edit_query`]: the highlighted query text field it reuses.
//! - `crate::ui::web::editor`: the browser rendering of the same tree.
//!
//! # Examples
//!
//! ```
//! use liquers_lib::egui::editor::show_editor;
//! use liquers_lib::ui::editor::{EditorTree, EditorView};
//!
//! let tree = EditorTree { text: "-R/data.csv".into(), text_editable: true, ..Default::default() };
//! let mut view = EditorView::default();
//! let ctx = egui::Context::default();
//! let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
//!     let commands = show_editor(ui, &tree, &mut view);
//!     assert!(commands.is_empty());
//! });
//! output.textures_delta.clear();
//! ```

use egui::{Color32, RichText};
use liquers_core::command_metadata::ArgumentGUIInfo;

use crate::ui::editor::{
    field_id, EditorCommand, EditorField, EditorNode, EditorTree, EditorView, FieldValue,
    NewElement, NodePath, RESOURCE_HEADERS,
};

/// Draws `tree` and returns the commands of this frame.
pub fn show_editor(ui: &mut egui::Ui, tree: &EditorTree, view: &mut EditorView) -> Vec<EditorCommand> {
    let mut out = Vec::new();
    toolbar(ui, tree, &mut out);
    query_text(ui, tree, view, &mut out);
    if let Some((_, message)) = &tree.message {
        ui.colored_label(Color32::from_rgb(220, 120, 60), message);
    }
    ui.add_enabled_ui(!tree.read_only, |ui| {
        nodes(ui, &tree.nodes, view, &mut out);
        if tree.has_filename {
            ui.horizontal(|ui| {
                if ui.button("+ action").on_hover_text("Append a command").clicked() {
                    out.push(EditorCommand::SearchCommands { insert_at: None, filter: String::new() });
                }
                ui.menu_button("+ directive", |ui| {
                    if ui.button("q (quote)").clicked() {
                        out.push(EditorCommand::Append(NewElement::Quote));
                    }
                    if ui.button("v (volatile)").clicked() {
                        out.push(EditorCommand::Append(NewElement::Volatile));
                    }
                });
            });
            filename(ui, tree, view, &mut out);
        }
    });
    picker(ui, tree, view, &mut out);
    browser(ui, tree, &mut out);
    confirmation(ui, tree, &mut out);
    out
}

fn toolbar(ui: &mut egui::Ui, tree: &EditorTree, out: &mut Vec<EditorCommand>) {
    ui.horizontal(|ui| {
        let mut continuous = tree.continuous;
        if ui
            .checkbox(&mut continuous, "Update immediately")
            .on_hover_text("Set each edit into where the value lives at once")
            .changed()
        {
            out.push(EditorCommand::SetContinuous(continuous));
        }
        if !tree.continuous {
            if ui.add_enabled(tree.pending, egui::Button::new("Update")).clicked() {
                out.push(EditorCommand::Update);
            }
            if ui.button("Revert").clicked() {
                out.push(EditorCommand::Revert);
            }
        }
        if tree.pending {
            ui.label(RichText::new("pending").italics());
        }
    });
}

fn query_text(ui: &mut egui::Ui, tree: &EditorTree, view: &mut EditorView, out: &mut Vec<EditorCommand>) {
    if tree.text_editable {
        let buffer = view.text_buffer(&tree.text);
        let response = crate::egui::widgets::edit_query(ui, buffer);
        let committed = response.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter));
        if committed && *buffer != tree.text {
            out.push(EditorCommand::SetText(buffer.clone()));
        }
    } else if !tree.text.is_empty() {
        ui.label(crate::egui::widgets::query_to_layout_job(tree.text.clone()));
    }
    if let Some(note) = &tree.text_note {
        ui.colored_label(Color32::from_rgb(220, 80, 80), note);
    }
    if tree.stale {
        ui.label(RichText::new("The tree shows the last valid query.").italics());
    }
}

fn nodes(ui: &mut egui::Ui, nodes: &[EditorNode], view: &mut EditorView, out: &mut Vec<EditorCommand>) {
    for node in nodes {
        node_ui(ui, node, view, out);
    }
}

fn node_menu(ui: &mut egui::Ui, path: &NodePath, out: &mut Vec<EditorCommand>) {
    ui.menu_button("⋯", |ui| {
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

fn node_ui(ui: &mut egui::Ui, node: &EditorNode, view: &mut EditorView, out: &mut Vec<EditorCommand>) {
    match node {
        EditorNode::Resource { path, header, key, note } => {
            ui.horizontal(|ui| {
                ui.label("resource");
                if let Some(header) = header {
                    let mut chosen = header.clone();
                    egui::ComboBox::from_id_salt(("header", path.id()))
                        .selected_text(chosen.clone())
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
                let response = ui.add(egui::TextEdit::singleline(buffer).desired_width(220.0));
                if response.lost_focus() && *buffer != *key {
                    out.push(EditorCommand::SetKey { path: path.clone(), key: buffer.clone() });
                }
                if ui.button("📁").on_hover_text("Browse the store").clicked() {
                    let folder = key.rsplit_once('/').map(|(f, _)| f.to_string()).unwrap_or_default();
                    out.push(EditorCommand::Browse { path: path.clone(), folder });
                }
                if !path.0.is_empty() {
                    node_menu(ui, path, out);
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
                    let id = egui::Id::new(("ns", path.id(), i));
                    let response = ui
                        .dnd_drag_source(id, i, |ui| {
                            ui.label(RichText::new(ns).monospace());
                        })
                        .response;
                    if let Some(from) = response.dnd_release_payload::<usize>() {
                        moved = Some((*from, i));
                    }
                    if ui.small_button("✕").on_hover_text("Remove this namespace").clicked() {
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
        EditorNode::Action { path, command, label, doc, presets, next, filename, fields, excess, .. } => {
            let header = egui::CollapsingHeader::new(RichText::new(&command.name).strong())
                .id_salt(("action", path.id()))
                .default_open(true);
            let response = header.show(ui, |ui| {
                egui::Grid::new(("fields", path.id())).num_columns(3).show(ui, |ui| {
                    for field in fields {
                        field_row(ui, path, field, view, out);
                        ui.end_row();
                    }
                });
                if !excess.is_empty() {
                    ui.label(RichText::new(format!("Extra parameters (not used): {}", excess.join(", "))).italics());
                }
            });
            response.header_response.on_hover_text(format!("{label}\n{doc}"));
            ui.horizontal(|ui| {
                if !presets.is_empty() {
                    ui.menu_button("presets", |ui| {
                        for (i, p) in presets.iter().enumerate() {
                            if ui.button(&p.label).on_hover_text(&p.description).clicked() {
                                out.push(EditorCommand::ApplyPreset { path: path.clone(), index: i });
                            }
                        }
                    });
                }
                if !next.is_empty() {
                    ui.menu_button("next", |ui| {
                        for (i, p) in next.iter().enumerate() {
                            if ui.button(&p.label).on_hover_text(&p.description).clicked() {
                                out.push(EditorCommand::ApplyNext { path: path.clone(), index: i });
                            }
                        }
                    });
                }
                if let Some(name) = filename {
                    if ui.button("filename").on_hover_text(name).clicked() {
                        out.push(EditorCommand::FilenameFrom { path: path.clone() });
                    }
                }
                node_menu(ui, path, out);
            });
        }
        EditorNode::Directive { path, name } => {
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).monospace())
                    .on_hover_text(if name == "q" { "Quote: pass the query so far as a value" } else { "Volatile: never cache" });
                node_menu(ui, path, out);
            });
        }
        EditorNode::Value { json, .. } => {
            ui.label(RichText::new(json).monospace());
        }
        EditorNode::Nested { path, label, nodes: inner } => {
            egui::CollapsingHeader::new(label)
                .id_salt(("nested", path.id()))
                .show(ui, |ui| {
                    ui.add_enabled_ui(false, |ui| nodes(ui, inner, view, out));
                });
        }
        EditorNode::Opaque { path, text, reason } => {
            ui.horizontal(|ui| {
                ui.label(RichText::new(text).monospace()).on_hover_text(reason);
                ui.label(RichText::new(reason).italics());
                if !path.0.is_empty() {
                    node_menu(ui, path, out);
                }
            });
        }
    }
}

fn field_row(ui: &mut egui::Ui, path: &NodePath, field: &EditorField, view: &mut EditorView, out: &mut Vec<EditorCommand>) {
    ui.label(&field.label).on_hover_text(field.hover());
    ui.vertical(|ui| match &field.value {
        FieldValue::Link { query, nodes: inner, is_key } => {
            let id = field_id(path, &field.name, None);
            let buffer = view.buffer(&id, query);
            let response = ui.add(egui::TextEdit::singleline(buffer).desired_width(260.0));
            if response.lost_focus() && *buffer != *query {
                out.push(EditorCommand::SetLink { path: path.clone(), arg: field.name.clone(), query: buffer.clone() });
            }
            egui::CollapsingHeader::new(if *is_key { "key link" } else { "link" })
                .id_salt(("link", id))
                .default_open(true)
                .show(ui, |ui| nodes(ui, inner, view, out));
        }
        FieldValue::Items(items) => items_ui(ui, path, field, items, view, out),
        FieldValue::Other(text) => {
            ui.label(RichText::new(text).italics());
        }
        FieldValue::Unset | FieldValue::Text(_) => {
            let text = match &field.value {
                FieldValue::Text(t) => t.clone(),
                FieldValue::Unset | FieldValue::Link { .. } | FieldValue::Items(_) | FieldValue::Other(_) => String::new(),
            };
            if field.multiple {
                items_ui(ui, path, field, &[], view, out);
            } else if let Some(command) = widget(ui, path, field, &text, None, view) {
                out.push(command);
            }
        }
    });
    ui.horizontal(|ui| {
        if !field.presets.is_empty() {
            ui.menu_button("▾", |ui| {
                for (i, p) in field.presets.iter().enumerate() {
                    if ui.button(&p.label).on_hover_text(&p.description).clicked() {
                        out.push(EditorCommand::ApplyArgumentPreset { path: path.clone(), arg: field.name.clone(), index: i });
                    }
                }
            });
        }
        let is_link = matches!(field.value, FieldValue::Link { .. });
        if ui.small_button(if is_link { "value" } else { "link" }).on_hover_text("Switch between a value and a link").clicked() {
            out.push(if is_link {
                EditorCommand::ToValue { path: path.clone(), arg: field.name.clone() }
            } else {
                EditorCommand::ToLink { path: path.clone(), arg: field.name.clone() }
            });
        }
        if ui.small_button("↺").on_hover_text("Reset to the default").clicked() {
            out.push(EditorCommand::Reset { path: path.clone(), arg: field.name.clone() });
        }
    });
}

fn items_ui(ui: &mut egui::Ui, path: &NodePath, field: &EditorField, items: &[FieldValue], view: &mut EditorView, out: &mut Vec<EditorCommand>) {
    ui.horizontal_wrapped(|ui| {
        let mut moved: Option<(usize, usize)> = None;
        for (k, item) in items.iter().enumerate() {
            let id = egui::Id::new(("item", path.id(), &field.name, k));
            let response = ui
                .dnd_drag_source(id, k, |ui| match item {
                    FieldValue::Text(text) => {
                        if let Some(command) = widget(ui, path, field, text, Some(k), view) {
                            out.push(command);
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
            if ui.small_button("✕").clicked() {
                out.push(EditorCommand::RemoveItem { path: path.clone(), arg: field.name.clone(), at: k });
            }
        }
        if let Some((from, to)) = moved {
            if from != to {
                out.push(EditorCommand::MoveItem { path: path.clone(), arg: field.name.clone(), from, to });
            }
        }
        let id = format!("{}:new", field_id(path, &field.name, None));
        let buffer = view.buffer(&id, "");
        let response = ui.add(egui::TextEdit::singleline(buffer).desired_width(60.0).hint_text("+"));
        if response.lost_focus() && !buffer.is_empty() {
            out.push(EditorCommand::InsertItem {
                path: path.clone(),
                arg: field.name.clone(),
                at: items.len(),
                text: std::mem::take(buffer),
            });
        }
    });
}

/// The widget of one value (AC-2): returns a command when the value was committed.
fn widget(
    ui: &mut egui::Ui,
    path: &NodePath,
    field: &EditorField,
    text: &str,
    item: Option<usize>,
    view: &mut EditorView,
) -> Option<EditorCommand> {
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
            ui.checkbox(&mut on, "").changed().then(|| commit(if on { "t" } else { "f" }.to_string()))
        }
        ArgumentGUIInfo::RadioBoolean { true_label, false_label } => {
            let on = matches!(shown.as_str(), "true" | "t" | "yes" | "y" | "1");
            let mut chosen = on;
            ui.radio_value(&mut chosen, true, true_label);
            ui.radio_value(&mut chosen, false, false_label);
            (chosen != on).then(|| commit(if chosen { "t" } else { "f" }.to_string()))
        }
        ArgumentGUIInfo::IntegerSlider { min, max, .. } | ArgumentGUIInfo::IntegerRange { min, max } => {
            let mut n: i64 = shown.parse().unwrap_or(*min);
            ui.add(egui::Slider::new(&mut n, *min..=*max))
                .changed()
                .then(|| commit(n.to_string()))
        }
        ArgumentGUIInfo::FloatSlider { min, max, .. } => {
            let mut x: f64 = shown.parse().unwrap_or(*min);
            ui.add(egui::Slider::new(&mut x, *min..=*max))
                .changed()
                .then(|| commit(x.to_string()))
        }
        ArgumentGUIInfo::EnumSelector | ArgumentGUIInfo::HorizontalRadioEnum | ArgumentGUIInfo::VerticalRadioEnum
            if !field.alternatives.is_empty() =>
        {
            let mut chosen = shown.clone();
            match &field.widget {
                ArgumentGUIInfo::HorizontalRadioEnum => {
                    ui.horizontal(|ui| {
                        for a in &field.alternatives {
                            ui.radio_value(&mut chosen, a.clone(), a);
                        }
                    });
                }
                ArgumentGUIInfo::VerticalRadioEnum => {
                    for a in &field.alternatives {
                        ui.radio_value(&mut chosen, a.clone(), a);
                    }
                }
                _ => {
                    egui::ComboBox::from_id_salt(("enum", &id)).selected_text(chosen.clone()).show_ui(ui, |ui| {
                        for a in &field.alternatives {
                            ui.selectable_value(&mut chosen, a.clone(), a);
                        }
                    });
                }
            }
            (chosen != shown).then(|| commit(chosen))
        }
        ArgumentGUIInfo::ColorString => {
            let mut color = parse_color(&shown).unwrap_or(Color32::WHITE);
            let response = ui.color_edit_button_srgba(&mut color);
            response.changed().then(|| commit(color_text(color)))
        }
        ArgumentGUIInfo::Hide => None,
        ArgumentGUIInfo::TextArea(w, h) | ArgumentGUIInfo::CodeArea(w, h, _) => {
            let buffer = view.buffer(&id, text);
            let response = ui.add(
                egui::TextEdit::multiline(buffer)
                    .desired_width(*w as f32 * 8.0)
                    .desired_rows(*h)
                    .code_editor(),
            );
            (response.lost_focus() && *buffer != text).then(|| commit(buffer.clone()))
        }
        ArgumentGUIInfo::TextField(_)
        | ArgumentGUIInfo::CodeField(_, _)
        | ArgumentGUIInfo::IntegerField
        | ArgumentGUIInfo::FloatField
        | ArgumentGUIInfo::DateField(_)
        | ArgumentGUIInfo::EnumSelector
        | ArgumentGUIInfo::HorizontalRadioEnum
        | ArgumentGUIInfo::VerticalRadioEnum
        | ArgumentGUIInfo::None => {
            let width = match &field.widget {
                ArgumentGUIInfo::TextField(w) | ArgumentGUIInfo::CodeField(w, _) | ArgumentGUIInfo::DateField(w) => {
                    (*w).clamp(4, 60) as f32 * 7.0
                }
                _ => 90.0,
            };
            let buffer = view.buffer(&id, text);
            let mut edit = egui::TextEdit::singleline(buffer).desired_width(width);
            if let Some(d) = &field.default_text {
                edit = edit.hint_text(d.as_str());
            }
            let response = ui.add(edit);
            (response.lost_focus() && *buffer != text).then(|| commit(buffer.clone()))
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

    use liquers_core::command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry};
    use liquers_core::value::ValueInterface;

    use crate::ui::editor::{EditorKind, EditorState};
    use crate::value::Value;

    /// query-pipeline-editor AC-11, AC-3: every node kind draws headless and produces no command
    /// without input.
    #[test]
    fn show_editor_draws_every_node_kind() {
        let mut cmr = CommandMetadataRegistry::new();
        let mut is_in = CommandMetadata::new("is_in");
        is_in
            .with_namespace("pl")
            .with_argument(ArgumentInfo::string_argument("column"))
            .with_argument(ArgumentInfo::string_argument("values").set_multiple());
        cmr.add_command(&is_in);
        let mut head = CommandMetadata::new("head");
        head.with_namespace("pl").with_argument(ArgumentInfo::integer_argument("n", false).with_default(5));
        head.with_doc("Return first N rows");
        cmr.add_command(&head);
        let mut state = EditorState::new(EditorKind::Query, Arc::new(cmr));
        let query = liquers_core::parse::parse_query(
            "-R/data/sales.csv/-/ns-pl/is_in-region-EU-US/v/head-~X~-R/config/limit.txt~E/unknown-1",
        )
        .expect("query");
        state.on_snapshot(&Value::from_query(&query), None).expect("snapshot");
        let tree = state.tree().clone();
        assert!(tree.nodes.len() >= 5, "{:?}", tree.nodes);
        let mut view = EditorView::default();
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let commands = show_editor(ui, &tree, &mut view);
                assert!(commands.is_empty(), "{commands:?}");
            });
            output.textures_delta.clear();
        }
    }
}
