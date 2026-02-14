# Phase 4: Implementation Plan - QueryConsoleElement

## Overview

**Feature:** QueryConsoleElement — browser-like interactive query console widget

**Architecture:** Passive widget receives `AssetSnapshot` updates pushed by AppRunner via new `UpdateMessage::AssetUpdate`. AppRunner monitors assets via `monitoring: HashMap<UIHandle, MonitoredAsset<E>>`. Widget manages query history, preset resolution, and data/metadata view toggle.

**Estimated complexity:** Medium

**Estimated time:** 6-8 hours for experienced Rust developer

**Prerequisites:**
- Phases 1, 2, 3 approved
- All open questions resolved
- No new external dependencies needed

## Implementation Steps

### Step 1: Add AssetSnapshot and RequestAssetUpdates to message.rs

**File:** `liquers-lib/src/ui/message.rs`

**Action:**
- Add `AssetSnapshot` struct (non-generic snapshot of monitored asset)
- Add `RequestAssetUpdates { handle, query }` variant to `AppMessage`
- Add necessary imports

**Code changes:**
```rust
// NEW: Add imports at top
use std::sync::Arc;
use liquers_core::error::Error;
use liquers_core::metadata::{Metadata, Status};
use crate::value::Value;

// NEW: AssetSnapshot struct (before AppMessage enum)
/// Full non-generic snapshot of a monitored asset.
/// Pushed by AppRunner via UpdateMessage::AssetUpdate.
#[derive(Clone, Debug)]
pub struct AssetSnapshot {
    /// The current value (if evaluation has completed successfully).
    pub value: Option<Arc<Value>>,
    /// Full metadata, always available via AssetRef::get_metadata().
    pub metadata: Metadata,
    /// Error from evaluation failure.
    pub error: Option<Error>,
    /// Current asset status.
    pub status: Status,
}

// MODIFY: Add new variant to AppMessage
pub enum AppMessage {
    SubmitQuery { handle: UIHandle, query: String },
    /// Request AppRunner to evaluate a query and push AssetSnapshot updates.
    /// Monitoring auto-stops when the element is removed from AppState.
    RequestAssetUpdates { handle: UIHandle, query: String },
    Quit,
    Serialize { path: String },
    Deserialize { path: String },
}
```

**Validation:**
```bash
cargo check -p liquers-lib
# Expected: Compiles (warnings OK — new types not yet used)
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/message.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-lib/src/ui/message.rs`, Phase 2 architecture
- **Rationale:** Follows existing enum pattern; struct is straightforward

---

### Step 2: Add AssetUpdate variant to UpdateMessage

**File:** `liquers-lib/src/ui/element.rs`

**Action:**
- Add `AssetUpdate(AssetSnapshot)` variant to `UpdateMessage`
- Import `AssetSnapshot` from message module
- Update all existing `match` statements on `UpdateMessage` to include the new variant

**Code changes:**
```rust
// MODIFY: Add import
use super::message::AssetSnapshot;

// MODIFY: Add variant to UpdateMessage
pub enum UpdateMessage {
    AssetNotification(liquers_core::assets::AssetNotificationMessage),
    Timer { elapsed_ms: u64 },
    Custom(Box<dyn std::any::Any + Send>),
    /// Full asset snapshot pushed by AppRunner.
    AssetUpdate(AssetSnapshot),
}
```

**Important:** All existing `match` on `UpdateMessage` must add the new arm (CLAUDE.md: no default arms). Search and update:
- `Placeholder::update()` in element.rs — add `UpdateMessage::AssetUpdate(_) => UpdateResponse::Unchanged`
- `AssetViewElement::update()` in element.rs — add `UpdateMessage::AssetUpdate(_) => UpdateResponse::Unchanged`
- `StateViewElement::update()` in element.rs — add `UpdateMessage::AssetUpdate(_) => UpdateResponse::Unchanged`
- `UISpecElement::update()` in `liquers-lib/src/ui/widgets/ui_spec_element.rs` — add `UpdateMessage::AssetUpdate(_) => UpdateResponse::Unchanged`

**Validation:**
```bash
cargo check -p liquers-lib
# Expected: Compiles with no errors
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/element.rs
git checkout liquers-lib/src/ui/widgets/ui_spec_element.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-lib/src/ui/element.rs`, `liquers-lib/src/ui/widgets/ui_spec_element.rs`, Phase 2 architecture
- **Rationale:** Must find and update all match statements across multiple files

