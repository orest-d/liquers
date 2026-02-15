//! UISpecElement - Flexible UI widget defined by YAML specification
//!
//! Supports menu bars, configurable layouts (horizontal, vertical, grid, tabs, windows),
//! and keyboard shortcuts. Children are created by init queries and arranged by layout.

use liquers_core::context::Context;
use liquers_core::error::Error;
use liquers_core::recipes::Recipe;
use liquers_core::state::State;
use liquers_core::value::Value;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::ui::app_state::AppState;
use crate::ui::element::{UIElement, UpdateMessage, UpdateResponse};
use crate::ui::handle::UIHandle;
use crate::ui::ui_context::UIContext;

// ============================================================================
// YAML Specification Structs
// ============================================================================

/// Top-level UI specification (deserialized from YAML)
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UISpec {
    /// Initialization queries submitted during element's init() call
    #[serde(default)]
    pub init: Vec<InitQuery>,

    /// Menu bar specification (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu: Option<MenuBarSpec>,

    /// Layout specification (how to arrange children)
    #[serde(default)]
    pub layout: LayoutSpec,
}

/// Init query - either a query string or a recipe
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum InitQuery {
    Query(String),
    Recipe(Recipe),
}

/// Menu bar specification
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MenuBarSpec {
    pub items: Vec<TopLevelItem>,
}

/// Top-level menu item (menu or button)
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum TopLevelItem {
    Menu {
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shortcut: Option<String>,
        items: Vec<MenuItem>,
    },
    Button {
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shortcut: Option<String>,
        #[serde(default)]
        action: MenuAction,
    },
}

/// Menu item (button, submenu, or separator)
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum MenuItem {
    Button {
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shortcut: Option<String>,
        #[serde(default)]
        action: MenuAction,
    },
    Submenu {
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shortcut: Option<String>,
        items: Vec<MenuItem>,
    },
    Separator,
}

/// Menu action (query submission, quit, or no-op)
///
/// YAML formats:
/// - `action: quit` → `MenuAction::Quit`
/// - `action: { query: "..." }` → `MenuAction::Query("...")`
/// - `action: null` or omitted → `MenuAction::None`
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum MenuAction {
    #[default]
    None,
    Quit,
    Query(String),
}

/// Helper for custom deserialization: accepts null, string ("quit"), or map ({query: "..."}).
#[derive(Deserialize)]
#[serde(untagged)]
enum MenuActionDe {
    Null(()),
    String(String),
    Query { query: String },
}

impl<'de> Deserialize<'de> for MenuAction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let de = MenuActionDe::deserialize(deserializer)?;
        match de {
            MenuActionDe::Null(()) => Ok(MenuAction::None),
            MenuActionDe::String(s) => match s.as_str() {
                "quit" => Ok(MenuAction::Quit),
                "none" => Ok(MenuAction::None),
                other => Err(serde::de::Error::custom(format!(
                    "unknown menu action: '{}' (expected 'quit' or {{query: \"...\"}})",
                    other
                ))),
            },
            MenuActionDe::Query { query } => Ok(MenuAction::Query(query)),
        }
    }
}

/// Layout specification (how to arrange children)
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum LayoutSpec {
    #[default]
    Horizontal,
    Vertical,
    Grid {
        #[serde(default)]
        rows: usize,
        #[serde(default)]
        columns: usize,
    },
    Tabs {
        #[serde(default)]
        selected: usize,
    },
    Windows,
}

impl UISpec {
    /// Deserialize from YAML text
    pub fn from_yaml(yaml: &str) -> Result<Self, Error> {
        serde_yaml::from_str(yaml)
            .map_err(|e| Error::general_error(format!("YAML parse error: {}", e)))
    }

    /// Deserialize from YAML bytes
    pub fn from_yaml_bytes(bytes: &[u8]) -> Result<Self, Error> {
        serde_yaml::from_slice(bytes)
            .map_err(|e| Error::general_error(format!("YAML parse error: {}", e)))
    }
}

impl MenuBarSpec {
    /// Extract all shortcuts, detect conflicts
    pub fn validate_shortcuts(&self) -> Vec<(String, usize)> {
        let mut shortcuts = std::collections::HashMap::new();
        for item in &self.items {
            collect_shortcuts_from_top_level(item, &mut shortcuts);
        }
        shortcuts
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .collect()
    }
}

