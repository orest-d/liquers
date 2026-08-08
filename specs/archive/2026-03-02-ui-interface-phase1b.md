# UI Interface Phase 1b — AppState Message Processing and Non-blocking Evaluation

**Version:** 1.0
**Date:** 2026-02-11
**Status:** Specification
**Supersedes:** UI_INTERFACE_PHASE1_FSD.md (Phase 1a)

## Overview

Phase 1b extends Phase 1a with centralized message processing and non-blocking element evaluation. The key changes enable:

1. **AppState owns message processing** — moves receiver from app to AppState
2. **Explicit element lifecycle states** — replaces binary has/hasn't-element with state machine
3. **Non-blocking evaluation** — polls AssetRef for readiness instead of blocking with `.await`
4. **Browser-compatible execution** — `run()` called every frame, doesn't block event loop
5. **Type-safe state tracking** — invalid states become unrepresentable

### Problem Statement

**Phase 1a limitations:**
- Message processing scattered in app's update() loop
- Blocking evaluation freezes single-threaded environments (browsers)
- Cannot distinguish "not yet evaluated" vs "evaluating" vs "error"
- No centralized element lifecycle management

**Phase 1b solution:**
- AppState owns message receiver, provides `async run()` method
- run() processes messages and polls in-progress evaluations
- ElementState enum makes lifecycle explicit and type-safe
- Frame-by-frame execution works in browsers

---

## Architecture Changes

### Lifecycle Evolution

**Phase 1 (v5.0):**
```
ElementSource (Query) → [blocking eval] → element: Option<Box<dyn UIElement>>
```

**Phase 1a (v5.2):**
```
ElementSource → [async eval with UIContext] → element: Option<Box<dyn UIElement>>
Message channel enables sync→async communication
```

**Phase 1b (v6.0):**
```
ElementSource → Pending → Evaluating (AssetRef) → Ready/Error
       ↓                      ↓                       ↓
AppState owns         run() polls          Element or
message receiver      asset status         error message
```

### State Machine

```
┌─────────┐
│ Pending │ ← Node created with source, no element
└────┬────┘
     │ run() encounters pending node
     │ starts evaluation
     ▼
┌────────────┐
│ Evaluating │ ← AssetRef stored, polling in progress
│ {asset}    │
└──┬───┬─────┘
   │   │ run() polls AssetRef
   │   │
   ▼   ▼
┌───────┐  ┌─────────┐
│ Ready │  │  Error  │
│{element}  │{message}│
└───────┘  └─────────┘
```

**State transitions:**
- `Pending` → `Evaluating`: run() starts evaluation
- `Evaluating` → `Ready`: asset poll succeeds
- `Evaluating` → `Error`: asset poll fails
- `Error` → `Evaluating`: retry (clear error, re-submit query)
- `Ready` → `Evaluating`: re-evaluation (replace element)

---

## Core Data Structures

### ElementState Enum

**Location:** `liquers-lib/src/ui/element.rs`

```rust
/// Element lifecycle state.
///
/// Represents the current status of an element in the tree. Each variant
/// corresponds to a distinct phase of the evaluation lifecycle.
#[derive(Clone, Debug)]
pub enum ElementState<E: Environment> {
    /// Element has a source but evaluation has not started.
    Pending,

    /// Evaluation in progress.
    ///
    /// The AssetRef is not serializable and will be skipped during
    /// serialization. Upon deserialization, Evaluating nodes become Pending.
    /// 
    /// Serialize it as pending if serde supports such a possibility,
    /// otherwise serde(skip) the asset.
    Evaluating {
        asset: AssetRef<E>,
    },

    /// Evaluation completed successfully.
    Ready {
        element: Box<dyn UIElement>,
    },

    /// Evaluation failed.
    Error {
        message: liquers_core::error::Error,
    },
}
```

**Serialization behavior:**
- `Pending`: serializes as-is
- `Evaluating`: AssetRef skipped, deserializes as `Pending` (correct: needs re-evaluation)
- `Ready`: element serialized if UIElement impl supports it
- `Error`: message preserved

### NodeData Update

**Location:** `liquers-lib/src/ui/app_state.rs`

```rust
/// Node in the UI element tree.
///
/// Phase 1b change: `element: Option<Box<dyn UIElement>>` replaced with
/// `state: ElementState` for explicit lifecycle tracking.
pub struct NodeData<E: Environment> {
    /// Parent handle, or None for root nodes.
    pub parent: Option<UIHandle>,

    /// Ordered children handles.
    pub children: Vec<UIHandle>,

    /// How this element was generated (Query, Recipe, or None for manual).
    ///
    /// Persisted during serialization. Used for re-evaluation after deserialization.
    pub source: ElementSource,

    /// Current element lifecycle state.
    ///
    /// Replaces the Phase 1 `element: Option<Box<dyn UIElement>>` field.
    pub state: ElementState<E>,
}
```

**Breaking change:** All code accessing `node.element` must be updated to pattern match on `node.state`.

