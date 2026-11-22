//! The query editor of `specs/design/query-editor-ergonomics/` on its own: the problem example in
//! Simple and Advanced mode, in the built-in theme. `Ctrl+B` switches the mode, `Ctrl+J` jumps
//! between the query text and the field at the cursor, `Ctrl+Shift+L` switches dark and light, and
//! `Ctrl+Shift+D` switches the density.
//!
//! Run with: `cargo run --example ui_query_editor -p liquers-lib`

use std::sync::Arc;

use liquers_core::command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry, CommandPreset};
use liquers_core::parse::parse_query;
use liquers_core::value::ValueInterface;

use liquers_lib::egui::editor::show_editor;
use liquers_lib::egui::theme::apply_theme;
use liquers_lib::ui::editor::{EditorKind, EditorMode, EditorState, EditorView};
use liquers_lib::ui::theme::{Density, ThemeCell, ThemeMode, ThemeState};
use liquers_lib::value::Value;

const QUERY: &str = "-R/data/sales.csv/-/ns-pl/gt-amount-~X~-R/config/limit.txt~E/sort-amount-f/head-20";

fn registry() -> CommandMetadataRegistry {
    let mut cmr = CommandMetadataRegistry::new();
    let mut gt = CommandMetadata::new("gt");
    gt.with_namespace("pl")
        .with_label("Greater than")
        .with_doc("Keep the rows whose column is greater than the value")
        .with_hint("icon", serde_json::json!("funnel"))
        .with_argument(ArgumentInfo::string_argument("column").with_description("The column to compare"))
        .with_argument(ArgumentInfo::string_argument("value"));
    cmr.add_command(&gt);
    let mut sort = CommandMetadata::new("sort");
    sort.with_namespace("pl")
        .with_label("Sort by column")
        .with_hint("icon", serde_json::json!("sort-ascending"))
        .with_argument(ArgumentInfo::string_argument("column"))
        .with_argument(ArgumentInfo::boolean_argument("ascending").with_default(true));
    cmr.add_command(&sort);
    let mut head = CommandMetadata::new("head");
    head.with_namespace("pl")
        .with_label("Get first rows")
        .with_doc("Return the first N rows")
        .with_hint("icon", serde_json::json!("rows"));
    head.with_argument(ArgumentInfo::integer_argument("n", false).with_default(5).with_description("Number of rows"));
    for (q, label) in [("head-5", "Top 5"), ("head-20", "Top 20"), ("head-100", "Top 100"), ("head-1000", "Top 1000")] {
        head.presets.push(CommandPreset::new(q, label, "").expect("preset"));
    }
    head.next.push(CommandPreset::new("sort", "Sort", "Sort the rows").expect("next"));
    cmr.add_command(&head);
    cmr
}

struct EditorApp {
    state: EditorState,
    view: EditorView,
    mode: EditorMode,
    theme: ThemeCell,
}

impl eframe::App for EditorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::L)) {
            let mode = match self.theme.get().mode {
                ThemeMode::Dark => ThemeMode::Light,
                ThemeMode::Light => ThemeMode::Dark,
            };
            self.theme.set_mode(mode);
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::D)) {
            let density = match self.theme.get().density {
                Density::Comfortable => Density::Compact,
                Density::Compact => Density::Comfortable,
            };
            self.theme.set_density(density);
        }
        let theme = self.theme.get();
        apply_theme(ui.ctx(), &theme);
        ui.painter().rect_filled(ui.max_rect(), 0.0, ui.visuals().panel_fill);
        egui::ScrollArea::vertical().show(ui, |ui| {
            let tree = self.state.tree().clone();
            let out = show_editor(ui, &tree, &mut self.view, self.mode, theme.density, &theme, None);
            if let Some(mode) = out.mode {
                self.mode = mode;
            }
            for command in out.commands {
                if let Err(e) = self.state.apply(command) {
                    eprintln!("refused: {e}");
                }
            }
        });
    }
}

fn main() -> eframe::Result {
    let mode = match std::env::args().nth(1).as_deref() {
        Some("advanced") => EditorMode::Advanced,
        Some(_) | None => EditorMode::Simple,
    };
    let mut state = EditorState::new(EditorKind::Query, Arc::new(registry()));
    let query = parse_query(QUERY).expect("query");
    state.on_snapshot(&Value::from_query(&query), None).expect("snapshot");
    let theme = ThemeCell::new(ThemeState::default());
    if std::env::args().any(|a| a == "light") {
        theme.set_mode(ThemeMode::Light);
    }
    if std::env::args().any(|a| a == "compact") {
        theme.set_density(Density::Compact);
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Liquers query editor",
        options,
        Box::new(move |_cc| Ok(Box::new(EditorApp { state, view: EditorView::default(), mode, theme }))),
    )
}