---

### Step 3: Add re-exports for AssetSnapshot in ui/mod.rs

**File:** `liquers-lib/src/ui/mod.rs`

**Action:**
- Re-export `AssetSnapshot` from message module

**Code changes:**
```rust
// MODIFY: Add to existing re-exports
pub use message::AssetSnapshot;
```

**Validation:**
```bash
cargo check -p liquers-lib
# Expected: Compiles
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/mod.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** —
- **Knowledge:** `liquers-lib/src/ui/mod.rs`
- **Rationale:** One-line addition following existing pattern

---

### Step 4: Create utils module with NextPreset and find_next_presets

**File:** `liquers-lib/src/utils.rs` (new)

**Action:**
- Create new module with `NextPreset` struct
- Implement `find_next_presets()` function
- Parse query to find last action, look up command metadata, construct preset queries with namespace injection

**File:** `liquers-lib/src/lib.rs` (modify)

**Action:**
- Add `pub mod utils;`

**Code changes (utils.rs):**
```rust
// NEW FILE: liquers-lib/src/utils.rs
use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::error::Error;
use liquers_core::state::State;
use crate::value::Value;

/// A resolved next-command preset with the full query already constructed.
#[derive(Clone, Debug)]
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
/// Sources of presets:
/// 1. Explicit presets from CommandMetadata.next of the last action
/// 2. (Future) Implicit presets based on value type
///
/// Returns fully-constructed query strings with namespace injection when needed.
/// Returns empty Vec if query cannot be parsed or has no transform segment.
pub fn find_next_presets(
    query: &str,
    _state: &State<Value>,
    registry: &CommandMetadataRegistry,
) -> Vec<NextPreset> {
    // Implementation:
    // 1. Parse query to find last TransformQuerySegment
    // 2. Look up command metadata for the last action
    // 3. For each CommandPreset in metadata.next:
    //    a. Encode preset action
    //    b. If preset's namespace differs from query's active ns, prepend ns-<namespace>/
    //    c. Construct full query = original_query + "/" + (ns-prefix?) + encoded_action
    // 4. Return Vec<NextPreset>
    vec![]
}
```

**Validation:**
```bash
cargo check -p liquers-lib
# Expected: Compiles
```

**Rollback:**
```bash
rm liquers-lib/src/utils.rs
git checkout liquers-lib/src/lib.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture, `liquers-core/src/command_metadata.rs`, `liquers-core/src/query.rs`, `liquers-core/src/parse.rs`, `ISSUES.md` (CommandPreset ns issue)
- **Rationale:** Requires understanding of query parsing, command metadata registry, and namespace injection logic

---

### Step 5: Create QueryConsoleElement widget

**File:** `liquers-lib/src/ui/widgets/query_console_element.rs` (new)

**Action:**
- Define `QueryConsoleElement` struct with persistent and runtime fields
- Implement `UIElement` trait with `#[typetag::serde]`
- Implement widget methods: `submit_query`, `history_back`, `history_forward`, `resolve_presets`, `apply_preset`
- Implement `show_in_egui` with single-row toolbar and content area

**Code changes (key signatures):**
```rust
// NEW FILE: liquers-lib/src/ui/widgets/query_console_element.rs

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryConsoleElement {
    handle: Option<UIHandle>,
    title_text: String,
    pub query_text: String,
    history: Vec<String>,
    history_index: usize,
    data_view: bool,

    #[serde(skip)]
    value: Option<Arc<Value>>,
    #[serde(skip)]
    metadata: Option<Metadata>,
    #[serde(skip)]
    error: Option<Error>,
    #[serde(skip)]
    status: Status,
    #[serde(skip)]
    next_presets: Vec<NextPreset>,
}

impl QueryConsoleElement {
    pub fn new(title: String, initial_query: String) -> Self;
    fn submit_query(&mut self, ctx: &UIContext);
    fn history_back(&mut self) -> bool;
    fn history_forward(&mut self) -> bool;
    fn resolve_presets(&mut self, state: &State<Value>, registry: &CommandMetadataRegistry);
    fn apply_preset(&mut self, preset_index: usize, ctx: &UIContext);
    fn show_toolbar(&mut self, ui: &mut egui::Ui, ctx: &UIContext);
    fn show_content(&mut self, ui: &mut egui::Ui, ctx: &UIContext);
    fn show_metadata_pane(&self, ui: &mut egui::Ui);
}

#[typetag::serde]
impl UIElement for QueryConsoleElement {
    fn type_name(&self) -> &'static str { "QueryConsoleElement" }
    fn handle(&self) -> Option<UIHandle> { self.handle }
    fn set_handle(&mut self, handle: UIHandle) { self.handle = Some(handle); }
    fn title(&self) -> String { self.title_text.clone() }
    fn set_title(&mut self, title: String) { self.title_text = title; }
    fn clone_boxed(&self) -> Box<dyn UIElement> { Box::new(self.clone()) }
    fn init(&mut self, handle: UIHandle, ctx: &UIContext) -> Result<(), Error>;
    fn update(&mut self, message: &UpdateMessage, _ctx: &UIContext) -> UpdateResponse;
    fn get_value(&self) -> Option<Arc<Value>>;
    fn get_metadata(&self) -> Option<Metadata>;
    fn show_in_egui(...) -> egui::Response;
}
```

