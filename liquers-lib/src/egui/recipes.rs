//! The egui renderer of the recipe editors (design `recipes-editor`): the narrow side panel, its
//! context menus and dialogs, a window per open recipe and the YAML window.
//!
//! The renderer is a thin layer over [`crate::ui::recipes`]: it draws the rows and forms of a
//! [`RecipesState`] and turns clicks and typing into [`RecipesCommand`]s and [`FormCommand`]s,
//! which the element applies. Nothing here changes a recipe directly.
//!
//! # Main components
//!
//! - [`show_recipes`]: the panel of a [`RecipesElement`] — toolbar (Phosphor icons for outline,
//!   dependencies, YAML, settings and new, [`TOOLBAR_ICONS`]), the notice of an invalid tree and
//!   the unsaved marker, rows (type icon, label, status circle or percentage, findings
//!   marker, tooltips), the right-click menu, inline rename, the New / Add child / Save as dialogs,
//!   recipe windows, the YAML window and the windows of View and Metadata.
//! - [`show_form`]: one recipe as a form, with the query editor inside; used by the panel's
//!   windows and by [`RecipeElement`](crate::ui::recipes::RecipeElement).
//! - [`yaml_layout`]: a highlighted layout of a `recipes.yaml` text.
//!
//! # Where it is used
//!
//! `RecipesElement::show_in_egui` and `RecipeElement::show_in_egui`.
//!
//! # Related concepts
//!
//! - `crate::egui::editor`: the query editor drawn inside a form.
//! - `specs/design/recipes-editor/`: the mockups the panel follows.
//!
//! # Examples
//!
//! ```
//! use liquers_lib::egui::recipes::yaml_layout;
//! use liquers_lib::ui::theme::ThemeState;
//!
//! let job = yaml_layout("recipes:\n- {query: -R/a.csv/-/head}\n", &ThemeState::default(), 300.0);
//! assert!(job.sections.len() > 3);
//! ```

use egui::text::LayoutJob;
use egui::{Color32, RichText, Sense, TextFormat};

use crate::egui::theme::color;
use crate::ui::app_state::AppState;
use crate::ui::element::UIElement;
use crate::ui::recipes::{
    menu_items, yaml_spans, FormCommand, MenuAction, NewDialog, RecipeForm, RecipeLabel, RecipeRow, RecipesCommand,
    RecipesElement, RecipesState, RecipesView, RowStatus,
};
use crate::ui::theme::ThemeState;
use crate::ui::ui_context::UIContext;

/// What [`show_form`] produced.
pub struct FormOutput {
    /// The edits, in order.
    pub commands: Vec<FormCommand>,
    /// The form's response.
    pub response: egui::Response,
}

/// How often a busy panel asks for its status again, in seconds.
const REFRESH: f64 = 0.5;

/// The highlighted layout of a `recipes.yaml` text (AC-12).
pub fn yaml_layout(text: &str, theme: &ThemeState, wrap_width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font = egui::FontId::monospace(13.0);
    for (range, role) in yaml_spans(text) {
        let colour = color(theme.theme.syntax(theme.mode, role));
        job.append(&text[range], 0.0, TextFormat::simple(font.clone(), colour));
    }
    job.wrap.max_width = wrap_width;
    job
}

fn tone_color(status: &RowStatus, theme: &ThemeState) -> Color32 {
    let palette = theme.theme.palette(theme.mode);
    match status.tone() {
        "ok" => color(palette.state.success),
        "busy" => color(palette.state.accent),
        "error" => color(palette.state.danger),
        "stale" => color(palette.state.warning),
        _ => color(palette.text.ink_faint),
    }
}

/// A single-line text field that reports its text when committed (Enter or focus lost) and
/// changed. Its buffer follows `current` while it is not being edited.
fn text_field(ui: &mut egui::Ui, id: egui::Id, current: &str, multiline: bool, width: f32) -> Option<String> {
    let mut buffer = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| current.to_owned());
    let edit = if multiline {
        egui::TextEdit::multiline(&mut buffer).desired_rows(2)
    } else {
        egui::TextEdit::singleline(&mut buffer)
    };
    let response = ui.add(edit.id(id).desired_width(width));
    let committed = response.lost_focus() && buffer != current;
    if response.has_focus() || committed {
        ui.data_mut(|d| d.insert_temp(id, buffer.clone()));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    if committed {
        ui.data_mut(|d| d.remove::<String>(id));
        Some(buffer)
    } else {
        None
    }
}

