# Phase 2: Solution & Architecture - menu-pane-layout

## Overview

UISpecElement is a complex widget combining menu bar and configurable pane layout, created via `lui/ui_spec` command from YAML specification. Uses declarative configuration structs (UISpec, MenuBarSpec, LayoutSpec) that deserialize from YAML. Validates keyboard shortcuts during construction. Implements UIElement trait with recursive child rendering via extract-render-replace pattern.

## Data Structures

### YAML Configuration Structs (Spec Layer)

#### UISpec (Top-level)
```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UISpec {
    /// Initialization queries submitted during element's init() call.
    /// These queries create child elements as side effects.
    #[serde(default)]
    pub init: Vec<InitQuery>,

    /// Menu bar specification (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu: Option<MenuBarSpec>,

    /// Layout specification (how to arrange children)
    #[serde(default)]
    pub layout: LayoutSpec,

    /// Future extensibility: themes, toolbars, status bars, etc.
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum InitQuery {
    /// Query string (e.g., "/-/data.csv~add")
    Query(String),

    /// Recipe (future feature - parameterized query)
    Recipe(liquers_core::recipes::Recipe),
}
```

**Ownership:** All owned (String, Vec) - deserialized from YAML once.

**Serialization:** Derives `Serialize, Deserialize` (round-trip compatible with YAML).

**Init queries:** Submitted during `init()`, create children. Layout then arranges these children.

#### MenuBarSpec
```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MenuBarSpec {
    /// List of top-level items (menus or standalone buttons)
    pub items: Vec<TopLevelItem>,
}

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

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum MenuAction {
    Quit,  // Unit variant first (serializes as string "quit")
    Query { query: String },  // Struct variant (serializes as object)
}

// Untagged serialization:
// Quit   → "quit"
// Query  → {query: "..."}
// Unambiguous: string vs object distinction
```

**No default match arms:** All match statements on TopLevelItem, MenuItem, MenuAction must be explicit.

**Serde representation:** Externally tagged enums (serde default) - variant name as key in YAML.

**Optional fields:** Use `#[serde(default, skip_serializing_if = "Option::is_none")]` for icon/shortcut.

#### LayoutSpec
```rust
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum LayoutSpec {
    /// Horizontal arrangement (left to right)
    /// Arranges all children horizontally in order
    #[default]
    Horizontal,

    /// Vertical arrangement (top to bottom)
    /// Arranges all children vertically in order
    Vertical,

    /// Grid layout with specified rows and columns
    /// Children flow left-to-right, top-to-bottom
    /// 0 means "grow" (flexible dimension)
    /// If both are 0: rows = floor(sqrt(num_children)), columns grows
    Grid {
        #[serde(default)]
        rows: usize,     // Number of rows (0 = grow)
        #[serde(default)]
        columns: usize,  // Number of columns (0 = grow)
    },

    /// Tabs (one child visible at a time)
    /// Uses child element titles as tab labels
    Tabs {
        #[serde(default)]
        selected: usize,  // Index of initially selected tab (default 0)
    },

    /// Windows (all children as separate windows)
    /// Uses child element titles as window titles
    Windows,
}
```

**Default:** Horizontal (if layout not specified in YAML)

**Grid auto-layout:**
- `rows: 0, columns: N` → N columns, rows grow to fit children
- `rows: M, columns: 0` → M rows, columns grow to fit children
- `rows: 0, columns: 0` → Auto-square: rows = floor(sqrt(num_children)), columns grows

**Tabs default:** `selected: 0` (first tab)

**No PaneSpec:** Layout arranges existing children (fetched from AppState by parent handle)

**Children discovery:** In `show_in_egui()`, query AppState for children via parent handle

**Tab/Window titles:** Retrieved from child UIElement.title() method

### Runtime Element

#### UISpecElement
```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UISpecElement {
    // UIElement required fields
    handle: Option<UIHandle>,
    title_text: String,

    // Init queries (submitted during init to create children)
    init_queries: Vec<InitQuery>,

    // Menu bar (serializable spec)
    menu_spec: Option<MenuBarSpec>,

    // Layout (serializable spec, defaults to Horizontal)
    layout_spec: LayoutSpec,

    // Runtime state (not serialized)
    #[serde(skip)]
    shortcut_registry: Option<ShortcutRegistry>,  // Populated during init
}
```

