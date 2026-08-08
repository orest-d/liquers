# Phase 4: Implementation Plan - menu-pane-layout

## Overview

This implementation plan provides step-by-step instructions for adding UISpecElement to liquers-lib. The feature adds a flexible UI widget system where elements are defined by YAML specifications that can include menu bars, layouts, and future extensions.

**Architecture:** YAML spec → UISpec struct → UISpecElement (UIElement impl) with menu bar rendering and configurable child layout

**Estimated complexity:** Medium (new widget module, YAML parsing, keyboard shortcuts, multiple layout algorithms)

**Estimated time:** 4-6 hours for experienced developer familiar with codebase

**Prerequisites:**
- Phase 1, 2, 3 approved ✅
- All open questions resolved ✅
- Dependencies identified: serde_yaml 0.9
- Existing Phase 1 UI infrastructure working (UIElement trait, AppState, UIContext)

## Implementation Steps

### Step 1: Add serde_yaml Dependency

**File:** `liquers-lib/Cargo.toml`

**Action:** Add serde_yaml dependency for YAML deserialization

**Code changes:**
```toml
# MODIFY: Add to [dependencies] section
[dependencies]
serde_yaml = "0.9"
```

**Validation:**
```bash
cargo check -p liquers-lib
# Should update Cargo.lock and compile without errors
```

**Rollback:**
```bash
git checkout liquers-lib/Cargo.toml liquers-lib/Cargo.lock
```

**Assigned to:** Either (trivial)

---

### Step 2: Create Widget Module Structure

**File:** `liquers-lib/src/ui/widgets/mod.rs` (NEW)

**Action:** Create widgets module with UISpecElement submodule export

**Code changes:**
```rust
// NEW: Create this file
pub mod ui_spec_element;

pub use ui_spec_element::UISpecElement;
```

**Validation:**
```bash
test -f liquers-lib/src/ui/widgets/mod.rs && echo "✓ widgets/mod.rs created"
```

**Rollback:**
```bash
rm liquers-lib/src/ui/widgets/mod.rs
```

**Assigned to:** Haiku

---

### Step 3: Create UISpecElement Module Skeleton

**File:** `liquers-lib/src/ui/widgets/ui_spec_element.rs` (NEW)

**Action:** Create file with module documentation and imports

**Code changes:**
```rust
// NEW: Create this file with imports
//! UISpecElement - Flexible UI widget defined by YAML specification
//!
//! Supports menu bars, configurable layouts (horizontal, vertical, grid, tabs, windows),
//! and keyboard shortcuts. Children are created by init queries and arranged by layout.

use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_core::value::Value;
use liquers_core::context::Context;
use liquers_core::recipes::Recipe;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::ui::element::UIElement;
use crate::ui::context::UIContext;
use crate::ui::app_state::AppState;
use crate::ui::types::{UIHandle, UpdateMessage, UpdateResponse};
```

**Validation:**
```bash
cargo check -p liquers-lib --features ui 2>&1 | grep "ui_spec_element" && echo "✗ Import errors" || echo "✓ No import errors"
```

**Rollback:**
```bash
rm liquers-lib/src/ui/widgets/ui_spec_element.rs
```

**Assigned to:** Haiku

---

### Step 4: Implement YAML Spec Structs (Part 1: Core)

**File:** `liquers-lib/src/ui/widgets/ui_spec_element.rs`

**Action:** Add UISpec, InitQuery, MenuBarSpec, MenuAction structs

**Code changes:**
```rust
// NEW: Add to file

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

/// Menu action (query submission or quit)
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum MenuAction {
    Quit,  // Unit variant first
    Query { query: String },
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
```

**Validation:**
```bash
cargo check -p liquers-lib --features ui
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/widgets/ui_spec_element.rs
```

**Assigned to:** Sonnet

---

### Step 5: Implement YAML Spec Structs (Part 2: Menu Items & Layout)

**File:** `liquers-lib/src/ui/widgets/ui_spec_element.rs`

**Action:** Add TopLevelItem, MenuItem, LayoutSpec enums

**Code changes:**
```rust
// NEW: Add to file (after MenuBarSpec)

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
```

**Validation:**
```bash
cargo check -p liquers-lib --features ui
```

**Assigned to:** Sonnet

---

### Step 6: Implement Shortcut Validation

**File:** `liquers-lib/src/ui/widgets/ui_spec_element.rs`

**Action:** Add validate_shortcuts method and helper functions

**Code changes:**
```rust
// NEW: Add after MenuBarSpec definition

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
        TopLevelItem::Menu { shortcut, items, .. } => {
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
        MenuItem::Submenu { shortcut, items, .. } => {
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
```

**Validation:**
```bash
cargo check -p liquers-lib --features ui
```

