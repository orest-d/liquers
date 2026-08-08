# Phase 3: Integration Tests & Corner Cases - Overview

## Document Contents

This phase defines comprehensive integration tests and corner case analysis for QueryConsoleElement.

**Main document:** `phase3-integration-tests-corner-cases.md`

### Key Sections

#### 1. Integration Tests (6 Tests)

Located in: `liquers-lib/tests/query_console_integration.rs`

Each test uses the standard pattern from `liquers-lib/tests/ui_runner.rs`:
- `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]`
- `DefaultEnvironment<Value, SimpleUIPayload>` with registered commands
- Non-blocking message channel loop
- Arc<tokio::sync::Mutex<dyn AppState>> wrapping DirectAppState

**Tests:**

1. **test_query_console_creation** - Verify element is created via `lui/query_console` command
2. **test_request_asset_flow** - Verify RequestAsset message processing and AssetRefData delivery via oneshot
3. **test_query_console_full_lifecycle** - Test creation, history management, and deserialization + init re-evaluation
4. **test_query_console_error_flow** - Submit invalid query and verify error is captured in AssetRefData
5. **test_query_console_serialization** - Round-trip serialization: history persists, runtime state clears
6. **test_query_console_in_ui_spec** - Create console via UISpec init query and verify tree insertion

#### 2. Corner Cases (5 Categories, 17 Scenarios)

##### 2.1 Memory
- Large query history (10,000 queries)
- Large value display (multi-MB DataFrames)

**Mitigations:** Arc sharing prevents copies; future phases can add history truncation or pagination.

##### 2.2 Concurrency
- Oneshot channel races (multiple RequestAsset in flight)
- Notification channel backpressure (watch channel keeps latest only)
- Oneshot sender dropped (receiver removed before send completes)

**Mitigations:** Oneshot channels are independent; watch channel semantics handle buffering; errors are logged, not fatal.

##### 2.3 Errors
- Invalid query syntax
- Command registry lookup fails (graceful degradation)
- Evaluation fails mid-stream

**Mitigations:** All `evaluate()` calls wrap errors; AssetRefData has error field; errors don't crash the app.

##### 2.4 Serialization
- Round-trip with active runtime state (skip fields)
- Deserialization without tokio runtime active

**Mitigations:** `#[serde(skip)]` on non-persistent fields; deserialization is pure sync; init() is called when runtime is available.

##### 2.5 Integration
- QueryConsoleElement + UISpec (init queries create consoles)
- Cross-crate pattern: AppRunner + AssetViewElement + QueryConsoleElement
- Performance: frequent RequestAsset messages
- UIElement trait compatibility

**Mitigations:** Non-blocking message loop; AppRunner uses try_recv(); trait methods are all implemented.

---

## Implementation Checklist for Phase 4

### File Implementations
- [ ] `liquers-lib/src/ui/widgets/query_console_element.rs` (new file, ~600 lines)
- [ ] `liquers-lib/src/ui/message.rs` (add RequestAsset variant)
- [ ] `liquers-lib/src/ui/runner.rs` (add RequestAsset handler, implement handle_request_asset)
- [ ] `liquers-lib/src/ui/commands.rs` (add query_console command function)
- [ ] `liquers-lib/src/ui/widgets/mod.rs` (re-export QueryConsoleElement)
- [ ] `liquers-lib/src/ui/mod.rs` (re-export QueryConsoleElement)

### Struct Definition
```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryConsoleElement {
    handle: Option<UIHandle>,
    title_text: String,
    query_text: String,
    history: Vec<String>,
    history_index: usize,
    data_view: bool,
    #[serde(skip)] value: Option<Arc<Value>>,
    #[serde(skip)] asset_info: Option<AssetInfo>,
    #[serde(skip)] error: Option<Error>,
    #[serde(skip)] notification_rx: Option<tokio::sync::watch::Receiver<AssetNotificationMessage>>,
    #[serde(skip)] asset_ref_rx: Option<tokio::sync::oneshot::Receiver<AssetRefData>>,
    #[serde(skip)] next_presets: Vec<CommandPreset>,
}

pub struct AssetRefData {
    pub value: Option<Arc<Value>>,
    pub asset_info: Option<AssetInfo>,
    pub error: Option<Error>,
    pub notification_rx: tokio::sync::watch::Receiver<AssetNotificationMessage>,
    pub next_presets: Vec<CommandPreset>,
}
```

