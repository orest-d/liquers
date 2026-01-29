# UI Interface Specification

## Overview

Query-driven UI state management enabling platform-specific rendering without hard dependencies in command code. UI structure, content, and events are defined as queries stored in a JSON-serializable application state.

## Core Philosophy

**Query-Driven State**: UI configuration is data, not code.
- UI content: query that produces the content
- Events: query to execute on trigger
- Layout: tree of elements, each with its own query

**Platform Independence**: Application state is platform-agnostic JSON. Platform-specific commands (in `liquers-egui`, `liquers-web`) read AppState and render natively using their frameworks.

## Key Concepts

**Application State**: JSON-serializable tree of UI elements. Global singleton accessible via `Context`. Stores window layout, active queries, widget state, and user preferences.

**UI Widget**: Value type representing renderable content with optional internal state. In general, widget state is not required to be serializable. Commands return widgets; platforms render them.
Widgets may be platform independent (renderable with multiple platforms) or platform-specific.

**UI Element**: Container in the AppState tree (window, pane, form) with:
- Unique ID (handle)
- Primary query producing widget content
- Layout type (border, grid, tabs, stack)
- Child elements
- Widget-specific state (JSON)

**Handle**: String ID referencing an element in AppState. Commands receive the current element's handle via `Context` and can navigate to parent/children/siblings.

## Application State Schema