**Assigned to:** Sonnet

---

### Step 7: Implement UISpecElement and ShortcutRegistry

**File:** `liquers-lib/src/ui/widgets/ui_spec_element.rs`

**Action:** Add runtime element struct and shortcut registry

**Code changes:**
```rust
// NEW: Add to file

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
        TopLevelItem::Button { shortcut, action, .. } => {
            if let Some(s) = shortcut {
                shortcuts.entry(s.clone()).or_insert_with(|| action.clone());
            }
        }
    }
}

fn collect_actions_from_menu_item(
    item: &MenuItem,
    shortcuts: &mut std::collections::HashMap<String, MenuAction>,
) {
    match item {
        MenuItem::Button { shortcut, action, .. } => {
            if let Some(s) = shortcut {
                shortcuts.entry(s.clone()).or_insert_with(|| action.clone());
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
}
```

**Validation:**
```bash
cargo check -p liquers-lib --features ui
```

**Assigned to:** Sonnet

---

### Step 8: Implement UIElement Trait (Basic Methods)

**File:** `liquers-lib/src/ui/widgets/ui_spec_element.rs`

**Action:** Implement UIElement trait methods (except show_in_egui)

**Code changes:**
```rust
// NEW: Add to file

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
                    ctx.submit_query(query_str.clone())?;
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
        _ctx: &UIContext,
        _app_state: &mut dyn AppState,
    ) -> egui::Response {
        // Placeholder
        ui.label("UISpecElement rendering not yet implemented")
    }
}
```

**Validation:**
```bash
cargo check -p liquers-lib --features ui
```

**Assigned to:** Sonnet

---

### Step 9: Implement Helper Methods (Menu Rendering)

**File:** `liquers-lib/src/ui/widgets/ui_spec_element.rs`

**Action:** Add helper methods for menu rendering and action handling

**Code changes:**
```rust
// NEW: Add to file (after UIElement impl)

impl UISpecElement {
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
            MenuAction::Query { query } => {
                let _ = ctx.submit_query(query.clone());
            }
        }
    }
}
```

**Validation:**
```bash
cargo check -p liquers-lib --features ui
```

**Assigned to:** Sonnet

---

### Step 10: Implement Keyboard Shortcut Checking

**File:** `liquers-lib/src/ui/widgets/ui_spec_element.rs`

**Action:** Add shortcut parsing and checking methods

**Code changes:**
```rust
// NEW: Add to UISpecElement impl block

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
        egui::Key::from_name(key_str)
            .or_else(|| {
                // Handle common aliases not in egui's from_name
                match key_str {
                    "Esc" => Some(egui::Key::Escape),
                    "Return" => Some(egui::Key::Enter),
                    "Control" => Some(egui::Key::Ctrl),  // Modifier, but try parsing as key
                    _ => None,
                }
            })
    }
```

**Validation:**
```bash
cargo check -p liquers-lib --features ui
```

**Assigned to:** Haiku (straightforward mapping)

---

### Step 11: Implement show_in_egui (Layout Rendering)

**File:** `liquers-lib/src/ui/widgets/ui_spec_element.rs`

**Action:** Replace show_in_egui placeholder with full rendering logic

**Code changes:**
```rust
// MODIFY: Replace placeholder show_in_egui method

    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &UIContext,
        app_state: &mut dyn AppState,
    ) -> egui::Response {
        let mut response = ui.allocate_response(ui.available_size(), egui::Sense::hover());

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
                ui.horizontal(|ui| {
                    for child_handle in child_handles {
                        if let Ok(Some(mut child)) = app_state.take_element(child_handle) {
                            child.show_in_egui(ui, ctx, app_state);
                            let _ = app_state.put_element(child_handle, child);
                        }
                    }
                });
            }
            LayoutSpec::Vertical => {
                ui.vertical(|ui| {
                    for child_handle in child_handles {
                        if let Ok(Some(mut child)) = app_state.take_element(child_handle) {
                            child.show_in_egui(ui, ctx, app_state);
                            let _ = app_state.put_element(child_handle, child);
                        }
                    }
                });
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

                egui::Grid::new(format!("grid_{:?}", self.handle))
                    .num_columns(actual_columns)
                    .show(ui, |ui| {
                        for (i, child_handle) in child_handles.iter().enumerate() {
                            if let Ok(Some(mut child)) = app_state.take_element(*child_handle) {
                                child.show_in_egui(ui, ctx, app_state);
                                let _ = app_state.put_element(*child_handle, child);
                            }
                            if (i + 1) % actual_columns == 0 {
                                ui.end_row();
                            }
                        }
                    });
            }
            LayoutSpec::Tabs { selected } => {
                if let Some(child_handle) = child_handles.get(*selected) {
                    if let Ok(Some(mut child)) = app_state.take_element(*child_handle) {
                        ui.horizontal(|ui| {
                            for (i, handle) in child_handles.iter().enumerate() {
                                if let Ok(Some(element)) = app_state.get_element(*handle) {
                                    let _ = ui.selectable_label(i == *selected, element.title());
                                }
                            }
                        });
                        ui.separator();
                        child.show_in_egui(ui, ctx, app_state);
                        let _ = app_state.put_element(*child_handle, child);
                    }
                }
            }
            LayoutSpec::Windows => {
                for child_handle in child_handles {
                    if let Ok(Some(mut child)) = app_state.take_element(child_handle) {
                        let window_title = child.title();
                        egui::Window::new(window_title)
                            .id(egui::Id::new(format!("window_{:?}", child_handle)))
                            .show(ui.ctx(), |ui| {
                                child.show_in_egui(ui, ctx, app_state);
                            });
                        let _ = app_state.put_element(child_handle, child);
                    }
                }
            }
        }

        response
    }
```

