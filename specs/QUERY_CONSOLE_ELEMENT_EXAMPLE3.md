# QueryConsoleElement Phase 3 — Example 3: Error Handling & Serialization Round-trip

**Version:** 1.0
**Date:** 2026-02-14
**Status:** Example Scenario
**Context:** Browser-like query console with persistent history and stateful recovery

---

## Example 3: Error Handling & Serialization Round-trip

### Scenario

A user interacts with a QueryConsoleElement in an interactive UI application:

1. **Part A: Error Handling**
   - User submits an invalid/nonexistent command query
   - Widget receives error via AssetNotificationMessage
   - Metadata pane shows error; data_view forced to false
   - User corrects the query and resubmits successfully

2. **Part B: Serialization Round-trip**
   - Widget has active query with populated history
   - Application serializes UI state
   - Deserialization restores history and query_text
   - Runtime fields (asset_info, value, etc.) are None
   - init() re-submits the query, data reappears

### Context

**When encountered:**
- Error handling: Whenever user enters malformed syntax or references non-existent commands
- Serialization: When application saves/restores session (e.g., browser refresh, app restart)

**Relevant widget fields:**
```rust
pub struct QueryConsoleElement {
    handle: Option<UIHandle>,
    title_text: String,
    query_text: String,                    // Persisted
    history: Vec<String>,                  // Persisted
    history_index: usize,                  // Persisted
    data_view: bool,                       // Persisted
    #[serde(skip)] value: Option<Arc<Value>>,
    #[serde(skip)] asset_info: Option<AssetInfo>,
    #[serde(skip)] error: Option<Error>,
    #[serde(skip)] notification_rx: Option<tokio::sync::mpsc::UnboundedReceiver<AssetNotificationMessage<E>>>,
    #[serde(skip)] asset_ref_rx: Option<tokio::sync::oneshot::Receiver<AssetRef<E>>>,
    #[serde(skip)] next_presets: Vec<CommandPreset>,
}
```

---

## Code Example: Part A — Error Handling

### Initial State

```rust
// User has successfully evaluated a query
let mut console = QueryConsoleElement::new();
console.title_text = "Query Console".to_string();
console.query_text = "text/hello".to_string();
console.history = vec!["text/hello".to_string()];
console.history_index = 0;
console.data_view = true;  // Showing data

// Widget has active evaluation
console.asset_info = Some(AssetInfo {
    status: AssetStatus::Ready,
    value: Arc::new(Value::from("Hello, World!")),
    title: Some("Result".to_string()),
    metadata: Metadata::new(),
});
console.value = Some(Arc::new(Value::from("Hello, World!")));
```

### User Submits Invalid Query

```rust
// User types an invalid command and hits Enter
console.query_text = "/-/nonexistent-command/arg1".to_string();

// Widget receives the submit event (e.g., egui key press)
// show_in_egui() processes the change:
fn show_in_egui(&mut self, ui: &mut egui::Ui, ctx: &UIContext) -> egui::Response {
    // ─── Query Bar ──────────────────────────────────────────────
    let response = ui.text_edit_singleline(&mut self.query_text);

    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        // Submit query
        self.history.push(self.query_text.clone());
        self.history_index = self.history.len() - 1;

        // Spawn oneshot request (new RequestAsset pattern)
        let (tx, rx) = tokio::sync::oneshot::channel();
        ctx.evaluate_request_asset(&self.query_text, tx);  // No handle, use oneshot
        self.asset_ref_rx = Some(rx);
    }

    // ─── Processing Oneshot Response ────────────────────────────
    // (Would happen in update() or next show_in_egui() call)
    if let Some(mut rx) = self.asset_ref_rx.take() {
        // Non-blocking check: is response ready?
        match futures::executor::block_on(futures::future::poll_fn(|cx| {
            rx.poll_unpin(cx)
        })) {
            std::task::Poll::Ready(Ok(asset_ref)) => {
                // Start listening for notifications
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                asset_ref.subscribe(tx);
                self.notification_rx = Some(rx);
                self.asset_ref_rx = None;
            }
            std::task::Poll::Ready(Err(_)) => {
                // Oneshot channel closed (error submitting query)
                self.error = Some(Error::general_error(
                    "Failed to submit query".to_string()
                ));
                self.asset_ref_rx = None;
            }
            std::task::Poll::Pending => {
                // Not ready yet, keep it
                self.asset_ref_rx = Some(rx);
            }
        }
    }

    response
}

// ─── Metadata Pane (rendered in update loop or show_in_egui) ───
// Since query is invalid, metadata pane renders:
//   Status: Error (red)
//   Filename: (none)
//   Title: (none)
//   Description: (none)
//   Error: "Command not found: nonexistent-command"
//   Log: (empty)
fn show_metadata_pane(&self, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Status").strong());

    if let Some(err) = &self.error {
        // Error received immediately from parsing/validation
        ui.colored_label(egui::Color32::RED, format!("Error: {}", err));
    } else if self.asset_info.is_none() && self.asset_ref_rx.is_some() {
        // Still evaluating
        ui.label("Evaluating...");
    }

    ui.separator();
    ui.label("Error details:");
    if let Some(err) = &self.error {
        ui.text_edit_multiline(&mut err.to_string());
    }
}
```