fn collect_shortcuts_from_top_level(
    item: &TopLevelItem,
    shortcuts: &mut std::collections::HashMap<String, usize>,
) {
    match item {
        TopLevelItem::Menu {
            shortcut, items, ..
        } => {
            if let Some(s) = shortcut {
                *shortcuts.entry(s.clone()).or_insert(0) += 1;
            }
            for menu_item in items {
                collect_shortcuts_from_menu_item(menu_item, shortcuts);
            }
        }
        TopLevelItem::Button { shortcut, .. } => {
            if let Some(s) = shortcut {
                *shortcuts.entry(s.clone()).or_insert(0) += 1;
            }
        }
    }
}

fn collect_shortcuts_from_menu_item(
    item: &MenuItem,
    shortcuts: &mut std::collections::HashMap<String, usize>,
) {
    match item {
        MenuItem::Button { shortcut, .. } => {
            if let Some(s) = shortcut {
                *shortcuts.entry(s.clone()).or_insert(0) += 1;
            }
        }
        MenuItem::Submenu {
            shortcut, items, ..
        } => {
            if let Some(s) = shortcut {
                *shortcuts.entry(s.clone()).or_insert(0) += 1;
            }
            for menu_item in items {
                collect_shortcuts_from_menu_item(menu_item, shortcuts);
            }
        }
        MenuItem::Separator => {}
    }
}

// ============================================================================
// Runtime Element
// ============================================================================

/// Shortcut registry (maps keyboard shortcuts to actions)
#[derive(Clone, Debug)]
struct ShortcutRegistry {
    shortcuts: std::collections::HashMap<String, MenuAction>,
}

impl ShortcutRegistry {
    fn from_menu_spec(menu_spec: &MenuBarSpec) -> Self {
        let mut shortcuts = std::collections::HashMap::new();
        for item in &menu_spec.items {
            collect_actions_from_top_level(item, &mut shortcuts);
        }
        Self { shortcuts }
    }
}

fn collect_actions_from_top_level(
    item: &TopLevelItem,
    shortcuts: &mut std::collections::HashMap<String, MenuAction>,
) {
    match item {
        TopLevelItem::Menu { items, .. } => {
            for menu_item in items {
                collect_actions_from_menu_item(menu_item, shortcuts);
            }
        }
        TopLevelItem::Button {
            shortcut, action, ..
        } => {
            if let Some(s) = shortcut {
                if !matches!(action, MenuAction::None) {
                    shortcuts.entry(s.clone()).or_insert_with(|| action.clone());
                }
            }
        }
    }
}

fn collect_actions_from_menu_item(
    item: &MenuItem,
    shortcuts: &mut std::collections::HashMap<String, MenuAction>,
) {
    match item {
        MenuItem::Button {
            shortcut, action, ..
        } => {
            if let Some(s) = shortcut {
                if !matches!(action, MenuAction::None) {
                    shortcuts.entry(s.clone()).or_insert_with(|| action.clone());
                }
            }
        }
        MenuItem::Submenu { items, .. } => {
            for menu_item in items {
                collect_actions_from_menu_item(menu_item, shortcuts);
            }
        }
        MenuItem::Separator => {}
    }
}

/// UISpecElement - Flexible UI widget defined by YAML specification
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UISpecElement {
    handle: Option<UIHandle>,
    title_text: String,
    init_queries: Vec<InitQuery>,
    menu_spec: Option<MenuBarSpec>,
    layout_spec: LayoutSpec,
    #[serde(skip)]
    shortcut_registry: Option<ShortcutRegistry>,
}

impl UISpecElement {
    pub fn from_spec(title: String, spec: UISpec) -> Self {
        Self {
            handle: None,
            title_text: title,
            init_queries: spec.init,
            menu_spec: spec.menu,
            layout_spec: spec.layout,
            shortcut_registry: None,
        }
    }

    // Helper methods for menu rendering
    fn render_menu_bar(&self, ui: &mut egui::Ui, menu_spec: &MenuBarSpec, ctx: &UIContext) {
        for item in &menu_spec.items {
            match item {
                TopLevelItem::Menu { label, items, .. } => {
                    ui.menu_button(label, |ui| {
                        self.render_menu_items(ui, items, ctx);
                    });
                }
                TopLevelItem::Button { label, action, .. } => {
                    if ui.button(label).clicked() {
                        self.handle_menu_action(action, ctx);
                    }
                }
            }
        }
    }

    fn render_menu_items(&self, ui: &mut egui::Ui, items: &[MenuItem], ctx: &UIContext) {
        for item in items {
            match item {
                MenuItem::Button { label, action, .. } => {
                    if ui.button(label).clicked() {
                        self.handle_menu_action(action, ctx);
                        ui.close_menu();
                    }
                }
                MenuItem::Submenu { label, items, .. } => {
                    ui.menu_button(label, |ui| {
                        self.render_menu_items(ui, items, ctx);
                    });
                }
                MenuItem::Separator => {
                    ui.separator();
                }
            }
        }
    }

