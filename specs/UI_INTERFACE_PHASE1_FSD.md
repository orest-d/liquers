# UI Interface Phase 1 — Functional Specification

## Overview

Query-driven UI state management for Liquers. Phase 1 establishes the core abstractions
for managing a tree of UI elements via commands, with lazy evaluation through the
Asset system and optional egui rendering.

**Phase 1 Goals:**
- Define `UIElement` trait (element identity, data, cloning, optional rendering)
- Define `AppState` trait (tree structure, navigation, modification, evaluation trigger)
- Define `UIPayload` trait (injection bridge between payload and AppState)
- Define element lifecycle (lazy: add creates source, rendering triggers evaluation)
- Implement `AssetDisplayUIElement` for evaluation progress display
- Implement `DisplayElement` for wrapping evaluated Values
- Implement `lui` namespace commands for tree manipulation
- Implement utility functions for target/reference resolution
- Establish unit test patterns covering all layers

**Phase 1 does NOT include:**
- Platform-specific widget implementations (orthodox commander, tab container, etc.)
- Command aliases or shorthand syntax
- Recipe-based evaluation (Recipe variant is defined but not exercised)
- Timers, event loops, or periodic re-evaluation

**Key References:**
- `PAYLOAD_GUIDE.md` — General payload system
- `REGISTER_COMMAND_FSD.md` — Macro syntax for command registration
- `COMMAND_REGISTRATION_GUIDE.md` — Patterns for library-level commands
- `UI_INTERFACE_FSD.md` — Original design draft with use cases
- `UI_PAYLOAD_DESIGN.md` — Payload architecture (partially superseded by this spec)
- `PROJECT_OVERVIEW.md` — Query language syntax, `ns` and `q` instructions

---

## 1. Core Types

### 1.1 UIHandle

Type-safe handle for UI elements. A plain unique identifier carrying no structural
information — all relationships are managed by AppState.

**Location:** `liquers-lib/src/ui/handle.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UIHandle(pub u64);
```

Existing `From<u64>` and `Into<u64>` conversions are retained.

### 1.2 ElementSource

Describes how an element was generated. Serializable. Stored per node in AppState
alongside the UIElement.

**Location:** `liquers-lib/src/ui/element.rs`

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ElementSource {
    /// Element was created directly (e.g. manually constructed).
    /// No generating query — cannot be re-evaluated.
    None,

    /// Query text to evaluate (produces a UIElement or a Value to display).
    Query(String),

    /// Parameterized query with metadata.
    /// Uses the existing `liquers_core::recipes::Recipe` type which already
    /// derives Serialize + Deserialize.
    Recipe(liquers_core::recipes::Recipe),
}
```

**Difference between Query and Recipe:** A `Query` is a raw query string. A `Recipe`
adds arguments (parameterized queries), title, description, CWD, and a volatile flag.
Recipe enables re-evaluation with different parameters without reconstructing the query
string. For Phase 1, `Query` and `None` are the primary variants; `Recipe` is defined
and usable but may not be exercised until recipe-producing commands exist.

### 1.3 ElementType (Removed)

The `ElementType` enum from the previous design is removed. Element identity and
rendering behavior are determined by the `dyn UIElement` implementation, not by an
enum tag.

---

## 2. UIElement Trait

UIElement is a **trait**, not a struct. Different implementations represent different
kinds of elements (windows, panels, display wrappers, asset status indicators, etc.).

UIElement instances are stored in AppState as `Box<dyn UIElement>` and are serializable
via the `typetag` crate. UIElement instances flow through the value system as
`ExtValue::UIElement { value: Arc<dyn UIElement> }` (see §5).

**Location:** `liquers-lib/src/ui/element.rs`

### 2.1 Trait Definition

```rust
#[typetag::serde]
pub trait UIElement: Send + Sync + std::fmt::Debug {
    /// Machine-readable type name, e.g. "placeholder", "display", "asset_display".
    /// Used for logging, debugging, element type identification, and error messages.
    /// Must be constant for a given implementation (not instance-dependent).
    fn type_name(&self) -> &str;

    /// Human-readable title for the element.
    /// Used in tree visualization, tab titles, window titles, and error messages.
    /// May vary per instance.
    /// Default: returns type_name.
    /// For elements created from evaluated State, this should default to the
    /// metadata title from the generating query's result (MetadataRecord.title).
    fn title(&self) -> String {
        self.type_name().to_string()
    }

    /// Create a boxed clone of this element.
    /// Required because Clone is not object-safe.
    /// Used when:
    /// - Extracting from Arc<dyn UIElement> (value system) into Box<dyn UIElement> (AppState)
    /// - The replace operation needs to return the old element
    fn clone_boxed(&self) -> Box<dyn UIElement>;

    /// Render this element using egui.
    /// Default implementation displays the title as a label.
    /// Implementations override this for custom rendering.
    #[cfg(feature = "egui")]
    fn show(&mut self, ui: &mut egui::Ui) -> egui::Response {
        ui.label(self.title())
    }
}
```

### 2.2 Design Rationale

**`title()` instead of `display_name()`.** The method is called `title` to align with
`MetadataRecord.title` from the asset system. When an element is created from an
evaluated query result, the title defaults to the metadata title of the State. For
elements without a generating State (e.g., manually constructed), title() falls back
to type_name().

**`show()` behind `#[cfg(feature = "egui")]`.** Phase 1 includes egui rendering as an
optional feature. Each UIElement implementation provides its own `show()` method for
egui rendering. The default renders the title as a label. This avoids the need for
external downcasting-based dispatch — the element itself knows how to render.

For future rendering frameworks (ratatui, web), additional cfg-gated methods can be
added to the trait (e.g., `#[cfg(feature = "ratatui")] fn render(...)`).

**No `as_any` / `as_any_mut`.** With `show()` on the trait, downcasting is not the
primary rendering mechanism. If element-specific data access is needed from framework
code, it can be added as a Phase 2 extension.

**No `id: UIHandle` field.** UIElement does not know its own handle. The handle is
assigned and managed by AppState. When rendering or event handling needs the handle,
it is provided as context by the caller.

**`Debug` supertrait.** Required for meaningful error messages and tree visualization.
`typetag` already requires Serialize + Deserialize; Debug adds diagnostic capability.

**`clone_boxed` instead of `Clone` supertrait.** `Clone` is not object-safe. All
implementations derive `Clone` and delegate:
`fn clone_boxed(&self) -> Box<dyn UIElement> { Box::new(self.clone()) }`.

### 2.3 UIElement Constraints

All UIElement implementations must:
- Be `Send + Sync` (stored in AppState behind `tokio::sync::Mutex`)
- Be serializable via `serde` (required by typetag) — or use `#[serde(skip)]` for
  non-serializable fields with appropriate defaults
- Provide `clone_boxed` (typically by deriving `Clone`)

UIElement implementations must NOT:
- Store their own UIHandle (handle is an AppState concept)
- Store child/parent relationships (topology owned by AppState)

---

## 3. Phase 1 UIElement Implementations

### 3.1 Placeholder

A minimal serializable element. Can be used when:
- Deserializing a saved tree where re-evaluation is needed
- A command wants to reserve a position with visible feedback
- Testing without actual evaluation