**Toolbar layout (single row):**
```
[<] [>] [query_text_field___________] [Data/Metadata] [Presets v] [status_icon]
```
- Data/Metadata: single toggle button, label shows current view name
- Initially shows "Metadata" when no value available (forced)
- Presets dropdown only visible when `next_presets` is non-empty

**Validation:**
```bash
cargo check -p liquers-lib
# Expected: Compiles
```

**Rollback:**
```bash
rm liquers-lib/src/ui/widgets/query_console_element.rs
```

**Agent Specification:**
- **Model:** opus
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture (full), Phase 3 examples (full), `liquers-lib/src/ui/element.rs` (UIElement trait, AssetViewElement pattern), `liquers-lib/src/ui/widgets/ui_spec_element.rs` (egui rendering patterns), `liquers-lib/src/ui/message.rs` (AssetSnapshot), `liquers-lib/src/utils.rs` (NextPreset)
- **Rationale:** Core widget with complex UIElement impl, egui rendering, history state machine, preset integration — requires deep architectural understanding

---

### Step 6: Export QueryConsoleElement from widget and UI modules

**File:** `liquers-lib/src/ui/widgets/mod.rs`

**Action:**
- Add `pub mod query_console_element;`
- Add `pub use query_console_element::QueryConsoleElement;`

**File:** `liquers-lib/src/ui/mod.rs`

**Action:**
- Add re-export: `pub use widgets::QueryConsoleElement;`

**Code changes (widgets/mod.rs):**
```rust
pub mod query_console_element;
pub mod ui_spec_element;

pub use query_console_element::QueryConsoleElement;
pub use ui_spec_element::UISpecElement;
```

**Validation:**
```bash
cargo check -p liquers-lib
# Expected: Compiles
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/widgets/mod.rs
git checkout liquers-lib/src/ui/mod.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** —
- **Knowledge:** `liquers-lib/src/ui/widgets/mod.rs`, `liquers-lib/src/ui/mod.rs`
- **Rationale:** Simple module export following existing pattern

---

### Step 7: Add AppRunner monitoring infrastructure

**File:** `liquers-lib/src/ui/runner.rs`

**Action:**
- Add `MonitoredAsset<E>` struct (asset_ref + notification_rx)
- Add `monitoring: HashMap<UIHandle, MonitoredAsset<E>>` field to `AppRunner`
- Initialize `monitoring` in `new()`
- Add `RequestAssetUpdates` arm to `process_messages()`
- Implement `handle_request_asset_updates()` — evaluate, subscribe, build initial snapshot, deliver, store
- Implement `poll_monitored_assets()` — check notification changes, build snapshots, deliver, auto-stop cleanup
- Implement `build_snapshot()` — read AssetRef fields into AssetSnapshot
- Implement `deliver_snapshot()` — lock AppState, call element.update(), return bool (false if element gone)
- Add Phase 4 call `self.poll_monitored_assets(app_state).await` to `run()`

**Code changes (key additions):**
```rust
// NEW: MonitoredAsset struct
struct MonitoredAsset<E: Environment> {
    asset_ref: AssetRef<E>,
    notification_rx: tokio::sync::watch::Receiver<AssetNotificationMessage>,
}

// MODIFY: AppRunner struct — add field
pub struct AppRunner<E: Environment> {
    envref: EnvRef<E>,
    evaluating: HashMap<UIHandle, AssetRef<E>>,
    monitoring: HashMap<UIHandle, MonitoredAsset<E>>,  // NEW
    message_rx: AppMessageReceiver,
    sender: AppMessageSender,
}