### Error Received via AssetNotificationMessage

```rust
// In update() loop or show_in_egui(), check for notifications
fn process_notifications(&mut self) {
    if let Some(ref mut rx) = self.notification_rx {
        while let Ok(notification) = rx.try_recv() {
            match notification {
                AssetNotificationMessage::ErrorOccurred { error } => {
                    // Store error for display
                    self.error = Some(error.clone());

                    // Force metadata view (hide data even if it exists)
                    self.data_view = false;

                    // Clear asset info (no valid result)
                    self.asset_info = None;
                    self.value = None;
                }
                AssetNotificationMessage::StateChanged { state } => {
                    // State became ready (shouldn't happen after error)
                    self.asset_info = Some(AssetInfo {
                        status: AssetStatus::Ready,
                        value: Arc::new(state.data.clone()),
                        title: state.metadata.get_title(),
                        metadata: state.metadata.clone(),
                    });
                    self.error = None;  // Clear any prior error
                }
                AssetNotificationMessage::ProgressUpdate { progress } => {
                    // Progress indication (if needed)
                }
            }
        }
    }
}

// Metadata pane now shows error:
//   Status: Error (red)
//   Error: "Command not found: nonexistent-command"
//   (data_view is false, so value pane is hidden)
```

### User Corrects Query and Resubmits

```rust
// User edits the query_text in the query bar
console.query_text = "text/hello".to_string();  // Valid command

// User presses Enter again (triggers show_in_egui logic)
console.history.push("text/hello".to_string());
console.history_index = 1;

// Clear prior error
console.error = None;

// Submit new query via oneshot pattern
let (tx, rx) = tokio::sync::oneshot::channel();
ctx.evaluate_request_asset("text/hello", tx);
console.asset_ref_rx = Some(rx);

// Next frame: asset_ref_rx resolves with valid AssetRef
// Subscribe to notifications (as before)
// Eventually notification arrives:
//   AssetNotificationMessage::StateChanged { state }
//   - state.data = "Hello, World!"
//   - state.metadata has proper title/description

// Update widget state
console.asset_info = Some(AssetInfo {
    status: AssetStatus::Ready,
    value: Arc::new(Value::from("Hello, World!")),
    title: Some("Result".to_string()),
    metadata: Metadata::new(),
});
console.value = Some(Arc::new(Value::from("Hello, World!")));
console.error = None;

// Metadata pane now shows:
//   Status: Ready (green)
//   Title: "Result"
//   Value pane (when data_view = true):
//     "Hello, World!"
```

---

## Code Example: Part B — Serialization Round-trip

### Before Serialization

```rust
// Widget state before serialization
let mut console = QueryConsoleElement::new();
console.handle = Some(UIHandle::from(42));
console.title_text = "Query Console".to_string();
console.query_text = "text/hello/q/uppercase".to_string();
console.history = vec![
    "text/hello".to_string(),
    "text/hello/q/uppercase".to_string(),
];
console.history_index = 1;
console.data_view = true;

// Runtime state (will be skipped during serialization)
console.value = Some(Arc::new(Value::from("HELLO")));
console.asset_info = Some(AssetInfo {
    status: AssetStatus::Ready,
    value: Arc::new(Value::from("HELLO")),
    title: Some("Uppercase Result".to_string()),
    metadata: Metadata::new(),
});
console.error = None;
console.notification_rx = Some(/* channel receiver */);
console.asset_ref_rx = None;
console.next_presets = vec![/* presets */];
```

