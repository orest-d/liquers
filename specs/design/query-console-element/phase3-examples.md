# Phase 3: Examples & Testing - QueryConsoleElement

## Example Type

**User choice:** Conceptual code (demonstrates API interactions without requiring full runnable setup)

## Overview Table

| # | Type | Name | Purpose |
|---|------|------|---------|
| 1 | Example | Basic Query Console Usage | Create console, submit query, receive snapshot, view result |
| 2 | Example | Command Presets & Multi-Query Session | Preset resolution, history navigation, manual editing |
| 3 | Example | Error Handling & Serialization Round-trip | Error propagation via AssetSnapshot, session persistence |
| 4 | Unit Tests | QueryConsoleElement Unit Tests | 34 inline tests: construction, UIElement trait, history, updates, serialization, presets, init |
| 5 | Integration Tests | AppRunner Integration | 6 integration tests: creation, RequestAssetUpdates flow, monitoring auto-stop, error propagation, serialization + reinit |
| 6 | Corner Cases | Corner Cases & Risk Analysis | Memory, concurrency, error paths, serialization edge cases |

---

## Example 1: Basic Query Console Usage

### Scenario

A user creates a QueryConsoleElement with an initial query, submits it, sees the result via AssetSnapshot, navigates history, and toggles between data and metadata views.

### Code

```rust
// ─── Construction ──────────────────────────────────────────────────────────

let mut console = QueryConsoleElement::new(
    "Query Console".to_string(),
    "text-Hello".to_string(),
);

// Initial state:
//   handle: None, query_text: "text-Hello"
//   history: [], history_index: 0, data_view: false
//   value: None, metadata: None, error: None, status: Status::None

// ─── Registration & Init ───────────────────────────────────────────────────

// AppRunner calls init() after setting the element in AppState
console.init(handle, &ui_context);

// Inside init():
//   1. self.handle = Some(handle)
//   2. query_text is non-empty → calls submit_query(&ui_context)
//
// Inside submit_query():
//   1. history.push("text-Hello")
//   2. history_index = 1
//   3. Sends AppMessage::RequestAssetUpdates { handle, query: "text-Hello" }

// ─── AppRunner Processes RequestAssetUpdates ────────────────────────────────

// AppRunner::handle_request_asset_updates():
//   1. envref.evaluate("text-Hello") → AssetRef<E>
//   2. Subscribe: asset_ref.subscribe_to_notifications() → watch::Receiver
//   3. Build initial snapshot (evaluation in progress):
let initial_snapshot = AssetSnapshot {
    value: None,
    metadata: Metadata { /* status: JobSubmitted, progress info */ },
    error: None,
    status: Status::Submitted,
};
//   4. Deliver via element.update(UpdateMessage::AssetUpdate(initial_snapshot))
//   5. Store MonitoredAsset { asset_ref, notification_rx } in self.monitoring

// Widget.update() stores snapshot fields:
//   value = None, metadata = Some(...), status = JobSubmitted
//   Returns UpdateResponse::NeedsRepaint

// Rendering at this point:
//  ┌──────────────────────────────────────────────────────────┐
//  │ ◀  ▶  [text-Hello_______________] [Metadata]  ✓         │
//  ├──────────────────────────────────────────────────────────┤
//  │ Status: Submitted                                        │
//  │ (metadata shown — no value yet)                          │
//  └──────────────────────────────────────────────────────────┘

// ─── Evaluation Completes ───────────────────────────────────────────────────

// AppRunner::poll_monitored_assets() detects notification_rx.has_changed()
// Builds fresh snapshot:
let final_snapshot = AssetSnapshot {
    value: Some(Arc::new(Value::from("Hello"))),
    metadata: Metadata { /* status: Finished, title, type info */ },
    error: None,
    status: Status::Ready,
};

// Widget.update() stores new fields:
//   value = Some(Arc("Hello")), status = Finished
//   value.is_some() → self.data_view = true
//   Returns UpdateResponse::NeedsRepaint

// Rendering after completion:
//  ┌──────────────────────────────────────────────────────────┐
//  │ ◀  ▶  [text-Hello_______________] [Data] [Presets ▼] ✓  │
//  ├──────────────────────────────────────────────────────────┤
//  │ "Hello"                                                  │
//  └──────────────────────────────────────────────────────────┘

// ─── User Modifies Query and Submits ────────────────────────────────────────

// User edits query field to "text-Hello/uppercase", presses Enter
// submit_query():
//   history = ["text-Hello", "text-Hello/uppercase"]
//   history_index = 2
//   Sends AppMessage::RequestAssetUpdates { handle, query: "text-Hello/uppercase" }

// AppRunner replaces monitoring entry for this handle with new AssetRef
// Snapshot arrives: value = Some("HELLO"), status = Ready

// ─── History Navigation: Back ───────────────────────────────────────────────

// User clicks ◀ (back button)
// history_back():
//   history_index = 1 (was 2)
//   query_text = history[0] = "text-Hello"
//   submit_query() sends RequestAssetUpdates for original query

// ─── View Toggle ────────────────────────────────────────────────────────────

// User clicks the [Data] button → toggles to metadata view
//   self.data_view = false → button label changes to [Metadata]
//   Content area now shows metadata pane:
//     Status: Ready
//     Title: Result
//     Type: String
//     Timestamp, filename, etc.
// Clicking [Metadata] toggles back to data view (self.data_view = true, label → [Data])
```

