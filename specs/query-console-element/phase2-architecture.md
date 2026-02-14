# Phase 2: Solution & Architecture - QueryConsoleElement

## Overview

QueryConsoleElement is a new UIElement widget implementing a browser-like query console. It manages its own query history, command preset resolution, and view toggle. Asset monitoring is fully delegated to the AppRunner via a new `AppMessage::RequestAssetUpdates` message. The AppRunner monitors the asset lifecycle and pushes full `AssetSnapshot` snapshots to the widget through `UpdateMessage::AssetUpdate(AssetSnapshot)`. The widget is passive — it stores the latest snapshot and renders it.

## Data Structures

### AssetSnapshot

Non-generic full snapshot of an asset's current state. Pushed by AppRunner each time the monitored asset changes. The widget stores the most recent snapshot.

```rust
/// Full non-generic snapshot of a monitored asset.
/// Pushed by AppRunner via UpdateMessage::AssetUpdate.
#[derive(Clone, Debug)]
pub struct AssetSnapshot {
    /// The current value (if evaluation has completed successfully).
    pub value: Option<Arc<Value>>,
    /// Full metadata, always available via AssetRef::get_metadata().
    /// During evaluation: contains status, progress, logs as they arrive.
    /// After completion: contains the full metadata from the evaluated State.
    pub metadata: Metadata,
    /// Error from evaluation failure.
    pub error: Option<Error>,
    /// Current asset status.
    pub status: Status,
}
```

**Field semantics:**
- `value` — populated from `State` via `AssetRef::poll_state()`. `None` while evaluation is in progress.
- `metadata` — always populated from `AssetRef::get_metadata()`. Available at all stages of the asset lifecycle. Contains status, progress, logs, title, type info, etc. `AssetInfo` can be derived from `Metadata` via `metadata.get_asset_info()` if a flat summary is needed, but there is no need to store it separately since `get_metadata()` is always available and `AssetInfo` has no update-frequency advantage.
- `error` — populated when evaluation fails, either from `AssetNotificationMessage::ErrorOccurred` or from `State::error_result()`.
- `status` — current `Status` from `AssetRef::status()`.

### QueryConsoleElement

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryConsoleElement {
    handle: Option<UIHandle>,
    title_text: String,

    /// The text currently in the query edit field.
    query_text: String,

    /// History of submitted queries (oldest first). Persistent.
    history: Vec<String>,

    /// Current position in history. Points to next entry after the latest = history.len().
    /// history[history_index] is the currently displayed query (if < history.len()).
    history_index: usize,

    /// Current view mode: true = data view, false = metadata view.
    data_view: bool,

    /// The current value from the most recent AssetSnapshot.
    #[serde(skip)]
    value: Option<Arc<Value>>,

    /// The current metadata from the most recent AssetSnapshot.
    /// Always available (AssetRef::get_metadata() works at all lifecycle stages).
    #[serde(skip)]
    metadata: Option<Metadata>,

    /// Error from the most recent AssetSnapshot.
    #[serde(skip)]
    error: Option<Error>,

    /// Current asset status from the most recent AssetSnapshot.
    #[serde(skip)]
    status: Status,

    /// Cached next-command presets for the current query's last action.
    /// Each entry contains the full query with the preset already applied.
    #[serde(skip)]
    next_presets: Vec<NextPreset>,
}
```

**Ownership rationale:**
- `value: Option<Arc<Value>>` — Arc for cheap clone, received from AssetSnapshot
- `metadata: Option<Metadata>` — owned clone from snapshot, always available
- `error: Option<Error>` — owned, set from AssetSnapshot
- `status: Status` — copy type, set from AssetSnapshot
- `next_presets` — owned vec of `NextPreset`, rebuilt via `resolve_presets()` after each AssetUpdate with a value
- `history` / `history_index` — serialized for session persistence

**Serialization:**
- Derives: `Serialize, Deserialize` (via typetag)
- `#[serde(skip)]` on: `value`, `metadata`, `error`, `status`, `next_presets`
- `status` defaults to `Status::None` after deserialization
- After deserialization, history is restored but runtime state is gone. The widget shows the query text from history but no data until re-submitted.