/// One recipe as a form (AC-5): title, description, filename, the query editor, options and
/// argument overrides. `read_only` disables the fields.
pub fn show_form(
    ui: &mut egui::Ui,
    form: &mut RecipeForm,
    theme: &ThemeState,
    ctx: Option<&UIContext>,
    message: Option<&str>,
    read_only: bool,
) -> FormOutput {
    let mut commands = Vec::new();
    let base = egui::Id::new(("lq-recipe-form", form.id.clone()));
    let recipe = form.recipe().clone();
    let palette = theme.theme.palette(theme.mode);
    let response = ui
        .add_enabled_ui(!read_only, |ui| {
            egui::Grid::new(base.with("grid")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Title");
                if let Some(t) = text_field(ui, base.with("title"), &recipe.title, false, 260.0) {
                    commands.push(FormCommand::SetTitle(t));
                }
                ui.end_row();
                ui.label("Description");
                if let Some(t) = text_field(ui, base.with("description"), &recipe.description, true, 260.0) {
                    commands.push(FormCommand::SetDescription(t));
                }
                ui.end_row();
                ui.label("Filename");
                let filename = form.filename().unwrap_or_default();
                if let Some(t) = text_field(ui, base.with("filename"), &filename, false, 260.0) {
                    commands.push(FormCommand::SetFilename(t));
                }
                ui.end_row();
            });
            ui.separator();
            ui.label(RichText::new("Query").strong());
            let tree = form.query_editor().tree().clone();
            let mode = form.mode;
            let out = ui
                .push_id(base.with("query"), |ui| {
                    crate::egui::editor::show_editor(ui, &tree, &mut form.view, mode, theme.density, theme, ctx)
                })
                .inner;
            if let Some(mode) = out.mode {
                commands.push(FormCommand::SetMode(mode));
            }
            commands.extend(out.commands.into_iter().map(FormCommand::Editor));
            ui.separator();
            let header = if form.options_open { "▾ Options" } else { "▸ Options" };
            if ui.selectable_label(false, header).clicked() {
                commands.push(FormCommand::ToggleOptions);
            }
            if form.options_open {
                egui::Grid::new(base.with("options")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label("Working folder");
                    if let Some(t) = text_field(ui, base.with("cwd"), recipe.cwd.as_deref().unwrap_or(""), false, 200.0) {
                        commands.push(FormCommand::SetOption { field: "cwd".into(), text: t });
                    }
                    ui.end_row();
                    ui.label("Volatile");
                    let mut volatile = recipe.volatile;
                    if ui.checkbox(&mut volatile, "").changed() {
                        commands.push(FormCommand::SetOption { field: "volatile".into(), text: volatile.to_string() });
                    }
                    ui.end_row();
                    ui.label("Expires");
                    if let Some(t) = text_field(ui, base.with("expires"), &recipe.expires.to_string(), false, 200.0) {
                        commands.push(FormCommand::SetOption { field: "expires".into(), text: t });
                    }
                    ui.end_row();
                    ui.label("Stored");
                    let current = recipe.stored.map(|s| s.to_string()).unwrap_or_default();
                    let shown = if current.is_empty() { "default" } else { current.as_str() };
                    egui::ComboBox::from_id_salt(base.with("stored")).selected_text(shown).show_ui(ui, |ui| {
                        for (label, text) in [("default", ""), ("true", "true"), ("false", "false")] {
                            if ui.selectable_label(current == text, label).clicked() && current != text {
                                commands.push(FormCommand::SetOption { field: "stored".into(), text: text.into() });
                            }
                        }
                    });
                    ui.end_row();
                    let mut overrides: Vec<(String, String)> = recipe
                        .arguments
                        .iter()
                        .map(|(k, v)| (k.clone(), v.to_string()))
                        .chain(recipe.links.iter().map(|(k, v)| (k.clone(), format!("link {v}"))))
                        .collect();
                    overrides.sort();
                    for (name, value) in overrides {
                        ui.label(&name);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(value).monospace());
                            if ui.small_button("✕").on_hover_text("Remove the override").clicked() {
                                commands.push(FormCommand::RemoveOverride(name.clone()));
                            }
                        });
                        ui.end_row();
                    }
                    ui.label("Override");
                    ui.horizontal(|ui| {
                        let name_id = base.with("override-name");
                        let value_id = base.with("override-value");
                        let mut name = ui.data_mut(|d| d.get_temp::<String>(name_id)).unwrap_or_default();
                        let mut value = ui.data_mut(|d| d.get_temp::<String>(value_id)).unwrap_or_default();
                        ui.add(egui::TextEdit::singleline(&mut name).hint_text("argument").desired_width(80.0));
                        ui.add(egui::TextEdit::singleline(&mut value).hint_text("value").desired_width(100.0));
                        if ui.small_button("Add").clicked() && !name.trim().is_empty() {
                            commands.push(FormCommand::SetOverride { name: name.clone(), text: value.clone() });
                            name.clear();
                            value.clear();
                        }
                        ui.data_mut(|d| {
                            d.insert_temp(name_id, name);
                            d.insert_temp(value_id, value);
                        });
                    });
                    ui.end_row();
                });
            }
            if let Some(message) = message.or(form.message.as_deref()) {
                ui.colored_label(color(palette.state.danger), message);
            }
        })
        .response;
    FormOutput { commands, response }
}

