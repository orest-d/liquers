# Phase 3 Delivery Summary - QueryConsoleElement Integration Tests & Corner Cases

## Deliverables

### 1. Main Document: `phase3-integration-tests-corner-cases.md`

**Content:** ~900 lines covering integration tests and corner case analysis

#### Section A: Integration Tests

**File Location:** `liquers-lib/tests/query_console_integration.rs` (ready to implement)

**6 Complete Integration Tests:**

1. **test_query_console_creation**
   - Verifies element creation via `lui/query_console` command
   - Confirms element appears in AppState
   - Assertions: element type = "QueryConsoleElement"

2. **test_request_asset_flow**
   - Tests the RequestAsset message flow through AppRunner
   - Manually sends RequestAsset with async evaluation
   - Verifies AssetRefData arrives via oneshot channel
   - Assertions: value received and matches expected query result

3. **test_query_console_full_lifecycle**
   - Tests creation, query history, and state persistence
   - Simulates deserialization + init re-evaluation
   - Verifies history is maintained across rounds

4. **test_query_console_error_flow**
   - Submits invalid query (nonexistent command)
   - Verifies error is captured in AssetRefData
   - Assertions: error field is Some or value is None

5. **test_query_console_serialization**
   - Round-trip serialization/deserialization
   - Verifies persistent fields survive: query_text, history, history_index, data_view
   - Verifies runtime fields are cleared: value, notification_rx, asset_ref_rx
   - Tests init() re-evaluation after deserialization

6. **test_query_console_in_ui_spec**
   - Creates query console via UISpec init query
   - Submits `lui/query_console/ns-lui/add-child`
   - Verifies child node is inserted with correct type

**Test Configuration:**
- Runtime: `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]`
- Environment: `DefaultEnvironment<Value, SimpleUIPayload>` with basic commands
- Pattern: matches existing `liquers-lib/tests/ui_runner.rs`
- No `unwrap()` in library code
- Explicit match statements (no `_ =>` defaults)
- Queries use `/` separators (no spaces)

#### Section B: Corner Cases (5 Categories, 17 Detailed Scenarios)

##### 2.1 Memory (2 scenarios)
- **Large Query History:** 10,000 queries, ~5 MB total
  - Expected: serialization < 1 second, navigation O(1)
  - Mitigation: Arc sharing, future truncation possible

- **Large Value Display:** Multi-MB DataFrames/images
  - Expected: Arc prevents copies, non-blocking rendering
  - Mitigation: always use Arc<Value>, streaming in future phases

##### 2.2 Concurrency (3 scenarios)
- **Oneshot Channel Races:** Multiple RequestAsset in flight
  - Expected: each gets independent sender, no cross-talk
  - Mitigation: oneshot channels are inherently isolated

- **Notification Channel Backpressure:** Fast updates, slow polling
  - Expected: watch channel keeps latest, may skip intermediate
  - Mitigation: acceptable, final state always captured

- **Oneshot Sender Dropped:** Receiver dropped before send
  - Expected: send error handled gracefully (logged)
  - Mitigation: `let _ = respond_to.send(data)`, no panic

##### 2.3 Errors (3 scenarios)
- **Invalid Query Syntax:** Malformed query string
  - Expected: parse error caught, wrapped in AssetRefData.error
  - Mitigation: all evaluate() calls wrap in Result

- **Command Registry Lookup Fails:** Metadata not available for preset resolution
  - Expected: graceful degradation (empty presets, no crash)
  - Mitigation: preset resolution is optional

- **Evaluation Fails Mid-Stream:** Error during async evaluation
  - Expected: ErrorOccurred notification captured
  - Mitigation: notification handling is explicit (no defaults)

##### 2.4 Serialization (2 scenarios)
- **Round-Trip with Active Runtime State:** Serialize before evaluation completes
  - Expected: skip fields are dropped, deserialization succeeds
  - Mitigation: #[serde(skip)] on all runtime fields

- **Deserialization Without Runtime:** No tokio runtime during serde_json::from_str
  - Expected: pure sync deserialization, no tokio calls
  - Mitigation: runtime fields skipped, init() called later when runtime available

##### 2.5 Integration (4 scenarios)
- **QueryConsoleElement + UISpec:** Init queries create consoles
  - Expected: UISpec evaluates init, console is inserted
  - Mitigation: init evaluation is synchronous

- **Cross-Crate Pattern:** AppRunner + AssetViewElement + QueryConsoleElement
  - Expected: AssetRefData bridges generic AssetRef<E> to non-generic widget
  - Mitigation: AssetRefData structure is non-generic

- **Performance:** Frequent RequestAsset messages
  - Expected: non-blocking loop, multiple evals in flight
  - Mitigation: try_recv(), no global lock

- **UIElement Trait Compatibility:** QueryConsoleElement must implement full trait
  - Expected: all methods present, Send + Sync + Debug + Serialize
  - Mitigation: Phase 2 specifies all methods

---

### 2. Overview Document: `PHASE3_OVERVIEW.md`

**Content:** ~400 lines covering:

- Summary of test contents
- Implementation checklist for Phase 4
- Struct definitions (QueryConsoleElement, AssetRefData)
- Method signatures for Phase 4
- Key integration points
- Test coverage matrix
- Coding standards and references
- Notes on patterns and performance

---

## Key Design Patterns Documented

### 1. RequestAsset Message Pattern

```rust
AppMessage::RequestAsset {
    query: String,
    respond_to: tokio::sync::oneshot::Sender<AssetRefData>,
}
```

- Handle-less async evaluation
- Oneshot delivery of AssetRefData (value, asset_info, error, notification_rx, next_presets)
- Distinct from SubmitQuery (which is handle-based, inline, with payload)