### DirectAppState Update

**Location:** `liquers-lib/src/ui/app_state.rs`

```rust
/// In-memory AppState implementation.
///
/// Phase 1b changes:
/// - Generic over Environment type (for AssetRef in ElementState)
/// - Owns AppMessageReceiver (moved from app struct)
/// - Stores EnvRef for query evaluation
pub struct DirectAppState<E: Environment> {
    /// Node storage (handle → NodeData).
    nodes: HashMap<UIHandle, NodeData<E>>,

    /// Auto-incrementing handle generator.
    next_id: AtomicU64,

    /// Currently active (focused) element.
    active_handle: Option<UIHandle>,

    /// Environment reference for evaluating queries.
    ///
    /// NEW in Phase 1b. Stored at construction, immutable after.
    envref: EnvRef<E>,

    /// Message receiver.
    ///
    /// NEW in Phase 1b. Moved from app struct. Drained by run() method.
    message_rx: AppMessageReceiver,

    /// Message sender.
    ///
    /// NEW in Phase 1b. Used to create new senders via get_app_message_sender().
    message_tx: AppMessageSender,
}
```

### ElementStatusInfo Enum

**Location:** `liquers-lib/src/ui/element.rs`

```rust
/// Read-only element status for queries.
///
/// Simplified view of ElementState for external inspection (debugging, UI display).
/// Does not expose AssetRef or internal element reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementStatusInfo {
    Pending,
    Evaluating,
    Ready,
    Error,
}
```

### AppStateEvent Enum

**Location:** `liquers-lib/src/ui/element.rs`

```rust
/// Events emitted by AppState for logging/monitoring.
#[derive(Debug, Clone)]
pub enum AppStateEvent {
    EvaluationStarted {
        handle: UIHandle,
        query: String,
    },
    EvaluationCompleted {
        handle: UIHandle,
    },
    EvaluationFailed {
        handle: UIHandle,
        error: liquers_core::error::Error,
    },
    MessageProcessed {
        message: AppMessage,
    },
}
```

---

## AppState Trait Changes

**Location:** `liquers-lib/src/ui/app_state.rs`

### New Methods

```rust
pub trait AppState<E: Environment>: Send + Sync + std::fmt::Debug {
    // ─── NEW: Phase 1b Methods ─────────────────────────────────────────

    /// Process queued messages and poll in-progress evaluations.
    ///
    /// Should be called regularly from the event loop (e.g., every frame).
    /// Does not block — polls assets with try_poll_state() for non-blocking operation.
    ///
    /// Steps:
    /// 1. Drain message receiver with try_recv() (non-blocking)
    /// 2. Process each message (SubmitQuery, EvaluatePending, etc.)
    /// 3. Poll all nodes in Evaluating state
    /// 4. Transition Ready/Error when asset completes
    ///
    /// Returns Ok(()) if all operations succeeded.
    async fn run(&mut self) -> Result<(), Error>;

    /// Get a new message sender.
    ///
    /// Used to create UIContext instances and allow elements to submit messages.
    /// The sender is cloneable and can be distributed freely.
    fn get_app_message_sender(&self) -> AppMessageSender;

    /// Query the current status of an element.
    ///
    /// Returns ElementStatusInfo (Pending, Evaluating, Ready, Error).
    /// Used for debugging, tooltips, status displays.
    fn get_element_status(&self, handle: UIHandle) -> Result<ElementStatusInfo, Error>;

    /// Event handler for logging and monitoring.
    ///
    /// Default implementation prints to stdout. Override to route events
    /// to a log window, telemetry system, etc.
    ///
    /// Called by run() when significant events occur:
    /// - Evaluation started/completed/failed
    /// - Messages processed
    fn on_app_state_event(&self, event: AppStateEvent) {
        println!("[AppState] {:?}", event);
    }

    // ─── Existing Methods (signatures unchanged, impl updated) ─────────

    fn add_node(
        &mut self,
        parent: Option<UIHandle>,
        position: usize,
        source: ElementSource,
    ) -> Result<UIHandle, Error>;

    fn get_element(&self, handle: UIHandle) -> Result<Option<&dyn UIElement>, Error>;
    fn get_source(&self, handle: UIHandle) -> Result<&ElementSource, Error>;
    fn set_element(&mut self, handle: UIHandle, element: Box<dyn UIElement>) -> Result<(), Error>;

    // ... all other existing methods
}
```

### Signature Changes

**Breaking:** AppState trait becomes generic over Environment:
```rust
// Phase 1a
pub trait AppState: Send + Sync + std::fmt::Debug { ... }

// Phase 1b
pub trait AppState<E: Environment>: Send + Sync + std::fmt::Debug { ... }
```

**Reason:** ElementState contains AssetRef<E>, so NodeData<E> and AppState<E> must be generic.

**Impact:** All `Arc<tokio::sync::Mutex<dyn AppState>>` must become `Arc<tokio::sync::Mutex<dyn AppState<E>>>` or use a concrete type.