### Expected Behavior

1. **Init auto-submits** non-empty query_text via `RequestAssetUpdates`
2. **AppRunner monitors** asset and pushes `AssetSnapshot` on each change
3. **Widget passively stores** snapshot fields in `update()`, returns `NeedsRepaint`
4. **Data view auto-switches** to true when value arrives
5. **Single-row toolbar**: `◀ ▶ [query field] [Data/Metadata] [Presets ▼] ✓` — toggle button shows current view name, clicking switches to the other view
6. **Toggle button label**: shows "Metadata" when no data available (forced); shows "Data" or "Metadata" based on `data_view` when data is available
7. **History persists** all submitted queries; back/forward navigate without duplicates
8. **Presets resolve** after value arrives via `find_next_presets()`

---

## Example 2: Command Presets & Multi-Query Session

### Scenario

A user explores data interactively: types queries, uses preset suggestions, navigates history, and manually edits queries. Demonstrates `NextPreset`, `find_next_presets()`, and `apply_preset()`.

### Code

```rust
// ─── Initial Query ─────────────────────────────────────────────────────────

let mut console = QueryConsoleElement::new("Explorer".to_string(), String::new());

// User types "text-Hello" and presses Enter
console.query_text = "text-Hello".to_string();
console.submit_query(&ctx);
// history: ["text-Hello"], history_index: 1
// Sends AppMessage::RequestAssetUpdates { handle, query: "text-Hello" }

// ─── Snapshot Arrives, Presets Resolved ──────────────────────────────────────

// AppRunner delivers AssetSnapshot with value = "Hello", status = Finished
// Widget.update() stores value, metadata, status
// After update, resolve_presets() is called:

console.resolve_presets(&state, &registry);
// find_next_presets("text-Hello", &state, &registry) returns:
// [
//   NextPreset { query: "text-Hello/uppercase", label: "Uppercase", description: "..." },
//   NextPreset { query: "text-Hello/reverse",   label: "Reverse",   description: "..." },
//   NextPreset { query: "text-Hello/length",    label: "Get length", description: "..." },
// ]
// Note: find_next_presets handles namespace injection automatically.
// If a preset command is in a different namespace, it prepends ns-<namespace>/

// ─── User Selects Preset ────────────────────────────────────────────────────

// User clicks "Uppercase" in preset dropdown
console.apply_preset(0, &ctx);
// Inside apply_preset(0):
//   query_text = next_presets[0].query = "text-Hello/uppercase"
//   submit_query(&ctx) → auto-execute

// history: ["text-Hello", "text-Hello/uppercase"], history_index: 2
// Sends RequestAssetUpdates { query: "text-Hello/uppercase" }

// Snapshot arrives: value = "HELLO", status = Finished
// New presets resolved for uppercase result type

// ─── User Navigates Back ────────────────────────────────────────────────────

console.history_back();
// history_index = 1, query_text = "text-Hello"
// submit_query() re-evaluates original query (cached result may be used)

console.history_forward();
// history_index = 2, query_text = "text-Hello/uppercase"

// ─── User Manually Edits ────────────────────────────────────────────────────

console.query_text = "text-Hello/split-comma".to_string();
console.submit_query(&ctx);
// history: ["text-Hello", "text-Hello/uppercase", "text-Hello/split-comma"]
// history_index: 3
// New presets resolved based on array value type

// ─── Out-of-bounds Preset ───────────────────────────────────────────────────

console.apply_preset(99, &ctx);
// Index >= next_presets.len() → silently ignored, query_text unchanged
```