### 2. Async Evaluation Bridge

```rust
AppRunner::handle_request_asset {
    1. Call envref.evaluate(&query) → AssetRef<E>
    2. Poll initial state
    3. Subscribe to notifications
    4. Extract AssetInfo
    5. Resolve next_presets from CommandMetadataRegistry
    6. Construct non-generic AssetRefData
    7. Send via oneshot channel
}
```

### 3. Non-Blocking Polling

QueryConsoleElement update loop:
- Poll oneshot: `asset_ref_rx.try_recv()` (non-blocking)
- Poll notifications: `notification_rx.has_changed()` (non-blocking)
- Update fields synchronously
- No await in update()

### 4. Serialization Strategy

Persistent fields (serialized):
- `handle`, `title_text`, `query_text`, `history`, `history_index`, `data_view`

Runtime fields (#[serde(skip)]):
- `value`, `asset_info`, `error`, `notification_rx`, `asset_ref_rx`, `next_presets`

Post-deserialization:
- History restored, query text restored
- init() called by AppRunner
- init() submits query if query_text is non-empty
- User sees history but no value until query re-executes

---

## Standards and Conventions Enforced

### Rust Code Quality
- **No unwrap()** in library code (Result propagation or Option::and_then)
- **Explicit match statements** (no `_ =>` catch-all, catches future enum variants at compile time)
- **Typed errors** (Error::general_error, Error::key_not_found, no Error::new)
- **Async trait methods** use #[async_trait]
- **No blocking I/O** in async contexts
- **Arc for shared values** (cheap cloning, thread-safe)
- **Send + Sync + Debug** required for UIElement trait

### Test Quality
- **Tokio multi-threaded runtime** for realistic concurrency testing
- **Non-blocking message loops** (try_recv in a loop, not blocking recv)
- **Deterministic assertions** (verify type, value, presence)
- **Timeout-based retry loops** for async operations (not infinite waits)
- **Result error propagation** in test helpers

### Naming Conventions
- Commands: `query_console` (no namespace prefix in function name)
- Traits: `UIElement`, `ValueInterface`, `AppState`
- Tests: `test_<feature>_<scenario>()` (descriptive names)
- Match arms: all variants explicit

---

## Implementation Readiness

### Files Ready for Phase 4
- `liquers-lib/src/ui/widgets/query_console_element.rs` (new file, ~600 lines)
- `liquers-lib/tests/query_console_integration.rs` (test file, ~450 lines)

### Modifications to Existing Files
- `liquers-lib/src/ui/message.rs` - add RequestAsset variant
- `liquers-lib/src/ui/runner.rs` - add RequestAsset handler
- `liquers-lib/src/ui/commands.rs` - add query_console command
- `liquers-lib/src/ui/widgets/mod.rs` - re-export QueryConsoleElement
- `liquers-lib/src/ui/mod.rs` - re-export QueryConsoleElement

### No External Dependencies Added
All functionality uses existing crates: tokio, serde, egui, liquers-core

---

## Test Execution

When Phase 4 implementation is complete, run:

```bash
# Run query console tests
cargo test --lib --test query_console_integration -- --nocapture

# Run full test suite
cargo test --lib ui::tests

# Run with specific test
cargo test test_query_console_creation -- --nocapture
```

---

## Relationship to Other Phases

| Phase | Focus | Deliverable |
|-------|-------|-------------|
| 1 | High-level design | Feature overview, use cases |
| 2 | Architecture | Data structures, function signatures, error handling |
| **3** | **Integration & Corner Cases** | **Test suite, edge case analysis, standards** |
| 4 | Implementation | QueryConsoleElement code, egui rendering, preset resolution |
| 5+ | Polish & Optimization | Performance tuning, UX refinement |

---

## References

### Phase Documents
- `phase1-high-level-design.md` - Feature overview and commands
- `phase2-architecture.md` - Detailed type signatures and design decisions
- `phase3-integration-tests-corner-cases.md` - **This document** (tests & corners)
- `phase3-examples.md` - Usage examples
- `phase4-implementation.md` - Placeholder for Phase 4 start

### Project Standards
- `/home/orest/zlos/rust/liquers/CLAUDE.md` - Project conventions, match rules, error handling
- `/home/orest/.claude/projects/.../MEMORY.md` - UI interface patterns, Phase 1c/1d notes

### Existing Tests
- `liquers-lib/tests/ui_runner.rs` - Pattern for AppRunner integration tests
- `liquers-lib/tests/ui_spec_integration.rs` - UISpec element testing

---

## Notes for Phase 4 Teams

1. **Test-Driven Development:** Implement QueryConsoleElement to pass the 6 integration tests in order (1-6). Do not implement features not covered by tests.

2. **Corner Cases as Future Work:** Most corner case scenarios document expected behavior, not immediate test implementations. Phase 4 focus is on basic functionality; advanced scenarios (memory limits, heavy concurrency) are deferred to Phase 5+.

3. **Serialization:** The round-trip serialization test (Test 5) is critical for session persistence. Verify all persistent fields survive serde_json round-trip.

4. **Error Handling:** No custom error types. Use only `liquers_core::error::Error` with typed constructors.

5. **Async Patterns:** `query_console` command can be `async fn`, macro will handle wrapping. `AppRunner::handle_request_asset` is async and spawns background tasks.

6. **Performance:** RequestAsset pattern is meant for handle-less, async evaluation. For handle-based, inline evaluation with payload, use SubmitQuery (existing pattern).

---

**Document Status:** Complete and ready for Phase 4 implementation

**Date:** 2026-02-14

**Total Coverage:** 6 integration tests + 17 corner case scenarios across 5 categories
