# UI Interface Phase 1 — Functional Specification

## Overview

Query-driven UI state management for Liquers. Phase 1 establishes the core abstractions
for managing a tree of UI elements via commands, with lazy evaluation through the
Asset system and egui rendering.

**Phase 1 Goals:**
- Define `UIElement` trait (element identity, data, cloning, lifecycle, rendering)
- Define `AppState` trait (tree structure, navigation, modification, extract-render-replace)
- Define `UIPayload` trait (injection bridge between payload and AppState)
- Define element lifecycle (lazy: add creates source, rendering triggers evaluation)
- Implement `AssetViewElement` for unified progress/value/error display
- Implement `Placeholder` for stubs and reserved positions
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
`ExtValue::UIElement { value: Arc<dyn UIElement> }` (see §6).

**Location:** `liquers-lib/src/ui/element.rs`

### 2.1 Trait Definition

```rust
#[typetag::serde]
pub trait UIElement: Send + Sync + std::fmt::Debug {
    /// Machine-readable type name, e.g. "Placeholder", "AssetViewElement".
    /// Used for logging, debugging, element type identification, and error messages.
    /// Must be constant for a given implementation (not instance-dependent).
    fn type_name(&self) -> &'static str;

    /// Per-instance handle, None until init is called.
    /// Every implementation stores `handle: Option<UIHandle>` internally.
    fn handle(&self) -> Option<UIHandle>;

    /// Set the handle. Called by init. Must not be called more than once.
    fn set_handle(&mut self, handle: UIHandle);

    /// True if init has been called (handle is Some).
    fn is_initialised(&self) -> bool {
        self.handle().is_some()
    }

    /// Human-readable title. Defaults to type_name().
    /// Set initially from MetadataRecord.title (by the add command),
    /// can be overridden in init() or later by commands.
    fn title(&self) -> String {
        self.type_name().to_string()
    }

    /// Override the title.
    fn set_title(&mut self, title: String);

    /// Create a boxed clone of this element.
    /// Required because Clone is not object-safe.
    fn clone_boxed(&self) -> Box<dyn UIElement>;

    /// Called once after the element is registered in AppState.
    /// Default: stores the handle via set_handle.
    /// Implementations can override to inspect the tree (read-only)
    /// during initialization.
    ///
    /// `app_state` is a read-only reference to the AppState (the lock
    /// is already held by the caller; the element has been temporarily
    /// extracted so there is no self-referential borrow).
    fn init(
        &mut self,
        handle: UIHandle,
        _app_state: &dyn AppState,
    ) -> Result<(), Error> {
        self.set_handle(handle);
        Ok(())
    }

    /// React to a framework-agnostic update message.
    /// Default: no-op. Elements override to handle asset notifications,
    /// timer ticks, etc.
    fn update(&mut self, _message: &UpdateMessage) -> UpdateResponse {
        UpdateResponse::Unchanged
    }

    /// Render in egui. Handle is available via self.handle().
    /// The caller does NOT hold the AppState lock when calling this method.
    /// The UIContext provides access to AppState (via try_sync_lock)
    /// and a message channel for submitting async work (e.g. query evaluation).
    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &UIContext,
    ) -> egui::Response {
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

**Handle on element.** The handle is immutable once assigned. `set_handle` is called
only by `init`. Knowing the handle is important for show methods that need to identify
themselves when interacting with AppState (e.g., reading children).

**`set_title`.** Title is a mutable property. Set from MetadataRecord.title by default
(in the `add` command, before init), overridable in init or by commands.

**`init` takes `&dyn AppState`.** This is object-safe and provides everything UIPayload
offers (handle passed separately, tree readable via AppState). The AppState trait is
object-safe (no generic methods, no Self returns). The caller holds the lock and has
temporarily extracted the element, so there is no self-referential borrow.

**`show_in_egui` instead of `show`.** The method name includes the framework to leave
room for future platform-specific methods (e.g., `show_in_ratatui`). The signature
takes `&UIContext` — the caller does NOT hold the lock. The element can access
AppState via `try_sync_lock(ctx.app_state())` if it needs to read children, and
can submit async work (e.g. query evaluation) via `ctx.submit_query()`.

**`UIContext`.** Bundles `Arc<tokio::sync::Mutex<dyn AppState>>` and an
`AppMessageSender` (unbounded mpsc channel). Passed to `show_in_egui` so elements
can both read state and submit async work without needing access to the tokio
runtime or `EnvRef` directly.

**`tokio::sync::Mutex`.** Used for AppState wrapping to ensure consistent locking
semantics across async command execution. Commands use `.lock().await`. For
synchronous contexts (egui render loop), `try_lock()` (via `try_sync_lock()` helper)
is used instead of `blocking_lock()` for WASM compatibility. On WASM (single-threaded),
`try_lock()` never fails during synchronous rendering because no other task can be
running concurrently. On native, failure is extremely rare (async commands hold locks
for microseconds). On failure, a placeholder is shown and a repaint is requested.
The lock discipline from §12.4 applies — never hold across `.await` points beyond
the initial acquisition.

**No `as_any` / `as_any_mut`.** With `show_in_egui()` on the trait, downcasting is
not the primary rendering mechanism. If element-specific data access is needed from
framework code, it can be added as a Phase 2 extension.

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
- Store `handle: Option<UIHandle>` and `title_text: String` fields

UIElement implementations must NOT:
- Store child/parent relationships (topology owned by AppState)

### 2.4 UpdateMessage and UpdateResponse

Framework-agnostic update messaging for elements.

```rust
/// Framework-agnostic update messages delivered to elements.
pub enum UpdateMessage {
    /// Asset notification from the evaluation system.
    AssetNotification(AssetNotificationMessage),
    /// Periodic timer tick.
    Timer { elapsed_ms: u64 },
    /// Custom application-defined message.
    Custom(Box<dyn std::any::Any + Send>),
}

