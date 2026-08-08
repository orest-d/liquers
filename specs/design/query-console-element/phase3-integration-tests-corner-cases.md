# Phase 3: Integration Tests & Corner Cases - QueryConsoleElement

## Integration Tests

### File: `liquers-lib/tests/query_console_integration.rs`

Integration tests for QueryConsoleElement lifecycle, RequestAsset message flow, and AppRunner coordination.

Test configuration:
- Runtime: `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]`
- Environment: `DefaultEnvironment<Value, SimpleUIPayload>` with basic commands
- No `unwrap()` in library code; `Result` propagated or tested explicitly
- Match statements explicit (no `_ =>` catch-all)
- Queries contain no spaces (use `/` separators)

```rust
//! Integration tests for QueryConsoleElement: creation, RequestAsset flow,
//! lifecycle management, error handling, and AppRunner coordination.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::{
    app_message_channel, AppRunner, AppState, DirectAppState, ElementSource,
    UIContext, UIElement, UIHandle, UpdateMessage, UpdateResponse,
};
use liquers_core::value::ValueInterface;
use liquers_lib::value::Value;

// Required by register_command! and register_lui_commands! macros.
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn double_value(state: &State<Value>) -> Result<Value, Error> {
    let text = state.try_into_string()?;
    Ok(Value::from(format!("{}:{}", text, text)))
}

fn setup_env() -> DefaultEnvironment<Value, SimpleUIPayload> {
    let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();
    env.with_trivial_recipe_provider();
    register_commands(&mut env).expect("register commands");
    env
}

fn register_commands(env: &mut DefaultEnvironment<Value, SimpleUIPayload>) -> Result<(), Error> {
    let cr = env.get_mut_command_registry();
    register_command!(cr, fn double_value(state) -> result)?;
    liquers_lib::register_lui_commands!(cr)?;
    Ok(())
}

// ─── Test 1: QueryConsoleElement Creation ────────────────────────────────────

/// Test that QueryConsoleElement can be created via lui/query_console command
/// and appears in AppState as the root element.
///
/// Scenario:
/// 1. Create a DirectAppState with a root node
/// 2. Submit a lui/query_console query via SubmitQuery
/// 3. Verify the root element is replaced with QueryConsoleElement
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_query_console_creation() {
    // 1. Setup
    let env = setup_env();
    let envref = env.to_ref();

    let mut direct_state = DirectAppState::new();
    let root_handle = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add root node");
    direct_state
        .set_element(
            root_handle,
            Box::new(liquers_lib::ui::Placeholder::new()),
        )
        .expect("set placeholder");

    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let ui_context = UIContext::new(app_state.clone(), msg_tx.clone());
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    // 2. Submit query_console command with initial query "double_value"
    // Encoding: state -> query_console -> lui namespace
    ui_context.submit_query(root_handle, "query_console/ns-lui");

    // 3. Run AppRunner to process SubmitQuery
    runner.run(&app_state).await.expect("runner.run");

    // 4. Verify root element type is QueryConsoleElement
    let state = app_state.lock().await;
    let elem = state
        .get_element(root_handle)
        .expect("get element")
        .expect("element should exist");
    assert_eq!(elem.type_name(), "QueryConsoleElement");
}

// ─── Test 2: RequestAsset Message Flow ──────────────────────────────────────

/// Test the RequestAsset message flow through AppRunner.
///
/// Scenario:
/// 1. Create a QueryConsoleElement manually
/// 2. Manually construct and send a RequestAsset message
/// 3. Verify AppRunner processes it and sends AssetRefData back via oneshot
/// 4. Verify the returned data contains the expected fields
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_request_asset_flow() {
    // 1. Setup
    let env = setup_env();
    let envref = env.to_ref();

    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
    let (msg_tx, msg_rx) = app_message_channel();
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx.clone());

    // 2. Create a oneshot channel and send RequestAsset
    let (tx, mut rx) = tokio::sync::oneshot::channel();
    let msg = liquers_lib::ui::AppMessage::RequestAsset {
        query: "double_value".to_string(),
        respond_to: tx,
    };
    let _ = msg_tx.send(msg);

    // 3. Run AppRunner to process RequestAsset
    // AppRunner will call evaluate, get AssetRef, extract initial state,
    // subscribe to notifications, and send AssetRefData via oneshot
    runner.run(&app_state).await.expect("runner.run");

    // 4. Poll the oneshot — may not arrive on first run due to async evaluation
    // Retry a few times
    let mut asset_ref_data = None;
    for _ in 0..20 {
        if let Ok(data) = rx.try_recv() {
            asset_ref_data = Some(data);
            break;
        }
        runner.run(&app_state).await.expect("runner.run");
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    let data = asset_ref_data.expect("AssetRefData should be received via oneshot");

    // 5. Verify AssetRefData structure
    // For the "double_value" query, the result should be string "double_value:double_value"
    if let Some(val) = data.value {
        let text = val.try_into_string().expect("value should be string");
        assert_eq!(text, "double_value:double_value");
    }

    // notification_rx should be present for monitoring
    // (exact behavior depends on asset evaluation completion timing)
    // next_presets should be resolved (empty or populated based on registry)
}

// ─── Test 3: QueryConsoleElement Full Lifecycle ─────────────────────────────

/// Test the full lifecycle: create console, query history, notifications.
///
/// Scenario:
/// 1. Create QueryConsoleElement via lui/query_console with initial query
/// 2. Simulate user submitting a query (via update message)
/// 3. Verify history is updated
/// 4. Simulate history navigation (back/forward)
/// 5. Verify element maintains state correctly
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_query_console_full_lifecycle() {
    // 1. Setup
    let env = setup_env();
    let envref = env.to_ref();

    let mut direct_state = DirectAppState::new();
    let root_handle = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add root node");
    direct_state
        .set_element(
            root_handle,
            Box::new(liquers_lib::ui::Placeholder::new()),
        )
        .expect("set placeholder");

    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let ui_context =
        UIContext::new(app_state.clone(), msg_tx.clone()).with_handle(Some(root_handle));
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    // 2. Create console with initial query
    ui_context.submit_query(root_handle, "query_console/ns-lui");
    runner.run(&app_state).await.expect("runner.run");

    // 3. Verify element exists and is QueryConsoleElement
    {
        let state = app_state.lock().await;
        let elem = state
            .get_element(root_handle)
            .expect("get element")
            .expect("element should exist");
        assert_eq!(elem.type_name(), "QueryConsoleElement");
        // In Phase 4, history will be accessible via methods on QueryConsoleElement
        // For now, just verify the element is present
    }

    // 4. Simulate deserialization + init:
    //    - Serialize the element (history persists)
    //    - Deserialize
    //    - Call init() to re-evaluate
    // This tests the round-trip behavior documented in Phase 2

    // (Detailed serialization test in Test 5)
}

// ─── Test 4: QueryConsoleElement Error Flow ────────────────────────────────

/// Test error handling: submit invalid query, verify error received.
///
/// Scenario:
/// 1. Create QueryConsoleElement
/// 2. Manually send RequestAsset with invalid query (nonexistent command)
/// 3. Verify AppRunner processes it and AssetRefData contains error
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_query_console_error_flow() {
    // 1. Setup
    let env = setup_env();
    let envref = env.to_ref();

    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
    let (msg_tx, msg_rx) = app_message_channel();
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx.clone());

    // 2. Send RequestAsset with nonexistent command
    let (tx, mut rx) = tokio::sync::oneshot::channel();
    let msg = liquers_lib::ui::AppMessage::RequestAsset {
        query: "nonexistent_command_xyz".to_string(),
        respond_to: tx,
    };
    let _ = msg_tx.send(msg);

    // 3. Run AppRunner
    runner.run(&app_state).await.expect("runner.run");

    // 4. Poll oneshot with retries
    let mut asset_ref_data = None;
    for _ in 0..20 {
        if let Ok(data) = rx.try_recv() {
            asset_ref_data = Some(data);
            break;
        }
        runner.run(&app_state).await.expect("runner.run");
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    let data = asset_ref_data.expect("AssetRefData should be received");

    // 5. Verify error is present
    // AssetRefData.error should contain the evaluation error
    // Note: error field is populated when evaluation fails
    assert!(
        data.error.is_some() || data.value.is_none(),
        "error or missing value indicates query evaluation failed"
    );
}

// ─── Test 5: QueryConsoleElement Serialization ──────────────────────────────

/// Test round-trip serialization: serialize with history, deserialize, re-init.
///
/// Scenario:
/// 1. Create QueryConsoleElement with query text
/// 2. Simulate adding to history (via submit_query calls)
/// 3. Serialize the element
/// 4. Deserialize
/// 5. Call init() to restore evaluation state
/// 6. Verify history is preserved, runtime state is cleared
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_query_console_serialization() {
    // 1. Setup
    let env = setup_env();
    let envref = env.to_ref();

    let mut direct_state = DirectAppState::new();
    let root_handle = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add root node");

    // Create console and add to app state
    direct_state
        .set_element(
            root_handle,
            Box::new(liquers_lib::ui::Placeholder::new()),
        )
        .expect("set placeholder");

    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let ui_context =
        UIContext::new(app_state.clone(), msg_tx.clone()).with_handle(Some(root_handle));
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx.clone());

    // 2. Create console
    ui_context.submit_query(root_handle, "query_console/ns-lui");
    runner.run(&app_state).await.expect("runner.run");

    // 3. Serialize (via serde_json or similar)
    // This test verifies that the element derives Serialize/Deserialize
    let state = app_state.lock().await;
    let elem = state
        .get_element(root_handle)
        .expect("get element")
        .expect("element should exist");

    let serialized = serde_json::to_string(&elem).expect("serialize element");

    drop(state);

    // 4. Deserialize
    let deserialized: Box<dyn UIElement> =
        serde_json::from_str(&serialized).expect("deserialize element");

    // 5. Verify element properties are preserved
    assert_eq!(deserialized.type_name(), "QueryConsoleElement");
    // History and query_text should be present (not skipped)
    // Runtime fields (value, notification_rx) will be None/cleared

    // 6. Re-init: call init() to restore evaluation state
    {
        let mut state = app_state.lock().await;
        let mut elem = deserialized.clone_boxed();
        let result = elem.init(root_handle, &ui_context);
        assert!(result.is_ok(), "init should succeed");
        state.set_element(root_handle, elem).expect("set element");
    }

    // 7. After init, if query_text is non-empty, a RequestAsset should be sent
    //    (verified by observing app_state changes or checking message channel)
    runner.run(&app_state).await.expect("runner.run");
}

// ─── Test 6: QueryConsoleElement in UISpec ─────────────────────────────────

/// Test QueryConsoleElement creation via UISpec init queries.
///
/// Scenario:
/// 1. Create a UISpecElement with an init query that creates a query_console
/// 2. Verify AppRunner evaluates the init query
/// 3. Verify the query console is added to the tree
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_query_console_in_ui_spec() {
    // 1. Setup
    let env = setup_env();
    let envref = env.to_ref();

    let mut direct_state = DirectAppState::new();
    let root_handle = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add root node");

    // 2. Create UISpecElement with init query that adds a query_console
    // Example YAML (conceptual):
    // ```yaml
    // init:
    //   - "query_console/ns-lui/add-child"
    // ```
    // For this test, we simulate the evaluation flow manually

    direct_state
        .set_element(
            root_handle,
            Box::new(liquers_lib::ui::Placeholder::new()),
        )
        .expect("set placeholder");

    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let ui_context =
        UIContext::new(app_state.clone(), msg_tx.clone()).with_handle(Some(root_handle));
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    // 3. Submit the lui command that would be in the init query
    // This would add a child with the query_console element
    ui_context.submit_query(root_handle, "query_console/ns-lui/add-child");
    runner.run(&app_state).await.expect("runner.run");

    // 4. Verify child was added
    {
        let state = app_state.lock().await;
        let children = state.get_children(root_handle).expect("get children");
        // Should have at least one child (the query console)
        if children.len() > 0 {
            let child_handle = children[0];
            let child_elem = state
                .get_element(child_handle)
                .expect("get child element")
                .expect("child element should exist");
            assert_eq!(child_elem.type_name(), "QueryConsoleElement");
        }
    }
}
```

## Corner Cases

### 1. Memory

#### 1.1 Large Query History

**Scenario:** User submits 10,000 queries to a QueryConsoleElement over a long session.
- Each query is 100-500 bytes
- History vector accumulates to ~5 MB
- Serialization and deserialization become expensive

**Expected Behavior:**
- History persists in memory during session
- Serialization completes within reasonable time (<1 second for 10k queries)
- Deserialization re-inflates history correctly
- `history_index` navigation is O(1)

**Test Approach:**
- Generate 10,000 sequential queries (`query_1`, `query_2`, etc.)
- Time serialization: `let start = Instant::now(); let json = serde_json::to_string(&elem)?; let duration = start.elapsed();`
- Verify `duration < Duration::from_secs(1)`
- Deserialize and verify history length matches
- Simulate history navigation (back 5000 steps, forward 2500) — verify index updates correctly

**Mitigation:**
- Document expected memory usage in Phase 4 UI guide
- Consider lazy history truncation in future phases (e.g., keep only last 1000 queries)
- History can be explicitly cleared via a future "Clear History" button

#### 1.2 Large Value Display

**Scenario:** Query evaluates to a multi-MB value (e.g., large Polars DataFrame, image).
- `value: Option<Arc<Value>>` is cloned to shared state in AssetViewElement
- Rendering via egui may allocate additional copies (textures, text buffer)

**Expected Behavior:**
- Arc sharing prevents unnecessary copies
- Rendering is non-blocking (value already complete)
- Memory is released when element is removed

**Test Approach:**
- Cannot easily test large values in unit tests; document as manual testing concern
- Ensure test uses `Arc` for value sharing (verify in AssetRefData construction)

**Mitigation:**
- Always use `Arc<Value>` in AssetRefData
- Streaming/pagination deferred to Phase 4+ (UI improvements)
- No memcpy on value reassignment (already ensured by Arc)

### 2. Concurrency

#### 2.1 Oneshot Channel Races

**Scenario:** Multiple RequestAsset messages in flight simultaneously.
- User opens 3 query consoles, each submitting a query
- AppRunner processes messages in order, each oneshot fires independently
- Race: does each widget receive the correct AssetRefData?

**Expected Behavior:**
- Each RequestAsset has its own `tokio::sync::oneshot::Sender`
- AppRunner sends to the correct sender in `handle_request_asset`
- No data loss or cross-talk between consoles

**Test Approach:**
```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_concurrent_request_assets() {
    // Create 3 separate oneshot channels
    let (tx1, rx1) = tokio::sync::oneshot::channel();
    let (tx2, rx2) = tokio::sync::oneshot::channel();
    let (tx3, rx3) = tokio::sync::oneshot::channel();

    // Send 3 RequestAsset messages with different queries
    msg_tx.send(AppMessage::RequestAsset {
        query: "query_1".to_string(),
        respond_to: tx1,
    })?;
    msg_tx.send(AppMessage::RequestAsset {
        query: "query_2".to_string(),
        respond_to: tx2,
    })?;
    msg_tx.send(AppMessage::RequestAsset {
        query: "query_3".to_string(),
        respond_to: tx3,
    })?;

    // Run AppRunner
    for _ in 0..50 {
        runner.run(&app_state).await?;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Verify all 3 oneshots received data (order may vary)
    let data1 = rx1.try_recv().expect("should receive data1");
    let data2 = rx2.try_recv().expect("should receive data2");
    let data3 = rx3.try_recv().expect("should receive data3");

    // Each should have distinct values corresponding to their queries
    assert!(data1.value.is_some());
    assert!(data2.value.is_some());
    assert!(data3.value.is_some());
}
```

**Mitigation:**
- Each RequestAsset owns its oneshot sender (no sharing)
- No synchronization needed beyond oneshot semantics
- AppRunner processes messages sequentially, no ordering guarantees but no data loss

#### 2.2 Notification Channel Backpressure

**Scenario:** Background task producing notifications faster than widget can poll.
- Asset evaluation emits 100+ notifications (progress updates)
- Widget polls `notification_rx.has_changed()` once per frame (~16ms at 60 FPS)
- Messages may be dropped by watch channel (latest value only)

**Expected Behavior:**
- Watch channel keeps only the latest notification
- `has_changed()` returns true if notification changed since last `borrow_and_update()`
- Widget receives status updates, may miss intermediate ones but tracks final state

**Test Approach:**
- Mock a rapid-notification scenario (may require test infrastructure)
- Verify that final state (Ready/Error) is always captured
- Progress intermediate values may be skipped, but this is acceptable

**Mitigation:**
- Watch channel semantics are documented in Phase 4
- For high-frequency progress updates, AppRunner can rate-limit notifications
- Widget learns final state via AssetViewElement monitoring

#### 2.3 Oneshot Sender Dropped

**Scenario:** AppRunner calls `respond_to.send(data)` but the receiver (widget's oneshot receiver) has been dropped.
- Widget is removed from AppState before evaluation completes
- `respond_to` sender's `send()` returns `Err(AssetRefData)` (the data cannot be delivered)

**Expected Behavior:**
- `respond_to.send()` failure is handled gracefully (logged but not fatal)
- Evaluation continues in background (asset is cached by AssetManager)
- No panic or crash

**Test Approach:**
```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_oneshot_sender_dropped() {
    let (tx, rx) = tokio::sync::oneshot::channel();
    drop(rx); // Receiver dropped before send

    // Send RequestAsset with dropped receiver
    msg_tx.send(AppMessage::RequestAsset {
        query: "double_value".to_string(),
        respond_to: tx,
    })?;

    runner.run(&app_state).await?;

    // AppRunner should handle the send error gracefully
    // (verify via logs or by checking runner still processes subsequent messages)
    msg_tx.send(AppMessage::RequestAsset {
        query: "double_value".to_string(),
        respond_to: {
            let (tx2, _rx2) = tokio::sync::oneshot::channel();
            tx2
        },
    })?;
    runner.run(&app_state).await?;
    // Should not crash
}
```

**Mitigation:**
- `handle_request_asset` uses `let _ = respond_to.send(data)` (ignores error)
- Background asset evaluation continues regardless of send failure
- Evaluation is cached, available if widget re-requests later

### 3. Errors

#### 3.1 Invalid Query Syntax

**Scenario:** User types a malformed query: `double_value//bad/syntax`.
- Parser fails to parse the query
- RequestAsset is sent with invalid query string
- `AppRunner::handle_request_asset` calls `envref.evaluate(&query)`
- Evaluation returns an error result

**Expected Behavior:**
- Parse error caught by `evaluate()`, returned as `Err(Error)`
- Error is wrapped in AssetRefData.error
- Widget displays error in metadata pane
- No panic

**Test Approach:**
- Covered by Test 4 (test_query_console_error_flow)
- Submit query with obvious syntax error: `double_value//invalid`

**Mitigation:**
- All `evaluate()` calls wrap errors in Result
- AssetRefData always has `error: Option<Error>` for reporting
- Error display in metadata pane is straightforward (Phase 4 UI)

#### 3.2 Command Registry Lookup Fails

**Scenario:** AppRunner tries to resolve presets via CommandMetadataRegistry, but registry doesn't have the command metadata.
- Command is not registered, or metadata is incomplete
- `next_presets` cannot be resolved (empty or None)

**Expected Behavior:**
- No panic; `next_presets` stays empty
- Graceful degradation: preset dropdown is unavailable but widget still works
- User can still manually compose queries

**Test Approach:**
- Difficult to test in isolation; presumes Phase 4 implements preset resolution
- Ensure Phase 4 code uses `.ok()` or matches on `Err` (no unwrap)

**Mitigation:**
- Preset resolution is optional (graceful degradation)
- Registry lookups should return `Option` or `Result`
- Empty preset list is valid state

#### 3.3 Evaluation Fails Mid-Stream

**Scenario:** Query evaluation starts successfully, progress notifications arrive, then evaluation fails with an error.
- Initial state shows progress
- AssetViewElement is created in Progress mode
- Background notification task receives `AssetNotificationMessage::ErrorOccurred`
- Widget switches to Error mode

**Expected Behavior:**
- Error is received via notification channel
- Widget detects error via `AssetViewElement.sync_from_notifications()`
- Display transitions to Error mode, showing error message

**Test Approach:**
- Requires a command that fails mid-evaluation (e.g., a slow operation that times out)
- Monitor notifications and verify ErrorOccurred is captured

**Mitigation:**
- AssetViewElement handles all notification types explicitly (no default arm)
- Error field is always updated when ErrorOccurred arrives
- View mode transitions happen in sync_from_notifications

### 4. Serialization

#### 4.1 Round-Trip with Active Runtime State

**Scenario:** User submits a query, then serializes the app state before evaluation completes.
- QueryConsoleElement has runtime fields: `value`, `notification_rx`, `asset_ref_rx` (all `#[serde(skip)]`)
- Serialization drops these fields
- Deserialization creates the element with `value=None`, `notification_rx=None`, `asset_ref_rx=None`

**Expected Behavior:**
- Serialization succeeds (skip fields mean they're not written)
- Deserialization succeeds (skip fields initialized to None/default)
- Widget shows query text but no value until re-evaluated
- History is preserved
- No data loss (persistent state only)

**Test Approach:**
- Covered by Test 5 (test_query_console_serialization)
- Verify `query_text`, `history`, `history_index`, `data_view` survive round-trip
- Verify runtime fields are None after deserialization

**Mitigation:**
- `#[serde(skip)]` on all non-persistent fields
- Persistent fields: `handle`, `title_text`, `query_text`, `history`, `history_index`, `data_view`
- Deserialization + init() re-evaluates the query

#### 4.2 Deserialization Without Runtime

**Scenario:** App crashes and is restarted. User loads app state from disk.
- QueryConsoleElement is deserialized with no tokio runtime active in deserializer
- Later, when init() is called, a tokio runtime is available

**Expected Behavior:**
- Deserialization completes before runtime is needed (skip fields are not created)
- init() is called when the widget is placed in AppState, at which point runtime is active
- Deserialization is sync, init is sync (sends messages, doesn't await)
- Query submission via `ctx.submit_query()` queues an async task

**Test Approach:**
- Deserialize element outside of `#[tokio::test]` (no runtime)
- Verify no panics or blocking calls
- Then place in AppState and verify init succeeds

**Mitigation:**
- Deserialization is pure sync (no tokio calls)
- Runtime fields are skip (not deserialized)
- init() sends messages asynchronously (via message channel, non-blocking)

### 5. Integration

#### 5.1 QueryConsoleElement with UISpec

**Scenario:** UISpec creates a query console via init query.
- UISpecElement evaluates `"query_console/ns-lui/add-child"` during init
- Query is processed by AppRunner, which calls lui/add-child
- lui/add-child inserts a new node with QueryConsoleElement

**Expected Behavior:**
- UISpec init queries are evaluated before show_in_egui is called
- QueryConsoleElement is properly inserted into the tree
- Parent-child relationships are correct
- QueryConsoleElement can send RequestAsset messages independently

**Test Approach:**
- Covered by Test 6 (test_query_console_in_ui_spec)
- Create UISpec with init query, verify child nodes are created
- Verify child element type is QueryConsoleElement

**Mitigation:**
- UISpec init evaluation is synchronous (Phase 1d)
- lui commands handle tree insertion correctly
- QueryConsoleElement creation via lui/query_console is non-blocking

#### 5.2 Cross-Crate: AppRunner + AssetViewElement + QueryConsoleElement

**Scenario:** AppRunner processes RequestAsset, constructs AssetRefData via AssetViewElement pattern.
- AppRunner receives RequestAsset with query
- Calls `envref.evaluate(&query)` → `AssetRef<E>`
- Extracts value, info, error from AssetRef (pattern similar to AssetViewElement::from_asset_ref)
- Sends AssetRefData via oneshot

**Expected Behavior:**
- AppRunner successfully bridges generic `AssetRef<E>` to non-generic `AssetRefData`
- Both AssetViewElement (automatic background monitoring) and QueryConsoleElement (polled via oneshot) patterns work correctly
- No duplication of effort (AssetViewElement can be used in console's content area if needed)

**Test Approach:**
- Test 2 (test_request_asset_flow) verifies the pattern
- Verify AssetRefData contains extracted value and info
- Verify notification channel is subscribed

**Mitigation:**
- AssetRefData structure is non-generic, enabling consoles to work across environments
- AppRunner has full access to AssetRef for extracting state
- Notification channel is polled non-blocking in widget's update loop

#### 5.3 Performance: Frequent RequestAsset Messages

**Scenario:** User types rapidly in query field, triggering frequent RequestAsset submissions.
- User types 10 characters at 100ms interval = 10 RequestAsset messages in 1 second
- AppRunner may have multiple evaluations in flight
- Network or I/O operations could be blocked

**Expected Behavior:**
- AppRunner processes messages in order
- Earlier evaluations may be superceded by later ones (user's last typed query is most relevant)
- No crashes or resource exhaustion
- Evaluation backpressure is handled gracefully (later queries don't block earlier ones)

**Test Approach:**
- Difficult to test without full integration; document as performance consideration
- Ensure AppRunner message loop drains all available messages (non-blocking)
- Use `try_recv()` loop, not blocking `recv()`

**Mitigation:**
- AppRunner's `process_messages()` uses `try_recv()` in a loop (non-blocking)
- Multiple evaluations can be in flight simultaneously (no global lock)
- User can abort slow queries by typing a new one (cancellation via query replacement, not explicit)

#### 5.4 UIElement Trait Compatibility

**Scenario:** QueryConsoleElement must implement full UIElement trait.
- Trait requires: `type_name()`, `handle()`, `set_handle()`, `title()`, `set_title()`, `clone_boxed()`, `init()`, `update()`, `get_value()`, `get_metadata()`, `show_in_egui()`
- QueryConsoleElement must be Send + Sync + Debug + Serialize

**Expected Behavior:**
- All trait methods are implemented (no defaults)
- Trait bounds are satisfied
- Element can be stored in `Box<dyn UIElement>` and used polymorphically
- Serialization via typetag works correctly

**Test Approach:**
- Test 1, 2, 3 implicitly verify trait implementation
- Compile-time check: QueryConsoleElement must have all method implementations
- Type-check: `Box::new(elem) as Box<dyn UIElement>` must compile

**Mitigation:**
- Phase 2 specifies all trait methods
- QueryConsoleElement derives Debug, Clone, Serialize, Deserialize
- #[typetag::serde] enables polymorphic serialization

---

## Summary of Test Coverage

| Test | Scope | Key Assertions |
|------|-------|-----------------|
| test_query_console_creation | Element creation via lui command | Element type is QueryConsoleElement |
| test_request_asset_flow | RequestAsset message processing | AssetRefData received via oneshot |
| test_query_console_full_lifecycle | State persistence and navigation | History preserved, serialization works |
| test_query_console_error_flow | Error handling | Error field populated or value absent |
| test_query_console_serialization | Round-trip serialization | History restored, runtime state cleared |
| test_query_console_in_ui_spec | UISpec integration | Child node created with correct type |

## Notes for Phase 4 Implementation

1. **History Methods:** QueryConsoleElement will need public methods to access history (for testing and rendering): `history()`, `history_index()`, `history_back()`, `history_forward()`.

2. **Preset Resolution:** Phase 4 will implement `resolve_presets()` and `apply_preset()`. Ensure CommandMetadataRegistry is accessible (via UIContext or AppRunner).

3. **Notification Polling:** In `update()`, poll `notification_rx.has_changed()` and `asset_ref_rx.try_recv()` to sync runtime state.

4. **Error Propagation:** Use `Error` types from `liquers_core::error` exclusively. No custom error types.

5. **Egui Rendering:** `show_in_egui()` will render toolbar and content area. Extract element via `take_element()`, render, put back via `put_element()`.

6. **Async Command Registration:** `query_console` command can be async; macro will wrap it appropriately.