    fn handle_menu_action(&self, action: &MenuAction, ctx: &UIContext) {
        match action {
            MenuAction::Quit => {
                std::process::exit(0);
            }
            MenuAction::Query(query) => {
                if let Some(handle) = self.handle {
                    ctx.submit_query(handle, query.clone());
                }
            }
            MenuAction::None => {}
        }
    }

    fn check_shortcut(&self, ui: &egui::Ui, shortcut_str: &str) -> bool {
        let parts: Vec<&str> = shortcut_str.split('+').collect();
        if parts.is_empty() {
            return false;
        }

        let mut modifiers = egui::Modifiers::default();
        let mut key_opt = None;

        for part in parts {
            match part {
                "Ctrl" | "Control" => modifiers.ctrl = true,
                "Shift" => modifiers.shift = true,
                "Alt" => modifiers.alt = true,
                "Command" | "Cmd" => modifiers.command = true,
                _ => {
                    key_opt = Self::parse_key(part);
                }
            }
        }

        if let Some(key) = key_opt {
            let shortcut = egui::KeyboardShortcut::new(modifiers, key);
            ui.input_mut(|i| i.consume_shortcut(&shortcut))
        } else {
            false
        }
    }

    fn parse_key(key_str: &str) -> Option<egui::Key> {
        // Use egui's built-in Key::from_name for most cases
        egui::Key::from_name(key_str).or_else(|| {
            // Handle common aliases not in egui's from_name
            match key_str {
                "Esc" => Some(egui::Key::Escape),
                "Return" => Some(egui::Key::Enter),
                _ => None,
            }
        })
    }
}

// ============================================================================
// UIElement Trait Implementation
// ============================================================================

#[typetag::serde]
impl UIElement for UISpecElement {
    fn type_name(&self) -> &'static str {
        "UISpecElement"
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

        // Build shortcut registry
        if let Some(menu_spec) = &self.menu_spec {
            self.shortcut_registry = Some(ShortcutRegistry::from_menu_spec(menu_spec));
        }

        // Submit init queries
        for init_query in &self.init_queries {
            match init_query {
                InitQuery::Query(query_str) => {
                    ctx.submit_query(handle, query_str.clone());
                }
                InitQuery::Recipe(_) => {
                    // Future: submit recipe
                }
            }
        }