/// Element's response to an update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateResponse {
    /// No visual change — framework may skip repaint.
    Unchanged,
    /// Element state changed — framework should repaint.
    NeedsRepaint,
}
```

- **Asset notifications** are framework-agnostic: the background listener task
  delivers `AssetNotification` messages to the element via AppState.
- **Timer** is framework-agnostic: AppState or a coordinator delivers ticks.
- **Keyboard/mouse** are framework-specific: handled within `show_in_egui` (egui
  processes input during rendering) or via platform-specific trait methods.

### 2.5 Rendering Pattern — Extract-Render-Replace

The `show_in_egui` method takes `&UIContext` but the caller does NOT hold the lock.
To render an element, the framework uses the extract-render-replace pattern with
`try_sync_lock` for WASM compatibility:

```rust
pub fn render_element(
    ui: &mut egui::Ui,
    handle: UIHandle,
    ctx: &UIContext,
) {
    // 1. Extract element from AppState via try_lock.
    let element = match try_sync_lock(ctx.app_state()) {
        Ok(mut state) => state.take_element(handle),
        Err(_) => {
            // Lock held by async task — show placeholder, repaint next frame.
            ui.label(format!("Loading {:?}...", handle));
            ui.ctx().request_repaint();
            return;
        }
    };

    match element {
        Ok(mut element) => {
            // 2. Render (element can access AppState via ctx if needed).
            element.show_in_egui(ui, ctx);

            // 3. Put element back.
            match try_sync_lock(ctx.app_state()) {
                Ok(mut state) => {
                    let _ = state.put_element(handle, element);
                }
                Err(_) => {
                    // Very rare: lock acquired between take and put.
                    ui.ctx().request_repaint();
                }
            }
        }
        Err(_) => {
            ui.label(format!("Element {:?} not found", handle));
        }
    }
}
```

AppState provides two helper methods to support this pattern:

```rust
fn take_element(&mut self, handle: UIHandle) -> Result<Box<dyn UIElement>, Error>;
fn put_element(&mut self, handle: UIHandle, element: Box<dyn UIElement>) -> Result<(), Error>;
```

**`take_element`** removes the element from the node (setting it to None) and returns
it. The node remains in the tree with its topology intact.

**`put_element`** places an element back into a node. The node must exist and currently
have no element (i.e., it was previously taken).

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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Placeholder {
    handle: Option<UIHandle>,
    title_text: String,
}

impl Placeholder {
    pub fn new() -> Self {
        Self {
            handle: None,
            title_text: "Placeholder".to_string(),
        }
    }

    pub fn with_title(mut self, title: String) -> Self {
        self.title_text = title;
        self
    }
}

#[typetag::serde]
impl UIElement for Placeholder {
    fn type_name(&self) -> &'static str { "Placeholder" }

    fn handle(&self) -> Option<UIHandle> { self.handle }
    fn set_handle(&mut self, handle: UIHandle) { self.handle = Some(handle); }

    fn title(&self) -> String { self.title_text.clone() }
    fn set_title(&mut self, title: String) { self.title_text = title; }

    fn clone_boxed(&self) -> Box<dyn UIElement> { Box::new(self.clone()) }
}
```

### 3.2 AssetViewElement

General-purpose viewer for evaluated values. Covers the full asset lifecycle
in a single element — replaces both the progress indicator and the result display.