### ConsoleViewMode (removed)

Per user decision: no enum needed. A simple `bool data_view` toggles between data and metadata display. When data is not available, metadata pane is forced regardless of toggle.

## New AppMessage Variant

```rust
// In liquers-lib/src/ui/message.rs
#[derive(Debug, Clone)]
pub enum AppMessage {
    SubmitQuery { handle: UIHandle, query: String },
    /// Request AppRunner to evaluate a query and push AssetSnapshot updates to the handle.
    /// AppRunner monitors the asset lifecycle and delivers UpdateMessage::AssetUpdate
    /// on each notification change. Monitoring auto-stops when the element is removed.
    RequestAssetUpdates {
        handle: UIHandle,
        query: String,
    },
    Quit,
    Serialize { path: String },
    Deserialize { path: String },
}
```

**Note:** `AppMessage` retains `Clone` derive — `RequestAssetUpdates` carries only `handle: UIHandle` and `query: String` (both `Clone`). The `handle` identifies the widget that requested asset monitoring, so AppRunner can deliver snapshots to the correct element and auto-stop when the element is removed.

## New UpdateMessage Variant

```rust
// In liquers-lib/src/ui/element.rs
pub enum UpdateMessage {
    AssetNotification(AssetNotificationMessage),
    Timer { elapsed_ms: u64 },
    Custom(Box<dyn std::any::Any + Send>),
    /// Full asset snapshot pushed by AppRunner.
    /// Delivered whenever the monitored asset's state changes.
    AssetUpdate(AssetSnapshot),
}
```

**Delivery flow:**
1. Widget sends `AppMessage::RequestAssetUpdates { handle, query }` via `ctx.sender`
2. AppRunner receives the message, calls `envref.evaluate(&query)`
3. AppRunner stores the `AssetRef` in its `monitoring` map keyed by `handle`
4. AppRunner takes an initial snapshot and delivers `UpdateMessage::AssetUpdate(snapshot)` to the element
5. On each subsequent `run()` cycle, AppRunner checks `notification_rx.has_changed()` for all monitored assets
6. When a change is detected, AppRunner builds a fresh `AssetSnapshot` and delivers it to the element
7. When the element at `handle` no longer exists in AppState, AppRunner removes it from `monitoring` (auto-stop)

## Trait Implementations

### UIElement for QueryConsoleElement

```rust
#[typetag::serde]
impl UIElement for QueryConsoleElement {
    fn type_name(&self) -> &'static str { "QueryConsoleElement" }
    fn handle(&self) -> Option<UIHandle> { self.handle }
    fn set_handle(&mut self, handle: UIHandle) { self.handle = Some(handle); }
    fn title(&self) -> String { self.title_text.clone() }
    fn set_title(&mut self, title: String) { self.title_text = title; }
    fn clone_boxed(&self) -> Box<dyn UIElement> { Box::new(self.clone()) }

    fn init(&mut self, handle: UIHandle, ctx: &UIContext) -> Result<(), Error> {
        self.set_handle(handle);
        // If query_text is non-empty (e.g. from deserialization or creation), submit it
        if !self.query_text.is_empty() {
            self.submit_query(ctx);
        }
        Ok(())
    }

    fn update(&mut self, message: &UpdateMessage, _ctx: &UIContext) -> UpdateResponse {
        match message {
            UpdateMessage::AssetUpdate(snapshot) => {
                // Full snapshot pushed by AppRunner
                self.value = snapshot.value.clone();
                self.metadata = Some(snapshot.metadata.clone());
                self.error = snapshot.error.clone();
                self.status = snapshot.status;
                if self.value.is_some() {
                    self.data_view = true;
                }
                UpdateResponse::NeedsRepaint
            }
            UpdateMessage::AssetNotification(_) => UpdateResponse::Unchanged,
            UpdateMessage::Timer { .. } => UpdateResponse::Unchanged,
            UpdateMessage::Custom(_) => UpdateResponse::Unchanged,
        }
    }

    fn get_value(&self) -> Option<Arc<Value>> { self.value.clone() }
    fn get_metadata(&self) -> Option<Metadata> {
        self.metadata.clone()
    }

    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &UIContext,
        _app_state: &mut dyn AppState,
    ) -> egui::Response {
        // Toolbar + content area rendering
        // Implementation in Phase 4
        ui.label("QueryConsole")
    }
}
```