**Location**: `liquers-lib/src/ui/app_state.rs`

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct AppState {
    pub elements: HashMap<String, UiElement>,
    pub root_windows: Vec<String>,  // Root element handles
    pub preferences: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UiElement {
    pub id: String,
    pub element_type: ElementType,
    pub query: Query,               // Query producing content
    pub layout: LayoutKind,
    pub children: Vec<String>,      // Child element handles
    pub parent: Option<String>,     // Parent element handle
    pub state: serde_json::Value,   // Widget-specific state
    pub events: HashMap<String, Query>,  // "click" -> query
}

#[derive(Serialize, Deserialize, Clone)]
pub enum ElementType {
    Window { title: String },
    Pane,
    Form,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum LayoutKind {
    Single,
    Border { center: String, top: Option<String>, /* ... */ },
    Grid { rows: usize, cols: usize },
    Tabs { active: usize },
}
```

## Handle Navigation

Commands access AppState and navigate via handles:

```rust
use std::borrow::Cow;

impl AppState {
    pub fn get(&self, handle: &str) -> Option<&UiElement>;
    pub fn get_mut(&mut self, handle: &str) -> Option<&mut UiElement>;
    pub fn parent(&self, handle: &str) -> Option<Cow<'_, str>>;
    pub fn children(&self, handle: &str) -> Vec<Cow<'_, str>>;
    pub fn sibling(&self, handle: &str, offset: isize) -> Option<Cow<'_, str>>;
    pub fn create_element(&mut self, parent: Option<&str>, element_type: ElementType, query: Query) -> Cow<'_, str>;
}
```

**Example**: Left pane query wants to update right pane.
```rust
fn update_sibling(context: &Context) -> Result<Value, Error> {
    let handle = context.current_element_handle()?;
    let app_state = context.get_app_state()?;

    // Navigation returns handles, no borrow conflicts
    let sibling_handle = app_state.sibling(&handle, 1)
        .ok_or_else(|| Error::general_error("No sibling found".to_string()))?;

    drop(app_state); // Release read lock
    let mut app_state = context.get_app_state_mut()?;
    let sibling_elem = app_state.get_mut(sibling_handle).unwrap();
    sibling_elem.query = parse_query("/data/updated/-/render_chart");

    Ok(Value::none())
}
```

## AppState Access Design

### Core Abstractions

Two-trait design with pluggable backends:

**Location**: `liquers-lib/src/ui/traits.rs`

```rust
use std::borrow::Cow;

/// Execution scope for UI element queries. Injected as payload.
pub trait UiScope: Send + Sync {
    fn current_element(&self) -> Option<Cow<'_, str>>;
    fn app_state_provider(&self) -> &dyn AppStateProvider;
}

/// Abstract provider for UI state access. Multiple implementations possible.
pub trait AppStateProvider: Send + Sync {
    fn get_element(&self, handle: &str) -> Result<Arc<RwLock<UiElement>>, Error>;
    fn get_preferences(&self) -> Result<Arc<RwLock<serde_json::Value>>, Error>;
    fn create_element(&self, parent: Option<&str>, element_type: ElementType, query: Query) -> Result<Cow<'_, str>, Error>;
    fn list_children(&self, handle: &str) -> Result<Vec<String>, Error>;
}
```

### Hierarchical Asset Implementation

**Location**: `liquers-lib/src/ui/asset_provider.rs`

Each UIElement is a separate asset in a hierarchical structure:

```rust
pub struct AssetAppStateProvider {
    asset_manager: Arc<AssetManager>,
    root_path: String,  // e.g., "/ui/elements"
}

impl AppStateProvider for AssetAppStateProvider {
    fn get_element(&self, handle: &str) -> Result<Arc<RwLock<UiElement>>, Error> {
        // Handle IS the key path: e.g., "/ui/elements/window1/left_pane"
        let asset = self.asset_manager.get(handle)?;

        // UIElement stored as Source/Override status asset (mutable)
        let value = asset.get_value()?;
        // Extract Arc<RwLock<UiElement>> from value
    }

    fn create_element(&self, parent: Option<&str>, element_type: ElementType, query: Query) -> Result<Cow<'_, str>, Error> {
        let handle = if let Some(parent) = parent {
            format!("{}/child_{}", parent, uuid::Uuid::new_v4())
        } else {
            format!("{}/window_{}", self.root_path, uuid::Uuid::new_v4())
        };

        let element = UiElement {
            id: handle.clone(),
            element_type,
            query,
            layout: LayoutKind::Single,
            children: vec![],
            parent: parent.map(String::from),
            state: serde_json::Value::Null,
            events: HashMap::new(),
        };

        // Create as Source status asset (mutable, persisted via cache)
        self.asset_manager.register_asset(
            &handle,
            Asset::new_source(Arc::new(RwLock::new(element)))
        )?;

        Ok(Cow::Owned(handle))
    }

    fn list_children(&self, handle: &str) -> Result<Vec<String>, Error> {
        // List all assets under handle path
        self.asset_manager.list_matching(&format!("{}/*", handle))
    }
}
```

**Key benefits:**
- **Keys as handles**: Handle is just the asset key path (e.g., `/ui/elements/window1/pane_left`)
- **Concurrency**: AssetManager's concurrent map allows multi-threaded access to different elements
- **Transparency**: UI structure visible as folder tree in asset inspection tools
- **Persistence**: Asset cache naturally persists desktop settings
- **No serialization**: Assets store `Arc<RwLock<UiElement>>` directly

### Direct Memory Implementation

For simpler cases without persistence/transparency needs:

```rust
pub struct DirectAppStateProvider {
    elements: Arc<RwLock<HashMap<String, Arc<RwLock<UiElement>>>>>,
    preferences: Arc<RwLock<serde_json::Value>>,
}

impl AppStateProvider for DirectAppStateProvider {
    fn get_element(&self, handle: &str) -> Result<Arc<RwLock<UiElement>>, Error> {
        self.elements.read().unwrap()
            .get(handle)
            .cloned()
            .ok_or_else(|| Error::general_error(format!("Element not found: {}", handle)))
    }

    // ... similar implementation without asset layer
}
```

**Use when**: Prototyping, testing, or when persistence/transparency not needed

### Usage Pattern

Commands access individual elements via handles:
```rust
fn update_sibling(context: &Context) -> Result<Value, Error> {
    let ui_scope = context.payload::<dyn UiScope>()?;
    let provider = ui_scope.app_state_provider();

    let handle = ui_scope.current_element()
        .ok_or_else(|| Error::general_error("No current element".to_string()))?;

    // Get parent path from handle (key navigation)
    let parent_handle = handle.rsplit_once('/')
        .map(|(parent, _)| parent)
        .ok_or(...)?;

    // List siblings
    let siblings = provider.list_children(parent_handle)?;
    let sibling_handle = siblings.get(1).ok_or(...)?;

    // Update sibling element
    let sibling = provider.get_element(sibling_handle)?;
    let mut elem = sibling.write().unwrap();
    elem.query = parse_query("/data/updated/-/render_chart");

    Ok(Value::none())
}
```

### Handle Navigation Helpers

**Location**: `liquers-lib/src/ui/navigation.rs`

```rust
/// Helper functions for navigating key-based handles
pub fn parent_handle(handle: &str) -> Option<&str> {
    handle.rsplit_once('/').map(|(parent, _)| parent)
}

pub fn sibling_handle(provider: &dyn AppStateProvider, handle: &str, offset: isize) -> Result<Option<String>, Error> {
    let parent = parent_handle(handle).ok_or(...)?;
    let siblings = provider.list_children(parent)?;
    let current_idx = siblings.iter().position(|s| s == handle).ok_or(...)?;
    let target_idx = (current_idx as isize + offset) as usize;
    Ok(siblings.get(target_idx).cloned())
}
```

### Backend Comparison

| Backend | Speed | Persistent | Transparent | Concurrency | Use Case |
|---------|-------|------------|-------------|-------------|----------|
| Direct | Fast | No | No | Coarse (single lock) | Prototype/testing |
| Asset | Fast | Yes (cache) | Yes (folder tree) | Fine-grained | Production desktop/web |

## Value Integration

**Location**: `liquers-lib/src/value/extended.rs`

Add widget value type:
```rust
pub enum ExtValue {
    // ...existing variants
    UiWidget(UiWidgetData),
}

#[derive(Clone)]
pub struct UiWidgetData {
    pub widget_type: String,        // "egui::Window", "html::div"
    pub content: serde_json::Value, // Platform-specific data
}
```

Commands return widgets; platform decides how to render:
```rust
fn render_table(state: &State<Value>) -> Result<Value, Error> {
    let df = state.try_into_dataframe()?;
    Ok(Value::from(UiWidgetData {
        widget_type: "dataframe".to_string(),
        content: serde_json::to_value(df)?,
    }))
}
```

## Event Handling

Events stored as queries in element state:
```rust
fn create_button(context: &Context) -> Result<Value, Error> {
    let handle = context.current_element_handle()?;
    let mut app_state = context.get_app_state_mut()?;

    let elem = app_state.get_mut(&handle).unwrap();
    elem.events.insert("click".to_string(), parse_query("/data/-/refresh"));

    Ok(Value::from(UiWidgetData {
        widget_type: "button".to_string(),
        content: json!({"label": "Refresh"}),
    }))
}
```

Platform-specific code executes the query when event fires.

## Use Cases

Use cases drive interface design by identifying common operations and interaction patterns.

### Use Case 1: Orthodox Commander/Explorer

**Goal**: Two-pane file browser for navigating key-based hierarchical structures (store/assets).

#### Initial Setup (Query-Driven)

Launch query:
```
/data/folder1/-/orthodox_commander
```

This creates:
- Parent container element (handle: auto-generated u64)
- Two child pane elements:
  - Left pane: `query = /data/folder1/-/orthodox_commander_list`
  - Right pane: `query = /data/folder1/-/orthodox_commander_list`
- Both panes start with same key (different keys possible via saved presets)

#### Rendering Pipeline

1. Each pane evaluates its query: `/data/folder1/-/orthodox_commander_list`
2. Query returns either:
   - **Direct**: `UiWidget` (DirectoryListWidget) → display immediately
   - **Indirect**: Other value (DataFrame, text) → apply generic "show" transformation → UiWidget
3. Widget renders in platform-specific manner (egui, web)

#### Widget Structure with Event Queries

DirectoryListWidget contains queries as event callbacks:

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct DirectoryListWidget {
    pub items: Vec<DirectoryItem>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DirectoryItem {
    pub name: String,
    pub key: String,           // e.g., "/data/folder1"
    pub is_directory: bool,
    pub on_click: Query,       // Query as event callback
}
```

Example items:
```rust
vec![
    DirectoryItem {
        name: "..".to_string(),
        key: "/data".to_string(),
        is_directory: true,
        on_click: parse_query("/data/-/orthodox_commander_list/-/set_current"),
    },
    DirectoryItem {
        name: "subfolder".to_string(),
        key: "/data/folder1/subfolder".to_string(),
        is_directory: true,
        on_click: parse_query("/data/folder1/subfolder/-/orthodox_commander_list/-/set_current"),
    },
    DirectoryItem {
        name: "file.csv".to_string(),
        key: "/data/folder1/file.csv".to_string(),
        is_directory: false,
        on_click: parse_query("/data/folder1/file.csv/-/view_file/-/set_sibling(1)"),
    },
]
```

#### Interaction Patterns

**1. Navigate folder (in same pane):**
- User clicks: "subfolder"
- Platform executes: `/data/folder1/subfolder/-/orthodox_commander_list/-/set_current`
- `set_current` command:
  - Accesses current element handle via payload
  - Updates current element's query to `/data/folder1/subfolder/-/orthodox_commander_list`
  - Triggers re-render of pane
- Result: Left pane displays subfolder contents

**2. Navigate up (parent directory):**
- User clicks: ".."
- Platform executes: `/data/-/orthodox_commander_list/-/set_current`
- Effect: Current pane's query updated to parent key
- Result: Pane displays parent directory

**3. View file (in sibling pane):**
- User clicks: "file.csv"
- Platform executes: `/data/folder1/file.csv/-/view_file/-/set_sibling(1)`
- `set_sibling(1)` command:
  - Accesses current element handle via payload
  - Navigates to sibling (offset +1)
  - Updates sibling element's query to `/data/folder1/file.csv/-/view_file`
  - Triggers re-render of sibling pane
- Result: Right pane displays file content (table/chart/text widget)

#### Operations Identified

- `set_current` - Update current element's query, trigger re-render
- `set_sibling(offset: isize)` - Update sibling element's query by offset

#### Design Notes

- **Queries as callbacks**: Event handlers are queries, not closures. Platform-independent, serializable.
- **Widget state persistence**: DirectoryListWidget is serializable. Can be saved as application preset (handles renumbered on import).
- **Fallback rendering**: If query returns non-widget value, apply generic "show" transformation.
- **Query chaining**: Event queries can chain multiple commands (e.g., `/key/-/process/-/update_target`)

#### Open Questions

- Query update semantics: Should updating element's query auto-trigger re-evaluation, or require explicit render command?
  - **Decision**: Probably auto-invalidate cached widget when query changes
- Navigation history: Back/undo functionality - per element, per window, or not at all?
  - **Deferred**: Different UI patterns need different semantics (explored in tab view use case)

#### Notes

- `-R-key` syntax: Key reference mechanism defined in `liquers-core/src/plan.rs`
- Show transformation: Implemented in `plan.rs` evaluation pipeline as Action/Evaluate steps

---

### Use Case 2: Tab View

**Goal**: Container with multiple independent views, only one visible at a time. Each tab can contain arbitrary UI structure (orthodox commander, single pane, dashboard, etc.).

#### Structure

```
TabContainer (handle: 100, active_child: 457)
  ├─ Tab 1 (handle: 456, query: /data/project1/-/orthodox_commander)
  ├─ Tab 2 (handle: 457, query: /data/project2/-/listdir)
  └─ Tab 3 (handle: 458, query: /reports/-/dashboard)
```

**Active tab state**: Stored in parent container (`active_child: 457`)

#### Initialization

Typically created via **preset** (saved UI configuration):

```json
{
  "element_type": "TabContainer",
  "layout": { "Tabs": { "active": 1 } },
  "children": [
    {
      "query": "/data/project1/-/orthodox_commander",
      "label": "Project 1"
    },
    {
      "query": "/data/project2/-/listdir",
      "label": "Project 2"
    },
    {
      "query": "/reports/-/dashboard",
      "label": "Reports"
    }
  ]
}
```

On load:
1. Create TabContainer element
2. Create child elements with specified queries
3. Set active tab index
4. Evaluate visible tab's query

#### Query vs Widget Duality

**Fundamental principle**: Each element stores both query (generative) and widget (cached result).

```rust
struct UiElement {
    handle: u64,
    query: Query,                    // How to produce content
    cached_widget: Option<Widget>,   // Last evaluation result
    // ...
}
```

**Evaluation flow:**
1. Element created with query
2. Query evaluated → produces widget
3. Widget cached in element
4. Subsequent renders use cached widget (unless invalidated)

**Preset Serialization (Hybrid Model)**:
- **Save**: Both query and widget
- **Load**: Restore widget if present, fallback to re-evaluating query
- **Widget serialization rules**:
  - ✅ Serialize: Identity (keys, config, layout preferences)
  - ❌ Don't serialize: Content (fetched data, computed results)
  - Widget responsible for self-refresh after deserialization

**Example - DirectoryListWidget serialization:**
```rust
#[derive(Serialize, Deserialize)]
struct DirectoryListWidget {
    pub key: String,                        // ✅ Serialize: which directory
    pub items: Vec<DirectoryItem>,          // ❌ Skip: fetched content
    pub sort_order: SortOrder,              // ✅ Serialize: user preference

    #[serde(skip)]
    pub items: Vec<DirectoryItem>,          // Refresh on load
}
```

**Query invalidation**: When element's query is updated, cached widget is auto-invalidated and re-evaluated.

#### Tab Operations

**Switch tab** (user clicks tab or uses keyboard shortcut):
```
/-/switch_tab-2
```
- Updates parent's `active_child` to index 2
- Triggers render of newly active tab
- Previously active tab remains in memory (cached widget preserved)

**Add new tab**:
```
/-/add_tab-query("/data/new_project/-/listdir")
```
- Creates new child element with specified query
- Evaluates query → generates widget
- Optionally switches to new tab

**Close tab**:
```
/-/close_tab-1
```
- Removes child element at index 1
- Adjusts active index if necessary
- Destroyed element and widget freed

**Rename tab**:
Tab labels can be specified via:
- Preset configuration (`"label": "Project 1"`)
- Command metadata (query's filename/label metadata)
- Explicit command: `/-/set_tab_label-1-"New Name"`

#### Active Element Pattern (General)

Tab view reveals a **universal active element pattern**:

**Global active**: Single element receiving keyboard input (application-wide)
- Only one element is globally active at any time
- Arrow keys, Enter, etc. routed to this element

**Local active**: Widgets track their own active child
- Orthodox commander: which pane is active (responds to arrow keys)
- Tab container: which tab is visible
- Form: which input field has focus

**Key navigation behaviors**:

1. **Window-scoped Tab cycling**:
   ```
   /-/next_sibling    # Tab key: cycle within current container
   /-/prev_sibling    # Shift+Tab: cycle backward
   ```
   - Orthodox commander: Tab switches between left/right panes within that window
   - Stays within current container boundaries

2. **Application-scoped cycling**:
   ```
   /-/next_active     # Cycle through all activable elements
   /-/prev_active     # Cycle backward globally
   ```
   - Cycles across all containers
   - Example: Tab through left pane A → right pane A → left pane B → right pane B

**Active element storage**: Implementation-dependent
- Could be in parent container: `TabContainer {active_child: 457}`
- Could be in children: `Tab {is_active: true}`
- Could be global payload: `AppState {active_element: 457}`

#### Navigation History (Deferred)

**Problem**: In orthodox commander, viewing a file in right pane changes its query. User may want to "go back" to directory list.

**Potential solutions**:
- **Undo stack per element**: Traditional undo/redo (but deep chains are unergonomic)
- **Primary/secondary query**: Persistent base + temporary overlay (supports quick "back to home")
- **Named views**: Explicit mode switching between predefined views
- **No history**: Just navigate forward (midnight commander model)

**Decision**: Different UI patterns need different semantics. Don't bake one model into `UiElement`. Instead:
- Provide command primitives: `push_query`, `pop_query`, `pop_to_root`, `set_overlay`, `clear_overlay`
- Let widgets/commands choose appropriate model for their use case
- Orthodox commander could use overlay model (view as temporary)
- Browser-like explorer could use history stack

#### Operations Identified

- `switch_tab-index` - Change active child in container
- `add_tab-query` - Add new child element
- `close_tab-index` - Remove child element
- `set_tab_label-index-label` - Update tab metadata
- `next_sibling` / `prev_sibling` - Cycle active within container (window-scoped)
- `next_active` / `prev_active` - Cycle active across all containers (application-scoped)
- `set_active-handle` - Explicitly set global active element

#### Design Notes

- **Arbitrary nesting**: Tabs can contain orthodox commanders, which can be inside other tabs
- **Lazy evaluation**: Inactive tabs' queries not re-evaluated unless explicitly invalidated
- **State preservation**: Switching away from tab preserves cached widget (instant return)
- **Preset flexibility**: Same preset can be loaded multiple times with different handles

---

## Operations Summary (Work in Progress)

Candidate universal operations identified across use cases:

**Element Query Management:**
1. `set_current-query` - Update current element's query (orthodox commander navigation)
2. `set_sibling-offset-query` - Update sibling's query (orthodox commander file view)

**Active Element Management:**
3. `set_active-handle` - Set global active element
4. `next_sibling` / `prev_sibling` - Cycle within container (Tab key)
5. `next_active` / `prev_active` - Cycle across all containers

**Container Management:**
6. `add_child-query` - Add new child element (add tab, split pane)
7. `remove_child-index` - Remove child element (close tab, merge pane)

**Navigation History (Deferred):**
- `push_query-query` / `pop_query` / `pop_to_root` - History stack
- `set_overlay-query` / `clear_overlay` - Primary/secondary query model

**Goal**: Finalize ~5 universal operations that cover majority of use cases. Continue with more use cases to validate.

## Implementation Plan

1. **liquers-core**: Add `app_state` and `current_element` to `Context`
2. **liquers-lib**: `AppState`, `UiElement`, `ExtValue::UiWidget`, navigation methods
3. **liquers-egui**: Platform commands read AppState, render with egui, execute event queries
4. **liquers-web**: Platform commands render AppState as HTML/WASM

## Open Questions

1. Should `Context.app_state` be `Option` (UI-only) or always present?
2. How to handle widget state updates (polling vs events)?
3. Do we need standard widget types or fully platform-specific?