---

## Implementation Details

### DirectAppState::new()

```rust
impl<E: Environment> DirectAppState<E> {
    pub fn new(envref: EnvRef<E>) -> (Self, AppMessageSender) {
        let (message_tx, message_rx) = app_message_channel();

        let state = Self {
            nodes: HashMap::new(),
            next_id: AtomicU64::new(0),
            active_handle: None,
            envref,
            message_rx,
            message_tx: message_tx.clone(),
        };

        (state, message_tx)
    }
}
```

**Returns tuple:** `(DirectAppState, AppMessageSender)`
- App keeps sender for creating UIContext
- AppState owns receiver

### DirectAppState::run()

```rust
impl<E: Environment> DirectAppState<E> {
    pub async fn run(&mut self) -> Result<(), Error> {
        // ─── Phase 1: Process Messages ─────────────────────────────────

        while let Ok(msg) = self.message_rx.try_recv() {
            self.on_event(AppStateEvent::MessageProcessed {
                message: msg.clone(),
            });

            match msg {
                AppMessage::SubmitQuery { handle, query } => {
                    self.start_evaluation(handle, query).await?;
                }
                AppMessage::EvaluatePending => {
                    self.evaluate_all_pending().await?;
                }
                AppMessage::Quit => {
                    self.on_quit(); // Add to interface. std::process::exit() by default.
                }
                AppMessage::Serialize { path } => {
                    // Serialize app state to file
                    let snapshot = serde_json::to_string_pretty(&self)?;
                    tokio::fs::write(&path, snapshot).await?;
                }
                AppMessage::Deserialize { path } => {
                    // Deserialize and merge (implementation-specific)
                }
            }
        }

        // ─── Phase 2: Evaluate the  source ────────────────────────────

        let evaluating: Vec<UIHandle> = self
            .nodes
            .iter()
            .filter(|(_, node)| matches!(node.state, ElementState::Pending))
            .map(|(handle, node)| (*handle, node.source))
            .collect();

        for (handle, source) in evaluating {
            self.evaluate_source(handle, source).await?;
        }

        // ─── Phase 3: Poll Evaluating Nodes ────────────────────────────

        let evaluating: Vec<UIHandle> = self
            .nodes
            .iter()
            .filter(|(_, node)| matches!(node.state, ElementState::Evaluating { .. }))
            .map(|(handle, _)| *handle)
            .collect();

        for handle in evaluating {
            self.poll_evaluation(handle).await?;
        }

        Ok(())
    }

    async fn start_evaluation(
        &mut self,
        handle: UIHandle,
        query: String,
    ) -> Result<(), Error> {
        // Create payload with this handle as current
        // FIXME: This is a wrong design - AppState can't be cloned!!!
        // Possibly the start_evaluation method could be moved to UIContext?
        let ui_context = UIContext::new(
            Arc::new(tokio::sync::Mutex::new(/* clone of self */)),
            self.message_tx.clone(),
        );
        let ui_context = ui_context.with_handle(Some(handle));
        let payload = SimpleUIPayload::new(ui_context);

        // Start evaluation (non-blocking)
        let asset_ref = self.envref.evaluate_immediately(&query, payload).await?;

        // Store AssetRef in ElementState
        let node = self.nodes.get_mut(&handle).ok_or_else(|| {
            Error::general_error(format!("Node not found: {:?}", handle))
        })?;
        node.state = ElementState::Evaluating {
            asset: Some(asset_ref),
        };

        self.on_event(AppStateEvent::EvaluationStarted {
            handle,
            query,
        });

        Ok(())
    }

    async fn poll_evaluation(&mut self, handle: UIHandle) -> Result<(), Error> {
        // Extract asset from state
        let asset_opt = match &self.nodes.get(&handle).map(|n| &n.state) {
            Some(ElementState::Evaluating { asset }) => asset.clone(),
            _ => return Ok(()), // No longer evaluating
        };

        let asset = match asset_opt {
            Some(a) => a,
            None => return Ok(()), // AssetRef was None (shouldn't happen)
        };

        // Non-blocking poll
        if let Some(state) = asset.try_poll_state() {
            // Success: create element from result

            // FIXME: Check - Here an error is checked
            let _ = state.metadata.error_result()?;

            let value = state.data.clone();
            // FIXME: Here we need to test whether the value is an Element.
            // This will only work for values with ExtValueInterface.
            // If it is the value, make a clone, otherwise do the wrapping:
            let element: Box<dyn UIElement> = Box::new(
                AssetViewElement::new_value(
                    state.metadata.get_title(), // FIXME: Check how to get the title from metadata
                    value,
                )
            );

            // Initialize and store element
            element.init(handle, self)?;
            let node = self.nodes.get_mut(&handle).ok_or_else(|| {
                // FIXME: This is actually an unexpected_error
                Error::general_error(format!("Node not found: {:?}", handle))
            })?;
            node.state = ElementState::Ready { element };

            self.on_event(AppStateEvent::EvaluationCompleted { handle });
        } else if let Some(err) = asset.try_poll_error() {
            // Error: store error message
            let node = self.nodes.get_mut(&handle).ok_or_else(|| {
                Error::general_error(format!("Node not found: {:?}", handle))
            })?;
            node.state = ElementState::Error {
                message: err.to_string(),
            };

            self.on_event(AppStateEvent::EvaluationFailed {
                handle,
                error: err.to_string(),
            });
        }
        // Still pending: leave as-is, poll again next run()

        Ok(())
    }

    async fn evaluate_all_pending(&mut self) -> Result<(), Error> {
        let pending: Vec<(UIHandle, String)> = self
            .nodes
            .iter()
            .filter_map(|(handle, node)| {
                match (&node.state, &node.source) {
                    (ElementState::Pending, ElementSource::Query(q)) => {
                        Some((*handle, q.clone()))
                    }
                    _ => None,
                }
            })
            .collect();

        for (handle, query) in pending {
            self.start_evaluation(handle, query).await?;
        }

        Ok(())
    }
}
```