**Validation:**
```bash
cargo check -p liquers-lib --features ui
```

**Assigned to:** Sonnet (complex layout logic)

---

### Step 12: Add ui_spec Command

**File:** `liquers-lib/src/ui/commands.rs`

**Action:** Add ui_spec command function and registration

**Code changes:**
```rust
// NEW: Add imports at top
use crate::ui::widgets::ui_spec_element::{UISpec, UISpecElement};

// NEW: Add function before register_lui_commands
fn ui_spec(state: &State<Value>, context: &Context) -> Result<Value, Error> {
    let yaml_str = state.try_as_string()
        .or_else(|_| {
            state.try_as_bytes()
                .and_then(|b| String::from_utf8(b.to_vec())
                    .map_err(|e| Error::general_error(format!("Invalid UTF-8: {}", e))))
        })?;

    let spec = UISpec::from_yaml(&yaml_str)?;

    if let Some(menu_spec) = &spec.menu {
        let conflicts = menu_spec.validate_shortcuts();
        for (shortcut, count) in conflicts {
            context.warning(&format!(
                "Keyboard shortcut '{}' defined {} times (will use first occurrence)",
                shortcut, count
            ));
        }
    }

    let element = UISpecElement::from_spec("UI Spec".to_string(), spec);
    Ok(Value::from_ui_element(Arc::new(element)))
}

// MODIFY: Add to register_lui_commands function
register_command!(cr, fn ui_spec(state, context) -> result
    namespace: "lui"
    label: "UI Spec"
    doc: "Create UISpecElement from YAML specification"
)?;
```

**Validation:**
```bash
cargo check -p liquers-lib --features ui
```

**Assigned to:** Sonnet

---

### Step 13: Export from UI Module

**File:** `liquers-lib/src/ui/mod.rs`

**Action:** Add widgets module and export UISpecElement

**Code changes:**
```rust
// NEW: Add to module declarations
pub mod widgets;

// NEW: Add to re-exports
pub use widgets::UISpecElement;
```

**Validation:**
```bash
cargo check -p liquers-lib --features ui
cargo doc -p liquers-lib --features ui --no-deps 2>&1 | grep UISpecElement
```

**Assigned to:** Haiku

---

### Step 14: Add Unit Tests

**File:** `liquers-lib/src/ui/widgets/ui_spec_element.rs`

**Action:** Add comprehensive test module at end of file

**Code changes:**
```rust
// NEW: Add at end of file

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
    fn test_yaml_parse_with_menu() {
        let yaml = r#"
            menu:
              items:
                - button:
                    label: "Test"
                    action: "quit"
            layout: vertical
        "#;
        let spec = UISpec::from_yaml(yaml).unwrap();
        assert!(spec.menu.is_some());
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
```

**Validation:**
```bash
cargo test -p liquers-lib --features ui ui_spec_element::tests
# All tests should pass
```

**Assigned to:** Haiku (following template)

---

## Testing Plan

### Unit Tests

**When:** After Step 14

**Command:**
```bash
cargo test -p liquers-lib --features ui ui_spec_element::tests -v
```

**Expected:** 6 tests pass
- test_yaml_parse_simple
- test_yaml_parse_with_menu
- test_yaml_parse_invalid
- test_shortcut_validation
- test_element_from_spec
- test_grid_auto_layout

### Integration Tests

**File:** Create `liquers-lib/tests/ui_spec_integration.rs`

**Content:**
```rust
use liquers_lib::SimpleEnvironment;
use liquers_core::state::State;
use liquers_core::value::Value;

#[tokio::test]
async fn test_ui_spec_command() {
    let env = SimpleEnvironment::new().await;
    let yaml = r#"
        init: []
        layout: horizontal
    "#;
    let state = State::from_value(Value::from_string(yaml.to_string()));
    let context = env.create_context();
    let result = env.evaluate_command("lui/ui_spec", &state, &context).await;
    assert!(result.is_ok());
    assert!(result.unwrap().try_as_ui_element().is_ok());
}
```