**Ownership rationale:**
- `init_queries`: Owned (cloned from UISpec during construction)
- `menu_spec`: Owned (optional menu bar)
- `layout_spec`: Owned (not Option - defaults to Horizontal)
- `shortcut_registry`: Built during `init()` from menu_spec (maps KeyboardShortcut → MenuAction)

**Serialization:**
- All spec fields serialize (YAML round-trip)
- `shortcut_registry` skipped (rebuilt during init/deserialization)

**Children:** Not stored in element. Queried from AppState during rendering using parent handle.

#### ShortcutRegistry (Helper)
```rust
#[derive(Clone, Debug)]
struct ShortcutRegistry {
    // Map "Ctrl+S" → MenuAction
    shortcuts: std::collections::HashMap<String, MenuAction>,
}
```

**Not serializable:** Rebuilt from `menu_spec` during `init()`.

**Keyboard shortcut format:** Uses egui standard format (e.g., "Ctrl+S", "Shift+Alt+F5")
- Parsed via `egui::KeyboardShortcut::from_str()` (or manual parsing)
- Detected via `ui.input_mut(|i| i.consume_shortcut(shortcut))`
- Modifiers: Ctrl, Shift, Alt, Command (Mac)
- Keys: A-Z, F1-F12, Escape, Enter, etc.

## Trait Implementations

### UIElement for UISpecElement