Placeholder is NOT required by the core lifecycle (nodes without elements are
represented by `element: None` in NodeData). It exists as a convenience.

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Placeholder {
    /// Optional label for debugging (e.g. the query text).
    pub label: Option<String>,
}

#[typetag::serde]
impl UIElement for Placeholder {
    fn type_name(&self) -> &str { "placeholder" }

    fn title(&self) -> String {
        match &self.label {
            Some(label) => format!("placeholder({})", label),
            Option::None => "placeholder".to_string(),
        }
    }

    fn clone_boxed(&self) -> Box<dyn UIElement> { Box::new(self.clone()) }

    #[cfg(feature = "egui")]
    fn show(&mut self, ui: &mut egui::Ui) -> egui::Response {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(self.title());
        }).response
    }
}
```

### 3.2 DisplayElement

Wraps an evaluated Value for display. Created when evaluation produces a non-UIElement
value (text, number, DataFrame, image, etc.).

**The value field wraps the actual `Value` type** (CombinedValue<SimpleValue, ExtValue>),
not `serde_json::Value`. Values are often large files or calculation results and should
not be serialized as part of UI configuration.

```rust
#[derive(Clone, Debug)]
pub struct DisplayElement {
    /// Human-readable title, typically from MetadataRecord.title.
    pub title_text: String,

    /// Type identifier of the value (e.g. "text", "i64", "image", "polars_dataframe").
    pub type_identifier: String,

    /// The wrapped Value. Skipped during serialization.
    /// After deserialization this is None — the node needs re-evaluation.
    #[serde(skip)]
    pub value: Option<Arc<Value>>,
}
```

**Serialization behavior:** The `value` field is `#[serde(skip)]`. When serialized,
only `title_text` and `type_identifier` are stored. After deserialization, `value` is
`None`. AppState detects this and schedules re-evaluation from the node's ElementSource.

**Custom Serialize/Deserialize:** Because `#[derive(Serialize, Deserialize)]` cannot
be used directly (Arc<Value> is not Serialize), DisplayElement implements Serialize and
Deserialize manually. The implementation serializes `title_text` and `type_identifier`
only, and deserializes with `value: None`.

```rust
#[typetag::serde]
impl UIElement for DisplayElement {
    fn type_name(&self) -> &str { "display" }

    fn title(&self) -> String {
        self.title_text.clone()
    }

    fn clone_boxed(&self) -> Box<dyn UIElement> { Box::new(self.clone()) }

    #[cfg(feature = "egui")]
    fn show(&mut self, ui: &mut egui::Ui) -> egui::Response {
        match &self.value {
            Some(value) => {
                // Delegate to UIValueExtension::show for the actual value
                value.show(ui);
                ui.label("") // placeholder response
            }
            Option::None => {
                ui.label(format!("{} (not loaded)", self.title_text))
            }
        }
    }
}
```

**Note on egui rendering:** The `show()` method delegates to the existing
`UIValueExtension::show()` trait from `liquers-lib/src/egui/mod.rs`, which already
handles rendering for Image, PolarsDataFrame, Widget, and SimpleValue types.

### 3.3 AssetDisplayUIElement

Shows evaluation progress while a query is being evaluated asynchronously. Created
by AppState when it triggers evaluation of a node's ElementSource. Replaced by the
evaluation result (UIElement or DisplayElement) when evaluation completes.

Based on the existing `AssetStatus<E>` widget pattern in `liquers-lib/src/egui/widgets.rs`.

```rust
#[derive(Clone, Debug)]
pub struct AssetDisplayUIElement {
    /// The query being evaluated.
    pub query_text: String,

    /// Current asset status information, updated by a background listener task.
    /// Wrapped in std::sync::RwLock for thread-safe reads during rendering.
    #[serde(skip)]
    pub asset_info: Arc<std::sync::RwLock<Option<AssetInfo>>>,

    /// Error message if evaluation failed.
    #[serde(skip)]
    pub error_message: Arc<std::sync::RwLock<Option<String>>>,
}
```

**Serialization behavior:** Like DisplayElement, the live fields are `#[serde(skip)]`.
After deserialization, only `query_text` survives. The node needs re-evaluation.

**Custom Serialize/Deserialize:** Serializes only `query_text`. Deserializes with
`asset_info: None`, `error_message: None`.

```rust
#[typetag::serde]
impl UIElement for AssetDisplayUIElement {
    fn type_name(&self) -> &str { "asset_display" }

    fn title(&self) -> String {
        let info = self.asset_info.read().ok();
        let title = info.as_ref()
            .and_then(|lock| lock.as_ref())
            .map(|ai| ai.title.clone());
        match title {
            Some(t) if !t.is_empty() => t,
            _ => format!("evaluating: {}", self.query_text),
        }
    }

    fn clone_boxed(&self) -> Box<dyn UIElement> { Box::new(self.clone()) }

    #[cfg(feature = "egui")]
    fn show(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let error = self.error_message.read().ok()
            .and_then(|lock| lock.clone());
        if let Some(err) = error {
            return ui.colored_label(egui::Color32::RED, err);
        }

        let info = self.asset_info.read().ok()
            .and_then(|lock| lock.clone());
        match info {
            Some(ai) => {
                ui.horizontal(|ui| {
                    if ai.status.is_processing() {
                        ui.spinner();
                    }
                    ui.label(&ai.title);
                    if !ai.message.is_empty() {
                        ui.label(&ai.message);
                    }
                    // Show progress if available
                    if ai.progress.total > 0 {
                        let frac = ai.progress.done as f32 / ai.progress.total as f32;
                        ui.add(egui::ProgressBar::new(frac));
                    }
                }).response
            }
            Option::None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("Evaluating: {}", self.query_text));
                }).response
            }
        }
    }
}
```

**Background listener:** When AppState creates an AssetDisplayUIElement, it also spawns
a background tokio task that subscribes to the asset's notification channel
(`asset_ref.subscribe_to_notifications()`). This task:
1. Updates `asset_info` when `AssetNotificationMessage` arrives
2. Updates `error_message` on `ErrorOccurred`
3. Runs until the asset status `is_finished()`

The background task does NOT replace the element. Element replacement is handled by
the evaluation completion callback in AppState (see §6.4).

### 3.4 Creating Custom UIElement Implementations

Application-specific elements implement UIElement. These live outside the `ui` module —
typically in egui-specific or application-specific code.

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Panel {
    pub title_text: String,
    pub content_query: Option<String>,
}

#[typetag::serde]
impl UIElement for Panel {
    fn type_name(&self) -> &str { "panel" }
    fn title(&self) -> String { self.title_text.clone() }
    fn clone_boxed(&self) -> Box<dyn UIElement> { Box::new(self.clone()) }

    #[cfg(feature = "egui")]
    fn show(&mut self, ui: &mut egui::Ui) -> egui::Response {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.heading(&self.title_text);
            // Render children via AppState (handle passed externally)
        }).response
    }
}
```

The `#[typetag::serde]` attribute on the impl block registers this type with
typetag's runtime registry, enabling serialization/deserialization of
`Box<dyn UIElement>` containing a `Panel`.

---

## 4. AppState Trait

The central abstraction. Owns the tree topology (parent-child relationships, child
ordering), per-node generating source (`ElementSource`), and per-node element
(`Option<Box<dyn UIElement>>`).