/// Draws one row; returns the commands its clicks and menu produce.
fn show_row(
    ui: &mut egui::Ui,
    row: &RecipeRow,
    state: &RecipesState,
    ui_state: &mut crate::ui::recipes::RecipesUi,
    theme: &ThemeState,
    commands: &mut Vec<RecipesCommand>,
) {
    let palette = theme.theme.palette(theme.mode);
    let label = state.label();
    let file = row.filename.clone().unwrap_or_else(|| row.id.clone());
    let renaming = ui_state.renaming.as_ref().filter(|(id, _)| *id == row.id).map(|(_, t)| t.clone());
    let inner = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.add_space(row.depth as f32 * 12.0);
        match row.expandable {
            Some(expanded) => {
                if ui.small_button(if expanded { "▾" } else { "▸" }).clicked() {
                    commands.push(RecipesCommand::Toggle(row.id.clone()));
                }
            }
            None => {
                if state.view() == RecipesView::Dependencies {
                    ui.add_space(18.0);
                }
            }
        }
        ui.label(&row.icon);
        let mut row_response = None;
        if let Some(mut text) = renaming {
            let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(110.0));
            response.request_focus();
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                ui_state.renaming = None;
            } else if response.lost_focus() {
                ui_state.renaming = None;
                if !text.trim().is_empty() && text.trim() != row.name {
                    commands.push(RecipesCommand::Rename { from: file.clone(), to: text.trim().to_owned() });
                }
            } else {
                ui_state.renaming = Some((row.id.clone(), text));
            }
        } else {
            let (main, extra) = row.label(label);
            let mut text = RichText::new(main);
            if row.is_result {
                text = text.strong();
            }
            if row.repeated || row.cycle {
                text = text.italics().color(color(palette.text.ink_muted));
            }
            let open = state.form(&file).is_some();
            let response = ui.selectable_label(open, text);
            let tooltip = match label {
                RecipeLabel::Title if !row.title.is_empty() => row.title_tooltip(),
                RecipeLabel::Title | RecipeLabel::Name | RecipeLabel::Both => row.name_tooltip(),
            };
            let response = if tooltip.is_empty() { response } else { response.on_hover_text(tooltip) };
            if response.clicked() {
                commands.push(RecipesCommand::Open(file.clone()));
            }
            if let Some(extra) = extra {
                let small = ui.label(RichText::new(extra).small().color(color(palette.text.ink_muted)));
                if !row.description.is_empty() {
                    small.on_hover_text(row.title_tooltip());
                }
            }
            if row.cycle {
                ui.label(RichText::new("↻").color(color(palette.state.warning))).on_hover_text("Closes a cycle");
            }
            row_response = Some(response);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            match &row.status {
                RowStatus::Progress { percent, .. } => {
                    ui.label(RichText::new(format!("{percent}%")).small().color(tone_color(&row.status, theme)))
                        .on_hover_text(row.status.tooltip());
                }
                RowStatus::None | RowStatus::Status(_) | RowStatus::Error(_) => {
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
                    ui.painter().circle_filled(rect.center(), 4.0, tone_color(&row.status, theme));
                    response.on_hover_text(row.status.tooltip());
                }
            }
            if !row.findings.is_empty() {
                let (mark, tone) = if row.has_error {
                    ("⚠", palette.state.danger)
                } else {
                    ("⚠", palette.state.warning)
                };
                ui.label(RichText::new(mark).color(color(tone))).on_hover_text(row.findings_tooltip());
            }
        });
        row_response
    });
    let response = inner.inner.unwrap_or(inner.response);
    response.context_menu(|ui| {
        let writable = state.read_only().is_none();
        for item in menu_items(row, state.is_tree(), writable, cfg!(not(target_arch = "wasm32"))) {
            if let MenuAction::Delete = item.action {
                ui.separator();
            }
            let button = ui.add_enabled(item.disabled.is_none(), egui::Button::new(item.action.label()));
            let button = match &item.disabled {
                Some(reason) => button.on_disabled_hover_text(reason),
                None => button,
            };
            if !button.clicked() {
                continue;
            }
            ui.close();
            match item.action {
                MenuAction::View => commands.push(RecipesCommand::View(file.clone())),
                MenuAction::Metadata => commands.push(RecipesCommand::Metadata(file.clone())),
                MenuAction::Edit => commands.push(RecipesCommand::Open(file.clone())),
                MenuAction::Evaluate => commands.push(RecipesCommand::Evaluate(file.clone())),
                MenuAction::Rename => ui_state.renaming = Some((row.id.clone(), row.name.clone())),
                MenuAction::Duplicate => commands.push(RecipesCommand::Duplicate(file.clone())),
                MenuAction::AddChild => {
                    ui_state.new_dialog = Some(NewDialog {
                        parent: Some(file.clone()),
                        title: String::new(),
                        extension: String::new(),
                        slot: 0,
                    })
                }
                MenuAction::SetResult => commands.push(RecipesCommand::SetResult(Some(file.clone()))),
                MenuAction::SaveAs => ui_state.save_as = Some((file.clone(), None, default_path(&file, None))),
                MenuAction::Export(format) => {
                    let path = default_path(&file, Some(&format));
                    ui_state.save_as = Some((file.clone(), Some(format), path));
                }
                MenuAction::Delete => commands.push(RecipesCommand::Delete(file.clone())),
            }
        }
    });
}