## Function Signatures

### QueryConsoleElement Methods

```rust
impl QueryConsoleElement {
    /// Create a new QueryConsoleElement with an initial query.
    pub fn new(title: String, initial_query: String) -> Self;

    /// Submit the current query_text for evaluation.
    /// Pushes to history, sends RequestAssetUpdates { handle, query } message via ctx.
    fn submit_query(&mut self, ctx: &UIContext);

    /// Navigate history backward (undo). Returns true if position changed.
    fn history_back(&mut self) -> bool;

    /// Navigate history forward (redo). Returns true if position changed.
    fn history_forward(&mut self) -> bool;

    /// Resolve next-command presets for the current query and state.
    /// Calls find_next_presets() from liquers_lib::utils.
    /// Populates self.next_presets with fully-constructed query strings.
    fn resolve_presets(&mut self, state: &State<Value>, registry: &CommandMetadataRegistry);

    /// Apply a preset: set query_text to the preset's query and submit.
    fn apply_preset(&mut self, preset_index: usize, ctx: &UIContext);

    /// Render the toolbar: back/forward buttons, query field, preset dropdown, view toggle.
    fn show_toolbar(&mut self, ui: &mut egui::Ui, ctx: &UIContext);

    /// Render the content area: either data view or metadata pane.
    fn show_content(&mut self, ui: &mut egui::Ui, ctx: &UIContext);

    /// Render the scrollable metadata pane.
    fn show_metadata_pane(&self, ui: &mut egui::Ui);
}
```

### Command Function

```rust
/// Command: lui/query_console
/// Creates a QueryConsoleElement from the input state (query text, key, or string).
pub fn query_console<E: Environment<Value = Value>>(
    state: &State<Value>,
) -> Result<Value, Error>
where
    E::Payload: UIPayload,
{
    // Extract query string from state (try_into_string or key encoding)
    // Create QueryConsoleElement::new(title, query_string)
    // Wrap in ExtValue::UIElement
}
```

### AppRunner Additions

```rust
/// Entry in the monitoring map. Tracks a monitored asset and its notification channel.
struct MonitoredAsset<E: Environment> {
    asset_ref: AssetRef<E>,
    notification_rx: tokio::sync::watch::Receiver<AssetNotificationMessage>,
}

impl<E> AppRunner<E>
where
    E: Environment<Value = Value>,
    E::Payload: UIPayload + From<SimpleUIPayload>,
{
    // New field added to AppRunner:
    // monitoring: HashMap<UIHandle, MonitoredAsset<E>>,

    /// Handle RequestAssetUpdates message: call evaluate, subscribe to notifications,
    /// take initial snapshot, deliver to widget, store in monitoring map.
    async fn handle_request_asset_updates(
        &mut self,
        handle: UIHandle,
        query: String,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    );

    /// Poll all monitored assets for notification changes.
    /// Build and deliver AssetSnapshot on change.
    /// Remove entries where the element no longer exists (auto-stop).
    async fn poll_monitored_assets(
        &mut self,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    );

    /// Build an AssetSnapshot from an AssetRef (async: reads lock).
    async fn build_snapshot(asset_ref: &AssetRef<E>) -> AssetSnapshot;

    /// Deliver an AssetSnapshot to an element via update().
    /// Returns false if element no longer exists (caller should remove from monitoring).
    async fn deliver_snapshot(
        handle: UIHandle,
        snapshot: AssetSnapshot,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
        sender: &AppMessageSender,
    ) -> bool;
}
```