AppState is also responsible for triggering evaluation of nodes that have a source
but no element.

**Location:** `liquers-lib/src/ui/app_state.rs`

### 4.1 Trait Definition

```rust
pub trait AppState: Send + Sync {

    // ── Navigation ──────────────────────────────────────────────────

    /// All root elements (no parent). Order is deterministic (sorted by handle).
    fn roots(&self) -> Vec<UIHandle>;

    /// Parent of the given element, or None if it is a root.
    fn parent(&self, handle: UIHandle) -> Result<Option<UIHandle>, Error>;

    /// Ordered children of the given element.
    fn children(&self, handle: UIHandle) -> Result<Vec<UIHandle>, Error>;

    /// Sibling at a relative offset among the parent's children.
    /// offset == 0 returns the element itself.
    /// Returns Ok(None) if root (no siblings) or index out of range.
    fn sibling(&self, handle: UIHandle, offset: i32) -> Result<Option<UIHandle>, Error>;

    /// Previous sibling (offset -1).
    fn previous_sibling(&self, handle: UIHandle) -> Result<Option<UIHandle>, Error>;

    /// Next sibling (offset +1).
    fn next_sibling(&self, handle: UIHandle) -> Result<Option<UIHandle>, Error>;

    // ── Element access ──────────────────────────────────────────────

    /// Access the UIElement at this handle.
    /// Returns None if the node exists but has no element yet (pending evaluation).
    fn get_element(&self, handle: UIHandle) -> Result<Option<&dyn UIElement>, Error>;

    /// Mutable access to the UIElement at this handle.
    /// Returns None if the node exists but has no element yet.
    fn get_element_mut(&mut self, handle: UIHandle) -> Result<Option<&mut dyn UIElement>, Error>;

    /// Access the generating source for this handle.
    fn get_source(&self, handle: UIHandle) -> Result<&ElementSource, Error>;

    /// Update the generating source for this handle.
    fn set_source(&mut self, handle: UIHandle, source: ElementSource) -> Result<(), Error>;

    /// Check if a node exists and has no element (pending evaluation).
    fn is_pending(&self, handle: UIHandle) -> Result<bool, Error>;

    // ── Modification ────────────────────────────────────────────────

    /// Create a new node with source but no element (pending evaluation).
    /// If parent is Some, inserts as child at the given index.
    /// If parent is None, creates a root node.
    /// Returns the new handle.
    fn add_node(
        &mut self,
        parent: Option<UIHandle>,
        index: usize,
        source: ElementSource,
    ) -> Result<UIHandle, Error>;

    /// Set the element at a handle. Used when evaluation completes.
    /// The node must already exist. Replaces any existing element.
    fn set_element(
        &mut self,
        handle: UIHandle,
        element: Box<dyn UIElement>,
    ) -> Result<(), Error>;

    /// Insert a new node with both source and element.
    /// Convenience method combining add_node + set_element.
    fn insert_child(
        &mut self,
        parent: UIHandle,
        index: usize,
        source: ElementSource,
        element: Box<dyn UIElement>,
    ) -> Result<UIHandle, Error>;

    /// Add a new root element with both source and element.
    fn add_root(
        &mut self,
        source: ElementSource,
        element: Box<dyn UIElement>,
    ) -> Result<UIHandle, Error>;

    /// Remove element and its entire subtree recursively.
    /// Returns the removed element (if any).
    fn remove(&mut self, handle: UIHandle) -> Result<Option<Box<dyn UIElement>>, Error>;

    /// Replace element at `handle`, keeping same handle and position.
    /// Old element's children are recursively removed.
    /// New element starts with no children.
    /// Returns the old element (if any).
    fn replace(
        &mut self,
        handle: UIHandle,
        source: ElementSource,
        element: Box<dyn UIElement>,
    ) -> Result<Option<Box<dyn UIElement>>, Error>;

    // ── Active element ──────────────────────────────────────────────

    /// The currently active element (receives keyboard events, etc.).
    fn active(&self) -> Option<UIHandle>;

    /// Set the active element.
    fn set_active(&mut self, handle: Option<UIHandle>) -> Result<(), Error>;

    // ── Pending nodes ───────────────────────────────────────────────

    /// All handles that have a source but no element (pending evaluation).
    /// Used by initialization and post-deserialization to trigger evaluation.
    fn pending_nodes(&self) -> Vec<UIHandle>;
}
```

### 4.2 Design Rationale

- **`element: Option<Box<dyn UIElement>>` per node.** A node can exist in AppState
  with source and topology but no element. This is the "pending" state. When `add`
  creates a node from a query, the element starts as `None`. Evaluation populates it.

- **`add_node` creates nodes without elements.** This is the primitive used by the
  `add` command for deferred evaluation. `insert_child` and `add_root` combine node
  creation with element assignment for cases where the element is already available.

- **`set_element` populates a pending node.** Called when evaluation completes to
  place the result (AssetDisplayUIElement during evaluation, then the final element).

- **`pending_nodes()` for batch initialization.** After construction or deserialization,
  AppState (or the framework) calls this to find all nodes that need evaluation and
  triggers their evaluation. This is the platform-independent initialization path.

- **Relationships owned by AppState, not by elements.** UIElement implementations
  contain no parent/children fields.

- **`replace` keeps the handle.** The element at the given handle is swapped out.
  Children of the old element are recursively removed. The new element starts with
  no children.

- **`remove` is recursive.** Removing an element removes its entire subtree.
  ElementSource and element are also removed.

- **Active element** is a dedicated field. At most one element is active globally.

- **No `Serialize + DeserializeOwned` bound on trait.** Serialization is handled
  by the concrete implementation (see §10). The trait focuses on the operational API.

### 4.3 Phase 1 Implementation: DirectAppState

In-memory implementation using `HashMap`. Handles are auto-generated from an
atomic counter.

**Location:** `liquers-lib/src/ui/app_state.rs`

#### NodeData

Internal per-node storage (implementation detail, not part of the trait):

```rust
#[derive(Serialize, Deserialize)]
struct NodeData {
    parent: Option<UIHandle>,
    children: Vec<UIHandle>,
    source: ElementSource,
    /// None = pending evaluation. Populated by set_element.
    element: Option<Box<dyn UIElement>>,
}
```

#### DirectAppState

```rust
#[derive(Serialize, Deserialize)]
pub struct DirectAppState {
    nodes: HashMap<UIHandle, NodeData>,
    #[serde(with = "atomic_u64_serde")]
    next_id: AtomicU64,
    active_handle: Option<UIHandle>,
}
```

Note: `AtomicU64` requires a custom serde module (`atomic_u64_serde`) that
serializes/deserializes the inner u64. This is a one-time helper.

### 4.4 Evaluation Triggering

AppState is responsible for initiating evaluation of pending nodes. This is a
platform-independent operation that should NOT be duplicated in framework-specific
code.

Evaluation is triggered in these situations:
1. **After initialization** — when the application starts and nodes are created
   with `add_node` (e.g., top-level window query)
2. **After deserialization** — nodes that lost their element during serialization
   (DisplayElement with `value: None`, AssetDisplayUIElement without live data)
3. **During rendering** — when a renderer encounters a pending node

The evaluation process is asynchronous and uses the Asset system (see §6).
AppState does not perform evaluation itself — it delegates to the Environment's
asset manager. The concrete mechanism:

```rust
impl DirectAppState {
    /// Trigger evaluation of all pending nodes.
    /// Called after initialization or deserialization.
    /// `evaluate_fn` is called for each pending node with (handle, query_text).
    /// The callback is responsible for spawning async evaluation tasks.
    pub fn evaluate_pending<F>(&self, evaluate_fn: F)
    where
        F: Fn(UIHandle, String),
    {
        for handle in self.pending_nodes() {
            if let Ok(source) = self.get_source(handle) {
                match source {
                    ElementSource::Query(query_text) => {
                        evaluate_fn(handle, query_text.clone());
                    }
                    ElementSource::Recipe(recipe) => {
                        evaluate_fn(handle, recipe.query.encode());
                    }
                    ElementSource::None => {}
                }
            }
        }
    }
}
```

The caller provides the evaluation callback, which typically:
1. Submits the query to the asset manager
2. Creates an AssetDisplayUIElement and places it at the handle via `set_element`
3. Spawns a background task to monitor the asset and replace the element when done

This pattern keeps AppState decoupled from the Environment type parameter while
ensuring the platform-independent flow lives in AppState.

---

## 5. UIPayload Trait

Bridge between the payload system and AppState. Allows commands to access the
element tree and the current element handle via injection.

**Location:** `liquers-lib/src/ui/payload.rs`

### 5.1 Trait Definition

```rust
pub trait UIPayload: PayloadType {
    /// The concrete AppState implementation.
    type State: AppState;

    /// The currently focused UI element handle, if any.
    fn handle(&self) -> Option<UIHandle>;

    /// Shared application state containing the element tree.
    fn app_state(&self) -> Arc<tokio::sync::Mutex<Self::State>>;
}
```

**Key design points:**
- Associated type `State` (not `dyn AppState`). Each application has one AppState
  implementation. This enables deserialization and avoids trait object overhead.
- `Arc<tokio::sync::Mutex<...>>` for shared ownership and async-safe locking.
- `handle()` returns `None` when no element is focused (e.g., background tasks).

### 5.2 SimpleUIPayload

Minimal concrete payload for applications that only need UI state.

**Location:** `liquers-lib/src/ui/payload.rs`

```rust
#[derive(Clone)]
pub struct SimpleUIPayload {
    current_handle: Option<UIHandle>,
    app_state: Arc<tokio::sync::Mutex<DirectAppState>>,
}

impl PayloadType for SimpleUIPayload {}

impl UIPayload for SimpleUIPayload {
    type State = DirectAppState;

    fn handle(&self) -> Option<UIHandle> {
        self.current_handle
    }

    fn app_state(&self) -> Arc<tokio::sync::Mutex<DirectAppState>> {
        self.app_state.clone()
    }
}
```

### 5.3 Injection Newtypes

**AppStateRef** — injects the shared AppState from payload:

```rust
pub struct AppStateRef(pub Arc<tokio::sync::Mutex<DirectAppState>>);

impl<E: Environment> InjectedFromContext<E> for AppStateRef
where
    E::Payload: UIPayload<State = DirectAppState>,
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload available".to_string()))?;
        Ok(AppStateRef(payload.app_state()))
    }
}
```

**UIHandle** — injects the current element handle from any UIPayload:

```rust
impl<E: Environment> InjectedFromContext<E> for UIHandle
where
    E::Payload: UIPayload,
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload available".to_string()))?;
        payload.handle()
            .ok_or_else(|| Error::general_error(
                "No current UI handle in payload".to_string()
            ))
    }
}
```

Note: `AppStateRef` is tied to `DirectAppState` (acceptable for Phase 1).
`UIHandle` injection is generic over any `UIPayload`.

---

## 6. UIElement in the Value System

UIElements need to flow through the Liquers value pipeline: a command produces a
UIElement, the pipeline carries it as a `Value`, and the `add` command extracts it
for storage in AppState. This section specifies how UIElement integrates with the
existing two-layer value system (`SimpleValue` + `ExtValue`).

### 6.1 ExtValue Variant

```rust
pub enum ExtValue {
    Image { value: Arc<image::DynamicImage> },
    PolarsDataFrame { value: Arc<polars::frame::DataFrame> },
    UiCommand { value: crate::egui::UiCommand },
    Widget { value: Arc<std::sync::Mutex<dyn crate::egui::widgets::WidgetValue>> },
    UIElement { value: Arc<dyn UIElement> },  // NEW
}
```

**Why `Arc<dyn UIElement>`:**
- `ExtValue` derives `Clone`. `Box<dyn UIElement>` is not Clone. `Arc` provides cheap
  cloning via reference counting. This matches the existing `Image` and `Widget` patterns.
- Not `Arc<Mutex<...>>` — UIElement values in the pipeline are immutable. Mutation
  happens only through AppState after the element is stored.

### 6.2 ExtValueInterface Additions

```rust
pub trait ExtValueInterface {
    // ... existing methods (from_image, as_image, from_polars_dataframe, ...) ...

    fn from_ui_element(element: Arc<dyn UIElement>) -> Self;
    fn as_ui_element(&self) -> Result<Arc<dyn UIElement>, Error>;
}
```

### 6.3 ValueExtension Additions

The UIElement variant needs entries in all `ValueExtension` match arms:

```rust
ExtValue::UIElement { .. } => {
    // identifier():          "ui_element"
    // type_name():           "ui_element"
    // default_extension():   "json"
    // default_filename():    "element.json"
    // default_media_type():  "application/json"
}
```

### 6.4 DefaultValueSerializer

`ExtValue` uses `DefaultValueSerializer` (not serde derives) for byte conversion.
For the UIElement variant:

- **`as_bytes("json")`**: `clone_boxed()` then `serde_json::to_vec(&boxed)` — typetag
  handles the `Box<dyn UIElement>` serialization, producing JSON with a `"type"` tag.
- **`deserialize_from_bytes(b, "ui_element", "json")`**: `serde_json::from_slice::<Box<dyn UIElement>>(b)`
  then wrap in `Arc`.
- Other formats: error.

### 6.5 Creating UIElement Values from Commands

A command that produces a UIElement for the tree:

```rust
fn create_panel(title: String) -> Result<Value, Error> {
    let element: Arc<dyn UIElement> = Arc::new(Panel {
        title_text: title,
        content_query: None,
    });
    Ok(Value::from(ExtValue::UIElement { value: element }))
}
```

Usage: `create_panel-My%20Panel/q/ns-lui/add`

The `/q/` causes `create_panel-My%20Panel` to be evaluated first, producing a
`State<Value>` where the Value is `ExtValue::UIElement`. This State is then passed
as the state argument to the `add` command (see §7.3 Path A).

---

## 7. Element Lifecycle

This section describes how elements are created, evaluated, stored, accessed, modified,
and destroyed. The lifecycle is **lazy** — evaluation is deferred until rendering or
explicit initialization needs it.

### 7.1 Per-Node Storage

Each node in AppState stores:

| Field | Type | Description |
|-------|------|-------------|
| handle | `UIHandle` | Unique identity, assigned by AppState |
| parent | `Option<UIHandle>` | Parent in the tree (None for roots) |
| children | `Vec<UIHandle>` | Ordered children |
| source | `ElementSource` | How this element was/will be generated |
| element | `Option<Box<dyn UIElement>>` | The element, or None if pending |