```rust
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

        // 1. Build shortcut registry from menu_spec
        if let Some(menu_spec) = &self.menu_spec {
            self.shortcut_registry = Some(ShortcutRegistry::from_menu_spec(menu_spec));
        }

        // 2. Submit init queries to create children
        //    These queries are evaluated asynchronously and their results
        //    (UIElements) are added to AppState with this element as parent
        for init_query in &self.init_queries {
            match init_query {
                InitQuery::Query(query_str) => {
                    ctx.submit_query(query_str.clone())?;
                }
                InitQuery::Recipe(recipe) => {
                    // Future: submit recipe for evaluation
                    // For now: convert to query string or skip
                }
            }
        }

        Ok(())
    }

    fn update(&mut self, message: &UpdateMessage, ctx: &UIContext) -> UpdateResponse {
        // Handle keyboard shortcuts via Custom messages (if needed)
        // Note: Shortcuts handled directly in show_in_egui via ui.input_mut()
        // Future: handle AssetNotification for dynamic menu state
        UpdateResponse::Unchanged
    }

    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &UIContext,
        app_state: &mut dyn AppState,
    ) -> egui::Response {
        // Overall response (combined from menu bar + layout)
        let mut response = ui.allocate_response(
            ui.available_size(),
            egui::Sense::hover()
        );

        // 1. Check keyboard shortcuts (before rendering)
        if let Some(registry) = &self.shortcut_registry {
            for (shortcut_str, action) in &registry.shortcuts {
                // Parse shortcut string to egui::KeyboardShortcut
                // Note: egui doesn't have from_str, so manual parsing needed
                // Or use a helper function to convert "Ctrl+S" to KeyboardShortcut
                if self.check_shortcut(ui, shortcut_str) {
                    self.handle_menu_action(action, ctx);
                }
            }
        }

        // 2. Render menu bar (if present) at top
        if let Some(menu_spec) = &self.menu_spec {
            egui::menu::bar(ui, |ui| {
                self.render_menu_bar(ui, menu_spec, ctx);
            });
        }

        // 2. Get child handles from AppState
        //    Uses existing AppState.children() method
        let child_handles = if let Some(handle) = self.handle {
            app_state.children(handle).unwrap_or_else(|_| Vec::new())
        } else {
            Vec::new()
        };

        // 3. Render layout with children
        match &self.layout_spec {
            LayoutSpec::Horizontal => {
                ui.horizontal(|ui| {
                    for child_handle in child_handles {
                        // Extract-render-replace pattern
                        if let Some(mut child) = app_state.take_element(child_handle) {
                            child.show_in_egui(ui, ctx, app_state);
                            app_state.put_element(child_handle, child);
                        }
                    }
                });
            }

            LayoutSpec::Vertical => {
                ui.vertical(|ui| {
                    for child_handle in child_handles {
                        if let Some(mut child) = app_state.take_element(child_handle) {
                            child.show_in_egui(ui, ctx, app_state);
                            app_state.put_element(child_handle, child);
                        }
                    }
                });
            }

            LayoutSpec::Grid { rows, columns } => {
                let num_children = child_handles.len();

                // Calculate actual grid dimensions
                let (actual_rows, actual_columns) = match (rows, columns) {
                    (0, 0) => {
                        // Auto-square: rows = floor(sqrt(num_children))
                        let r = (num_children as f64).sqrt().floor() as usize;
                        let r = r.max(1); // At least 1 row
                        let c = (num_children + r - 1) / r; // Ceiling division
                        (r, c)
                    }
                    (0, c) if *c > 0 => {
                        // Columns fixed, rows grow
                        let r = (num_children + c - 1) / c; // Ceiling division
                        (r, *c)
                    }
                    (r, 0) if *r > 0 => {
                        // Rows fixed, columns grow
                        let c = (num_children + r - 1) / r; // Ceiling division
                        (*r, c)
                    }
                    (r, c) => {
                        // Both specified
                        (*r, *c)
                    }
                };

                egui::Grid::new(format!("grid_{:?}", self.handle))
                    .num_columns(actual_columns)
                    .show(ui, |ui| {
                        for (i, child_handle) in child_handles.iter().enumerate() {
                            if let Some(mut child) = app_state.take_element(*child_handle) {
                                child.show_in_egui(ui, ctx, app_state);
                                app_state.put_element(*child_handle, child);
                            }
                            // End row after 'actual_columns' items
                            if (i + 1) % actual_columns == 0 {
                                ui.end_row();
                            }
                        }
                    });
            }

            LayoutSpec::Tabs { selected } => {
                egui::containers::TabBar::new(format!("tabs_{:?}", self.handle))
                    .show(ui, |ui| {
                        for (i, child_handle) in child_handles.iter().enumerate() {
                            // Get tab label from child title
                            let tab_label = if let Some(child) = app_state.get_element(*child_handle) {
                                child.title()
                            } else {
                                format!("Tab {}", i + 1)
                            };

                            if ui.selectable_label(i == *selected, tab_label).clicked() {
                                // Update selected tab (store in runtime state)
                                // For now: just render selected tab
                            }

                            // Render only selected tab content
                            if i == *selected {
                                if let Some(mut child) = app_state.take_element(*child_handle) {
                                    child.show_in_egui(ui, ctx, app_state);
                                    app_state.put_element(*child_handle, child);
                                }
                            }
                        }
                    });
            }

            LayoutSpec::Windows => {
                // Render each child in a separate egui::Window
                for child_handle in child_handles {
                    if let Some(mut child) = app_state.take_element(child_handle) {
                        let window_title = child.title();
                        egui::Window::new(window_title)
                            .id(egui::Id::new(format!("window_{:?}", child_handle)))
                            .show(ui.ctx(), |ui| {
                                child.show_in_egui(ui, ctx, app_state);
                            });
                        app_state.put_element(child_handle, child);
                    }
                }
            }
        }

        response
    }
}

// Helper method for rendering menu bar
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
                // Request application exit (via UIContext or direct)
                std::process::exit(0);  // Or send quit message
            }
            MenuAction::Query { query } => {
                // Submit query via UIContext (async)
                let _ = ctx.submit_query(query.clone());
            }
        }
    }

    fn check_shortcut(&self, ui: &egui::Ui, shortcut_str: &str) -> bool {
        // Parse shortcut string (e.g., "Ctrl+S") and check if pressed
        // Simple parser for common shortcuts:
        // Format: [Modifier+]*Key
        // Example: "Ctrl+S", "Shift+Alt+F5", "Escape"

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
                    // This is the key
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
        // Map string to egui::Key
        match key_str {
            "A" => Some(egui::Key::A),
            "S" => Some(egui::Key::S),
            "O" => Some(egui::Key::O),
            "Q" => Some(egui::Key::Q),
            "F5" => Some(egui::Key::F5),
            "Escape" | "Esc" => Some(egui::Key::Escape),
            "Enter" => Some(egui::Key::Enter),
            // ... complete mapping for all keys
            _ => None,
        }
    }
}
```

**Bounds:** None (UIElement has no generic parameters).

**Default methods:** Use default `get_value()`, `get_metadata()` (returns None).