**handle_request_asset_updates flow:**
1. Call `self.envref.evaluate(&query)` -> `AssetRef<E>`
2. Subscribe to notifications: `asset_ref.subscribe_to_notifications()` -> `watch::Receiver`
3. Build initial `AssetSnapshot` via `build_snapshot(&asset_ref)`
4. Deliver snapshot to element via `deliver_snapshot(handle, snapshot, app_state, &self.sender)`
5. If element exists, store `MonitoredAsset { asset_ref, notification_rx }` in `self.monitoring`
6. If element doesn't exist, discard (don't start monitoring)

**build_snapshot flow:**
1. `asset_ref.try_poll_state()` -> `Option<State<Value>>`
2. If state available: `value = Some(state.data.clone())`
3. `asset_ref.get_metadata()` -> `Metadata` (always available, contains full metadata at any lifecycle stage)
4. `asset_ref.status()` -> `Status`
5. Check for error in state via `state.error_result()`
6. Return `AssetSnapshot { value, metadata, error, status }`

**poll_monitored_assets flow:**
1. For each `(handle, monitored)` in `self.monitoring`:
   a. Check `monitored.notification_rx.has_changed()` (non-blocking)
   b. If changed: `notification_rx.borrow_and_update()` to acknowledge
   c. Build fresh `AssetSnapshot` via `build_snapshot(&monitored.asset_ref)`
   d. Deliver to element via `deliver_snapshot`
   e. If `deliver_snapshot` returns false (element gone), mark for removal
2. Remove all marked entries from `self.monitoring` (auto-stop)

**run() updated to include Phase 4:**
```rust
pub async fn run(&mut self, app_state: &Arc<tokio::sync::Mutex<dyn AppState>>) -> Result<(), Error> {
    // Phase 1: Process messages (includes RequestAssetUpdates)
    self.process_messages(app_state).await;
    // Phase 2: Auto-evaluate pending nodes
    self.evaluate_pending_nodes(app_state).await;
    // Phase 3: Poll evaluating nodes
    self.poll_evaluating_nodes(app_state).await;
    // Phase 4: Poll monitored assets, push snapshots
    self.poll_monitored_assets(app_state).await;
    Ok(())
}
```

## Sync vs Async Decisions

| Function | Async? | Rationale |
|----------|--------|-----------|
| `QueryConsoleElement::new` | No | Pure construction |
| `submit_query` | No | Sends message via channel (non-blocking) |
| `history_back/forward` | No | Index manipulation |
| `resolve_presets` | No | Registry lookup (in-memory) |
| `show_in_egui` | No | egui render (must be sync) |
| `update` | No | Receives AssetSnapshot synchronously |
| `AppRunner::handle_request_asset_updates` | Yes | Calls `envref.evaluate()`, reads AssetRef |
| `AppRunner::poll_monitored_assets` | Yes | Reads AssetRef lock, delivers to AppState |
| `AppRunner::build_snapshot` | Yes | Reads AssetRef lock |
| `AppRunner::deliver_snapshot` | Yes | Locks AppState |
| `query_console` command | No | Pure construction |

## Integration Points

### File: `liquers-lib/src/ui/widgets/query_console_element.rs` (new)

New widget module. Exports `QueryConsoleElement`.

### File: `liquers-lib/src/ui/message.rs` (modify)

- Add `AssetSnapshot` struct
- Add `RequestAssetUpdates { handle, query }` variant to `AppMessage`
- No breaking changes: `AppMessage` retains `Clone` derive

### File: `liquers-lib/src/ui/element.rs` (modify)

- Add `AssetUpdate(AssetSnapshot)` variant to `UpdateMessage`
- Import `AssetSnapshot` from message module

### File: `liquers-lib/src/ui/runner.rs` (modify)

- Add `MonitoredAsset<E>` struct
- Add `monitoring: HashMap<UIHandle, MonitoredAsset<E>>` field to `AppRunner`
- Add `AppMessage::RequestAssetUpdates` arm in `process_messages()`
- Implement `handle_request_asset_updates()`, `poll_monitored_assets()`, `build_snapshot()`, `deliver_snapshot()`
- Add Phase 4 call in `run()`

### File: `liquers-lib/src/ui/widgets/mod.rs` (modify)

```rust
pub mod query_console_element;
pub mod ui_spec_element;

pub use query_console_element::QueryConsoleElement;
pub use ui_spec_element::UISpecElement;
```