/// A suggested local path for Save as: the file's name (with the export format's extension) in
/// the working directory.
fn default_path(file: &str, format: Option<&str>) -> String {
    let name = file.rsplit('/').next().unwrap_or(file);
    match format {
        Some(format) => {
            let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
            format!("{stem}.{format}")
        }
        None => name.to_owned(),
    }
}

/// The toolbar's Phosphor icons, by name, with the text drawn when the icon font lacks one:
/// outline, dependencies, YAML, settings, new (design `strict-tree-editing`, AC-6). Symbols such
/// as `⑂` or `＋` are missing from egui's default fonts and drew as boxes.
pub const TOOLBAR_ICONS: [(&str, &str); 5] = [
    ("list", "="),
    ("tree-structure", "Deps"),
    ("brackets-curly", "{}"),
    ("gear", "Set"),
    ("plus", "+"),
];

fn toolbar_icon(index: usize) -> &'static str {
    let (name, fallback) = TOOLBAR_ICONS[index];
    crate::egui::theme::phosphor_glyph(name).unwrap_or(fallback)
}

fn toolbar(ui: &mut egui::Ui, element: &mut RecipesElement, state: &RecipesState, commands: &mut Vec<RecipesCommand>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let outline = state.view() == RecipesView::Outline;
        if ui.selectable_label(outline, toolbar_icon(0)).on_hover_text("Outline").clicked() && !outline {
            commands.push(RecipesCommand::SetView(RecipesView::Outline));
        }
        if ui.selectable_label(!outline, toolbar_icon(1)).on_hover_text("Dependencies").clicked() && outline {
            commands.push(RecipesCommand::SetView(RecipesView::Dependencies));
        }
        if ui.selectable_label(state.yaml().is_some(), toolbar_icon(2)).on_hover_text("Edit as YAML").clicked() {
            commands.push(if state.yaml().is_some() { RecipesCommand::CloseYaml } else { RecipesCommand::OpenYaml });
        }
        let settings = ui.selectable_label(element.ui.settings_open, toolbar_icon(3)).on_hover_text("Settings");
        if settings.clicked() {
            element.ui.settings_open = !element.ui.settings_open;
        }
        let new = ui
            .add_enabled(state.read_only().is_none(), egui::Button::new(toolbar_icon(4)).small())
            .on_hover_text("New node");
        if new.clicked() {
            // An empty `.node` node (AC-5): its query and filename are set in its window.
            element.ui.new_dialog = Some(NewDialog { parent: None, title: String::new(), extension: String::new(), slot: 0 });
        }
    });
    if element.ui.settings_open {
        ui.group(|ui| {
            ui.label(RichText::new("Show").small());
            ui.horizontal(|ui| {
                for label in [RecipeLabel::Name, RecipeLabel::Title, RecipeLabel::Both] {
                    if ui.selectable_label(state.label() == label, label.as_str()).clicked() {
                        commands.push(RecipesCommand::SetLabel(label));
                    }
                }
            });
            let mut continuous = state.continuous();
            if ui.checkbox(&mut continuous, "Apply immediately").changed() {
                commands.push(RecipesCommand::SetContinuous(continuous));
            }
        });
    }
    if !state.continuous() && state.is_pending() {
        ui.horizontal(|ui| {
            if ui.button("Update").clicked() {
                commands.push(RecipesCommand::Update);
            }
            if ui.button("Revert").clicked() {
                commands.push(RecipesCommand::Revert);
            }
        });
    }
}