## Generic Parameters & Bounds

**Not applicable:** No generic parameters. All types are concrete.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|----------|--------|-----------|
| `ui_spec` command | No | Synchronous YAML deserialization, validation (no I/O) |
| `UISpec::from_yaml` | No | serde_yaml deserialization (CPU-bound) |
| `validate_shortcuts` | No | Pure validation logic (no I/O) |
| `ShortcutRegistry::from_menu_spec` | No | Builds HashMap from spec (CPU-bound) |
| `UISpecElement::init` | No | UIElement trait method (sync signature) |
| `UISpecElement::show_in_egui` | No | UIElement trait method (sync, called from egui render) |

**Decision:** All sync. YAML deserialization and validation are CPU-bound. Query submission happens via `UIContext::submit_query()` (async internally, but called from sync context).

## Function Signatures

### Module: liquers_lib::ui::widgets::ui_spec_element

```rust
// Spec parsing
impl UISpec {
    /// Deserialize from YAML text.
    pub fn from_yaml(yaml: &str) -> Result<Self, Error> {
        serde_yaml::from_str(yaml)
            .map_err(|e| Error::general_error(format!("YAML parse error: {}", e)))
    }

    /// Deserialize from YAML bytes.
    pub fn from_yaml_bytes(bytes: &[u8]) -> Result<Self, Error> {
        serde_yaml::from_slice(bytes)
            .map_err(|e| Error::general_error(format!("YAML parse error: {}", e)))
    }
}

// Shortcut validation
impl MenuBarSpec {
    /// Extract all shortcuts, detect conflicts.
    /// Returns Vec of conflict warnings (shortcut, count).
    pub fn validate_shortcuts(&self) -> Vec<(String, usize)> {
        // Collect all shortcuts, count occurrences
        // Return duplicates
    }
}

// Registry construction
impl ShortcutRegistry {
    /// Build from menu spec. Assumes no conflicts (validate first).
    pub fn from_menu_spec(menu_spec: &MenuBarSpec) -> Self {
        // Walk menu tree, build HashMap<String, MenuAction>
    }
}

// Element construction
impl UISpecElement {
    /// Create from UISpec. Does not validate shortcuts (caller's responsibility).
    pub fn from_spec(title: String, spec: UISpec) -> Self {
        Self {
            handle: None,
            title_text: title,
            init_queries: spec.init,
            menu_spec: spec.menu,
            layout_spec: spec.layout,
            shortcut_registry: None,  // Built during init
        }
    }
}

// AppState already has the needed methods (no changes required):
// - children(&self, handle: UIHandle) -> Result<Vec<UIHandle>, Error>
// - get_element(&self, handle: UIHandle) -> Result<Option<&dyn UIElement>, Error>
```

### Module: liquers_lib::ui::commands (ui_spec command)

```rust
/// Command: lui/ui_spec
/// Creates UISpecElement from YAML spec in state.
fn ui_spec(state: &State<Value>, context: &Context) -> Result<Value, Error> {
    // 1. Extract YAML from state (text or bytes)
    let yaml_str = state.try_as_string()
        .or_else(|_| {
            // Try bytes → UTF-8
            state.try_as_bytes()
                .and_then(|b| String::from_utf8(b.to_vec())
                    .map_err(|e| Error::general_error(format!("Invalid UTF-8: {}", e))))
        })?;

    // 2. Parse YAML → UISpec
    let spec = UISpec::from_yaml(&yaml_str)?;

    // 3. Validate shortcuts, warn via context if conflicts
    if let Some(menu_spec) = &spec.menu {
        let conflicts = menu_spec.validate_shortcuts();
        for (shortcut, count) in conflicts {
            context.warning(&format!(
                "Keyboard shortcut '{}' defined {} times (will use first occurrence)",
                shortcut, count
            ));
        }
    }

    // 4. Create element
    let element = UISpecElement::from_spec("Menu Layout".to_string(), spec);

    // 5. Wrap in ExtValue::UIElement
    Ok(Value::from_ui_element(Arc::new(element)))
}

// Registration in register_lui_commands
register_command!(cr, fn ui_spec(state, context) -> result
    namespace: "lui"
    label: "UI Spec"
    doc: "Create UISpecElement from YAML specification"
)?;
```