### AssetRef Non-blocking Poll

**Location:** `liquers-core/src/assets.rs` (existing methods used)

```rust
impl<E: Environment> AssetRef<E> {
    /// Non-blocking poll for ready state.
    ///
    /// Returns Some(State) if asset is Ready, None otherwise.
    pub fn try_poll_state(&self) -> Option<State<E::Value>> {
        let lock = self.data.try_read().ok()?;
        lock.data.clone()
    }
}
```

**Note:** These methods already exist in Phase 1a. Phase 1b uses them for non-blocking polling.

---

## Example: Interactive Button Element

**File:** `liquers-lib/examples/ui_phase1b_demo.rs`

### ButtonElement Implementation

```rust
use liquers_lib::ui::*;
use std::sync::Arc;

/// Interactive button that submits a query on click.
#[derive(Debug, Clone)]
pub struct ButtonElement {
    handle: Option<UIHandle>,
    title: String,
    query_on_click: String,
}

impl ButtonElement {
    pub fn new(title: String, query_on_click: String) -> Self {
        Self {
            handle: None,
            title,
            query_on_click,
        }
    }
}

impl UIElement for ButtonElement {
    fn type_name(&self) -> &'static str {
        "ButtonElement"
    }

    fn handle(&self) -> Option<UIHandle> {
        self.handle
    }

    fn set_handle(&mut self, handle: UIHandle) {
        self.handle = Some(handle);
    }

    fn title(&self) -> String {
        self.title.clone()
    }

    fn set_title(&mut self, title: String) {
        self.title = title;
    }

    fn clone_boxed(&self) -> Box<dyn UIElement> {
        Box::new(self.clone())
    }

    fn init(&mut self, handle: UIHandle, _app_state: &dyn AppState) -> Result<(), Error> {
        self.handle = Some(handle);
        Ok(())
    }

    fn update(&mut self, _message: &UpdateMessage) -> UpdateResponse {
        UpdateResponse::Unchanged
    }

    fn show_in_egui(&mut self, ui: &mut egui::Ui, ctx: &UIContext) -> egui::Response {
        let response = ui.button(&self.title);

        if response.clicked() {
            // Submit query to replace this element
            let handle = self.handle.expect("ButtonElement not initialized");
            ctx.submit_query(&self.query_on_click); // FIXME: Note that submit_query needs to be modified to use the handle from the ctx
        }

        response
    }
}
```

### Application Structure

```rust
use liquers_core::context::SimpleEnvironment;
use liquers_lib::environment::DefaultEnvironment;
use liquers_lib::ui::*;
use liquers_lib::value::Value;

type Env = DefaultEnvironment<Value, SimpleUIPayload>; // FIXME: Check if we don't have cyclic definition - environmet - ui payload - ui context - app state - environment; if this is a problem, it needs to be redesigned. A possible solution is that AppState will not get a copy of envref, run will require it as an argument, but run would have to be generic.

struct Phase1bApp {
    app_state: Arc<tokio::sync::Mutex<DirectAppState<Env>>>,
    ui_context: UIContext,
    _runtime: tokio::runtime::Runtime,
}

impl Phase1bApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime");

        let (app_state, ui_context) = runtime.block_on(async {
            // 1. Create environment and register commands
            let mut env = Env::new();
            env.with_trivial_recipe_provider();
            register_all_commands(&mut env)?;
            let envref = env.to_ref();

            // 2. Create AppState with EnvRef
            let (mut app_state, message_tx) = DirectAppState::new(envref.clone());

            // 3. Add initial button element
            let button = Box::new(ButtonElement::new(
                "Click me!".to_string(),
                "/-/hello".to_string(),
            ));
            let root_handle = app_state.add_node(
                None,
                0,
                ElementSource::None, // Manual, not from query
            )?;
            app_state.set_element(root_handle, button)?;

            // 4. Wrap AppState and create UIContext
            let app_state_arc: Arc<tokio::sync::Mutex<DirectAppState<Env>>> =
                Arc::new(tokio::sync::Mutex::new(app_state));

            let ui_context = UIContext::new(app_state_arc.clone(), message_tx);

            Ok::<_, Error>((app_state_arc, ui_context))
        }).expect("Failed to initialize app");

        Self {
            app_state,
            ui_context,
            _runtime: runtime,
        }
    }
}

impl eframe::App for Phase1bApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ─── Call run() every frame ────────────────────────────────────

        if let Ok(mut state) = self.app_state.try_lock() {
            self._runtime.block_on(async {
                if let Err(e) = state.run().await {
                    eprintln!("[AppState::run] Error: {}", e);
                }
            });
        }

        // ─── Render UI ──────────────────────────────────────────────────

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Phase 1b Demo: Interactive UI");
            ui.separator();

            // Render all root elements
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

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Phase 1b Demo",
        options,
        Box::new(|cc| Ok(Box::new(Phase1bApp::new(cc)))),
    )
}
```