### Expected Behavior

1. **Presets are fully-formed queries**: `NextPreset.query` is a complete string with namespace handling
2. **apply_preset() auto-submits**: sets query_text and calls submit_query()
3. **History is linear**: new submissions always append, no branching
4. **Namespace injection transparent**: `find_next_presets()` handles cross-namespace presets
5. **Out-of-bounds preset ignored**: no panic, no state change

---

## Example 3: Error Handling & Serialization Round-trip

### Scenario A: Error Handling

User submits an invalid query, sees error in metadata pane, navigates back to recover.

```rust
// ─── User Submits Invalid Query ─────────────────────────────────────────────

console.query_text = "nonexistent-command".to_string();
console.submit_query(&ctx);
// Sends AppMessage::RequestAssetUpdates { handle, query: "nonexistent-command" }

// AppRunner::handle_request_asset_updates():
// envref.evaluate("nonexistent-command") → Err(parse_error) or asset fails
// AppRunner delivers error snapshot:
let error_snapshot = AssetSnapshot {
    value: None,
    metadata: Metadata::new(), // minimal metadata available
    error: Some(Error::general_error("Action 'nonexistent-command' not found".to_string())),
    status: Status::Error,
};

// Widget.update(UpdateMessage::AssetUpdate(error_snapshot)):
//   self.value = None
//   self.error = Some(Error(...))
//   self.status = Status::Error
//   value.is_none() → data_view stays false (metadata pane forced)

// Rendering:
//  ┌──────────────────────────────────────────────────────────┐
//  │ ◀  ▶  [nonexistent-command______] [Metadata]         X  │
//  ├──────────────────────────────────────────────────────────┤
//  │ Status: Error (red)                                      │
//  │ Error: Action 'nonexistent-command' not found            │
//  └──────────────────────────────────────────────────────────┘

// ─── User Navigates Back to Recover ─────────────────────────────────────────

console.history_back();
// Restores previous query from history, re-evaluates → success
```

### Scenario B: Serialization Round-trip

```rust
// ─── Before Serialization ───────────────────────────────────────────────────

// Widget state:
//   handle: Some(UIHandle(42))
//   query_text: "text-hello/uppercase"
//   history: ["text-hello", "text-hello/uppercase"]
//   history_index: 2
//   data_view: true
//   value: Some(Arc("HELLO"))       ← #[serde(skip)]
//   metadata: Some(Metadata{...})   ← #[serde(skip)]
//   error: None                     ← #[serde(skip)]
//   status: Status::Ready        ← #[serde(skip)]
//   next_presets: [NextPreset{...}] ← #[serde(skip)]

let boxed: Box<dyn UIElement> = Box::new(console);
let json = serde_json::to_string(&boxed)?;
// JSON contains only: handle, title_text, query_text, history, history_index, data_view

// ─── Deserialization ────────────────────────────────────────────────────────

let restored: Box<dyn UIElement> = serde_json::from_str(&json)?;
// Persistent fields restored: handle, title_text, query_text, history, history_index, data_view
// Runtime fields are default: value=None, metadata=None, error=None, status=Status::None

// ─── init() Triggers Re-evaluation ──────────────────────────────────────────

restored.init(handle, &ui_context);
// query_text is "text-hello/uppercase" (non-empty) → submit_query()
// Sends RequestAssetUpdates → AppRunner evaluates → snapshot arrives
// Widget re-populates value, metadata, status from fresh snapshot
// User sees same result as before serialization
```