### File: `liquers-lib/src/ui/commands.rs` (modify)

- Add `query_console` function
- Register in `register_lui_commands!` macro

### File: `liquers-lib/src/utils.rs` (new)

New utility module. Exports `NextPreset` struct and `find_next_presets()` function.

### File: `liquers-lib/src/lib.rs` (modify)

- Add `pub mod utils;`

### File: `liquers-lib/src/ui/mod.rs` (modify)

- Re-export `QueryConsoleElement` from widgets

### No new external dependencies

All functionality uses existing crates: `tokio::sync::watch`, `egui`, `serde`. No new channels needed on the widget side — AppRunner owns all monitoring infrastructure.

## Relevant Commands

### New Commands

| Command | Namespace | Parameters | Description |
|---------|-----------|------------|-------------|
| `query_console` | `lui` | state (query/key/string) | Create QueryConsoleElement from input |

```rust
register_command!($cr,
    fn query_console(state) -> result
    namespace: "lui"
    label: "Query Console"
    doc: "Create an interactive query console element"
)?;
```

### Relevant Existing Namespaces

| Namespace | Relevance | Key Commands |
|-----------|-----------|--------------|
| `lui` | Widget registration, tree manipulation | `add`, `remove`, `activate`, `ui_spec` |
| `egui` | Display helpers used in content rendering | `label`, `show_asset_info` |

## Error Handling

### Error Scenarios

| Scenario | Handling | Example |
|----------|----------|---------|
| Query parsing fails | `Error::general_error` | `Error::general_error(format!("Invalid query: {}", e))` |
| Asset evaluation fails | Propagated via AssetSnapshot | Error arrives in `snapshot.error` from `AssetNotificationMessage::ErrorOccurred` or `State::error_result()` |
| Widget removed before delivery | Auto-stop | `deliver_snapshot` detects missing element, returns false, monitoring entry removed |
| Command registry lookup fails | No error | `next_presets` stays empty (graceful degradation) |
| evaluate() fails | Error element | AppRunner sets `AssetViewElement::new_error` on the handle |

### Error Propagation

- Query parse errors shown in the metadata pane as error text
- Evaluation errors received via AssetSnapshot -> stored in `self.error`, shown in metadata pane
- No panics — all fallible operations return `Result` or use `Option`

## Serialization Strategy

### Round-trip Behavior

After deserialization:
- `history` and `history_index` are restored
- `query_text` is restored
- `data_view` is restored
- Runtime fields (`value`, `metadata`, `error`, `status`, `next_presets`) are `None`/empty/default
- Widget shows query text but no data — user must press Enter or call `init()` to re-evaluate

### Serde Annotations

All runtime state uses `#[serde(skip)]`. Persistent state: `handle`, `title_text`, `query_text`, `history`, `history_index`, `data_view`.

Note: `metadata` is runtime-only (`#[serde(skip)]`). After deserialization, metadata is rebuilt when the query is re-evaluated via `init()`.

## Concurrency Considerations

### Thread Safety

- `QueryConsoleElement` is `Send + Sync` (required by UIElement)
- `Arc<Value>` for value sharing (received from snapshot, cheap clone)
- `Metadata` owned clone (received from snapshot each update cycle)
- No channels, no poll functions, no background tasks on the widget side
- Widget is purely passive: receives `AssetSnapshot` via `update()`, stores fields, renders

### AppRunner Side

- `handle_request_asset_updates` runs in async context
- `MonitoredAsset` stores `AssetRef<E>` (generic) and `watch::Receiver`
- `poll_monitored_assets` uses `notification_rx.has_changed()` (non-blocking) for change detection
- `build_snapshot` uses `try_poll_state()` first (non-blocking), falls back to `poll_state().await` if needed
- Snapshot delivery locks AppState briefly to call `element.update()`
- Auto-stop: on each poll cycle, check if element exists in AppState; if not, drop MonitoredAsset

### Widget Passivity Guarantee