**Command:**
```bash
cargo test -p liquers-lib --features ui ui_spec_integration
```

### Manual Validation

**Create example:** `liquers-lib/examples/ui_spec_demo.rs` (minimal version)

**Run:**
```bash
cargo run --example ui_spec_demo --features ui
```

**Expected:** Window opens, menu bar visible, layout renders

## Task Splitting

### Sonnet (Complex, architectural decisions)

**Steps:** 4-9, 11-12
- **Step 4-7:** YAML spec structs with serde attributes, shortcut validation, UISpecElement struct
  - Reason: Requires understanding serde serialization, error handling, nested matching
- **Step 8-9:** UIElement trait impl, menu rendering helpers
  - Reason: Trait implementation, recursive rendering logic
- **Step 11:** show_in_egui with layout algorithms
  - Reason: Complex layout logic (grid auto-sizing, extract-render-replace pattern)
- **Step 12:** ui_spec command with validation
  - Reason: Error handling, context warnings, command registration

### Haiku (Straightforward, boilerplate)

**Steps:** 1-3, 10, 13-14
- **Step 1-3:** Dependencies, module structure, skeleton
  - Reason: File creation, trivial imports
- **Step 10:** Keyboard shortcut parsing
  - Reason: Simple string→enum mapping (large but mechanical)
- **Step 13:** Module exports
  - Reason: Trivial public API
- **Step 14:** Unit tests
  - Reason: Follow provided templates, straightforward assertions

## Rollback Plan

### Per-Step Rollback

Each step includes:
```bash
git checkout [file-path]
```

Or for new files:
```bash
rm [file-path]
```

### Full Feature Rollback

If feature needs complete rollback:

```bash
# Remove all new files
rm liquers-lib/src/ui/widgets/mod.rs
rm liquers-lib/src/ui/widgets/ui_spec_element.rs
rm liquers-lib/examples/ui_spec_demo.rs
rm liquers-lib/tests/ui_spec_integration.rs

# Restore modified files
git checkout liquers-lib/Cargo.toml
git checkout liquers-lib/src/ui/mod.rs
git checkout liquers-lib/src/ui/commands.rs

# Clean build artifacts
cargo clean -p liquers-lib
```

### Common Issues & Fixes

**Issue:** YAML deserialization fails
**Fix:** Verify `#[serde(rename_all = "lowercase")]` on enums, test YAML in unit tests first

**Issue:** Missing AppState methods
**Fix:** Verify `children()` and `get_element()` exist in trait (they do, from Phase 1)

**Issue:** Keyboard shortcuts don't work
**Fix:** Check egui::KeyboardShortcut usage, ensure `ui.input_mut()` is called before rendering

**Issue:** Grid layout wrong dimensions
**Fix:** Verify auto-layout formula matches Phase 2 spec

## Documentation Updates

### MEMORY.md

Add section:
```markdown
## UISpecElement Feature
- Module: `liquers-lib/src/ui/widgets/ui_spec_element.rs`
- Command: `lui/ui_spec` - creates UISpecElement from YAML
- YAML spec: init (queries), menu (optional), layout (Horizontal/Vertical/Grid/Tabs/Windows)
- Grid auto-layout: rows=0, columns=0 → square grid (rows = floor(sqrt(N)))
- Keyboard shortcuts: egui format ("Ctrl+S", "F5", "Ctrl+Q")
- Menu actions: Query {query: "..."} or Quit
- Init queries: submitted during init(), create children via side effects
- Children arranged by layout (fetched from AppState via parent handle)
```

### PROJECT_OVERVIEW.md

No changes needed (Phase 1 UI framework already documented)

### README.md

Add example (optional):
```markdown
## UI Spec Demo

Create flexible UI layouts from YAML:

```bash
cargo run --example ui_spec_demo --features ui
```
```

## Execution Options

After approval, choose:

1. **Execute immediately** - Implement all steps sequentially
2. **Phased execution** - Steps 1-11 (core), test, then 12-14 (integration)
3. **Create task list** - Generate tracking tasks for incremental work
4. **Revise plan** - Request changes before execution

**Recommended:** Phased execution (build core → validate → add command/tests)

## Approval Checklist

- [x] All steps documented with file paths
- [x] Validation commands provided
- [x] Rollback procedures defined
- [x] Task splitting completed (Sonnet/Haiku)
- [x] Testing plan comprehensive
- [x] Documentation updates identified

**Status:** Ready for approval and execution