### Serialization to JSON

```rust
// Serialize via serde_json::to_string_pretty()
// The #[serde(skip)] fields are NOT included

let json = serde_json::to_string_pretty(&console)?;

// Output:
{
  "handle": 42,
  "title_text": "Query Console",
  "query_text": "text/hello/q/uppercase",
  "history": [
    "text/hello",
    "text/hello/q/uppercase"
  ],
  "history_index": 1,
  "data_view": true
}
```

### Deserialization from JSON

```rust
// Deserialize via serde_json::from_str()
let json = r#"{
  "handle": 42,
  "title_text": "Query Console",
  "query_text": "text/hello/q/uppercase",
  "history": [
    "text/hello",
    "text/hello/q/uppercase"
  ],
  "history_index": 1,
  "data_view": true
}"#;

let mut console: QueryConsoleElement = serde_json::from_str(json)?;

// After deserialization:
assert_eq!(console.handle, Some(UIHandle::from(42)));
assert_eq!(console.title_text, "Query Console");
assert_eq!(console.query_text, "text/hello/q/uppercase");
assert_eq!(console.history, vec![
    "text/hello".to_string(),
    "text/hello/q/uppercase".to_string(),
]);
assert_eq!(console.history_index, 1);
assert_eq!(console.data_view, true);

// Runtime fields are None/empty (correct)
assert_eq!(console.value, None);
assert_eq!(console.asset_info, None);
assert_eq!(console.error, None);
assert_eq!(console.notification_rx, None);
assert_eq!(console.asset_ref_rx, None);
assert_eq!(console.next_presets, vec![]);
```

### init() Re-submits Query

```rust
// AppRunner or application calls init() after deserialization
impl UIElement for QueryConsoleElement {
    fn init(&mut self, handle: UIHandle, app_state: &dyn AppState)
        -> Result<(), Error>
    {
        self.handle = Some(handle);

        // Re-submit the persisted query_text if it's non-empty
        if !self.query_text.is_empty() {
            // FIXME: init() doesn't have UIContext; would need to be called
            // in a context that can evaluate (e.g., within show_in_egui or
            // a dedicated re-evaluation phase)

            // For now, mark that we need re-evaluation:
            // This could be done by:
            // 1. Returning a special flag from init()
            // 2. Having AppRunner check for non-empty query_text and
            //    pending asset_ref_rx, then call show_in_egui() to trigger
            //    query submission
            // 3. Using UIContext.submit_query_current() if init received ctx
        }

        Ok(())
    }
}

// In AppRunner's run() method or app's update loop:
impl<E: Environment> AppRunner<E> {
    async fn run(&mut self) -> Result<(), Error> {
        // ... existing message processing and evaluation logic ...

        // After init() phase, check for pending re-evaluation
        // (Pseudocode; actual impl depends on AppRunner design)
        for handle in /* nodes that were just deserialized */ {
            let node = self.app_state.get_node(handle)?;
            if let Some(ui_context) = self.get_ui_context_for_node(handle) {
                // Trigger re-evaluation via show_in_egui
                // or dedicated method
                if let Some(element) = node.get_element() {
                    if let Some(console) = element.as_any().downcast_ref::<QueryConsoleElement>() {
                        if !console.query_text.is_empty() && console.value.is_none() {
                            // Re-submit query
                            let (tx, rx) = tokio::sync::oneshot::channel();
                            ui_context.evaluate_request_asset(
                                &console.query_text.clone(),
                                tx
                            );
                            // Update element with new asset_ref_rx
                            // (requires mutable access to element)
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
```

### Re-evaluation Flow After Deserialization