The handle is immutable for the node's lifetime. Parent and children are managed
by AppState's modification methods. Source can be updated via `set_source`.
Element can be set via `set_element` or `replace`.

### 7.2 Lifecycle Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│  1. add command creates node:                                       │
│     handle + ElementSource::Query("some/query") + element: None     │
└────────────────────────────┬────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│  2. Rendering/init encounters pending node (element == None)        │
│     → AppState triggers async evaluation via Asset system           │
│     → AssetDisplayUIElement placed at handle (shows progress)       │
└────────────────────────────┬────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│  3. Evaluation completes (Asset status → Ready)                     │
│     → Result Value inspected:                                       │
│       • ExtValue::UIElement → extract via clone_boxed()             │
│       • Any other Value → wrap in DisplayElement                    │
│     → AssetDisplayUIElement replaced at handle                      │
└────────────────────────────┬────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│  4. Element is now live:                                            │
│     → Renders via show() on each frame                              │
│     → Can be replaced (add-instead), removed, or re-evaluated       │
└─────────────────────────────────────────────────────────────────────┘
```

### 7.3 Creation Pathways

#### Path A: Command → Pipeline → `add` (Deferred Evaluation)

The primary path. A query is associated with a new node; evaluation is deferred.

```
some/query/q/ns-lui/add-after-last-parent
│               │            │
│               │            └─ add command: extract source query, resolve position, create node
│               └─ /q/: evaluate preceding query, pass result as state
└─ some/query: the query whose text becomes the ElementSource
```

**Step-by-step:**

1. The Liquers pipeline evaluates `some/query`. This produces a `State<Value>`.

2. The `/q/` instruction passes this State to the `add` command.

3. The `add` command:
   a. **Extracts the generating query** from `State.metadata` → `MetadataRecord.query`.
      This is the text `"some/query"` — the source for later re-evaluation.
   b. **Checks the Value type:**
      - If `ExtValue::UIElement`: the element is available immediately. Creates node
        with both source and element (via `insert_child` or `add_root`). No deferred
        evaluation needed.
      - If any other Value type: creates node with `ElementSource::Query(query_text)`
        and `element: None` (via `add_node`). The value from `/q/` is discarded —
        the query will be re-evaluated through the Asset system, which provides
        progress tracking and status updates.
   c. **Resolves position** using the 3-arg model (see §8).
   d. **Inserts** via `add_node` or `insert_child` / `add_root` / `replace` as appropriate.

4. The `add` command returns the new handle as `Value::from(handle.0 as i64)`.

**Why discard the eager result for non-UIElement values?** The Asset system provides
progress tracking, cancellation, and caching. Re-evaluating through Assets ensures
consistent lifecycle management. For UIElement values, the element is lightweight
(configuration, not data) and is stored immediately.

#### Path B: Manual Construction

In tests or initialization code, elements are created directly and inserted into
AppState without going through the pipeline.

```rust
let mut app_state = DirectAppState::new();

// Add a root panel (element available immediately)
let root = app_state.add_root(
    ElementSource::None,
    Box::new(Panel { title_text: "Root".into(), content_query: None }),
)?;

// Add a child with deferred evaluation
let child = app_state.add_node(
    Some(root),
    0,
    ElementSource::Query("file_list-home".into()),
)?;
// child has element: None, will be evaluated when rendering encounters it
```

#### Path C: Deserialization

A saved AppState is loaded from JSON/YAML. Nodes whose elements were non-serializable
(DisplayElement with value: None, AssetDisplayUIElement without live data) need
re-evaluation.

```rust
let json = std::fs::read_to_string("layout.json")?;
let app_state: DirectAppState = serde_json::from_str(&json)?;

// Trigger re-evaluation of pending nodes
app_state.evaluate_pending(|handle, query_text| {
    // Spawn async evaluation task for each pending node
    // (see §7.4 for the evaluation process)
});
```

After deserialization:
- Nodes with serializable elements (Placeholder, Panel, etc.) are immediately usable.
- Nodes with DisplayElement have `value: None` — detected as needing re-evaluation
  (the element exists but is stale; DisplayElement's show() displays "not loaded").
- Nodes with `element: None` need evaluation from their ElementSource.
- Handle counter state is preserved (new handles don't collide).

#### Path D: Compound Widget Construction

Specialized commands that create multi-element subtrees (e.g., orthodox commander
with container + two panes) manipulate AppState directly via the injected
`AppStateRef`:

```rust
async fn create_orthodox_commander(
    app_state_ref: AppStateRef,
    handle: UIHandle,
) -> Result<Value, Error> {
    let mut app_state = app_state_ref.0.lock().await;

    // Create container as child of current element
    let container = app_state.insert_child(
        handle, 0,
        ElementSource::None,
        Box::new(OrthodoxCommander { /* ... */ }),
    )?;

    // Add two panes with deferred evaluation
    let _left = app_state.add_node(
        Some(container), 0,
        ElementSource::Query("file_list-home".into()),
    )?;
    let _right = app_state.add_node(
        Some(container), 1,
        ElementSource::Query("file_list-home".into()),
    )?;

    Ok(Value::from(container.0 as i64))
}
```

### 7.4 Evaluation Process

When a pending node needs evaluation (triggered by rendering, initialization, or
post-deserialization), the process uses the Asset system:

1. **Submit query to asset manager.** The query text from ElementSource is submitted
   as an asset evaluation request. This returns an `AssetRef<E>`.

2. **Create AssetDisplayUIElement.** An AssetDisplayUIElement is created with the
   `AssetRef`'s notification channel data and placed at the handle via
   `app_state.set_element(handle, Box::new(asset_display))`.

3. **Spawn background listener.** A tokio task subscribes to the asset's notification
   channel (`asset_ref.subscribe_to_notifications()`). It updates the
   AssetDisplayUIElement's `asset_info` and `error_message` fields as notifications
   arrive.

4. **On completion** (asset status `is_finished()`):
   a. Read the asset value.
   b. **Classify the value:**
      - If `ExtValue::UIElement { value }`: call `value.clone_boxed()` to produce
        `Box<dyn UIElement>`.
      - If any other Value: create `DisplayElement` with:
        - `title_text` from `MetadataRecord.title`
        - `type_identifier` from the value's `identifier()`
        - `value` wrapping the `Arc<Value>`
   c. **Replace** the AssetDisplayUIElement at the handle:
      `app_state.set_element(handle, final_element)`.

5. **On error:** The AssetDisplayUIElement's `error_message` is set. The element
   remains at the handle, displaying the error. The user can trigger re-evaluation
   (Phase 2: refresh command).

### 7.5 Element Access

#### Read Access

Framework-specific rendering loops read elements from a locked AppState:

```rust
let app_state = app_state_ref.0.lock().await;

match app_state.get_element(handle)? {
    Some(element) => {
        // Element is available — render it
        // With egui feature: element.show(ui)
    }
    Option::None => {
        // Pending evaluation — trigger it
        // Or render a default spinner
    }
}
```

#### Write Access (Element Mutation)

With `show(&mut self, ...)`, the rendering framework needs mutable access:

```rust
let mut app_state = app_state_ref.0.lock().await;