### Expected Behavior

1. **Initial state:** App shows a button labeled "Click me!"
2. **User clicks button:** ButtonElement calls `ctx.submit_query("/-/hello")`
3. **Message queued:** AppMessage::SubmitQuery added to channel
4. **Next frame:** run() drains message, starts evaluation
   - ButtonElement replaced with AssetViewElement (Progress mode)
5. **Subsequent frames:** run() polls AssetRef
   - AssetViewElement shows spinner/progress
6. **Asset ready:** run() detects completion
   - AssetViewElement transitions to Value mode, displays "Hello, World!"

---

## Migration from Phase 1a

### Step 1: Update NodeData

**Before:**
```rust
pub struct NodeData {
    pub parent: Option<UIHandle>,
    pub children: Vec<UIHandle>,
    pub source: ElementSource,
    pub element: Option<Box<dyn UIElement>>,
}
```

**After:**
```rust
pub struct NodeData<E: Environment> {
    pub parent: Option<UIHandle>,
    pub children: Vec<UIHandle>,
    pub source: ElementSource,
    pub state: ElementState<E>,  // Replaces element field
}
```

### Step 2: Update Element Access

**Before:**
```rust
if let Some(element) = &node.element {
    element.show_in_egui(ui, ctx);
}

let has_element = node.element.is_some();
let pending = node.element.is_none();
```

**After:**
```rust
match &node.state {
    ElementState::Ready { element } => {
        element.show_in_egui(ui, ctx);
    }
    _ => {}
}

let has_element = matches!(node.state, ElementState::Ready { .. });
let pending = matches!(node.state, ElementState::Pending | ElementState::Evaluating { .. });
```

### Step 3: Update AppState Instantiation

**Before:**
```rust
let mut app_state = DirectAppState::new();
let app_state_arc = Arc::new(tokio::sync::Mutex::new(app_state));
let (msg_tx, msg_rx) = app_message_channel();
let ui_context = UIContext::new(app_state_arc.clone(), msg_tx);

// App owns msg_rx, drains in update()
```

**After:**
```rust
let envref = env.to_ref();
let (app_state, msg_tx) = DirectAppState::new(envref);
// AppState owns receiver, app calls run()

let app_state_arc = Arc::new(tokio::sync::Mutex::new(app_state));
let ui_context = UIContext::new(app_state_arc.clone(), msg_tx);
```

### Step 4: Update Event Loop

**Before:**
```rust
impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain messages
        while let Ok(msg) = self.message_rx.try_recv() {
            match msg {
                AppMessage::SubmitQuery { handle, query } => {
                    self.evaluate_node(handle, &query);
                }
                // ... other messages
            }
        }

        // Render UI
        egui::CentralPanel::default().show(ctx, |ui| { ... });
    }
}
```

**After:**
```rust
impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Call run() every frame
        if let Ok(mut state) = self.app_state.try_lock() {
            self._runtime.block_on(async {
                state.run().await.expect("run() failed");
            });
        }

        // Render UI (same as before)
        egui::CentralPanel::default().show(ctx, |ui| { ... });
    }
}
```

### Step 5: Update pending_nodes()

**Before:**
```rust
fn pending_nodes(&self) -> Vec<UIHandle> {
    self.nodes
        .iter()
        .filter(|(_, node)| node.element.is_none())
        .map(|(h, _)| *h)
        .collect()
}
```

**After:**
```rust
fn pending_nodes(&self) -> Vec<UIHandle> {
    self.nodes
        .iter()
        .filter(|(_, node)| {
            matches!(
                node.state,
                ElementState::Pending | ElementState::Evaluating { .. }
            )
        })
        .map(|(h, _)| *h)
        .collect()
}
```

---

## Type Safety Analysis

### Invalid States in Phase 1a