### Methods to Implement
- `QueryConsoleElement::new(title, initial_query)` - constructor
- `submit_query(&mut self, ctx: &UIContext)` - send RequestAsset
- `history_back/forward(&mut self)` - navigation
- `poll_asset_ref(&mut self)` - try_recv from oneshot
- `sync_from_notifications(&mut self)` - poll watch channel
- `resolve_presets(&mut self, registry)` - populate next_presets
- `apply_preset(&mut self, index, ctx)` - append preset and submit
- `show_toolbar/show_content()` - egui rendering helpers
- `update()` - handle UpdateMessage (oneshot, notifications)
- `show_in_egui()` - main rendering
- `get_value()/get_metadata()` - UIElement trait

### Key Integration Points
- AppRunner processes `AppMessage::RequestAsset` in `process_messages()`
- AppRunner implements `handle_request_asset()` calling `evaluate()`, extracting AssetRefData
- lui/query_console command created and registered in register_lui_commands!
- QueryConsoleElement works with UISpec (can be created via init query)
- QueryConsoleElement works with lui/add-child (tree insertion)

### Test Coverage
Run all 6 integration tests before Phase 4 is complete:
```bash
cargo test --lib --test query_console_integration -- --nocapture
```

---

## Coding Standards

### Match Statements
All enum matches must be explicit (no `_ =>` default arm). Examples:

```rust
// Correct: explicit match on all variants
match message {
    UpdateMessage::AssetNotification(n) => { ... },
    UpdateMessage::Custom(_) => { ... },
    UpdateMessage::Timer { .. } => { ... },
}

// Incorrect: default arm hides future variants
match message {
    UpdateMessage::AssetNotification(n) => { ... },
    _ => {},
}
```

### Error Handling
Use typed error constructors from `liquers_core::error`:

```rust
// Correct
Error::general_error("message".to_string())
Error::key_not_found(&key)
Error::from_error(ErrorType::ParseError, external_error)

// Incorrect
Error::new(ErrorType::ParseError, "...")  // Don't use Error::new directly
anyhow::Error  // Don't use external error types
```

### No unwrap() in Library Code
```rust
// Correct
let val = self.value.write().ok().and_then(|v| v.clone())

// Incorrect
let val = self.value.write().unwrap()  // Can panic
```

### Async Patterns
- Use `#[async_trait]` for async trait methods
- Default to async (e.g., `AppRunner::handle_request_asset`)
- Query console command can be async (macro will wrap)
- Render loop uses `blocking_lock()` (safe: outside tokio)

### Serialization
- Derive: `Serialize, Deserialize` via typetag
- Use `#[serde(skip)]` for non-persistent fields
- Persistent: `handle`, `title_text`, `query_text`, `history`, `history_index`, `data_view`
- After deserialization, runtime state is None/empty

---

## References

- Phase 1: `phase1-high-level-design.md` (feature overview, commands)
- Phase 2: `phase2-architecture.md` (data structures, function signatures, error handling)
- CLAUDE.md: Code conventions, error handling, match statement rules
- ui_runner.rs: Existing test pattern for AppRunner integration

---

## Notes

1. **RequestAsset Pattern:** Distinct from SubmitQuery. RequestAsset is handle-less, evaluates via `evaluate()` (async, no payload), and delivers AssetRef via oneshot. SubmitQuery is handle-based, evaluates inline with payload, and commands modify AppState directly.

2. **Notification Polling:** Watch channel `has_changed()` is polled each frame. Intermediate notifications may be dropped (latest value only), but final state is always captured.

3. **History Persistence:** Serialized with the widget. Can grow unbounded; future phases may add truncation (keep last 1000).

4. **Preset Resolution:** Deferred to Phase 4. Requires CommandMetadataRegistry access (via UIContext or app-level extension).

5. **Error Display:** Phase 4 renders errors in metadata pane. Phase 3 just ensures errors are captured and available.

6. **Performance:** AppRunner uses non-blocking `try_recv()` for messages. Multiple RequestAsset messages can be processed in one frame. Evaluations are async, no blocking operations.