if let Some(element) = app_state.get_element_mut(handle)? {
    #[cfg(feature = "egui")]
    element.show(ui);
}
```

### 7.6 Replacement

Replace swaps the element and source at a handle, keeping the handle and its position
in the tree. The old element's children are recursively removed, and the new element
starts with no children.

Via `lui` commands: `new_query/q/ns-lui/add-instead-current`

### 7.7 Removal

Remove deletes a node and its entire subtree. The node's handle is removed from its
parent's children list. ElementSource and element are both deleted.

Via `lui` commands: `ns-lui/remove` (removes current), `ns-lui/remove-42` (removes handle 42)

### 7.8 Lifecycle Summary Table

| State | element | Trigger | Transition |
|-------|---------|---------|------------|
| Pending | None | Rendering / init | → Evaluating |
| Evaluating | AssetDisplayUIElement | Asset completes | → Live |
| Live | UIElement / DisplayElement | User action | → Replaced / Removed |
| Stale | DisplayElement(value: None) | After deserialization | → Evaluating |

---

## 8. Target/Reference Resolution

Standalone utility functions that resolve command arguments to tree positions.

**Location:** `liquers-lib/src/ui/resolve.rs`

### 8.1 Vocabularies

**Navigation words** — used by both target and reference arguments:

| Word | Meaning |
|------|---------|
| `current` | Current element from payload |
| `parent` | Parent of context handle |
| `next` | Next sibling of context |
| `prev` | Previous sibling of context |
| `first` | First child of context |
| `last` | Last child of context |
| `root` | Root ancestor of current element |
| `<number>` | Direct handle by numeric ID |

**Position words** — used by the `add` command's first argument:

| Word | Meaning |
|------|---------|
| `before` | Before target in parent's child list |
| `after` | After target in parent's child list |
| `instead` | Replace target element (keep handle) |

The two vocabularies are **disjoint** — no word appears in both sets.

**Naming constraint:** No hyphens in vocabulary words. The `-` character is the
argument separator in query syntax.

### 8.2 Functions

```rust
/// Resolve a navigation word relative to a context handle.
/// Used for both target and reference resolution.
pub fn resolve_navigation(
    spec: &str,
    context_handle: UIHandle,
    app_state: &impl AppState,
) -> Result<UIHandle, Error>;

/// Resolve a position word relative to a target handle.
/// Returns an InsertionPoint describing where to place the new element.
pub fn resolve_position(
    spec: &str,
    target_handle: UIHandle,
    app_state: &impl AppState,
) -> Result<InsertionPoint, Error>;

pub enum InsertionPoint {
    /// Insert as child of parent at the given index.
    At(UIHandle, usize),
    /// Replace the element at this handle.
    Replace(UIHandle),
}
```

### 8.3 Resolution Flow

For `add-<position>-<target>-<reference>`:

1. `let anchor = resolve_navigation(reference, current_handle, &app_state)?;`
2. `let target = resolve_navigation(target, anchor, &app_state)?;`
3. `let insertion = resolve_position(position, target, &app_state)?;`

### 8.4 Position Resolution Rules

| Position | Target | Result |
|----------|--------|--------|
| `before` | handle H | `At(H.parent, H.index_in_parent)` |
| `after` | handle H | `At(H.parent, H.index_in_parent + 1)` |
| `instead` | handle H | `Replace(H)` |

### 8.5 Edge Cases

- **`first`/`last` on element with no children:** When used as target in `add` commands
  (e.g., `add-after-last-current` on an element with no children), the `add` command
  detects this and falls back to `insert_child(reference, 0, ...)` (append as first child).
  When used in `remove` or standalone navigation commands, this is an error.

- **`root`:** Resolves to the root ancestor of the current element (walks up the
  parent chain), not the first root globally.

- **`parent` on root:** Error: `"Cannot resolve 'parent': element {handle} is a root element"`

- **`next`/`prev` with no sibling:** Error with context about which element and direction.

- **`before`/`after` on root:** Error: `"Cannot resolve 'before': element {handle} is a root (no parent)"`

- **Invalid number:** Error: `"Element not found: {number}"`

### 8.6 Error Messages

All resolution errors must include context: the spec being resolved, the handle
involved, and why resolution failed. Example:
`"Cannot resolve 'parent': element 3 (PaneA) is a root element"`

---

## 9. Commands (`lui` Namespace)

Framework-agnostic commands for manipulating the UI tree. Registered in the `lui`
namespace.

**Location:** `liquers-lib/src/ui/commands.rs`

### 9.1 Query Syntax Recap

- `ns-lui` — switch to lui namespace (required before lui commands)
- `/q/` — wraps preceding query: evaluates it and passes result as state
- `~X~...~E` — embedded query in a parameter
- `-` — argument separator within a command

Example: `show_some_widget/q/ns-lui/add-after-last-parent`

See `PROJECT_OVERVIEW.md` §Query Language for full syntax.

### 9.2 Command Table

| Command | State | Arg 1 | Arg 2 | Arg 3 | Returns |
|---------|-------|-------|-------|-------|---------|
| `add` | Value (query via `/q/`) | position = `"after"` | target = `"last"` | reference = `"current"` | handle (i64) |
| `remove` | — | target = `"current"` | reference = `"current"` | — | handle (i64) |
| `children` | — | target = `"current"` | reference = `"current"` | — | list of i64 |
| `first` | — | target = `"current"` | reference = `"current"` | — | i64 or none |
| `last` | — | target = `"current"` | reference = `"current"` | — | i64 or none |
| `parent` | — | target = `"current"` | reference = `"current"` | — | i64 or none |
| `next` | — | target = `"current"` | reference = `"current"` | — | i64 or none |
| `prev` | — | target = `"current"` | reference = `"current"` | — | i64 or none |
| `roots` | — | — | — | — | list of i64 |
| `activate` | — | target = `"current"` | reference = `"current"` | — | handle (i64) |

### 9.3 `add` Command Semantics

The `add` command takes 3 arguments (position, target, reference) with defaults
`after`, `last`, `current`. The full lifecycle is specified in §7.3 Path A.

**Value classification:**

- **Value is `ExtValue::UIElement`:** Extract via `clone_boxed()`, store immediately
  in AppState with both source and element.
- **Value is any other type:** Store only the source query (from `State.metadata.query`),
  set element to None. Evaluation deferred to rendering/initialization.

**ElementSource extraction:**

The `add` command reads `State.metadata` → `MetadataRecord.query`. If the query is
non-empty, it uses `ElementSource::Query(query.encode())`. Otherwise `ElementSource::None`.

**Position `instead`** = replace. The target element's handle and position are kept;
its children are recursively removed; the new node replaces the old one.

### 9.4 Worked Examples

Tree:
```
Root (1)
├── Window (2)
│   ├── PaneA (3)  ← current
│   └── PaneB (4)
└── Window2 (6)
```

| Command | Position | Target | Ref | Resolved (parent, idx) | Effect |
|---------|----------|--------|-----|------------------------|--------|
| `add` | after | last(3)=none | 3 | (3, 0) | First child of PaneA |
| `add-after-last-parent` | after | last(2)=4 | parent(3)=2 | (2, 2) | After PaneB |
| `add-before-first-parent` | before | first(2)=3 | parent(3)=2 | (2, 0) | Before PaneA |
| `add-before-current` | before | 3 | 3 | (2, 0) | Before PaneA |
| `add-after-current` | after | 3 | 3 | (2, 1) | After PaneA |
| `add-instead-current` | instead | 3 | 3 | Replace(3) | Replace PaneA |
| `add-before-4` | before | 4 | 3 | (2, 1) | Between PaneA and PaneB |
| `add-after-last-2` | after | last(2)=4 | 2 | (2, 2) | After PaneB |

### 9.5 `remove`, Navigation, `activate`

`remove` uses 2-arg target+reference resolution (same navigation vocabulary) to
identify the element. Removes it and its entire subtree.

Navigation commands (`children`, `first`, `last`, `parent`, `next`, `prev`, `roots`)
use the same 2-arg target+reference pattern. They return handle values as integers
or lists of integers.

`activate` sets the active element field in AppState. Uses 2-arg target+reference
to identify which element to activate.

### 9.6 Return Types

- Single handles: returned as `i64` via `Value::from(handle.0 as i64)`
- Lists of handles: returned as `Value::List` of integers (see Issue 8: VALUE-LIST-SUPPORT)
- `None` results (e.g., `parent` of root): returned as `Value::None`

---

## 10. Serialization

### 10.1 Approach

DirectAppState implements `Serialize + Deserialize` via serde. `dyn UIElement` is
serializable via the `typetag` crate. The entire tree is serialized:
- Tree topology (parent-child relationships, child ordering)
- Handle counter state
- Per-node `ElementSource` (query text serialized as string)
- Per-node `Option<Box<dyn UIElement>>` (via typetag, None for pending nodes)
- Active element handle

No prescribed format — any serde-compatible format works (JSON, YAML preferred for
readability).

### 10.2 Non-Serializable Elements

Some UIElement implementations contain non-serializable data:
- **DisplayElement**: wraps `Arc<Value>` which is not Serialize. The `value` field
  uses `#[serde(skip)]` and becomes `None` after deserialization.