**Phase 1a allows:**
```rust
NodeData {
    source: ElementSource::Query("hello".into()),
    element: None,  // Could mean: not evaluated OR error OR currently evaluating
}
```

**Ambiguity:** Cannot distinguish:
- "Not yet evaluated" (pending)
- "Currently evaluating" (in progress)
- "Evaluation failed" (error, but no error message stored)

### Type-Safe States in Phase 1b

**Phase 1b enforces:**
```rust
pub enum ElementState {
    Pending,                              // Explicit: not started
    Evaluating { asset: AssetRef },       // Explicit: in progress
    Ready { element: UIElement },         // Explicit: complete
    Error { message: String },            // Explicit: failed
}
```

**Impossible states:**
- Cannot have both element and asset
- Cannot be Ready without an element
- Cannot be Error without a message
- Cannot be Evaluating without an asset (or None if deserializing)

**Compile-time guarantees:**
```rust
// Exhaustive match required (per CLAUDE.md)
match node.state {
    ElementState::Pending => { /* handle pending */ }
    ElementState::Evaluating { asset } => { /* poll asset */ }
    ElementState::Ready { element } => { /* render element */ }
    ElementState::Error { message } => { /* show error */ }
    // No _ => arm allowed
}
```

---

## Performance Considerations

### Non-blocking Execution

**Frame budget:**
- Target: 16.67ms per frame (60 FPS)
- run() must complete within budget

**run() operations:**
1. **Message processing:** O(messages) — typically 0-5 per frame
2. **Polling evaluating nodes:** O(evaluating) — typically 0-10 concurrent
3. **try_poll_state():** O(1) — just read lock + clone

**Worst case:**
- 100 pending nodes → 100 evaluations started
- 100 concurrent evaluations polling
- Estimated: ~1-2ms per frame (well within budget)

### Memory Overhead

**Phase 1a:**
```rust
NodeData {
    parent: 8 bytes,
    children: 24 bytes (Vec),
    source: 32 bytes (enum),
    element: 16 bytes (Option<Box>),
}
Total: ~80 bytes + element size
```

**Phase 1b:**
```rust
NodeData {
    parent: 8 bytes,
    children: 24 bytes (Vec),
    source: 32 bytes (enum),
    state: 40 bytes (enum with largest variant),
}
Total: ~104 bytes + element size
```

**Overhead:** +24 bytes per node (30% increase)
**Acceptable:** For 1000 nodes, +24KB total

---

## Serialization

### Snapshot Format

**Example serialized state:**
```json
{
  "nodes": {
    "0": {
      "parent": null,
      "children": [1, 2],
      "source": {
        "type": "Query",
        "query": "/-/hello"
      },
      "state": {
        "type": "Ready",
        "element": {
          "type": "AssetViewElement",
          "title": "Hello",
          "value": "Hello, World!"
        }
      }
    },
    "1": {
      "parent": 0,
      "children": [],
      "source": {
        "type": "Query",
        "query": "/-/data/fetch"
      },
      "state": {
        "type": "Evaluating"
      }
    },
    "2": {
      "parent": 0,
      "children": [],
      "source": {
        "type": "Query",
        "query": "/-/invalid"
      },
      "state": {
        "type": "Error",
        "message": "Command not found: invalid"
      }
    }
  },
  "next_id": 3,
  "active_handle": 0
}
```

**Note:** `Evaluating` state loses `asset` field (not serializable). Upon deserialize, becomes `Pending` and must be re-evaluated.

### Deserialization Re-evaluation

```rust
// After deserialization:
let app_state = serde_json::from_str::<DirectAppState>(json)?;

// Re-evaluate all pending nodes (includes deserialized Evaluating → Pending)
app_state.send_message(AppMessage::EvaluatePending);

// Next run() cycle will start evaluations
```

---

## Testing Strategy

### Unit Tests

**File:** `liquers-lib/src/ui/app_state.rs` (tests module)

1. **State transitions:**
   - `Pending` → `Evaluating` → `Ready`
   - `Pending` → `Evaluating` → `Error`
   - `Error` → `Evaluating` → `Ready` (retry)

2. **Message processing:**
   - `SubmitQuery` starts evaluation
   - `EvaluatePending` processes all pending
   - `Serialize` / `Deserialize` round-trip

3. **Polling:**
   - `try_poll_state()` detects completion
   - `try_poll_error()` detects failure
   - Pending evaluation left as-is

4. **Serialization:**
   - `Evaluating` → `Pending` on deserialize
   - `Ready` preserved with element
   - `Error` preserved with message

### Integration Tests

**File:** `liquers-lib/tests/ui_phase1b_integration.rs`

1. **Full evaluation flow:**
   - Create AppState with EnvRef
   - Add node with Query source
   - Call run() until element ready
   - Verify element content

2. **Concurrent evaluations:**
   - Start 10 evaluations simultaneously
   - Call run() in loop
   - Verify all complete correctly

3. **Error handling:**
   - Submit invalid query
   - Verify Error state with message
   - Retry with valid query
   - Verify transitions to Ready