fn new_dialog(ctx: &egui::Context, element: &mut RecipesElement, state: &RecipesState, commands: &mut Vec<RecipesCommand>) {
    let Some(mut dialog) = element.ui.new_dialog.take() else {
        return;
    };
    let slots = dialog.parent.as_deref().map(|p| state.child_slots(p)).unwrap_or_default();
    let title = match &dialog.parent {
        Some(parent) => format!("Add a child of {parent}"),
        None => "New node".to_owned(),
    };
    let mut keep = true;
    let mut done = false;
    egui::Window::new(title)
        .id(egui::Id::new(("lq-recipes-new", element.handle().map(|h| h.0))))
        .collapsible(false)
        .resizable(false)
        .open(&mut keep)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Title");
                ui.text_edit_singleline(&mut dialog.title);
            });
            if dialog.parent.is_some() {
                if slots.is_empty() {
                    ui.label("The parent has no Node argument to name a child in.");
                } else {
                    let shown = slots
                        .get(dialog.slot)
                        .map(|s| format!("{} · {} (.{})", s.command, s.arg, s.extension))
                        .unwrap_or_default();
                    egui::ComboBox::from_label("Argument").selected_text(shown).show_ui(ui, |ui| {
                        for (i, s) in slots.iter().enumerate() {
                            ui.selectable_value(&mut dialog.slot, i, format!("{} · {} (.{})", s.command, s.arg, s.extension));
                        }
                    });
                }
            } else {
                ui.label(RichText::new("An empty node; set its query and filename in its window.").small());
            }
            ui.horizontal(|ui| {
                // The title is optional: an untitled node is named by the first free integer.
                let ready = dialog.parent.is_none() || slots.get(dialog.slot).is_some();
                if ui.add_enabled(ready, egui::Button::new("Create")).clicked() {
                    match (&dialog.parent, slots.get(dialog.slot)) {
                        (Some(parent), Some(slot)) => commands.push(RecipesCommand::AddChild {
                            parent: parent.clone(),
                            title: dialog.title.trim().to_owned(),
                            extension: slot.extension.clone(),
                            action: slot.action,
                            arg: slot.arg.clone(),
                        }),
                        (Some(_), None) | (None, _) => commands.push(RecipesCommand::New {
                            title: dialog.title.trim().to_owned(),
                            extension: dialog.extension.trim().to_owned(),
                        }),
                    }
                    done = true;
                }
                if ui.button("Cancel").clicked() {
                    done = true;
                }
            });
        });
    if keep && !done {
        element.ui.new_dialog = Some(dialog);
    }
}