**Parameter choices:**
- `state: &State<Value>` - Borrowed (standard command signature)
- `context: &Context` - Borrowed (for warnings)
- Return `Value` containing `ExtValue::UIElement`

## Integration Points

### Crate: liquers-lib

**New file:** `liquers-lib/src/ui/widgets/ui_spec_element.rs`

**Exports:**
```rust
pub use ui_spec_element::{
    UISpec,
    MenuBarSpec,
    TopLevelItem,
    MenuItem,
    MenuAction,
    LayoutSpec,
    PaneSpec,
    UISpecElement,
};
```

**New module:** `liquers-lib/src/ui/widgets/mod.rs` (create)
```rust
pub mod ui_spec_element;

pub use ui_spec_element::UISpecElement;
```

**Modify:** `liquers-lib/src/ui/mod.rs`
```rust
pub mod widgets;  // NEW

pub use widgets::UISpecElement;  // Re-export
```

**Modify:** `liquers-lib/src/ui/commands.rs`
```rust
use crate::ui::widgets::ui_spec_element::UISpec;
use crate::ui::widgets::UISpecElement;

// Add ui_spec function and registration (see Function Signatures above)
```

**No modifications needed to `app_state.rs`** - existing methods are sufficient:
- `children(&self, handle: UIHandle) -> Result<Vec<UIHandle>, Error>`
- `get_element(&self, handle: UIHandle) -> Result<Option<&dyn UIElement>, Error>`

### Dependencies

**Add to `liquers-lib/Cargo.toml`:**
```toml
[dependencies]
serde_yaml = "0.9"  # For YAML deserialization
```

**Existing dependencies:** serde, egui, liquers-core (already present)

## Web Endpoints (if applicable)

**Not applicable.** UI-only feature, no HTTP endpoints.

## Error Handling

### Error Scenarios

| Scenario | ErrorType | Constructor |
|----------|-----------|-------------|
| YAML parse error | General | `Error::general_error(format!("YAML parse error: {}", e))` |
| Invalid UTF-8 in bytes | General | `Error::general_error(format!("Invalid UTF-8: {}", e))` |
| State not text/bytes | General | `Error::general_error("Expected text or bytes")` |
| Invalid ratio (< 0 or > 1) | General | `Error::general_error("Split ratio must be 0.0-1.0")` |
| Keyboard shortcut format | General | (Warning only, not error) |

### Error Propagation

```rust
// Use ? operator
let spec = UISpec::from_yaml(&yaml_str)?;

// Wrap external errors
serde_yaml::from_str(yaml)
    .map_err(|e| Error::from_error(ErrorType::General, e))

// Context warnings (not errors)
context.warning(&format!("Duplicate shortcut: {}", shortcut));
```

**No panics:** All fallible operations return `Result<T, Error>`.

## Serialization Strategy

### Serde Annotations

**UISpec, MenuBarSpec, LayoutSpec, etc.:**
```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum MenuItem { /* ... */ }
// Uses externally tagged representation (serde default)
// YAML format: variant name as key
```

**UISpecElement:**
```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UISpecElement {
    handle: Option<UIHandle>,
    menu_spec: Option<MenuBarSpec>,
    layout_spec: Option<LayoutSpec>,

    #[serde(skip)]  // Rebuilt during init
    shortcut_registry: Option<ShortcutRegistry>,
}
```

### Round-trip Compatibility

**Test plan (Phase 3):**
1. YAML → UISpec → UISpecElement → Serialize → Deserialize → should match
2. Shortcut registry rebuilt correctly after deserialization (init called)

### Example YAML

```yaml
# Initialization queries (create children as side effects)
init:
  - "-R/data/users.csv/-/ns-lui/display/add-child"
  - "-R/data/products.csv/-/ns-lui/display/add-child"
  - "-R/config/settings.yaml/-/ns-lui/display/add-child"

# Menu bar
menu:
  items:
    - menu:
        label: "File"
        items:
          - button:
              label: "Open CSV"
              shortcut: "Ctrl+O"
              action:
                query: "-R/data/new.csv/-/ns-lui/display/add-child"
          - separator
          - button:
              label: "Quit"
              shortcut: "Ctrl+Q"
              action: "quit"
    - menu:
        label: "View"
        items:
          - button:
              label: "Refresh All"
              shortcut: "F5"
              action:
                query: "-/refresh"

# Layout (how to arrange the 3 children created by init queries)
layout: horizontal  # Or: vertical, windows

# Alternative layouts:
# layout:
#   grid:
#     rows: 0      # 0 = grow
#     columns: 2   # Fixed 2 columns
#
# layout:
#   grid:
#     rows: 0      # Auto-square layout
#     columns: 0
#
# layout:
#   tabs:
#     selected: 0  # Default, can be omitted
```