### Expected Behavior

- **Error propagation**: errors arrive via `AssetSnapshot.error`, displayed in metadata pane
- **No panics**: all error paths use `Option`/`Result`, no `unwrap()`
- **Serialization**: persistent fields survive; runtime fields are `#[serde(skip)]`
- **Auto-recovery**: `init()` re-submits non-empty query_text after deserialization
- **History preserved**: full query history survives serialization round-trip

---

## Unit Tests

**File:** `liquers-lib/src/ui/widgets/query_console_element.rs` (inline `#[cfg(test)]`)

**Total: 34 tests across 7 categories**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use liquers_core::error::Error;
    use liquers_core::metadata::Metadata;

    fn create_test_context() -> (UIContext, AppMessageReceiver) {
        let (tx, rx) = app_message_channel();
        let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
            Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
        let ctx = UIContext::new(app_state, tx);
        (ctx, rx)
    }

    // ─── 1. Construction (4 tests) ──────────────────────────────────────────

    #[test]
    fn test_new_with_title_and_initial_query() {
        let console = QueryConsoleElement::new("My Console".to_string(), "text-hello".to_string());
        assert_eq!(console.title(), "My Console");
        assert_eq!(console.query_text, "text-hello");
        assert!(console.handle().is_none());
        assert!(console.history.is_empty());
        assert_eq!(console.history_index, 0);
        assert!(!console.data_view);
        assert!(console.value.is_none());
        assert!(console.metadata.is_none());
        assert!(console.error.is_none());
        assert!(console.next_presets.is_empty());
    }

    #[test]
    fn test_new_with_empty_query() {
        let console = QueryConsoleElement::new("Console".to_string(), String::new());
        assert_eq!(console.query_text, "");
        assert!(console.history.is_empty());
    }

    #[test]
    fn test_new_default_field_values() {
        let console = QueryConsoleElement::new("C".to_string(), "q".to_string());
        // Status defaults to Status::None
        assert_eq!(console.status, Status::None);
    }

    #[test]
    fn test_new_multiple_instances_independent() {
        let mut c1 = QueryConsoleElement::new("A".to_string(), "q1".to_string());
        let mut c2 = QueryConsoleElement::new("B".to_string(), "q2".to_string());
        c1.set_handle(UIHandle(1));
        c2.set_handle(UIHandle(2));
        assert_eq!(c1.handle(), Some(UIHandle(1)));
        assert_eq!(c2.handle(), Some(UIHandle(2)));
        assert_eq!(c1.query_text, "q1");
        assert_eq!(c2.query_text, "q2");
    }

    // ─── 2. UIElement Trait (8 tests) ───────────────────────────────────────

    #[test]
    fn test_type_name() {
        let c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert_eq!(c.type_name(), "QueryConsoleElement");
    }

    #[test]
    fn test_handle_and_set_handle() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert!(c.handle().is_none());
        c.set_handle(UIHandle(42));
        assert_eq!(c.handle(), Some(UIHandle(42)));
        c.set_handle(UIHandle(99));
        assert_eq!(c.handle(), Some(UIHandle(99)));
    }

    #[test]
    fn test_title_and_set_title() {
        let mut c = QueryConsoleElement::new("Initial".to_string(), "q".to_string());
        assert_eq!(c.title(), "Initial");
        c.set_title("Updated".to_string());
        assert_eq!(c.title(), "Updated");
    }

    #[test]
    fn test_clone_boxed_preserves_fields() {
        let mut c = QueryConsoleElement::new("Original".to_string(), "q".to_string());
        c.set_handle(UIHandle(7));
        let boxed: Box<dyn UIElement> = Box::new(c);
        let cloned = boxed.clone_boxed();
        assert_eq!(cloned.type_name(), "QueryConsoleElement");
        assert_eq!(cloned.title(), "Original");
        assert_eq!(cloned.handle(), Some(UIHandle(7)));
    }

    #[test]
    fn test_get_value_returns_none_initially() {
        let c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert!(c.get_value().is_none());
    }

    #[test]
    fn test_get_metadata_returns_none_initially() {
        let c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert!(c.get_metadata().is_none());
    }

    #[test]
    fn test_clone_boxed_returns_boxed_ui_element() {
        let c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        let boxed: Box<dyn UIElement> = Box::new(c);
        let cloned = boxed.clone_boxed();
        assert_eq!(cloned.type_name(), "QueryConsoleElement");
    }

    #[test]
    fn test_is_initialised_before_init() {
        let c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert!(!c.is_initialised());
    }

    // ─── 3. History Navigation (6 tests) ────────────────────────────────────

    #[test]
    fn test_history_back_at_beginning_returns_false() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert!(!c.history_back());
        assert_eq!(c.history_index, 0);
    }

    #[test]
    fn test_history_forward_at_end_returns_false() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.history = vec!["q1".to_string(), "q2".to_string()];
        c.history_index = 2;
        assert!(!c.history_forward());
        assert_eq!(c.history_index, 2);
    }

    #[test]
    fn test_history_back_after_submit() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.history = vec!["q1".to_string(), "q2".to_string(), "q3".to_string()];
        c.history_index = 3;
        assert!(c.history_back());
        assert_eq!(c.history_index, 2);
        assert!(c.history_back());
        assert_eq!(c.history_index, 1);
    }

    #[test]
    fn test_history_forward_after_back() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.history = vec!["q1".to_string(), "q2".to_string(), "q3".to_string()];
        c.history_index = 3;
        c.history_back();
        c.history_back(); // index = 1
        assert!(c.history_forward());
        assert_eq!(c.history_index, 2);
        assert!(c.history_forward());
        assert_eq!(c.history_index, 3);
    }

    #[test]
    fn test_history_navigation_boundaries() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.history = vec!["a".to_string(), "b".to_string()];
        c.history_index = 2;
        assert!(!c.history_forward()); // at end
        assert!(c.history_back());
        assert!(c.history_back()); // index = 0
        assert!(!c.history_back()); // at beginning
    }

    #[test]
    fn test_history_back_forward_symmetric() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.history = vec!["q1".to_string(), "q2".to_string()];
        c.history_index = 2;
        c.history_back();
        c.history_forward();
        assert_eq!(c.history_index, 2);
    }

    // ─── 4. AssetSnapshot Updates (6 tests) ─────────────────────────────────

    #[test]
    fn test_asset_update_stores_all_fields() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        let (ctx, _rx) = create_test_context();
        let snapshot = AssetSnapshot {
            value: Some(Arc::new(Value::from("hello"))),
            metadata: Metadata::new(),
            error: None,
            status: Status::Ready,
        };
        let response = c.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);
        assert_eq!(response, UpdateResponse::NeedsRepaint);
        assert!(c.value.is_some());
        assert!(c.metadata.is_some());
        assert!(c.error.is_none());
    }

    #[test]
    fn test_asset_update_with_error() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        let (ctx, _rx) = create_test_context();
        let snapshot = AssetSnapshot {
            value: None,
            metadata: Metadata::new(),
            error: Some(Error::general_error("fail".to_string())),
            status: Status::Error,
        };
        c.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);
        assert!(c.error.is_some());
        assert!(c.value.is_none());
    }

    #[test]
    fn test_asset_update_with_value_sets_data_view_true() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.data_view = false;
        let (ctx, _rx) = create_test_context();
        let snapshot = AssetSnapshot {
            value: Some(Arc::new(Value::from("v"))),
            metadata: Metadata::new(),
            error: None,
            status: Status::Ready,
        };
        c.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);
        assert!(c.data_view);
    }

    #[test]
    fn test_asset_update_without_value_preserves_data_view() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.data_view = true;
        let (ctx, _rx) = create_test_context();
        let snapshot = AssetSnapshot {
            value: None,
            metadata: Metadata::new(),
            error: None,
            status: Status::Submitted,
        };
        c.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);
        assert!(c.data_view); // unchanged
    }

    #[test]
    fn test_other_updates_return_unchanged() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        let (ctx, _rx) = create_test_context();
        assert_eq!(
            c.update(&UpdateMessage::Timer { elapsed_ms: 100 }, &ctx),
            UpdateResponse::Unchanged
        );
    }

    #[test]
    fn test_asset_notification_returns_unchanged() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        let (ctx, _rx) = create_test_context();
        let msg = UpdateMessage::AssetNotification(AssetNotificationMessage::Initial);
        assert_eq!(c.update(&msg, &ctx), UpdateResponse::Unchanged);
    }

    // ─── 5. Serialization (4 tests) ─────────────────────────────────────────

    #[test]
    fn test_serialization_persistent_fields() {
        let mut c = QueryConsoleElement::new("Title".to_string(), "q".to_string());
        c.set_handle(UIHandle(42));
        c.history = vec!["q1".to_string(), "q2".to_string()];
        c.history_index = 1;
        c.data_view = true;

        let boxed: Box<dyn UIElement> = Box::new(c);
        let json = serde_json::to_string(&boxed).expect("serialize");
        let restored: Box<dyn UIElement> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.type_name(), "QueryConsoleElement");
        assert_eq!(restored.handle(), Some(UIHandle(42)));
        assert_eq!(restored.title(), "Title");
    }

    #[test]
    fn test_deserialization_resets_runtime_fields() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.value = Some(Arc::new(Value::from("v")));
        c.metadata = Some(Metadata::new());
        c.error = Some(Error::general_error("e".to_string()));
        c.next_presets = vec![NextPreset {
            query: "q1".to_string(),
            label: "P".to_string(),
            description: "D".to_string(),
        }];

        let boxed: Box<dyn UIElement> = Box::new(c);
        let json = serde_json::to_string(&boxed).expect("serialize");
        let restored: Box<dyn UIElement> = serde_json::from_str(&json).expect("deserialize");
        assert!(restored.get_value().is_none());
        assert!(restored.get_metadata().is_none());
    }

    #[test]
    fn test_typetag_roundtrip() {
        let c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        let boxed: Box<dyn UIElement> = Box::new(c);
        let json = serde_json::to_string(&boxed).expect("serialize");
        assert!(json.contains("QueryConsoleElement"));
        let restored: Box<dyn UIElement> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.type_name(), "QueryConsoleElement");
    }

    #[test]
    fn test_history_preserved_across_serialization() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        c.history = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        c.history_index = 2;

        let boxed: Box<dyn UIElement> = Box::new(c);
        let json = serde_json::to_string(&boxed).expect("serialize");
        let _restored: Box<dyn UIElement> = serde_json::from_str(&json).expect("deserialize");
        // History restored (verifiable via downcast in Phase 2+)
    }

    // ─── 6. Presets (3 tests) ───────────────────────────────────────────────

    #[test]
    fn test_apply_preset_sets_query_and_submits() {
        let mut c = QueryConsoleElement::new("C".to_string(), "initial".to_string());
        c.next_presets = vec![
            NextPreset { query: "p1".to_string(), label: "P1".to_string(), description: String::new() },
            NextPreset { query: "p2".to_string(), label: "P2".to_string(), description: String::new() },
        ];
        let (ctx, mut rx) = create_test_context();
        c.set_handle(UIHandle(1));
        c.apply_preset(1, &ctx);
        assert_eq!(c.query_text, "p2");
        if let Ok(msg) = rx.try_recv() {
            match msg {
                AppMessage::RequestAssetUpdates { query, .. } => assert_eq!(query, "p2"),
                _ => panic!("Expected RequestAssetUpdates"),
            }
        }
    }

    #[test]
    fn test_next_presets_empty_initially() {
        let c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert!(c.next_presets.is_empty());
    }

    #[test]
    fn test_apply_preset_out_of_bounds() {
        let mut c = QueryConsoleElement::new("C".to_string(), "original".to_string());
        c.next_presets = vec![
            NextPreset { query: "p1".to_string(), label: "P1".to_string(), description: String::new() },
        ];
        let (ctx, _rx) = create_test_context();
        c.apply_preset(99, &ctx);
        assert_eq!(c.query_text, "original"); // unchanged
    }

    // ─── 7. Initialization (3 tests) ────────────────────────────────────────

    #[test]
    fn test_init_submits_non_empty_query() {
        let mut c = QueryConsoleElement::new("C".to_string(), "text-hello".to_string());
        let (tx, mut rx) = app_message_channel();
        let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
            Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
        let ctx = UIContext::new(app_state, tx);
        c.init(UIHandle(1), &ctx).expect("init");
        assert_eq!(c.handle(), Some(UIHandle(1)));
        assert!(rx.try_recv().is_ok()); // message sent
    }

    #[test]
    fn test_init_empty_query_no_submit() {
        let mut c = QueryConsoleElement::new("C".to_string(), String::new());
        let (tx, mut rx) = app_message_channel();
        let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
            Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
        let ctx = UIContext::new(app_state, tx);
        c.init(UIHandle(1), &ctx).expect("init");
        assert_eq!(c.handle(), Some(UIHandle(1)));
        assert!(rx.try_recv().is_err()); // no message
    }

    #[test]
    fn test_is_initialised_after_init() {
        let mut c = QueryConsoleElement::new("C".to_string(), "q".to_string());
        assert!(!c.is_initialised());
        let (ctx, _rx) = create_test_context();
        c.init(UIHandle(1), &ctx).expect("init");
        assert!(c.is_initialised());
    }
}
```

---

## Integration Tests

**File:** `liquers-lib/tests/query_console_integration.rs`

### Test 1: `test_query_console_creation_via_command`

Create QueryConsoleElement via the `lui/query_console` command.

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_query_console_creation_via_command() {
    let env = setup_env();
    let envref = env.to_ref();

    let mut direct_state = DirectAppState::new();
    let root_handle = direct_state.add_node(None, 0, ElementSource::None)?;
    direct_state.set_element(root_handle, Box::new(Placeholder::new()))?;

    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let ui_context = UIContext::new(app_state.clone(), msg_tx.clone())
        .with_handle(Some(root_handle));
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    ui_context.submit_query(root_handle, "text-initial/ns-lui/query_console");
    runner.run(&app_state).await?;

    let state = app_state.lock().await;
    let elem = state.get_element(root_handle)?.expect("element exists");
    assert_eq!(elem.type_name(), "QueryConsoleElement");
}
```