fn save_as_dialog(ctx: &egui::Context, element: &mut RecipesElement, commands: &mut Vec<RecipesCommand>) {
    let Some((file, format, mut path)) = element.ui.save_as.take() else {
        return;
    };
    let title = match &format {
        Some(format) => format!("Export {file} as {format}"),
        None => format!("Save {file} as"),
    };
    let mut keep = true;
    let mut done = false;
    egui::Window::new(title)
        .id(egui::Id::new(("lq-recipes-save-as", element.handle().map(|h| h.0))))
        .collapsible(false)
        .open(&mut keep)
        .show(ctx, |ui| {
            ui.label("A file on this computer (not in the store):");
            ui.add(egui::TextEdit::singleline(&mut path).desired_width(320.0));
            ui.horizontal(|ui| {
                if ui.add_enabled(!path.trim().is_empty(), egui::Button::new("Save")).clicked() {
                    commands.push(RecipesCommand::SaveAs { filename: file.clone(), format: format.clone(), path: path.trim().to_owned() });
                    done = true;
                }
                if ui.button("Cancel").clicked() {
                    done = true;
                }
            });
        });
    if keep && !done {
        element.ui.save_as = Some((file, format, path));
    }
}

fn yaml_window(ctx: &egui::Context, element: &RecipesElement, state: &RecipesState, theme: &ThemeState, commands: &mut Vec<RecipesCommand>) {
    let Some(buffer) = state.yaml() else {
        return;
    };
    let mut text = buffer.text.clone();
    let mut open = true;
    let theme = theme.clone();
    egui::Window::new("recipes.yaml")
        .id(egui::Id::new(("lq-recipes-yaml", element.handle().map(|h| h.0))))
        .default_size([460.0, 380.0])
        .open(&mut open)
        .show(ctx, |ui| {
            let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                ui.fonts_mut(|f| f.layout_job(yaml_layout(text.as_str(), &theme, wrap_width)))
            };
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                let response = ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .layouter(&mut layouter),
                );
                if response.changed() {
                    commands.push(RecipesCommand::SetYaml(text.clone()));
                }
            });
            if let Some((line, message)) = &buffer.error {
                let palette = theme.theme.palette(theme.mode);
                let at = line.map(|l| format!("line {l}: ")).unwrap_or_default();
                ui.colored_label(color(palette.state.danger), format!("{at}{message}"));
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(buffer.error.is_none(), egui::Button::new("Apply")).clicked() {
                    commands.push(RecipesCommand::ApplyYaml);
                }
                if ui.button("Close").clicked() {
                    commands.push(RecipesCommand::CloseYaml);
                }
            });
        });
    if !open {
        commands.push(RecipesCommand::CloseYaml);
    }
}