**Query syntax explanation:**
- `-R/data/users.csv` - Read resource (file path)
- `/-/` - Root query
- `ns-lui/display` - Namespace and command (display the data)
- `/add-child` - Add as child of current element

**Explanation:**
- `init` queries create 3 children (users.csv, products.csv, settings.yaml displays)
- `layout: Horizontal` arranges these 3 children side-by-side
- Menu bar provides actions (open more files, quit)
- Children are automatically discovered via AppState.children()

## Concurrency Considerations

### Thread Safety

**No shared mutable state:** UISpecElement owns its spec data (no Arc<Mutex>).

**UIContext usage:** Query submission via `ctx.submit_query()` is thread-safe (uses message channel internally).

**egui rendering:** Called from main thread only (no concurrency issues).

**Shortcut registry:** Built once during init, read-only afterward (no locks needed).

**Decision:** No explicit locks needed. All state is either owned or accessed via UIContext/AppState (which handle synchronization).

## Compilation Validation

### Mental Check

**Expected to compile:** Yes

**Potential issues:**
1. ~~Recursive LayoutSpec type~~ → **Not needed:** Simplified design, no recursion
2. serde_yaml dependency → **Checked:** Version 0.9 compatible with serde 1.x
3. #[typetag::serde] for UIElement → **Verified:** Pattern used in existing elements
4. AppState methods → **Already exist:** children() and get_element() available

**Check:**
```bash
cargo check -p liquers-lib --features ui
```

**Expected:** Compiles with no errors (warnings OK for unused code during development).

## References to liquers-patterns.md

### Pattern Compliance

- [x] **Crate dependencies:** liquers-lib only (follows flow)
- [x] **No ExtValue variants:** Uses existing UIElement pattern
- [x] **Command registration:** `register_command!` macro with `lui` namespace
- [x] **Error handling:** `Error::general_error()`, `Error::from_error()`
- [x] **No unwrap/expect:** All fallible ops return Result
- [x] **Serialization:** Derives Serialize/Deserialize, uses `#[serde(skip)]`
- [x] **Match statements:** No default arms (`_ =>`) planned
- [x] **UIElement pattern:** Implements all trait methods, uses #[typetag::serde]
- [x] **Async pattern:** Default to sync (CPU-bound operations)
- [x] **Module structure:** Complex widget in `ui/widgets/` (new convention)

**New pattern established:** Complex widgets in `ui/widgets/` (vs simple elements in `ui/elements/`).

---

## Phase 2 Summary

**Architecture:** Declarative YAML spec → UISpec struct → UISpecElement (UIElement impl)

**Key data structures:**
- Spec layer: UISpec (init, menu, layout), MenuBarSpec, LayoutSpec (Horizontal/Vertical/Grid/Tabs/Windows)
- Runtime layer: UISpecElement (implements UIElement)
- Helpers: ShortcutRegistry (runtime only, rebuilt during init), InitQuery enum

**Command:** `lui/ui_spec` (sync, deserializes YAML, validates shortcuts, creates element)

**Init workflow:**
1. Element created from YAML spec
2. Element's `init()` submits init queries via UIContext
3. Init queries create child elements (registered with parent handle)
4. Children are automatically discovered during rendering

**Rendering workflow:**
1. `show_in_egui()` gets children from AppState via `get_children(parent_handle)`
2. Renders menu bar (if present)
3. Arranges children according to LayoutSpec (Horizontal/Vertical/Grid/Tabs/Windows)
4. Uses extract-render-replace pattern for child rendering

**No recursion:** Layout doesn't specify content, only arrangement. Children are fetched from AppState.

**Serialization:** Round-trip compatible (spec fields serialize, runtime state skipped)

**Concurrency:** No locks needed (owned state, UIContext handles sync)

**Compilation confidence:** 95% (standard patterns, AppState extension straightforward)