- **AssetDisplayUIElement**: wraps `Arc<RwLock<...>>` for live progress data. These
  fields use `#[serde(skip)]` and become `None` after deserialization.

After deserialization, these elements exist in a **stale** state. They have their
metadata (title, type identifier, query text) but lack live data. AppState detects
stale elements and schedules re-evaluation.

**Detection of stale elements:** A node is considered stale if:
- `element` is `None` (explicit pending state), OR
- `element` is `Some` but the element's non-serializable fields are empty AND
  the node has a non-None `ElementSource`

For Phase 1, the simplest approach: after deserialization, treat all nodes with
`ElementSource::Query` or `ElementSource::Recipe` as candidates for re-evaluation.
Nodes whose elements are already live can be skipped (the element will signal this).

### 10.3 Serializable Copy (Alternative Approach)

For scenarios where stale elements cause issues, AppState can create a serializable
copy of itself:

```rust
impl DirectAppState {
    /// Create a copy suitable for serialization.
    /// Nodes with non-serializable elements have their element set to None.
    pub fn to_serializable(&self) -> DirectAppState {
        let mut copy = self.clone();
        for node in copy.nodes.values_mut() {
            if let Some(element) = &node.element {
                if element.type_name() == "display" || element.type_name() == "asset_display" {
                    node.element = None;
                }
            }
        }
        copy
    }
}
```

This ensures the serialized output contains only fully-serializable elements.
After deserialization, `evaluate_pending()` re-evaluates nodes with `element: None`.

### 10.4 typetag Serialization Format

When typetag serializes a `Box<dyn UIElement>`, it produces a tagged representation.
In JSON:

```json
{
  "type": "Placeholder",
  "label": "file_list-home"
}
```

The `"type"` field is added by typetag and maps to the concrete struct name. On
deserialization, typetag uses this tag to find the correct type's Deserialize impl
in its runtime registry.

**Requirement:** Every UIElement implementation must have `#[typetag::serde]` on its
impl block. If an implementation is defined in a separate crate, that crate must be
linked for typetag to discover it.

### 10.5 Dependencies

- `serde`, `serde_json` — serialization framework
- `typetag` — serialization of `dyn UIElement` trait objects
- `UIHandle`, `ElementSource` — must derive `Serialize, Deserialize`
- `Recipe` — uses existing `liquers_core::recipes::Recipe` (already Serialize + Deserialize)

---

## 11. Command Registration

**Location:** `liquers-lib/src/ui/commands.rs`

### 11.1 Environment Type

```rust
type CommandEnvironment = SimpleEnvironmentWithPayload<Value, SimpleUIPayload>;
```

### 11.2 Command Functions

Functions are defined separately, named without namespace prefix. Namespace is set
in registration metadata. All lui commands are **async** (need `.lock().await` on
the tokio Mutex).

```rust
async fn add(
    state: State<Value>,
    app_state_ref: AppStateRef,
    handle: UIHandle,
    position: String,
    target: String,
    reference: String,
) -> Result<Value, Error> { ... }

async fn remove(
    app_state_ref: AppStateRef,
    handle: UIHandle,
    target: String,
    reference: String,
) -> Result<Value, Error> { ... }

// Navigation commands follow similar pattern (no state, 2 args)
```

### 11.3 Registration

```rust
pub fn register_lui_commands(env: &mut CommandEnvironment) -> Result<(), Error> {
    let cr = env.get_mut_command_registry();

    register_command!(cr,
        async fn add(state,
            app_state_ref: AppStateRef injected,
            handle: UIHandle injected,
            position: String = "after",
            target: String = "last",
            reference: String = "current"
        ) -> result
        namespace: "lui"
        doc: "Add element to UI tree"
    )?;

    register_command!(cr,
        async fn remove(
            app_state_ref: AppStateRef injected,
            handle: UIHandle injected,
            target: String = "current",
            reference: String = "current"
        ) -> result
        namespace: "lui"
        doc: "Remove element and subtree"
    )?;

    // ... children, first, last, parent, next, prev, roots, activate

    Ok(())
}
```

### 11.4 Lock Discipline

**Rule:** Locks on AppState must never be held across `.await` points. Acquire the
lock, perform synchronous operations, drop the guard, then perform any async work
(sub-query evaluation, etc.). This is a correctness requirement to prevent deadlocks.

---

## 12. Sync/Async Design

| Component | Sync/Async | Rationale |
|-----------|-----------|-----------|
| AppState trait methods | Sync | In-memory operations, no I/O |
| Mutex wrapping | `tokio::sync::Mutex` | Prevents deadlock with sub-queries |
| `lui` commands | Async | Need `.lock().await` |
| Utility functions (`resolve_*`) | Sync | Operate on already-locked `&impl AppState` |
| Evaluation triggering | Async | Asset evaluation is async |
| Background listeners | Async (tokio tasks) | Monitor asset notifications |

---

## 13. Testing Strategy

### 13.1 Layer 1: AppState (sync, no framework)

Test `DirectAppState` directly. No environment, no payload, no queries.

Covers: tree operations (add_node, insert_child, remove, replace, set_element),
navigation (parent, children, siblings), pending_nodes detection, edge cases
(empty children, root operations), child ordering preservation, active element.

### 13.2 Layer 2: Utility Functions (sync, no framework)