4. **Headless widget interaction test:**
   - Simulates the interactive example application without GUI
   - Tests complete widget → query submission → element replacement flow
   - Verifies AppState.run() correctly processes widget events

**Test specification: `test_widget_interaction_headless()`**

```rust
#[tokio::test]
async fn test_widget_interaction_headless() {
    // ─── Setup ──────────────────────────────────────────────────────

    // 1. Create environment and register hello command
    let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();
    env.with_trivial_recipe_provider();

    let cr = env.get_mut_command_registry();
    register_command!(cr, fn hello(state) -> result).unwrap();
    register_lui_commands!(cr).unwrap();

    let envref = env.to_ref();

    // 2. Create AppState
    let (mut app_state, message_tx) = DirectAppState::new(envref.clone());

    // 3. Define custom widget that submits query on event
    #[derive(Debug, Clone)]
    struct TestWidget {
        handle: Option<UIHandle>,
        title: String,
        query: String,
        event_triggered: Arc<AtomicBool>,  // For test control
    }

    impl UIElement for TestWidget {
        fn type_name(&self) -> &'static str { "TestWidget" }
        fn handle(&self) -> Option<UIHandle> { self.handle }
        fn set_handle(&mut self, handle: UIHandle) { self.handle = Some(handle); }
        fn title(&self) -> String { self.title.clone() }
        fn set_title(&mut self, title: String) { self.title = title; }
        fn clone_boxed(&self) -> Box<dyn UIElement> { Box::new(self.clone()) }

        fn init(&mut self, handle: UIHandle, _app_state: &dyn AppState)
            -> Result<(), Error>
        {
            self.handle = Some(handle);
            Ok(())
        }

        fn update(&mut self, message: &UpdateMessage) -> UpdateResponse {
            match message {
                UpdateMessage::Custom(msg) => {
                    // Check if this is our trigger event
                    if let Some(trigger) = msg.downcast_ref::<TriggerEvent>() {
                        if trigger.0 {
                            self.event_triggered.store(true, Ordering::SeqCst);
                        }
                    }
                }
                _ => {}
            }
            UpdateResponse::Unchanged
        }

        fn show_in_egui(&mut self, _ui: &mut egui::Ui, ctx: &UIContext)
            -> egui::Response
        {
            // Check if event was triggered
            if self.event_triggered.load(Ordering::SeqCst) {
                // Submit query to replace this element
                let handle = self.handle.unwrap();
                ctx.submit_query(&self.query);
                self.event_triggered.store(false, Ordering::SeqCst);
            }
            // Return dummy response (headless, no actual UI)
            egui::Response::default()
        }
    }

    // Trigger event type
    #[derive(Debug)]
    struct TriggerEvent(bool);

    // 4. Add TestWidget to AppState
    let widget = Box::new(TestWidget {
        handle: None,
        title: "Test Widget".to_string(),
        query: "hello/q/ns-lui/add-instead-current".to_string(),
        event_triggered: Arc::new(AtomicBool::new(false)),
    });

    let test_handle = app_state.add_node(
        None,
        0,
        ElementSource::None,  // Manual, not from query
    ).unwrap();

    app_state.set_element(test_handle, widget).unwrap();

    // ─── Verify Initial State ───────────────────────────────────────

    // Check that element is TestWidget
    let element = app_state.get_element(test_handle).unwrap().unwrap();
    assert_eq!(element.type_name(), "TestWidget");
    assert_eq!(element.title(), "Test Widget");

    // ─── Trigger Event ──────────────────────────────────────────────

    // Send custom event to widget
    let trigger_msg = UpdateMessage::Custom(Box::new(TriggerEvent(true)));

    // Extract widget, send update, replace
    let mut element = app_state.take_element(test_handle).unwrap();
    element.update(&trigger_msg);
    app_state.put_element(test_handle, element).unwrap();

    // Simulate show_in_egui call (would happen in render loop)
    let ui_context = UIContext::new(
        Arc::new(tokio::sync::Mutex::new(app_state)),
        message_tx.clone(),
    );

    {
        let mut app_state_lock = ui_context.app_state().lock().await;
        let mut element = app_state_lock.take_element(test_handle).unwrap();

        // Call show_in_egui with headless egui context
        let mut headless_ctx = egui::Context::default();
        headless_ctx.run(egui::RawInput::default(), |ctx| {
            let mut ui = egui::Ui::new(
                ctx.clone(),
                egui::LayerId::background(),
                egui::Id::new("test"),
                egui::Rect::EVERYTHING,
                egui::Rect::EVERYTHING,
            );
            element.show_in_egui(&mut ui, &ui_context);
        });

        app_state_lock.put_element(test_handle, element).unwrap();
    }

    // ─── Execute run() to process submitted query ──────────────────

    {
        let mut app_state_lock = ui_context.app_state().lock().await;
        app_state_lock.run().await.unwrap();
    }

    // At this point:
    // - AppMessage::SubmitQuery should be processed
    // - Evaluation should start (state = Evaluating)

    // Verify state is Evaluating
    {
        let app_state_lock = ui_context.app_state().lock().await;
        let status = app_state_lock.get_element_status(test_handle).unwrap();
        assert_eq!(status, ElementStatusInfo::Evaluating);
    }

    // ─── Poll until evaluation completes ────────────────────────────

    for _ in 0..100 {  // Max 100 iterations (safety limit)
        {
            let mut app_state_lock = ui_context.app_state().lock().await;
            app_state_lock.run().await.unwrap();
        }

        // Check if Ready
        let status = {
            let app_state_lock = ui_context.app_state().lock().await;
            app_state_lock.get_element_status(test_handle).unwrap()
        };

        if status == ElementStatusInfo::Ready {
            break;
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // ─── Verify Final State ─────────────────────────────────────────

    let app_state_lock = ui_context.app_state().lock().await;

    // Verify element is now AssetViewElement
    let element = app_state_lock.get_element(test_handle).unwrap().unwrap();
    assert_eq!(element.type_name(), "AssetViewElement");

    // Verify it contains the expected value
    // AssetViewElement should wrap the result of "hello" command
    // which returns Value::from("Hello, World!")
    let element_title = element.title();
    assert!(
        element_title.contains("Hello") || element_title.contains("Result"),
        "Expected element title to reference hello command result"
    );

    // Verify element is at same handle (replacement, not addition)
    let status = app_state_lock.get_element_status(test_handle).unwrap();
    assert_eq!(status, ElementStatusInfo::Ready);

    // Verify only one root element exists (replacement, not addition)
    let roots = app_state_lock.roots();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0], test_handle);
}
```