// MODIFY: run() — add Phase 4
pub async fn run(...) -> Result<(), Error> {
    self.process_messages(app_state).await;
    self.evaluate_pending_nodes(app_state).await;
    self.poll_evaluating_nodes(app_state).await;
    self.poll_monitored_assets(app_state).await;  // NEW
    Ok(())
}

// MODIFY: process_messages — add arm
AppMessage::RequestAssetUpdates { handle, query } => {
    self.handle_request_asset_updates(handle, query, app_state).await;
}

// NEW: handle_request_asset_updates
// 1. envref.evaluate(&query) → AssetRef
// 2. asset_ref.subscribe_to_notifications() → watch::Receiver
// 3. build_snapshot(&asset_ref) → AssetSnapshot
// 4. deliver_snapshot(handle, snapshot, app_state) → bool
// 5. Store MonitoredAsset in self.monitoring (replaces existing if any)

// NEW: poll_monitored_assets
// For each (handle, monitored) in self.monitoring:
//   if notification_rx.has_changed() → build + deliver snapshot
//   if deliver returns false → mark for removal
// Remove marked entries (auto-stop)

// NEW: build_snapshot
// try_poll_state → value, get_metadata → metadata, status → status
// Check for error in state

// NEW: deliver_snapshot
// Lock AppState, get_element_mut, call update(AssetUpdate(snapshot))
// Return false if element not found
```

**Validation:**
```bash
cargo check -p liquers-lib
# Expected: Compiles
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/runner.rs
```

**Agent Specification:**
- **Model:** opus
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture (full — AppRunner sections), `liquers-lib/src/ui/runner.rs` (current 3-phase pattern), `liquers-core/src/assets.rs` (AssetRef API: subscribe_to_notifications, try_poll_state, poll_state, get_metadata, status), `liquers-lib/src/ui/message.rs` (AssetSnapshot, AppMessage)
- **Rationale:** Most complex step — async monitoring, lock management, auto-stop logic, integration with existing 3-phase runner

---

### Step 8: Add query_console command and registration

**File:** `liquers-lib/src/ui/commands.rs`

**Action:**
- Add `query_console` function (sync — pure construction, like `ui_spec`)
- Register in `register_lui_commands!` macro

**Code changes:**
```rust
// NEW: Command function (sync, like ui_spec)
fn query_console(state: State<Value>) -> Result<Value, Error> {
    let query_string = state.try_into_string()?;
    let element = QueryConsoleElement::new("Query Console".to_string(), query_string);
    Ok(Value::from(ExtValue::UIElement {
        value: Arc::new(element),
    }))
}

// MODIFY: register_lui_commands! macro — add registration
register_command!($cr,
    fn query_console(state) -> result
    namespace: "lui"
    label: "Query Console"
    doc: "Create an interactive query console element"
)?;
```

**Validation:**
```bash
cargo check -p liquers-lib
# Expected: Compiles
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/commands.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-lib/src/ui/commands.rs` (especially `ui_spec` pattern), `specs/REGISTER_COMMAND_FSD.md`
- **Rationale:** Follows established command registration pattern

---

### Step 9: Write unit tests

**File:** `liquers-lib/src/ui/widgets/query_console_element.rs` (append `#[cfg(test)]` module)

**Action:**
- Add 34 unit tests across 7 categories as specified in Phase 3:
  - Construction (4), UIElement Trait (8), History Navigation (6), AssetSnapshot Updates (6), Serialization (4), Presets (3), Initialization (3)

**Validation:**
```bash
cargo test -p liquers-lib --lib query_console_element
# Expected: 34 tests pass
```

**Rollback:**
```bash
# Remove #[cfg(test)] module from query_console_element.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Phase 3 examples doc (unit tests section), `liquers-lib/src/ui/widgets/query_console_element.rs`, `liquers-lib/src/ui/element.rs` (existing test patterns)
- **Rationale:** 34 tests require understanding of test patterns and widget API

---

### Step 10: Write integration tests

**File:** `liquers-lib/tests/query_console_integration.rs` (new)

**Action:**
- Create 6 integration tests as specified in Phase 3:
  1. `test_query_console_creation_via_command`
  2. `test_request_asset_updates_flow`
  3. `test_monitoring_auto_stop`
  4. `test_monitoring_replacement`
  5. `test_error_propagation`
  6. `test_serialization_and_reinit`

**Validation:**
```bash
cargo test -p liquers-lib --test query_console_integration
# Expected: 6 tests pass
```

**Rollback:**
```bash
rm liquers-lib/tests/query_console_integration.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Phase 3 examples doc (integration tests section), `liquers-lib/tests/ui_runner.rs`, `liquers-lib/tests/ui_spec_integration.rs`
- **Rationale:** Integration tests require full environment setup and AppRunner interaction