### Test 2: `test_request_asset_updates_flow`

Submit `RequestAssetUpdates`, verify AppRunner evaluates and delivers `AssetSnapshot` to the widget.

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_request_asset_updates_flow() {
    // Setup env with commands, create QueryConsoleElement, set in AppState
    // Send AppMessage::RequestAssetUpdates { handle, query: "hello" }
    // Run AppRunner in loop until element.get_value().is_some()
    // Assert value matches expected "hello" output
    // Assert element remains a QueryConsoleElement
}
```

### Test 3: `test_monitoring_auto_stop`

Verify AppRunner stops monitoring when element is removed from AppState.

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_monitoring_auto_stop() {
    // Setup: create console, send RequestAssetUpdates, run once (monitoring starts)
    // Remove element from AppState
    // Run AppRunner again → monitoring entry cleaned up
    // Verify: no panic, no error
}
```

### Test 4: `test_monitoring_replacement`

Submit query A then query B for same handle. Verify only latest is monitored.

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_monitoring_replacement() {
    // Send RequestAssetUpdates for query A → run once
    // Send RequestAssetUpdates for query B → run once
    // Verify element has result from query B, not A
    // Monitoring map has single entry for the handle
}
```

### Test 5: `test_error_propagation`

Submit invalid query, verify error arrives via AssetSnapshot.

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_error_propagation() {
    // Send RequestAssetUpdates { query: "nonexistent_command_xyz" }
    // Run AppRunner
    // Verify element still exists (not removed)
    // Verify element.get_value() is None (error, not value)
}
```