The widget has no runtime state management responsibility:
- No `notification_rx` — AppRunner owns the notification channel
- No `poll_state_fn` — AppRunner polls the AssetRef
- No `sync_from_notifications()` — AppRunner pushes snapshots
- No background tasks — AppRunner's `run()` cycle handles everything

## Preset Resolution

### NextPreset and find_next_presets

Presets are resolved via a utility function in `liquers_lib::utils`, not by the widget directly. The function returns fully-constructed query strings so the widget doesn't need to deal with namespace injection or action encoding.

**Note:** `CommandPreset` has a design flaw — it lacks a `ns` (namespace) field, so when a preset action belongs to a different namespace than the query's active context, the action may resolve incorrectly. See `ISSUES.md: CommandPreset Missing Namespace Field`. The `find_next_presets` function works around this by using the command metadata's namespace to inject `ns-<namespace>/` when needed.

```rust
/// A resolved next-command preset with the full query already constructed.
/// The query string has the preset action already appended (with namespace
/// injection if needed), ready to be submitted directly.
pub struct NextPreset {
    /// The complete query string with the preset applied.
    pub query: String,
    /// Human-readable label for UI display.
    pub label: String,
    /// Description of what the preset does.
    pub description: String,
}

/// Find next-command presets for a given query and state.
///
/// Sources of presets (in order):
/// 1. **Explicit presets**: from `CommandMetadata.next` of the last action in the query.
///    The last action's command metadata is found via the registry using the query's
///    active namespace context.
/// 2. **Implicit presets**: based on the value's type identifier from state metadata.
///    For example, a DataFrame value might implicitly suggest `head`, `describe`, etc.
///    These are discovered by scanning the registry for commands whose state argument
///    type matches the current value's type identifier (future extension).
///
/// For each preset, the function constructs the full output query by:
/// 1. Parsing the input query to find the last TransformQuerySegment
/// 2. Determining the active namespace context (from `query.last_ns()` + defaults)
/// 3. For each CommandPreset: encoding the action, prepending `ns-<namespace>/`
///    if the preset's command lives in a different namespace than the active context
/// 4. Appending the (possibly ns-prefixed) action to the query string
///
/// Returns an empty Vec if the query cannot be parsed or has no transform segment.
pub fn find_next_presets(
    query: &str,
    state: &State<Value>,
    registry: &CommandMetadataRegistry,
) -> Vec<NextPreset>;
```

### File: `liquers-lib/src/utils.rs` (new)

New module exporting `NextPreset` and `find_next_presets`. Added to `liquers-lib/src/lib.rs` as `pub mod utils;`.

### How the Widget Uses Presets

The widget stores `Vec<NextPreset>` (not `Vec<CommandPreset>`). The `resolve_presets()` method on QueryConsoleElement calls `find_next_presets()` and stores the result.

```rust
impl QueryConsoleElement {
    /// Resolve next-command presets for the current query.
    /// Called after receiving an AssetUpdate with a value.
    fn resolve_presets(&mut self, state: &State<Value>, registry: &CommandMetadataRegistry) {
        self.next_presets = find_next_presets(&self.query_text, state, registry);
    }
}
```

**Timing:** `resolve_presets()` is called:
- After `update(UpdateMessage::AssetUpdate(..))` delivers a snapshot with a value

### How Presets Are Applied

When a preset is selected from the dropdown:
1. Get `self.next_presets[preset_index].query` — the full query string with preset already applied
2. Set `self.query_text = preset.query.clone()`
3. Call `submit_query(ctx)` (auto-execute)

This is simpler than the previous design because `find_next_presets` already constructs the full query string with namespace handling.

## Compilation Validation

- [x] All type signatures specified
- [x] All trait bounds minimal (UIElement requires Send+Sync+Debug, already satisfied)
- [x] No `unwrap()` or `expect()` in signatures
- [x] All imports documented
- [x] Generic parameters have clear purpose (MonitoredAsset<E> in AppRunner only)
- [x] Follows liquers-patterns: UIElement pattern, no default match arms, typed error constructors
- [x] Widget is non-generic — all generic handling lives in AppRunner
- [x] No channels or background tasks on the widget side