---

### Step 11: Final validation

**File:** (all files)

**Action:**
- Run full test suite
- Run clippy
- Verify no regressions

**Validation:**
```bash
cargo build -p liquers-lib
cargo test -p liquers-lib
cargo clippy -p liquers-lib -- -D warnings
```

**Expected:**
- All builds succeed
- All tests pass (existing + 34 unit + 6 integration = 40 new)
- No clippy warnings

**Rollback:** N/A (final check)

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** All implementation files
- **Rationale:** Final validation requires judgment on any issues found

## Testing Plan

### Unit Tests

**When to run:** After Step 9

**File:** `liquers-lib/src/ui/widgets/query_console_element.rs` (inline `#[cfg(test)]`)

**Command:**
```bash
cargo test -p liquers-lib --lib query_console_element
```

**Expected:**
- 34 new unit tests pass
- Existing unit tests still pass

### Integration Tests

**When to run:** After Step 10

**File:** `liquers-lib/tests/query_console_integration.rs`

**Command:**
```bash
cargo test -p liquers-lib --test query_console_integration
```

**Expected:**
- 6 integration tests pass
- Existing integration tests still pass

### Manual Validation

**When to run:** After Step 11

**Commands:**
```bash
# 1. Run existing ui_spec_demo to verify no regressions
cargo run -p liquers-lib --example ui_spec_demo
# Expected: UI renders without errors

# 2. Run full workspace tests
cargo test --workspace
# Expected: All tests pass
```

## Task Splitting (Agent Assignments)

| Step | Model | Skills | Rationale |
|------|-------|--------|-----------|
| 1 | haiku | rust-best-practices | Struct + enum variant (follows pattern) |
| 2 | sonnet | rust-best-practices | Must update match arms across files |
| 3 | haiku | — | One-line re-export |
| 4 | sonnet | rust-best-practices | Query parsing + namespace logic |
| 5 | opus | rust-best-practices | Core widget (complex UIElement + egui) |
| 6 | haiku | — | Module exports (follows pattern) |
| 7 | opus | rust-best-practices | AppRunner monitoring (async + locks) |
| 8 | haiku | rust-best-practices | Command registration (follows pattern) |
| 9 | sonnet | rust-best-practices, liquers-unittest | 34 unit tests |
| 10 | sonnet | rust-best-practices, liquers-unittest | 6 integration tests |
| 11 | sonnet | rust-best-practices | Final validation |

## Rollback Plan

### Full Feature Rollback

```bash
git checkout main
git branch -D feature/query-console-element
```

**New files to delete:**
```
liquers-lib/src/ui/widgets/query_console_element.rs
liquers-lib/src/utils.rs
liquers-lib/tests/query_console_integration.rs
```

**Modified files to restore:**
```
liquers-lib/src/ui/message.rs
liquers-lib/src/ui/element.rs
liquers-lib/src/ui/runner.rs
liquers-lib/src/ui/widgets/mod.rs
liquers-lib/src/ui/mod.rs
liquers-lib/src/ui/commands.rs
liquers-lib/src/lib.rs
```

### Partial Completion

If partially complete but need to pause:
1. Create feature branch: `git checkout -b feature/query-console-element`
2. Commit WIP: `git commit -m "WIP: QueryConsoleElement - completed steps 1-N"`
3. Update `specs/query-console-element/DESIGN.md` with completion status
4. Resume later from last completed step

## Documentation Updates

### CLAUDE.md

**No updates needed** — QueryConsoleElement follows existing UIElement patterns.

### MEMORY.md

**Update after completion:**
- Add QueryConsoleElement section with file location, key patterns
- Document AssetSnapshot / RequestAssetUpdates / monitoring pattern
- Note find_next_presets utility location

### ISSUES.md

**Already updated** — CommandPreset Missing Namespace Field issue documented.