Test `resolve_navigation` and `resolve_position` against a pre-built AppState.

Covers: all navigation words, all position words, error messages for edge cases,
disjoint vocabulary validation.

### 13.3 Layer 3: Full Query Evaluation (async, full framework)

Test end-to-end: environment → command registration → payload → query evaluation
→ inspect AppState.

Pattern:
1. Create `DirectAppState` with initial elements
2. Wrap in `Arc<tokio::sync::Mutex<...>>`
3. Create `SimpleUIPayload` with handle + app_state
4. Create `CommandEnvironment`, call `register_lui_commands`
5. Evaluate query via `envref.evaluate_immediately(query, payload).await`
6. Lock app_state, verify tree state

Key scenarios:

| Test | Query | Verifies |
|------|-------|----------|
| Add creates node | `widget/q/ns-lui/add` | Node created with source, UIElement stored immediately |
| Add defers non-UIElement | `echo-hello/q/ns-lui/add` | Node created with source, element: None |
| Add as sibling | `widget/q/ns-lui/add-after-current` | Position=after |
| Add to specific handle | `widget/q/ns-lui/add-after-last-42` | Numeric handle |
| Replace | `widget/q/ns-lui/add-instead-current` | Same handle kept, children removed |
| Remove | `ns-lui/remove` | Subtree removal |
| Navigate parent | `ns-lui/parent` | Returns parent handle as i64 |
| Navigate children | `ns-lui/children` | Returns list of i64 |
| Activate | `ns-lui/activate` | Active element updated |
| Pending detection | After add with query | `pending_nodes()` includes handle |
| Error: no handle | query without payload handle | Injection error |
| Error: parent of root | `ns-lui/parent-root` | Meaningful error message |

### 13.4 Layer 4: Serialization Round-Trip

Test that DirectAppState serializes and deserializes correctly:
- Topology preserved
- Handle counter preserved
- Serializable elements (Placeholder, Panel) survive round-trip
- Non-serializable elements (DisplayElement) lose their value field
- `pending_nodes()` correctly identifies nodes needing re-evaluation after deserialization

### 13.5 Test Dependencies

- No egui dependency. UIElement instances use `Placeholder` or a test-only `TestElement`.
- Manual tree construction (no builder utilities).
- `q` instruction behavior already tested in liquers-core.

---

## 14. Required Actions List

Derived from use cases in `UI_INTERFACE_FSD.md`. Shows how the `lui` command
semantics express each action.

### 14.1 Orthodox Commander

| Action | Query | Notes |
|--------|-------|-------|
| Create layout | Specialized command in egui namespace, not `lui` | Creates container + two panes (see §7.3 Path D) |
| Navigate folder (update pane source) | `new_path/-/oc_list/q/ns-lui/add-instead-current` | Replace current pane content |
| View file in sibling pane | `file/-/view/q/ns-lui/add-instead-next` | Replace next sibling |
| View file in sibling of elem 42 | `file/-/view/q/ns-lui/add-instead-next-42` | Explicit reference |
| Switch active pane | `ns-lui/activate-next` | Activate next sibling |

### 14.2 Tab View

| Action | Query | Notes |
|--------|-------|-------|
| Add tab | `tab_query/q/ns-lui/add-after-last-current` | Append child to tab container |
| Close current tab | `ns-lui/remove` | Remove current tab element |
| Close specific tab | `ns-lui/remove-42` | Remove by handle |
| Switch to next tab | `ns-lui/activate-next` | Same pattern as pane switching |
| Switch to specific tab | `ns-lui/activate-42` | By handle |

### 14.3 General

| Action | Query | Notes |
|--------|-------|-------|
| Add root window | `window_query/q/ns-lui/add` with no current | Needs design for "add root" case |
| Get all roots | `ns-lui/roots` | Returns list of handles |
| Inspect tree | `ns-lui/children` / `ns-lui/parent` / etc. | Navigation commands |

---

## 15. Phase 2 Backlog

Items explicitly deferred from Phase 1:

- **`move` (reparent)** — move an element to a different parent without remove+add
- **`set_source`** — change generating source without replacing element (preserves children).
  Requires designing reparenting. Find a use case where preserving children is needed.
- **Handle stability on serialization round-trip** — ensure handles don't change.
  Widgets may store handles that would become invalid.
- **Extended `remove` semantics** — remove by query match (pass query as state)
- **Human-readable layout files** — use serialized AppState as a GUI layout specification
- **Command aliases** — e.g., `replace` for `add-instead-current`
- **State as reference** — pass navigation result as state for chaining (e.g., `parent/parent` = grandparent)
- **State as query-based lookup** — find element by generating query, use as reference
- **Child cycling** — `cycle_active` or similar for cycling among children (Tab key in orthodox commander).
  May use UI framework's native focus system instead.
- **Refresh command** — re-evaluate ElementSource and replace element with result.
- **Configurable evaluation behavior** — choose between immediate display vs. deferred evaluation
- **`as_any` / `as_any_mut` downcasting** — for framework-specific element data access
  beyond what `show()` provides. Add when a concrete use case requires it.
- **Lifecycle hooks** — callbacks on element events (added to tree, removed, focused, etc.)
- **Timers and periodic re-evaluation** — event processing loop in AppState
- **ratatui rendering** — `#[cfg(feature = "ratatui")] fn render(...)` on UIElement

---

## 16. Dependencies

### Cargo.toml additions for `liquers-lib`

```toml
[dependencies]
typetag = "0.2"
tokio = { version = "1", features = ["sync"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

---

## 17. File Structure

```
liquers-lib/src/ui/
├── mod.rs              # Module declaration and re-exports
├── handle.rs           # UIHandle type
├── element.rs          # UIElement trait, ElementSource, Placeholder, DisplayElement, AssetDisplayUIElement
├── app_state.rs        # AppState trait, DirectAppState implementation, NodeData
├── payload.rs          # UIPayload trait, SimpleUIPayload, injection newtypes
├── resolve.rs          # resolve_navigation, resolve_position, InsertionPoint
└── commands.rs         # lui namespace commands, registration
```

---

## 18. Success Criteria

Phase 1 is complete when:

1. `UIElement` trait compiles with typetag, `title()` and `clone_boxed()` work in tests
2. `Placeholder`, `DisplayElement`, `AssetDisplayUIElement` serialize/deserialize correctly
3. `AppState` trait compiles with all specified methods including `pending_nodes()`
4. `DirectAppState` passes all Layer 1 tests (CRUD, navigation, pending detection)
5. `resolve_navigation` and `resolve_position` pass all Layer 2 tests
6. `UIPayload` trait and `SimpleUIPayload` work with injection
7. All `lui` commands registered and passing Layer 3 tests
8. `ExtValue::UIElement` variant integrated with value system
9. `activate` command works, active element field persists through serialization
10. Serialization round-trip preserves topology, sources, and serializable elements
11. `evaluate_pending()` correctly identifies nodes needing re-evaluation
12. (If egui feature enabled) `show()` renders Placeholder, DisplayElement, AssetDisplayUIElement

---

*Specification version: 4.0*
*Date: 2026-02-08*
*Supersedes: UI_INTERFACE_PHASE1_FSD v3.0, v2.1, v1 (Corrected), UI_PAYLOAD_DESIGN v1*