**Test flow:**
1. **Setup:** Create environment, register commands, create AppState
2. **Add widget:** Insert TestWidget with query `hello/q/ns-lui/add-instead-current`
3. **Verify initial:** Check element type is TestWidget
4. **Trigger event:** Send custom UpdateMessage to widget
5. **Simulate render:** Call show_in_egui (submits query to UIContext)
6. **Run AppState:** Call run() to process SubmitQuery message
7. **Verify evaluating:** Check element status is Evaluating
8. **Poll loop:** Call run() repeatedly until evaluation completes
9. **Verify final:** Check element replaced with AssetViewElement wrapping "Hello, World!"

**Key validations:**
- Widget correctly submits query on event
- AppState.run() processes message and starts evaluation
- Polling loop detects completion
- Element replaced at same handle (add-instead behavior)
- Final element type is AssetViewElement
- Final element contains hello command result
- Tree structure maintained (single root)

### Example Tests

**File:** `liquers-lib/examples/ui_phase1b_demo.rs` (manual testing)

1. **ButtonElement interaction:**
   - Click button
   - Verify query submission
   - Verify element replacement

2. **Event logging:**
   - Implement custom on_event()
   - Verify all events logged

---

## Future Extensions

### Phase 1c: Background Task Mode

For desktop applications, run() can run in a separate async task:

```rust
tokio::spawn(async move {
    loop {
        app_state.lock().await.run().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
});
```

**Benefits:**
- Decouples message processing from frame rate
- Can process messages faster than 60 FPS
- Reduces update() loop complexity

### Phase 1d: Cancellation

Add cancellation support:

```rust
pub enum ElementState {
    // ... existing variants
    Cancelling {
        asset: AssetRef<E>,
    },
}

impl AppState {
    fn cancel_evaluation(&mut self, handle: UIHandle) -> Result<(), Error>;
}
```

### Phase 1e: Progress Tracking

Store evaluation progress in ElementState:

```rust
pub enum ElementState {
    Evaluating {
        asset: AssetRef<E>,
        progress: Option<f32>,  // 0.0 to 1.0
        status_text: Option<String>,
    },
    // ...
}
```

---

## Glossary

- **AppState:** Trait managing UI element tree and message processing
- **DirectAppState:** In-memory AppState implementation
- **ElementSource:** How element was generated (Query, Recipe, None)
- **ElementState:** Current lifecycle state (Pending, Evaluating, Ready, Error)
- **AssetRef:** Reference to in-progress evaluation (contains Arc<RwLock<AssetData>>)
- **run():** Async method that processes messages and polls evaluations
- **try_poll_state():** Non-blocking poll for asset completion
- **UIContext:** Bundles AppState + sender + current handle for rendering
- **UIPayload:** Bundles UIContext for command execution

---

## References

- **Phase 1 (v5.0):** UI_INTERFACE_PHASE1_FSD.md §1-15
- **Phase 1a (v5.2):** UI_INTERFACE_PHASE1_FSD.md §16-17, UIContext introduction
- **Asset System:** specs/ASSETS.md
- **Command Registration:** specs/COMMAND_REGISTRATION_GUIDE.md
- **CLAUDE.md:** Project development guide

---

**End of Specification**