```rust
// Timeline after deserialization + init():

// Frame 1: init() completes
//   query_text = "text/hello/q/uppercase"
//   value = None
//   asset_info = None
//   asset_ref_rx = None
// Query bar shows "text/hello/q/uppercase"
// Metadata pane shows loading state

// Frame 2: show_in_egui() is called (or explicit re-eval phase)
//   Re-evaluation triggered: evaluate_request_asset("text/hello/q/uppercase", tx)
//   asset_ref_rx = Some(rx)
// Query bar still shows same query
// Metadata pane shows "Evaluating..."

// Frame 3: Oneshot resolves
//   asset_ref_rx receives AssetRef
//   Subscribe to notifications
// Metadata pane shows "Evaluating..."

// Frame 4-N: Notifications arrive
//   AssetNotificationMessage::StateChanged { state }
//   asset_info = Some(AssetInfo { ... })
//   value = Some(Arc::new(Value::from("HELLO")))
// Metadata pane shows "Ready", title "Uppercase Result"
// Value pane (data_view = true) shows "HELLO"

// Final state: Identical to state before serialization
assert_eq!(console.value, Some(Arc::new(Value::from("HELLO"))));
assert_eq!(console.data_view, true);
```

---

## Expected Behavior

### Part A: Error Handling

**Sequence:**
1. User submits invalid query "/-/nonexistent-command/arg1"
2. Query is submitted via oneshot channel
3. Command registry lookup fails or parser detects syntax error
4. AssetRef resolves, notification arrives with ErrorOccurred
5. Widget stores error in self.error
6. data_view is forced to false
7. Metadata pane displays error message in red
8. Query bar still editable
9. History updated with invalid query
10. User edits query_text to "text/hello"
11. User presses Enter
12. New query submitted via new oneshot channel
13. Evaluation succeeds
14. Notification arrives with StateChanged
15. Widget clears error, sets value, sets asset_info
16. Metadata pane shows success (green status, title, etc.)
17. data_view restored or set to true if user prefers
18. Value pane displays "Hello, World!"

**Key transitions:**
- Ready → Error (on error notification)
- Error → Evaluating (on new query submission)
- Evaluating → Ready (on success notification)

### Part B: Serialization Round-trip