        Ok(())
    }

    fn update(&mut self, _message: &UpdateMessage, _ctx: &UIContext) -> UpdateResponse {
        UpdateResponse::Unchanged
    }

    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &UIContext,
        app_state: &mut dyn AppState,
    ) -> egui::Response {
        //let mut response = ui.allocate_response(ui.available_size(), egui::Sense::hover());

        // 1. Check keyboard shortcuts
        if let Some(registry) = &self.shortcut_registry {
            for (shortcut_str, action) in &registry.shortcuts {
                if self.check_shortcut(ui, shortcut_str) {
                    self.handle_menu_action(action, ctx);
                }
            }
        }

        // 2. Render menu bar
        if let Some(menu_spec) = &self.menu_spec {
            egui::menu::bar(ui, |ui| {
                self.render_menu_bar(ui, menu_spec, ctx);
            });
        }

        // 3. Get children
        let child_handles = if let Some(handle) = self.handle {
            app_state.children(handle).unwrap_or_else(|_| Vec::new())
        } else {
            Vec::new()
        };

        // 4. Render layout
        match &self.layout_spec {
            LayoutSpec::Horizontal => {
                println!(
                    "Rendering horizontal layout with {} children",
                    child_handles.len()
                );
                return ui
                    .horizontal(|ui| {
                        for child_handle in child_handles {
                            if let Ok(mut child) = app_state.take_element(child_handle) {
                                println!("Rendering child with handle {:?}", child_handle);
                                child.show_in_egui(ui, ctx, app_state);
                                let _ = app_state.put_element(child_handle, child);
                            }
                        }
                    })
                    .response;
            }
            LayoutSpec::Vertical => {
                return ui
                    .vertical(|ui| {
                        for child_handle in child_handles {
                            if let Ok(mut child) = app_state.take_element(child_handle) {
                                child.show_in_egui(ui, ctx, app_state);
                                let _ = app_state.put_element(child_handle, child);
                            }
                        }
                    })
                    .response;
            }
            LayoutSpec::Grid { rows, columns } => {
                let num_children = child_handles.len();
                let (actual_rows, actual_columns) = match (rows, columns) {
                    (0, 0) => {
                        let r = (num_children as f64).sqrt().floor() as usize;
                        let r = r.max(1);
                        let c = (num_children + r - 1) / r;
                        (r, c)
                    }
                    (0, c) if *c > 0 => {
                        let r = (num_children + c - 1) / c;
                        (r, *c)
                    }
                    (r, 0) if *r > 0 => {
                        let c = (num_children + r - 1) / r;
                        (*r, c)
                    }
                    (r, c) => (*r, *c),
                };

                return egui::Grid::new(format!("grid_{:?}", self.handle))
                    .num_columns(actual_columns)
                    .show(ui, |ui| {
                        for (i, child_handle) in child_handles.iter().enumerate() {
                            if let Ok(mut child) = app_state.take_element(*child_handle) {
                                child.show_in_egui(ui, ctx, app_state);
                                let _ = app_state.put_element(*child_handle, child);
                            }
                            if (i + 1) % actual_columns == 0 {
                                ui.end_row();
                            }
                        }
                    })
                    .response;
            }
            LayoutSpec::Tabs { selected } => {
                return ui
                    .horizontal(|ui| {
                        if let Some(child_handle) = child_handles.get(*selected) {
                            if let Ok(mut child) = app_state.take_element(*child_handle) {
                                ui.horizontal(|ui| {
                                    for (i, handle) in child_handles.iter().enumerate() {
                                        if let Ok(Some(element)) = app_state.get_element(*handle) {
                                            let _ = ui
                                                .selectable_label(i == *selected, element.title());
                                        }
                                    }
                                });
                                ui.separator();
                                child.show_in_egui(ui, ctx, app_state);
                                let _ = app_state.put_element(*child_handle, child);
                            }
                        }
                    })
                    .response;
            }
            LayoutSpec::Windows => {
                return ui
                    .horizontal(|ui| {
                        let mut to_remove = Vec::new();
                        for child_handle in child_handles {
                            if let Ok(mut child) = app_state.take_element(child_handle) {
                                let window_title = child.title();
                                let mut open = true;
                                egui::Window::new(window_title)
                                    .id(egui::Id::new(format!("window_{:?}", child_handle)))
                                    .open(&mut open)
                                    .show(ui.ctx(), |ui| {
                                        child.show_in_egui(ui, ctx, app_state);
                                    });
                                if open {
                                    let _ = app_state.put_element(child_handle, child);
                                } else {
                                    to_remove.push(child_handle);
                                }
                            }
                        }
                        for handle in to_remove {
                            let _ = app_state.remove(handle);
                        }
                    })
                    .response;
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yaml_parse_simple() {
        let yaml = r#"
            init: []
            layout: horizontal
        "#;
        let spec = UISpec::from_yaml(yaml).unwrap();
        assert_eq!(spec.init.len(), 0);
        assert!(matches!(spec.layout, LayoutSpec::Horizontal));
    }

    #[test]
    fn test_yaml_parse_with_defaults() {
        let yaml = r#"init: []"#;
        let spec = UISpec::from_yaml(yaml).unwrap();
        assert_eq!(spec.init.len(), 0);
        assert!(matches!(spec.layout, LayoutSpec::Horizontal));
        assert!(spec.menu.is_none());
    }

    #[test]
    fn test_yaml_parse_invalid() {
        let yaml = "invalid: { yaml: [syntax";
        assert!(UISpec::from_yaml(yaml).is_err());
    }

    #[test]
    fn test_shortcut_validation() {
        let menu = MenuBarSpec {
            items: vec![
                TopLevelItem::Button {
                    label: "Save".to_string(),
                    icon: None,
                    shortcut: Some("Ctrl+S".to_string()),
                    action: MenuAction::Quit,
                },
                TopLevelItem::Button {
                    label: "Submit".to_string(),
                    icon: None,
                    shortcut: Some("Ctrl+S".to_string()),
                    action: MenuAction::Quit,
                },
            ],
        };
        let conflicts = menu.validate_shortcuts();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].0, "Ctrl+S");
        assert_eq!(conflicts[0].1, 2);
    }

    #[test]
    fn test_element_from_spec() {
        let spec = UISpec {
            init: vec![InitQuery::Query("-R/test/-/add".to_string())],
            menu: None,
            layout: LayoutSpec::Horizontal,
        };
        let element = UISpecElement::from_spec("Test".to_string(), spec);
        assert_eq!(element.title(), "Test");
        assert_eq!(element.init_queries.len(), 1);
    }

    #[test]
    fn test_grid_auto_layout() {
        let num = 9;
        let r = (num as f64).sqrt().floor() as usize;
        assert_eq!(r, 3);
        let c = (num + r - 1) / r;
        assert_eq!(c, 3);
    }
}
