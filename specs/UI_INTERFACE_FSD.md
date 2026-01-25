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

## Implementation Plan

1. **liquers-core**: Add `app_state` and `current_element` to `Context`
2. **liquers-lib**: `AppState`, `UiElement`, `ExtValue::UiWidget`, navigation methods
3. **liquers-egui**: Platform commands read AppState, render with egui, execute event queries
4. **liquers-web**: Platform commands render AppState as HTML/WASM

## Open Questions

1. Should `Context.app_state` be `Option` (UI-only) or always present?
2. How to handle widget state updates (polling vs events)?
3. Do we need standard widget types or fully platform-specific?