**Sequence:**
1. User has active query with populated history
2. Application calls serialize (e.g., AppMessage::Serialize)
3. JSON produced with: handle, title_text, query_text, history, history_index, data_view
4. All runtime fields (#[serde(skip)]) omitted
5. Deserialization reconstructs: handle, title_text, query_text, history, history_index, data_view
6. Runtime fields default to None/empty
7. init() called (may trigger query re-submission, depends on design)
8. show_in_egui() or dedicated phase re-evaluates query_text
9. Oneshot channel resolves with AssetRef
10. Notifications received
11. asset_info and value rebuilt
12. Widget state identical to pre-serialization (from user's perspective)

**Key invariant:**
- User-visible state (query_text, history, data_view) always persisted and restored
- Runtime state (value, asset_info, channels) always rebuilt on demand
- No loss of information across serialization round-trip

---

## Validation Checklist

### Part A: Error Handling
- [ ] Invalid query submitted without panic or unwrap
- [ ] Error received via AssetNotificationMessage::ErrorOccurred
- [ ] self.error stores the Error value
- [ ] data_view forced to false when error occurs
- [ ] Metadata pane renders error text in red
- [ ] Query bar remains editable for correction
- [ ] History includes invalid query
- [ ] User can correct query and resubmit
- [ ] New query clears prior error (self.error = None)
- [ ] Successful re-evaluation updates asset_info and value
- [ ] No command registry lookup panic (graceful degradation if command not found)
- [ ] Oneshot channel closed handled silently (not fatal)

### Part B: Serialization Round-trip
- [ ] All persistent fields (query_text, history, history_index, data_view, handle, title_text) serialized
- [ ] All #[serde(skip)] fields excluded from JSON
- [ ] JSON is valid and parseable
- [ ] Deserialized struct has identical persistent field values
- [ ] Deserialized runtime fields are None/empty
- [ ] init() can be called on deserialized widget without error
- [ ] Re-evaluation triggered (explicit or via show_in_egui)
- [ ] Query re-submitted successfully
- [ ] Oneshot channel resolves with valid AssetRef
- [ ] Notifications received and asset_info rebuilt
- [ ] value rebuilt correctly from state data
- [ ] Final widget state matches pre-serialization (from user's perspective)
- [ ] History navigation still works after deserialization
- [ ] data_view toggle still functional

### General Error Handling
- [ ] No unwrap() or expect() in show_in_egui, update, or init
- [ ] All Error values use liquers_core::error::Error constructors
- [ ] Oneshot channel close logged but not fatal
- [ ] Command registry lookup failures graceful (next_presets stays empty, not panic)
- [ ] Parse errors displayed in metadata pane as error text
- [ ] Evaluation errors received via notification (never via exception)

---

## Integration with Phase 3 Runtime

### AppRunner Integration

In AppRunner<E>::run():

```rust
async fn run(&mut self) -> Result<(), Error> {
    // 1. Drain messages (handles serialize/deserialize requests)
    while let Ok(msg) = self.message_rx.try_recv() {
        match msg {
            AppMessage::Serialize { path } => {
                // Serialize all elements (including QueryConsoleElement)
                let snapshot = serde_json::to_string_pretty(&self.app_state)?;
                tokio::fs::write(&path, snapshot).await?;
            }
            AppMessage::Deserialize { path } => {
                // Deserialize (runtime fields become None)
                let data = tokio::fs::read_to_string(&path).await?;
                let deserialized: DirectAppState = serde_json::from_str(&data)?;
                // Merge or replace app_state (implementation-specific)
            }
            // ... other messages ...
        }
    }

    // 2. Initialize nodes (calls init() on deserialized elements)
    for handle in /* pending inits */ {
        if let Some(element) = self.app_state.get_mut_element(handle) {
            element.init(handle, &self.ui_context)?;
        }
    }

    // 3. Re-evaluate deserialized QueryConsoleElements
    // (Could be done in init() itself if UIContext available, or here)
    for handle in /* nodes with non-empty query_text and no value */ {
        if let Some(element) = self.app_state.get_element(handle) {
            if let Some(console) = element.as_any().downcast_ref::<QueryConsoleElement>() {
                if !console.query_text.is_empty() && console.value.is_none() {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    self.ui_context.evaluate_request_asset(
                        &console.query_text.clone(),
                        tx
                    );
                    // Update element (needs mutable access and redesign)
                }
            }
        }
    }

    // 4. Auto-evaluate pending nodes
    self.app_state.auto_evaluate_pending()?;

    // 5. Poll evaluating nodes
    self.app_state.poll_evaluations().await?;

    Ok(())
}
```

### Element Lifecycle in Phase 3

QueryConsoleElement lifecycle with error handling and serialization:

```
Deserialized (runtime = None)
       ↓
init() called
       ↓
Re-evaluation triggered (show_in_egui or explicit phase)
       ↓
Oneshot channel resolves
       ↓
Notifications received
       ↓
asset_info and value populated
       ↓
Ready state (or Error state on failure)
       ↓
[Serialization event]
       ↓
Serialized (runtime omitted)
       ↓
Deserialized
       ↓ (cycle repeats)
```

---

## Notes

### Design Decisions

1. **Oneshot for immediate requests:** RequestAsset message pattern uses oneshot, not persistent channels, because:
   - Each query submission is independent
   - Decouples from element handle (flexible for eval-in-place pattern)
   - Simpler error handling (channel closed = submission failed)

2. **Forced metadata on error:** data_view forced to false when error occurs because:
   - Error text must be visible
   - Data pane irrelevant if evaluation failed
   - User can manually re-enable data_view after fixing query

3. **Silent channel close:** Oneshot channel closed handled gracefully because:
   - Evaluation may fail asynchronously
   - Not user's fault if system resources exhausted
   - Logging via on_app_state_event() provides visibility

4. **Re-evaluation on init():** Query re-submitted after deserialization because:
   - History and query_text are user-facing state, must be restored
   - Runtime state is ephemeral, rebuilt on demand
   - Ensures widget is functional immediately after load

### Implementation Considerations

- **Async boundaries:** evaluate_request_asset must be async (spawns background task)
- **Mutex requirements:** asset_info, value, error need Arc<Mutex<>> or similar for thread safety if notifications arrive on different thread
- **UI context in init():** Current design doesn't provide UIContext to init(), limiting re-eval options (workaround: explicit re-eval phase in AppRunner or deferred submit in show_in_egui)
- **Element downcasting:** Re-eval phase requires as_any() downcasting (Phase 2 feature)

---

**End of Example 3**