/// The recipes panel (AC-1 to AC-19).
pub fn show_recipes(
    element: &mut RecipesElement,
    ui: &mut egui::Ui,
    ctx: &UIContext,
    app_state: &mut dyn AppState,
) -> egui::Response {
    crate::egui::theme::ensure_icon_font(ui.ctx());
    let theme = ctx.theme();
    let Some(state) = element.state().cloned() else {
        return ui.label("Recipes: no command registry");
    };
    let mut commands = Vec::new();
    // Keep the status of assets being made fresh (AC-17).
    let now = ui.input(|i| i.time);
    if state.busy() {
        if now - element.ui.last_refresh > REFRESH {
            element.ui.last_refresh = now;
            element.refresh_status(ctx);
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(REFRESH));
    }
    let id = egui::Id::new(("lq-recipes", element.handle().map(|h| h.0)));
    let response = ui
        .push_id(id, |ui| {
            ui.set_max_width(ui.available_width().min(260.0));
            ui.horizontal(|ui| {
                ui.label(RichText::new(element.title()).strong());
            });
            toolbar(ui, element, &state, &mut commands);
            let palette = theme.theme.palette(theme.mode);
            if let Some(reason) = state.read_only() {
                ui.label(RichText::new(reason).small().color(color(palette.state.warning)));
            }
            // AC-9: an invalid tree is used as plain recipes, and says so.
            if let Some(notice) = state.tree_notice() {
                ui.label(RichText::new(notice).small().color(color(palette.state.warning)));
            }
            // AC-4: edits a failed write left in memory only.
            if state.unsaved().is_some() {
                ui.label(RichText::new("Unsaved edits").small().strong().color(color(palette.state.warning)))
                    .on_hover_text(state.unsaved().unwrap_or_default());
            }
            if let Some(message) = state.message() {
                ui.label(RichText::new(message).small().color(color(palette.text.ink_muted)));
            }
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                if state.rows().is_empty() && state.list().is_some() {
                    ui.label(RichText::new(format!("No recipes yet: {} adds one.", toolbar_icon(4))).small().color(color(palette.text.ink_faint)));
                }
                for (i, row) in state.rows().iter().enumerate() {
                    ui.push_id(i, |ui| show_row(ui, row, &state, &mut element.ui, &theme, &mut commands));
                }
            });
        })
        .response;

    let egui_ctx = ui.ctx().clone();
    new_dialog(&egui_ctx, element, &state, &mut commands);
    save_as_dialog(&egui_ctx, element, &mut commands);
    yaml_window(&egui_ctx, element, &state, &theme, &mut commands);

    // A window per open recipe (AC-4).
    let read_only = state.read_only().is_some();
    if let Some(live) = element.state_mut() {
        for form in live.forms_mut() {
            let id = form.id.clone();
            let mut open = true;
            let title = form.filename().unwrap_or_else(|| id.clone());
            egui::Window::new(title)
                .id(egui::Id::new(("lq-recipe-window", id.clone())))
                .default_width(420.0)
                .open(&mut open)
                .show(&egui_ctx, |ui| {
                    let out = show_form(ui, form, &theme, Some(ctx), None, read_only);
                    commands.extend(out.commands.into_iter().map(|command| RecipesCommand::Form { id: id.clone(), command }));
                });
            if !open {
                commands.push(RecipesCommand::Close(id));
            }
        }
    }

    // View and Metadata windows: the panel's children (AC-19).
    if let Some(handle) = element.handle() {
        let mut to_remove = Vec::new();
        for child_handle in app_state.children(handle).unwrap_or_default() {
            if let Ok(mut child) = app_state.take_element(child_handle) {
                let mut open = true;
                egui::Window::new(child.title())
                    .id(egui::Id::new(("lq-recipes-child", child_handle.0)))
                    .open(&mut open)
                    .show(&egui_ctx, |ui| {
                        child.show_in_egui(ui, ctx, app_state);
                    });
                if open {
                    let _ = app_state.put_element(child_handle, child);
                } else {
                    to_remove.push(child_handle);
                }
            }
        }
        for child in to_remove {
            let _ = app_state.remove(child);
        }
    }

    element.apply_commands(commands, ctx);
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AC-12: the YAML layout keeps the text and colours its spans.
    #[test]
    fn yaml_layout_covers_the_text() {
        let text = "recipes:\n- {query: -R/a.csv/-/head, title: A}\n";
        let job = yaml_layout(text, &ThemeState::default(), 300.0);
        assert_eq!(job.text, text);
        let colours: std::collections::BTreeSet<_> = job.sections.iter().map(|s| s.format.color.to_array()).collect();
        assert!(colours.len() > 2);
    }

    #[test]
    fn default_path_uses_the_export_format() {
        assert_eq!(default_path("scenes/car.glb", None), "car.glb");
        assert_eq!(default_path("table.csv", Some("parquet")), "table.parquet");
    }

    /// strict-tree-editing AC-6: every toolbar icon is a Phosphor glyph, not a symbol the default
    /// fonts lack.
    #[test]
    fn egui_toolbar_uses_phosphor_glyphs() {
        for (i, (name, _)) in TOOLBAR_ICONS.iter().enumerate() {
            let glyph = crate::egui::theme::phosphor_glyph(name).unwrap_or_else(|| panic!("no Phosphor icon {name}"));
            assert_eq!(toolbar_icon(i), glyph);
        }
    }
}