This replaces the previous `DisplayElement` (v4.0 §3.2) and `AssetDisplayUIElement`
(v4.0 §3.3) with a unified element that manages the full progress→value/error lifecycle.

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetViewElement {
    handle: Option<UIHandle>,
    title_text: String,
    type_identifier: String,

    /// The wrapped Value. Skipped during serialization.
    #[serde(skip)]
    value: Option<Arc<Value>>,

    /// Current display mode.
    view_mode: AssetViewMode,

    /// Error message if evaluation failed.
    #[serde(skip)]
    error_message: Option<String>,

    /// Live progress info, updated by background listener.
    #[serde(skip)]
    progress_info: Option<AssetInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AssetViewMode {
    /// Show evaluation progress (spinner, progress bar).
    Progress,
    /// Show the value (text, image, dataframe, etc.).
    Value,
    /// Show metadata (log, status, query, etc.).
    Metadata,
    /// Show error details.
    Error,
}
```

**Lifecycle:**
1. Created in `Progress` mode when evaluation starts (`new_progress`)
2. Created in `Value` mode when a pre-evaluated value is available (`new_value`)
3. Receives `UpdateMessage::AssetNotification` — updates `progress_info`
4. On completion: receives the Value via `set_value()`, transitions to `Value` mode
5. On error: transitions to `Error` mode automatically via `set_error()`
6. User can switch to `Metadata` or `Error` mode via UI controls

**update() implementation:**
```rust
fn update(&mut self, message: &UpdateMessage) -> UpdateResponse {
    match message {
        UpdateMessage::AssetNotification(notif) => {
            match notif {
                AssetNotificationMessage::ErrorOccurred(err) => {
                    self.set_error(err.message.clone());
                }
                AssetNotificationMessage::JobFinished => {}
                AssetNotificationMessage::PrimaryProgressUpdated(_) => {}
                // All other variants: no-op
                _ => {}
            }
            UpdateResponse::NeedsRepaint
        }
        UpdateMessage::Timer { .. } => UpdateResponse::Unchanged,
        UpdateMessage::Custom(_) => UpdateResponse::Unchanged,
    }
}
```

**show_in_egui renders based on view_mode:**
- `Progress`: spinner + progress bar (delegates to existing `display_progress`)
- `Value`: delegates to `UIValueExtension::show()` for the wrapped Value
- `Metadata`: shows asset info details
- `Error`: red error message with details

### 3.3 Background Listener Update Path

When AppState spawns a background listener for an evaluating node, the listener
delivers updates to the element via AppState:

```rust
// Background task (pseudocode):
loop {
    rx.changed().await;
    let notification = rx.borrow().clone();

    let mut state = app_state.blocking_lock();
    if let Some(element) = state.get_element_mut(handle) {
        let response = element.update(
            &UpdateMessage::AssetNotification(notification.clone())
        );
        // If NeedsRepaint, signal the framework (e.g., egui ctx.request_repaint())
    }

    if notification.is_finished() { break; }
}

// On completion: update element with final value
let asset_value = asset_ref.get().await?;
let mut state = app_state.blocking_lock();
if let Some(element) = state.get_element_mut(handle) {
    // For AssetViewElement: set value, transition to Value mode
}
```

The background listener updates the element in-place rather than creating
a separate progress element and replacing it. For non-AssetViewElement types, the
old replacement pattern can still be used.

### 3.4 Creating Custom UIElement Implementations

Application-specific elements implement UIElement. These live outside the `ui` module —
typically in egui-specific or application-specific code.

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Panel {
    handle: Option<UIHandle>,
    title_text: String,
    content_query: Option<String>,
}

#[typetag::serde]
impl UIElement for Panel {
    fn type_name(&self) -> &'static str { "Panel" }

    fn handle(&self) -> Option<UIHandle> { self.handle }
    fn set_handle(&mut self, handle: UIHandle) { self.handle = Some(handle); }

    fn title(&self) -> String { self.title_text.clone() }
    fn set_title(&mut self, title: String) { self.title_text = title; }

    fn clone_boxed(&self) -> Box<dyn UIElement> { Box::new(self.clone()) }

    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &UIContext,
    ) -> egui::Response {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.heading(&self.title_text);
            // Render children via ctx.app_state() (handle available via self.handle())
            // Submit async work via ctx.submit_query(handle, "query")
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

    // ── Node creation ──────────────────────────────────────────────

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

    /// Create a new node with both source and element.
    /// Convenience: add_node + set_element.
    fn insert_node(
        &mut self,
        parent: Option<UIHandle>,
        index: usize,
        source: ElementSource,
        element: Box<dyn UIElement>,
    ) -> Result<UIHandle, Error>;

    // ── Element access ──────────────────────────────────────────────

    /// Access the UIElement at this handle.
    /// Returns None if the node exists but has no element yet (pending).
    fn get_element(&self, handle: UIHandle) -> Result<Option<&dyn UIElement>, Error>;

    /// Mutable access to the UIElement at this handle.
    /// Returns None if the node exists but has no element yet.
    fn get_element_mut(
        &mut self,
        handle: UIHandle,
    ) -> Result<Option<&mut dyn UIElement>, Error>;

    /// Set or replace the element at a handle.
    /// The node must already exist. Calls element.init(handle, self).
    fn set_element(
        &mut self,
        handle: UIHandle,
        element: Box<dyn UIElement>,
    ) -> Result<(), Error>;

    /// Extract the element from a node (for extract-render-replace).
    /// The node remains in the tree with element set to None.
    fn take_element(
        &mut self,
        handle: UIHandle,
    ) -> Result<Box<dyn UIElement>, Error>;

    /// Put an element back into a node (after extract-render-replace).
    /// The node must exist and currently have no element.
    fn put_element(
        &mut self,
        handle: UIHandle,
        element: Box<dyn UIElement>,
    ) -> Result<(), Error>;

    // ── Node access ─────────────────────────────────────────────────

    /// Get the node data (for validation, etc.).
    fn get_node(&self, handle: UIHandle) -> Result<&NodeData, Error>;

    /// Get the generating source for this handle.
    fn get_source(&self, handle: UIHandle) -> Result<&ElementSource, Error>;

    // ── Navigation ──────────────────────────────────────────────────

    /// All root elements (no parent). Order is deterministic.
    fn roots(&self) -> Vec<UIHandle>;

    /// Parent of the given element, or None if it is a root.
    fn parent(&self, handle: UIHandle) -> Result<Option<UIHandle>, Error>;

    /// Ordered children of the given element.
    fn children(&self, handle: UIHandle) -> Result<Vec<UIHandle>, Error>;

    /// First child of the given element.
    fn first_child(&self, handle: UIHandle) -> Result<Option<UIHandle>, Error>;

    /// Last child of the given element.
    fn last_child(&self, handle: UIHandle) -> Result<Option<UIHandle>, Error>;

    /// Next sibling of the given element.
    fn next_sibling(&self, handle: UIHandle) -> Result<Option<UIHandle>, Error>;

    /// Previous sibling of the given element.
    fn previous_sibling(&self, handle: UIHandle) -> Result<Option<UIHandle>, Error>;

    // ── Modification ────────────────────────────────────────────────

    /// Remove element and its entire subtree recursively.
    fn remove(&mut self, handle: UIHandle) -> Result<(), Error>;

    // ── Active element ──────────────────────────────────────────────

    /// The currently active element (receives keyboard events, etc.).
    fn active_handle(&self) -> Option<UIHandle>;

    /// Set the active element.
    fn set_active_handle(&mut self, handle: Option<UIHandle>);

    // ── Pending nodes ───────────────────────────────────────────────

    /// All handles that have a source but no element (pending evaluation).
    fn pending_nodes(&self) -> Vec<UIHandle>;

    /// Total number of nodes in the tree.
    fn node_count(&self) -> usize;
}
```

### 4.2 Design Rationale

- **`element: Option<Box<dyn UIElement>>` per node.** A node can exist in AppState
  with source and topology but no element. This is the "pending" state. When `add`
  creates a node from a query, the element starts as `None`. Evaluation populates it.

- **`add_node` creates nodes without elements.** This is the primitive used by the
  `add` command for deferred evaluation. `insert_node` combines node creation with
  element assignment for cases where the element is already available.

- **`set_element` populates a pending node.** Called when evaluation completes to
  place the result. Calls `element.init(handle, self)` before storing, so elements
  receive their handle and can inspect the tree during initialization.

- **`take_element`/`put_element` for extract-render-replace.** Enables rendering
  without holding the AppState lock. See §2.5.

- **`pending_nodes()` for batch initialization.** After construction or deserialization,
  AppState (or the framework) calls this to find all nodes that need evaluation and
  triggers their evaluation. This is the platform-independent initialization path.

- **Relationships owned by AppState, not by elements.** UIElement implementations
  contain no parent/children fields (only their handle).

- **`remove` is recursive.** Removing an element removes its entire subtree.

- **Active element** is a dedicated field. At most one element is active globally.

- **`tokio::sync::Mutex` wrapping.** AppState is wrapped in `Arc<tokio::sync::Mutex<dyn AppState>>`
  for shared ownership. Commands use `.lock().await` for async-safe access.
  Synchronous contexts (egui rendering) use `try_sync_lock()` (a `try_lock()` wrapper)
  for WASM compatibility. On failure (rare on native, impossible on WASM), a placeholder
  is shown and the next frame is requested. See §2.2 for details.

### 4.3 Phase 1 Implementation: DirectAppState

In-memory implementation using `HashMap`. Handles are auto-generated from an
atomic counter.

**Location:** `liquers-lib/src/ui/app_state.rs`

#### NodeData

Per-node storage (public struct for access):

```rust
pub struct NodeData {
    pub parent: Option<UIHandle>,
    pub children: Vec<UIHandle>,
    pub source: ElementSource,
    /// None = pending evaluation. Populated by set_element.
    pub element: Option<Box<dyn UIElement>>,
}
```

#### DirectAppState

```rust
pub struct DirectAppState {
    nodes: HashMap<UIHandle, NodeData>,
    next_id: AtomicU64,
    active_handle: Option<UIHandle>,
}
```

**Serialization:** Uses a custom `DirectAppStateSnapshot` intermediary to handle
`AtomicU64` serialization (AtomicU64 → u64 on serialize, u64 → AtomicU64 on
deserialize).

**`set_element` init protocol:** When `set_element(handle, element)` is called,
it stores the element and then calls `element.init(handle, self)` to allow the
element to set its handle and inspect the tree.

### 4.4 Evaluation Triggering

AppState is responsible for initiating evaluation of pending nodes. This is a
platform-independent operation that should NOT be duplicated in framework-specific
code.

Evaluation is triggered in these situations:
1. **After initialization** — when the application starts and nodes are created
   with `add_node` (e.g., top-level window query)
2. **After deserialization** — nodes that lost their element during serialization
3. **During rendering** — when a renderer encounters a pending node

The evaluation process is asynchronous and uses the Asset system (see §7).
AppState does not perform evaluation itself — it delegates to the Environment's
asset manager. The concrete mechanism:

```rust
impl DirectAppState {
    /// Trigger evaluation of all pending nodes.
    /// `evaluate_fn` is called for each pending node with (handle, query_text).
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

---

## 5. UIPayload Trait

Bridge between the payload system and AppState. Allows commands to access the
element tree and the current element handle via the context.

**Location:** `liquers-lib/src/ui/payload.rs`

### 5.1 Trait Definition

```rust
pub trait UIPayload: PayloadType {
    /// The currently focused UI element handle, if any.
    fn handle(&self) -> Option<UIHandle>;

    /// Shared application state containing the element tree.
    fn app_state(&self) -> Arc<tokio::sync::Mutex<dyn AppState>>;
}
```

**Key design points:**
- `Arc<tokio::sync::Mutex<dyn AppState>>` for shared ownership. `tokio::sync::Mutex`
  ensures consistent async locking for commands; `blocking_lock()` for sync contexts.
- `handle()` returns `None` when no element is focused (e.g., background tasks).
- No associated type for AppState — uses `dyn AppState` for flexibility.

### 5.2 SimpleUIPayload

Minimal concrete payload for applications that only need UI state.

```rust
#[derive(Clone)]
pub struct SimpleUIPayload {
    current_handle: Option<UIHandle>,
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
}

impl UIPayload for SimpleUIPayload {
    fn handle(&self) -> Option<UIHandle> {
        self.current_handle
    }

    fn app_state(&self) -> Arc<tokio::sync::Mutex<dyn AppState>> {
        self.app_state.clone()
    }
}
```

### 5.3 Injection Newtypes

**AppStateRef** — injects the shared AppState from payload:

```rust
pub struct AppStateRef(pub Arc<tokio::sync::Mutex<dyn AppState>>);
```

Note: In Phase 1, `AppStateRef` is not used for injection via `InjectedFromContext`.
Instead, commands access AppState through the `UIPayload` trait on the context.

---

## 6. UIElement in the Value System

UIElements need to flow through the Liquers value pipeline: a command produces a
UIElement, the pipeline carries it as a `Value`, and the `add` command extracts it
for storage in AppState.

### 6.1 ExtValue Variant

```rust
pub enum ExtValue {
    Image { value: Arc<image::DynamicImage> },
    PolarsDataFrame { value: Arc<polars::frame::DataFrame> },
    UiCommand { value: crate::egui::UiCommand },
    Widget { value: Arc<tokio::sync::Mutex<dyn crate::egui::widgets::WidgetValue>> },
    UIElement { value: Arc<dyn UIElement> },
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
    fn from_ui_element(element: Arc<dyn UIElement>) -> Self;
    fn as_ui_element(&self) -> Result<Arc<dyn UIElement>, Error>;
}
```

### 6.3 ValueExtension Additions

The UIElement variant has entries in all `ValueExtension` match arms:

```rust
ExtValue::UIElement { .. } => {
    // identifier():          "ui_element"
    // type_name():           "ui_element"
    // default_extension():   "ui"
    // default_filename():    "element.ui"
    // default_media_type():  "application/octet-stream"
}
```

### 6.4 DefaultValueSerializer

`ExtValue` uses `DefaultValueSerializer` (not serde derives) for byte conversion.
For Phase 1, the UIElement variant returns `Err(SerializationError)` for all formats.
JSON serialization via typetag is available in Phase 2 if needed.

---

## 7. Element Lifecycle

This section describes how elements are created, evaluated, stored, accessed, modified,
and destroyed. The lifecycle is **lazy** — evaluation is deferred until rendering or
explicit initialization needs it.

### 7.1 Per-Node Storage

Each node in AppState stores:

| Field | Type | Description |
|-------|------|-------------|
| parent | `Option<UIHandle>` | Parent in the tree (None for roots) |
| children | `Vec<UIHandle>` | Ordered children |
| source | `ElementSource` | How this element was/will be generated |
| element | `Option<Box<dyn UIElement>>` | The element, or None if pending |

The handle is the key in the HashMap (not stored in NodeData). The element stores
its own handle internally (set during `init()`).

### 7.2 Lifecycle Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│  1. add command creates node:                                       │
│     handle + ElementSource::Query("some/query") + element: None     │
└────────────────────────────────┬────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│  2. Rendering/init encounters pending node (element == None)        │
│     → Triggers async evaluation via Asset system                    │
│     → AssetViewElement in Progress mode placed at handle            │
└────────────────────────────────┬────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│  3. Evaluation completes (Asset status → Ready)                     │
│     → Result Value inspected:                                       │
│       • ExtValue::UIElement → extract via clone_boxed()             │
│       • Any other Value → AssetViewElement transitions to Value mode │
└────────────────────────────────┬────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│  4. Element is now live:                                            │
│     → Renders via show_in_egui() on each frame                      │
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
   b. **Checks the Value type:**
      - If `ExtValue::UIElement`: extract via `clone_boxed()`, store immediately
        in AppState with both source and element.
      - If any other Value type: wrap in an `AssetViewElement::new_value()` with the
        value wrapped in `Arc`. Store immediately.
   c. **Resolves position** using the 2-arg model (position_word + reference_word).
   d. **Inserts** via `add_node` + `set_element`.

4. The `add` command returns the new handle as `Value::from(format!("{}", handle.0))`.

#### Path B: Manual Construction

In tests or initialization code, elements are created directly and inserted into
AppState without going through the pipeline.

```rust
let mut app_state = DirectAppState::new();

// Add a root node with element
let root = app_state.add_node(None, 0, ElementSource::None)?;
app_state.set_element(root, Box::new(Placeholder::new().with_title("Root".into())))?;

// Add a child with deferred evaluation
let child = app_state.add_node(
    Some(root),
    0,
    ElementSource::Query("file_list-home".into()),
)?;
// child has element: None, will be evaluated when rendering encounters it
```

#### Path C: Deserialization

A saved AppState is loaded from JSON/YAML. Nodes whose elements had non-serializable
data need re-evaluation.

```rust
let json = std::fs::read_to_string("layout.json")?;
let app_state: DirectAppState = serde_json::from_str(&json)?;

// Trigger re-evaluation of pending nodes
app_state.evaluate_pending(|handle, query_text| {
    // Spawn async evaluation task for each pending node
});
```

After deserialization:
- Nodes with fully serializable elements (Placeholder) are immediately usable.
- Nodes with AssetViewElement have `value: None` — display "not loaded".
- Nodes with `element: None` need evaluation from their ElementSource.
- Handle counter state is preserved (new handles don't collide).

### 7.4 Evaluation Process

When a pending node needs evaluation (triggered by rendering, initialization, or
post-deserialization), the process uses the Asset system:

1. **Submit query to asset manager.** The query text from ElementSource is submitted
   as an asset evaluation request. This returns an `AssetRef<E>`.

2. **Create AssetViewElement.** An AssetViewElement is created in Progress mode
   and placed at the handle via `app_state.set_element(handle, Box::new(asset_view))`.

3. **Spawn background listener.** A background task subscribes to the asset's
   notification channel. It updates the AssetViewElement via
   `element.update(&UpdateMessage::AssetNotification(notif))`.

4. **On completion** (asset status `is_finished()`):
   a. Read the asset value.
   b. **Classify the value:**
      - If `ExtValue::UIElement { value }`: call `value.clone_boxed()` to produce
        `Box<dyn UIElement>`. Replace the AssetViewElement.
      - If any other Value: call `asset_view.set_value(Arc::new(value))` to
        transition to Value mode.
   c. Signal the framework to repaint.

5. **On error:** The AssetViewElement's `set_error()` is called. The element
   remains at the handle, displaying the error in Error mode.

### 7.5 Element Access

#### Read Access

Framework-specific rendering loops use the extract-render-replace pattern (§2.5)
rather than locking AppState for the duration of rendering.

#### Write Access (Element Mutation)

Commands and background tasks access elements through the locked AppState:

```rust
let mut app_state = app_state_arc.lock().await;

if let Some(element) = app_state.get_element_mut(handle)? {
    element.update(&message);
}
```

### 7.6 Replacement

Replace swaps the element at a handle via the `instead` position word in the
`add` command. The handle and its position in the tree are kept.

Via `lui` commands: `new_query/q/ns-lui/add-instead-current`

### 7.7 Removal

Remove deletes a node and its entire subtree. The node's handle is removed from its
parent's children list. ElementSource and element are both deleted.

Via `lui` commands: `ns-lui/remove-current` or `ns-lui/remove-42`

### 7.8 Lifecycle Summary Table

| State | element | Trigger | Transition |
|-------|---------|---------|------------|
| Pending | None | Rendering / init | → Evaluating |
| Evaluating | AssetViewElement(Progress) | Asset completes | → Live |
| Live | UIElement / AssetViewElement(Value) | User action | → Replaced / Removed |
| Stale | AssetViewElement(value: None) | After deserialization | → Evaluating |

---

## 8. Target/Reference Resolution

Standalone utility functions that resolve command arguments to tree positions.

**Location:** `liquers-lib/src/ui/resolve.rs`

### 8.1 Vocabularies

**Navigation words** — used by both target and reference arguments:

| Word | Meaning |
|------|---------|
| `current` | Current element from payload |
| `parent` | Parent of current element |
| `next` | Next sibling of current |
| `prev` | Previous sibling of current |
| `first` | First child of current |
| `last` | Last child of current |
| `root` | First root element |
| `<number>` | Direct handle by numeric ID |

**Position words** — used by the `add` command's first argument:

| Word | Meaning |
|------|---------|
| `before` | Before reference in parent's child list |
| `after` | After reference in parent's child list |
| `instead` | Replace reference element (keep handle) |
| `first` | Insert as first child of reference |
| `last` / `child` | Insert as last child of reference |

**Naming constraint:** No hyphens in vocabulary words. The `-` character is the
argument separator in query syntax.

### 8.2 Functions

```rust
/// Resolve a navigation word relative to a current handle.
pub fn resolve_navigation(
    app_state: &dyn AppState,
    word: &str,
    current: Option<UIHandle>,
) -> Result<UIHandle, Error>;

/// Resolve a position word relative to a reference handle.
/// Returns an InsertionPoint describing where to place the new element.
pub fn resolve_position(
    position_word: &str,
    reference: UIHandle,
) -> Result<InsertionPoint, Error>;

/// Convert an InsertionPoint to (parent, position) arguments for add_node.
pub fn insertion_point_to_add_args(
    app_state: &dyn AppState,
    point: &InsertionPoint,
) -> Result<(Option<UIHandle>, usize), Error>;

pub enum InsertionPoint {
    FirstChild(UIHandle),
    LastChild(UIHandle),
    Before(UIHandle),
    After(UIHandle),
    Instead(UIHandle),
    Root,
}
```

### 8.3 Resolution Flow

For `add-<position_word>-<reference_word>`:

1. `let reference = resolve_navigation(&app_state, reference_word, current)?;`
2. `let insertion = resolve_position(position_word, reference)?;`
3. Handle `Instead` separately (replace); for others, convert to `(parent, index)`.

### 8.4 Position Resolution Rules

| Position | Reference | Result |
|----------|-----------|--------|
| `before` | handle H | `Before(H)` → `(H.parent, H.index_in_parent)` |
| `after` | handle H | `After(H)` → `(H.parent, H.index_in_parent + 1)` |
| `instead` | handle H | `Instead(H)` — replace element at H |
| `first` | handle H | `FirstChild(H)` → `(H, 0)` |
| `last` / `child` | handle H | `LastChild(H)` → `(H, children.len())` |

### 8.5 Edge Cases

- **`parent` on root:** Error: `"Current element has no parent"`

- **`next`/`prev` with no sibling:** Error with direction context.

- **`before`/`after` on root:** Error: `"Cannot insert before/after a root element"`

- **Invalid number:** Error: `"Element not found: {number}"`

- **Unknown word:** Error listing valid options.

---

## 9. Commands (`lui` Namespace)

Framework-agnostic commands for manipulating the UI tree. Registered in the `lui`
namespace.

**Location:** `liquers-lib/src/ui/commands.rs`

### 9.1 Query Syntax Recap

- `ns-lui` — switch to lui namespace (required before lui commands)
- `/q/` — wraps preceding query: evaluates it and passes result as state
- `-` — argument separator within a command

Example: `show_some_widget/q/ns-lui/add-after-last-parent`

### 9.2 Command Table

| Command | State | Arg 1 | Arg 2 | Returns |
|---------|-------|-------|-------|---------|
| `add` | Value (query via `/q/`) | position_word | reference_word | handle (string) |
| `remove` | — | target_word | — | none |
| `children` | — | target_word | — | comma-separated handles |
| `first` | — | target_word | — | handle (string) |
| `last` | — | target_word | — | handle (string) |
| `parent` | — | target_word | — | handle (string) |
| `next` | — | target_word | — | handle (string) |
| `prev` | — | target_word | — | handle (string) |
| `roots` | — | — | — | comma-separated handles |
| `activate` | — | target_word | — | handle (string) |

### 9.3 `add` Command Semantics

The `add` command takes 2 arguments (position_word, reference_word). The full
lifecycle is specified in §7.3 Path A.

**Value classification:**

- **Value is `ExtValue::UIElement`:** Extract via `clone_boxed()`, store immediately.
- **Value is any other type:** Wrap in `AssetViewElement::new_value()` with `Arc<Value>`.

**ElementSource extraction:**

The `add` command reads `State.metadata` → `MetadataRecord` → `asset_info.query`.
If the query is non-empty, it uses `ElementSource::Query(query.encode())`.
Otherwise `ElementSource::None`.

**Position `instead`** = replace. The element at the target handle is swapped out.

### 9.4 `remove`, Navigation, `activate`

`remove` resolves the target element via `resolve_navigation` and removes it
and its entire subtree.

Navigation commands (`children`, `first`, `last`, `parent`, `next`, `prev`, `roots`)
resolve the target and return handle values as strings.

`activate` sets the active element field in AppState.

### 9.5 Registration

Commands are registered via the `register_lui_commands!` macro, which is exported
from `liquers-lib`. The caller must define `type CommandEnvironment = ...` with a
concrete environment type whose Payload implements UIPayload.

```rust
#[macro_export]
macro_rules! register_lui_commands {
    ($cr:expr) => {{
        use liquers_macro::register_command;
        use $crate::ui::commands::*;

        register_command!($cr,
            fn add(state, context, position_word: String, reference_word: String) -> result
            namespace: "lui"
            label: "Add element"
            doc: "Add a new element to the UI tree"
        )?;

        register_command!($cr,
            fn remove(state, context, target_word: String) -> result
            namespace: "lui"
            label: "Remove element"
            doc: "Remove an element from the UI tree"
        )?;

        // ... children, first, last, parent, next, prev, roots, activate

        Ok::<(), liquers_core::error::Error>(())
    }};
}
```

**Note:** Commands are async. They use `tokio::sync::Mutex` with `.lock().await`
for brief in-memory operations. The lock is never held across other `.await` points.

---

## 10. Serialization

### 10.1 Approach

DirectAppState is serializable via serde. `dyn UIElement` is serializable via the
`typetag` crate. The entire tree is serialized:
- Tree topology (parent-child relationships, child ordering)
- Handle counter state (via DirectAppStateSnapshot intermediary)
- Per-node `ElementSource` (query text serialized as string)
- Per-node `Option<Box<dyn UIElement>>` (via typetag, None for pending nodes)
- Active element handle

### 10.2 Non-Serializable Fields

Some UIElement implementations contain non-serializable data:
- **AssetViewElement**: wraps `Arc<Value>` which is not Serialize. The `value`,
  `error_message`, and `progress_info` fields use `#[serde(skip)]` and become
  `None`/default after deserialization.

After deserialization, these elements exist in a **stale** state. They have their
serializable metadata (title, view_mode) but lack live data. AppState detects stale
elements via `pending_nodes()` and schedules re-evaluation.

### 10.3 typetag Serialization Format

When typetag serializes a `Box<dyn UIElement>`, it produces a tagged representation.
In JSON:

```json
{
  "type": "Placeholder",
  "handle": [42],
  "title_text": "My Panel"
}
```

The `"type"` field is added by typetag and maps to the concrete struct name. On
deserialization, typetag uses this tag to find the correct type's Deserialize impl.

**Requirement:** Every UIElement implementation must have `#[typetag::serde]` on its
impl block. If an implementation is defined in a separate crate, that crate must be
linked for typetag to discover it.

### 10.4 Dependencies

- `serde`, `serde_json` — serialization framework
- `typetag` — serialization of `dyn UIElement` trait objects
- `UIHandle`, `ElementSource` — derive `Serialize, Deserialize`
- `Recipe` — uses existing `liquers_core::recipes::Recipe`

---

## 11. Sync/Async Design

| Component | Sync/Async | Rationale |
|-----------|-----------|-----------|
| AppState trait methods | Sync | In-memory operations, no I/O |
| Mutex wrapping | `tokio::sync::Mutex` | Consistent async locking for commands |
| `lui` commands | Async | Use `.lock().await` on tokio Mutex |
| Utility functions (`resolve_*`) | Sync | Operate on `&dyn AppState` reference |
| egui rendering | Sync | Uses `try_sync_lock()` for WASM compatibility |
| Evaluation triggering | Async | Asset evaluation is async |
| Background listeners | Async (tokio tasks) | Monitor asset notifications |
| Task spawning | `spawn_ui_task()` | Native: `tokio::spawn`, WASM: `spawn_local` |

### 11.4 Lock Discipline

**Rule:** When used in async contexts, locks on AppState must never be held across
`.await` points. Acquire the lock, perform synchronous operations, drop the guard,
then perform any async work. This is a correctness requirement to prevent deadlocks.

---

## 12. Testing Strategy

### 12.1 Layer 1: AppState (sync, no framework)

Test `DirectAppState` directly. No environment, no payload, no queries.

Covers: tree operations (add_node, insert_node, remove, set_element, take/put_element),
navigation (parent, children, siblings, first/last child), pending_nodes detection,
edge cases (empty children, root operations), child ordering preservation, active element,
serialization round-trip.

### 12.2 Layer 2: Utility Functions (sync, no framework)

Test `resolve_navigation` and `resolve_position` against a pre-built DirectAppState.

Covers: all navigation words, all position words, error messages for edge cases,
`insertion_point_to_add_args` conversion.

### 12.3 Layer 3: Full Query Evaluation (async, full framework)

Test end-to-end: environment → command registration → payload → query evaluation
→ inspect AppState.

Pattern:
1. Create `DirectAppState` with initial elements
2. Wrap in `Arc<tokio::sync::Mutex<dyn AppState>>`
3. Create `SimpleUIPayload` with handle + app_state
4. Create environment, call `register_lui_commands!`
5. Evaluate query
6. Lock app_state, verify tree state

### 12.4 Layer 4: Serialization Round-Trip

Test that DirectAppState serializes and deserializes correctly:
- Topology preserved
- Handle counter preserved
- Serializable elements (Placeholder) survive round-trip
- Non-serializable fields lost (AssetViewElement.value)
- `pending_nodes()` correctly identifies nodes needing re-evaluation

---

## 13. Required Actions List

Derived from use cases in `UI_INTERFACE_FSD.md`. Shows how the `lui` command
semantics express each action.

### 13.1 Orthodox Commander

| Action | Query | Notes |
|--------|-------|-------|
| Create layout | Specialized command, not `lui` | Creates container + two panes |
| Navigate folder | `new_path/-/oc_list/q/ns-lui/add-instead-current` | Replace current pane |
| View file in sibling | `file/-/view/q/ns-lui/add-instead-next` | Replace next sibling |
| Switch active pane | `ns-lui/activate-next` | Activate next sibling |

### 13.2 Tab View

| Action | Query | Notes |
|--------|-------|-------|
| Add tab | `tab_query/q/ns-lui/add-last-current` | Append child to container |
| Close current tab | `ns-lui/remove-current` | Remove current tab |
| Switch to next tab | `ns-lui/activate-next` | Same pattern as pane switching |
| Switch to specific tab | `ns-lui/activate-42` | By handle |

---

## 14. Phase 2 Backlog

Items explicitly deferred from Phase 1:

- **`move` (reparent)** — move an element to a different parent without remove+add
- **`set_source`** — change generating source without replacing element
- **Handle stability on serialization round-trip** — ensure handles don't change
- **Extended `remove` semantics** — remove by query match
- **Human-readable layout files** — serialized AppState as GUI layout specification
- **Command aliases** — e.g., `replace` for `add-instead-current`
- **Child cycling** — `cycle_active` for cycling among children
- **Refresh command** — re-evaluate ElementSource and replace element
- **`as_any` / `as_any_mut` downcasting** — for element-specific data access
- **Lifecycle hooks** — callbacks on element events
- **Timers and periodic re-evaluation** — event processing loop
- **ratatui rendering** — `show_in_ratatui(...)` on UIElement
- **Dioxus/Leptos integration** — adapter pattern for reactive frameworks
- **Web rendering** — `show_in_browser(...)` on UIElement

---

## 15. Platform Design Notes

Brief design sketches for future rendering targets, created to validate the core
UIElement trait design. These are NOT Phase 1 deliverables.

### 15.1 ratatui

```rust
fn show_in_ratatui(
    &mut self,
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
);
```

- ratatui renders synchronously in a loop; `&mut self` + `Mutex` works.
- Elements compute sub-layouts for children using `ratatui::layout::Layout`.
- Input events delivered via `crossterm::event::read()` in the main loop.
- **No design issues identified.** The `&mut self` + `Arc<Mutex<dyn AppState>>`
  pattern maps naturally to ratatui's stateful widget model.

### 15.2 Dioxus / Leptos (Reactive Frameworks)

```rust
fn show_in_dioxus(&self) -> dioxus::Element;
```

- Reactive frameworks use functional components with signals, not `&mut self`.
- **Design tension:** `show_in_dioxus` needs `&self` (not `&mut self`).
- **Resolution:** The `update()` trait method handles mutation (framework-agnostic).
  The reactive show method reads state immutably. A `DioxusAdapter` component wraps
  `Arc<Mutex<dyn AppState>>` + `UIHandle`.
- **Phase 2 consideration:** Adding `fn show_data(&self) -> ShowData` as a
  framework-agnostic method that returns renderable data.

### 15.3 Generic Web (Browser)

```rust
fn show_in_browser(&self, doc: &web_sys::Document, container: &web_sys::Element);
```

- Web rendering creates/updates DOM elements.
- **Design tension:** DOM is persistent, unlike immediate-mode (egui).
- **Resolution:** Elements create DOM subtrees and update them on `update()`.
- **Serialization advantage:** typetag serialization means AppState can be sent
  over the wire (server-rendered initial state, hydrated on the client).
- The `handle()` on UIElement is useful for generating stable DOM IDs.

### 15.4 Cross-Platform Design Validation Summary

| Aspect | egui | ratatui | Dioxus/Leptos | Web |
|--------|------|---------|---------------|-----|
| `&mut self` show | Yes | Yes | No (`&self`) | Partial |
| `Arc<Mutex<AppState>>` | Yes | Yes | Via adapter | Yes |
| `update()` trait method | Yes | Yes | Yes | Yes |
| `handle()` on element | Useful | Useful | Useful | Essential |
| `typetag` serialization | Save/load | Save/load | SSR hydration | Wire transfer |
| Input handling | In show | Separate loop | Signals | DOM events |

**Conclusion:** The core trait design is sound for all platforms. The main tension is
`&mut self` vs `&self` for reactive frameworks, addressable via adapter pattern.

---

## 16. Command Registration Macros

### 16.1 Macro Pattern

All command domains expose `#[macro_export]` registration macros following a
consistent pattern. The caller defines `type CommandEnvironment = ...;` in scope
before invoking the macro:

```rust
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

let cr = env.get_mut_command_registry();
register_core_commands!(cr)?;
```

Each macro uses `$crate::` paths for full re-export safety and produces
`Ok::<(), liquers_core::error::Error>(())` at the end, so callers use `?` to
propagate errors.

**Internal structure of a registration macro:**

```rust
#[macro_export]
macro_rules! register_<domain>_commands {
    ($cr:expr) => {{
        use liquers_macro::register_command;
        use $crate::<module>::*;

        register_command!($cr,
            fn command_name(state, arg: Type) -> result
            namespace: "ns"
            label: "Human label"
            doc: "Description"
        )?;

        Ok::<(), liquers_core::error::Error>(())
    }};
}
```

**Important constraint:** The `register_command!` proc macro generates code
containing the `?` operator. This means registration macros must be called from
a context where `?` can propagate to a `Result` return type. They **cannot** be
expanded directly inside `async { }` blocks that return non-`Result` types.
Use a separate `fn setup_commands() -> Result<(), Error>` function when needed
inside async initialization.

### 16.2 Available Registration Macros

| Macro | Module | Namespace | Notes |
|-------|--------|-----------|-------|
| `register_core_commands!` | `commands` | (default) | `to_text`, `to_metadata` |
| `register_egui_commands!` | `egui::commands` | (default) | `label`, `text_editor`, `show_asset_info` |
| `register_image_commands!` | `image::commands` | (default) | ~40 image ops (requires `image-support` feature) |
| `register_polars_commands!` | `polars` | `pl` | Aggregate: calls all 6 sub-macros below |
| `register_polars_io_commands!` | `polars::io` | `pl` | `from_csv`, `to_csv` |
| `register_polars_selection_commands!` | `polars::selection` | `pl` | `select_columns`, `drop_columns`, `head`, `tail`, `slice` |
| `register_polars_filtering_commands!` | `polars::filtering` | `pl` | `eq`, `ne`, `gt`, `gte`, `lt`, `lte` |
| `register_polars_sorting_commands!` | `polars::sorting` | `pl` | `sort` |
| `register_polars_aggregation_commands!` | `polars::aggregation` | `pl` | `sum`, `mean`, `median`, `min`, `max`, `std`, `count`, `describe` |
| `register_polars_info_commands!` | `polars::info` | `pl` | `shape`, `nrows`, `ncols`, `schema` |
| `register_lui_commands!` | `ui::commands` | `lui` | UI tree manipulation (requires `UIPayload`) |

### 16.3 Master Registration Macro

`register_all_commands!` registers all command domains in one call:

```rust
#[macro_export]
macro_rules! register_all_commands {
    ($cr:expr) => {{
        $crate::register_core_commands!($cr)?;
        $crate::register_egui_commands!($cr)?;
        #[cfg(feature = "image-support")]
        { $crate::register_image_commands!($cr)?; }
        $crate::register_polars_commands!($cr)?;
        $crate::register_lui_commands!($cr)?;
        Ok::<(), liquers_core::error::Error>(())
    }};
}
```

Since `register_all_commands!` includes `register_lui_commands!`, the
`CommandEnvironment`'s `Payload` must implement `UIPayload`.

**Backward compatibility:** The `register_all_commands_fn()` function registers
all commands **except** lui commands, so it works with `DefaultEnvironment<Value>`
(where `Payload = ()`). This function predates the macros and remains for
environments that do not use UIPayload.

### 16.4 Payload-Aware Environment

`DefaultEnvironment` supports an optional payload type parameter:

```rust
pub struct DefaultEnvironment<V: ValueInterface, P: PayloadType = ()> { ... }
```

The default `P = ()` preserves backward compatibility — existing code using
`DefaultEnvironment<Value>` compiles unchanged. For UI applications, use
`DefaultEnvironment<Value, SimpleUIPayload>` to enable payload-aware commands.

### 16.5 evaluate_immediately with Payload

The `evaluate_immediately` method on `EnvRef` evaluates a query synchronously
(returning an `AssetRef`) with a payload attached. This is the mechanism for
connecting UI commands to the element tree:

```rust
let payload = SimpleUIPayload::new(app_state.clone())
    .with_handle(handle);
let asset_ref = envref
    .evaluate_immediately(&query, payload)
    .await?;
let state = asset_ref.get().await?;
```

The payload carries the `AppState` and the current element handle, making them
available to commands via the context's `InjectedFromContext` mechanism.

---

## 17. Egui Application Pattern

### 17.1 Application Structure

```rust
use liquers_lib::environment::DefaultEnvironment;
use liquers_lib::ui::{UIContext, AppMessage, AppMessageReceiver, app_message_channel,
                       try_sync_lock, render_element};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::value::Value;

type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

struct MyApp {
    ui_context: UIContext,
    message_rx: AppMessageReceiver,
    envref: EnvRef<DefaultEnvironment<Value, SimpleUIPayload>>,
    _runtime: tokio::runtime::Runtime,
}
```

**UIContext** bundles `Arc<tokio::sync::Mutex<dyn AppState>>` and an `AppMessageSender`.
It is passed to `render_element()` and `show_in_egui()` so that elements can both
read state (via `try_sync_lock`) and submit async work (via `submit_query()`).

**AppMessageReceiver** is drained in the `eframe::App::update()` method to process
messages submitted by elements (e.g., query evaluation requests, quit).

The tokio `Runtime` is created inside the eframe creation callback and stored
in the app struct (`_runtime` field) to keep it alive for the application's
lifetime. `SimpleEnvironment::new()` / `DefaultEnvironment::new()` call
`tokio::spawn()` internally, so they must be called inside a tokio runtime context.

### 17.2 Initialization Flow

Command registration must happen in a separate function (not directly in an
`async { }` block) because `register_command!` generates `?` operators:

```rust
fn setup_commands(env: &mut DefaultEnvironment<Value, SimpleUIPayload>) -> Result<(), Error> {
    let cr = env.get_mut_command_registry();
    register_command!(cr, fn hello(state) -> result)?;
    liquers_lib::register_all_commands!(cr)?;
    Ok(())
}
```

Inside `runtime.block_on(async { ... })`:

1. Create `DefaultEnvironment::<Value, SimpleUIPayload>::new()`
2. Set recipe provider via `with_trivial_recipe_provider()`
3. Call `setup_commands(&mut env)?` to register all commands
4. Create `DirectAppState` with root node via `add_node`
5. Wrap in `Arc<tokio::sync::Mutex<dyn AppState>>`
6. Create `EnvRef` via `env.to_ref()`
7. Evaluate root node using `evaluate_immediately` with `SimpleUIPayload`
8. Set the resulting element in app_state via `set_element`

**See `liquers-lib/examples/ui_payload_app.rs`** for a complete working example.

### 17.3 Render Loop (eframe::App)

```rust
impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Drain pending messages from the channel.
        while let Ok(msg) = self.message_rx.try_recv() {
            match msg {
                AppMessage::SubmitQuery { handle, query } => {
                    self.evaluate_node(handle, &query);
                }
                AppMessage::Quit => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                AppMessage::EvaluatePending => { /* ... */ }
                AppMessage::Serialize { .. } | AppMessage::Deserialize { .. } => { /* ... */ }
            }
        }

        // 2. Render UI.
        egui::CentralPanel::default().show(ctx, |ui| {
            let roots = match try_sync_lock(self.ui_context.app_state()) {
                Ok(state) => state.roots(),
                Err(_) => {
                    ui.spinner();
                    ctx.request_repaint();
                    return;
                }
            };
            for handle in roots {
                render_element(ui, handle, &self.ui_context);
            }
        });
    }
}
```

Uses `try_sync_lock` instead of `blocking_lock()` for WASM compatibility.
Messages from elements (submitted via `UIContext::submit_query()` etc.) are
drained at the start of each frame, before rendering.

### 17.4 Async Re-evaluation

To evaluate or re-evaluate a node asynchronously from the render loop, spawn
a task on the tokio runtime:

```rust
fn evaluate_node(&self, handle: UIHandle, query: &str) {
    let envref = self.envref.clone();
    let app_state = self.app_state.clone();
    let query = query.to_string();

    self._runtime.spawn(async move {
        let payload = SimpleUIPayload::new(app_state.clone())
            .with_handle(handle);
        let asset_ref = envref
            .evaluate_immediately(&query, payload)
            .await;
        match asset_ref {
            Ok(asset_ref) => {
                match asset_ref.get().await {
                    Ok(state) => {
                        let value = Arc::new((*state.data).clone());
                        let element = Box::new(AssetViewElement::new_value(
                            "Result".to_string(), value,
                        ));
                        let mut locked = app_state.lock().await;
                        if let Err(e) = locked.set_element(handle, element) {
                            eprintln!("Failed to set element: {}", e);
                        }
                    }
                    Err(e) => eprintln!("Failed to get result: {}", e),
                }
            }
            Err(e) => eprintln!("Failed to evaluate: {}", e),
        }
    });
}
```

---

## 18. Dependencies

### Cargo.toml additions for `liquers-lib`

```toml
[dependencies]
typetag = "0.2"
```

---

## 19. File Structure

```
liquers-lib/src/ui/
├── mod.rs              # Module declaration, re-exports, try_sync_lock, spawn_ui_task
├── handle.rs           # UIHandle type
├── element.rs          # UIElement trait, ElementSource, Placeholder, AssetViewElement
├── app_state.rs        # AppState trait, DirectAppState, NodeData
├── payload.rs          # UIPayload trait, SimpleUIPayload, AppStateRef
├── message.rs          # AppMessage enum, channel types
├── ui_context.rs       # UIContext struct (bundles AppState + message sender)
├── resolve.rs          # resolve_navigation, resolve_position, InsertionPoint
└── commands.rs         # lui namespace commands, register_lui_commands! macro
```

---

## 20. Success Criteria

Phase 1 is complete when:

1. `UIElement` trait compiles with typetag, `handle()`, `title()`, `clone_boxed()` work in tests
2. `Placeholder`, `AssetViewElement` serialize/deserialize correctly
3. `AppState` trait compiles with all specified methods including `take_element`/`put_element`
4. `DirectAppState` passes all Layer 1 tests (CRUD, navigation, pending detection)
5. `resolve_navigation` and `resolve_position` pass all Layer 2 tests
6. `UIPayload` trait and `SimpleUIPayload` work with command contexts
7. All `lui` commands registered and passing tests
8. `ExtValue::UIElement` variant integrated with value system
9. `activate` command works, active element field persists through serialization
10. Serialization round-trip preserves topology, sources, and serializable elements
11. `pending_nodes()` correctly identifies nodes needing re-evaluation
12. `show_in_egui()` renders Placeholder and AssetViewElement (all modes)
13. Extract-render-replace pattern implemented via `render_element()`

---

*Specification version: 5.2*
*Date: 2026-02-10*
*Supersedes: UI_INTERFACE_PHASE1_FSD v5.1, v5.0, v4.0, v3.0, v2.1, v1 (Corrected), UI_PAYLOAD_DESIGN v1*
*Changes in v5.2: UIContext replaces raw Arc<Mutex<AppState>> in show_in_egui and render_element. AppMessage/channel for sync→async communication. try_sync_lock replaces blocking_lock for WASM compatibility. spawn_ui_task for cross-platform task spawning. New files: message.rs, ui_context.rs.*