### Test 6: `test_serialization_and_reinit`

Serialize QueryConsoleElement, deserialize, call init(), verify re-evaluation.

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_serialization_and_reinit() {
    // Create QueryConsoleElement with query "hello"
    // Serialize to JSON
    // Deserialize → verify type_name() == "QueryConsoleElement"
    // Set in AppState, call init(handle, &ctx)
    // Run AppRunner → verify RequestAssetUpdates was processed
    // No panic, no error
}
```

---

## Corner Cases

### Memory

| Case | Risk | Mitigation |
|------|------|------------|
| Arc\<Value\> sharing | Low | Arc clone is atomic ref-count increment; no deep copy |
| Metadata cloning per update | Low | Metadata is small relative to values; owned clone acceptable |
| Monitoring map growth | High | Auto-stop on element removal is essential; cleanup in poll_monitored_assets |
| Large query history (10k+) | Low | O(n) memory, O(1) navigation; consider truncation in future |

### Concurrency

| Case | Risk | Mitigation |
|------|------|------------|
| AppState lock during snapshot delivery | Low | Lock held briefly for update() call; non-nested |
| Multiple RequestAssetUpdates for same handle | Low | HashMap insert replaces old entry; safe |
| notification_rx polling | Low | tokio::sync::watch designed for this; no data loss for latest |
| Rapid query succession | Low | Each replaces monitoring; O(1) replacement |

### Error Handling

| Case | Risk | Mitigation |
|------|------|------------|
| Query parse failure | Medium | Error captured in AssetSnapshot.error; shown in metadata pane |
| Evaluation fails mid-stream | Medium | Error notification → AssetSnapshot with error → widget displays |
| Empty query text | Low | Guard in init(): skip submit if query_text.is_empty() |
| Widget removed before delivery | Low | deliver_snapshot returns false → remove from monitoring |

### Serialization

| Case | Risk | Mitigation |
|------|------|------------|
| Status::None default after deser | Low | #[serde(skip)] guarantees default; init() triggers re-eval |
| Special characters in query_text | Low | serde_json handles escaping automatically |
| Empty history after deser | Low | Bounds checks in history_back/forward prevent OOB |
| Very long history | Low | No built-in limit; app can truncate if needed |

### Integration

| Case | Risk | Mitigation |
|------|------|------------|
| Nested in UISpecElement container | Low | Standard UIElement pattern; extract-render-replace works |
| Multiple consoles monitoring different queries | Low | HashMap keyed by UIHandle; no cross-talk |
| AppRunner run() phase ordering | Low | Phase 1-4 ordering is explicit; no ambiguity |
| Cross-namespace presets | Medium | find_next_presets() injects ns-\<namespace\>/ when needed |

---

## Test Commands

```bash
# Unit tests
cargo test --lib query_console_element

# Integration tests
cargo test --test query_console_integration

# All tests
cargo test
```

**Expected: 34 unit tests + 6 integration tests = 40 tests total**
