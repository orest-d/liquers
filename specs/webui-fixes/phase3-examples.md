# Phase 3: Examples & Use-cases - webui-fixes

## Example Type

**Choice:** Runnable prototypes. Each example is either an existing runnable example
(`liquers-lib/examples-web/ui_spec_demo`) or a test that lands in the repository — no
throwaway conceptual code, because all three fixes are behavioural and only observable at runtime.

## Overview Table

| # | Type | Name | What it demonstrates / checks | Issue |
|---|------|------|-------------------------------|-------|
| E1 | Example (browser) | Query console in `ui_spec_demo` | Type a query, press **Enter**, result renders | W1 |
| E2 | Example (browser) | Same console, second query | Input keeps the *submitted* query after re-render; volatile refresh re-uses it | W2 |
| E3 | Example (browser) | Menu action with no pending asset | DOM updates immediately after a synchronous mutation | W3 |
| U1 | Unit (native) | `render_web_puts_action_on_input` | The `<input>` carries `data-lq-action` with the same `Apply` as "Go" | W1 |
| U2 | Unit (native) | `snapshot_query_updates_query_text` | A snapshot with a new query sets `query_text` | W2 |
| U3 | Unit (native) | `snapshot_query_pushes_history_once` | Adopting a query appends to history; a repeat snapshot does not duplicate | W2 |
| U4 | Unit (native) | `expired_snapshot_refreshes_adopted_query` | After adopting, the expiry re-request uses the new query, not the stale one | W2 |
| U5 | Unit (native) | `volatile_refresh_uses_adopted_query` | Same for the volatile refresh path | W2 |
| U6 | Unit (native) | `snapshot_without_query_keeps_query_text` | Empty `snapshot.query` is a no-op (back-compat for hand-built snapshots) | W2 |
| I1 | Integration (native) | `runner_stamps_query_into_snapshot` | `AppRunner` puts the monitored query into every snapshot it delivers | W2 |
| I2 | Integration (native) | `submit_command_updates_console_state` | `ApplyToInput` → `lui/submit` → console shows the typed query | W1+W2 |
| I3 | Integration (native) | `repaint_requested_after_sync_mutation` | `take_repaint_request()` is true after a message-only `run`, false after being taken | W3 |
| I4 | Integration (native) | `repaint_request_false_on_idle_run` | An idle `run` does not request a repaint (no busy re-render) | W3 |
| E2E | Playwright | `webui.spec.ts` — 3 new cases | E1/E2/E3 in headless Chromium | W1–W3 |

## Example 1: Enter submits in the browser query console (W1)

**Scenario:** The demo page gains a query console. The user types `hello` into the input and
presses Enter.

**Context:** Today only clicking "Go" works — the fix's whole point.

**Code (added to `liquers-lib/examples-web/ui_spec_demo/src/lib.rs`):**

```rust
const DASHBOARD_YAML: &str = r#"
menu:
  items:
  - !button
    label: Add Dashboard
    action:
      query: "dashboard/q/ns-lui/add-child"
  - !button
    label: Add Console
    action:
      query: "hello/q/ns-lui/query_console/add-child"
layout: vertical
"#;

/// Trivial command so the console has something to evaluate. `hello/q/...` passes it to the
/// console as a *query value*, so the console opens with `hello` in its input.
fn hello(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("Hello from the browser!"))
}
```

**Expected output:** clicking *Add Console* renders

```html
<div class="lq-qc-toolbar">
  <input id="qc-input-2" class="lq-query-input" value="hello"
         data-lq-action='"apply:2:qc-input-2:ns-lui/submit"'/>
  <span class="lq-go" data-lq-action='"apply:2:qc-input-2:ns-lui/submit"'>Go</span>
  …
</div>
```

Pressing Enter in that input dispatches `Apply` → `AppMessage::ApplyToInput` → `lui/submit`, and
the console content becomes `Hello from the browser!`.

## Example 2: The console keeps the query the user submitted (W2)

**Scenario:** With the console showing `hello`, the user selects the input, types
`hello/ns-lui/markdown`, and presses Enter.

**Context:** Today the result renders but the input snaps back to `hello`, and any volatile or
expiry-driven refresh silently re-evaluates the *old* query.

**Flow after the fix:**

1. `ApplyToInput { handle, input: "hello/ns-lui/markdown", query: "ns-lui/submit" }`
2. `lui/submit` sends `RequestAssetUpdates { handle, query: "hello/ns-lui/markdown" }`
3. `AppRunner` evaluates, stores `MonitoredAsset { query }`, and stamps the query into the snapshot
4. `QueryConsoleElement::update` calls `adopt_query` → `query_text` + history updated
5. Re-render emits `value="hello/ns-lui/markdown"`, and the expiry/volatile paths re-request
   that query

**Expected output:** the input keeps the submitted text; `history == ["hello/ns-lui/markdown"]`.

## Example 3: Synchronous mutation repaints (W3)

**Scenario:** A menu action that only mutates `AppState` (e.g. a `lui/remove` or an `activate`
entry) with no asset left pending.

**Context:** With `ImmediateAssetManager` the whole evaluation resolves inline, so after
`runner.run` returns, `needs_repaint()` is already `false` and the DOM keeps showing the removed
element until some unrelated async update arrives.

**Code (browser loop, `liquers-lib/src/ui/web/app.rs`):**

```rust
loop {
    let _ = runner.run(&loop_state).await;
    let changed = runner.take_repaint_request();      // must be called unconditionally
    if first || changed || runner.needs_repaint() {
        render_roots_into(&loop_root, &loop_state);
        first = false;
    }
    gloo_timers::future::TimeoutFuture::new(16).await;
}
```

**Expected output:** the removed element disappears within one 16 ms tick.

## Corner Cases

### 1. Memory

- `AssetSnapshot::query` adds one `String` per snapshot; snapshots are already cloned per delivery,
  so the extra cost is one short allocation per notification. `MonitoredAsset` holds one more
  `String` per monitored handle — bounded by the number of live console elements.
- `history` grows unboundedly if a program submits queries in a loop. Pre-existing (`submit_query`
  already pushes on every submit); `adopt_query` must not make it worse — hence the
  "don't push a duplicate of the last entry" rule (U3).

### 2. Concurrency

- Two `RequestAssetUpdates` for the same handle in quick succession: the second replaces the
  monitoring entry, so the console adopts the second query — the last submit wins, which matches
  what the user sees. No lock is held across `update()`, so no deadlock is introduced.
- wasm is single-threaded; `dirty` is `&mut self` state on the single runner. On native, `AppRunner`
  is owned by one event loop — same guarantee.
- A repaint flag lost to a `take_repaint_request()` call in a *different* place would cause a stale
  frame, so exactly one consumer per loop is required (documented on the method).

### 3. Errors

- Evaluation failure: the error snapshot is stamped with the failing query, so the console shows
  the query the user typed together with the error, and the "Go"/Enter retry uses the same text.
- Malformed `data-lq-action` JSON: `dispatch_dom_event` returns silently (unchanged behaviour).
- Enter pressed in an input whose element is gone (handle removed mid-flight): `ApplyToInput` runs,
  `deliver_snapshot` reports `Missing`, monitoring stops — no panic.
- Empty input + Enter: `lui/submit` gets an empty query; `parse_query("")` yields the empty query,
  which is the pre-existing behaviour of clicking "Go" with an empty box.

### 4. Serialization

- `AssetSnapshot` is not serialized (`Clone, Debug` only) — no schema evolution concern.
- `QueryConsoleElement` typetag round-trip is unchanged in shape; after the fix, a serialized
  console restores the *last submitted* query, which is the intent of the persistent fields.
- Runtime-only fields (`value`, `metadata`, `error`, `next_presets`, `ui_element`) remain `#[serde(skip)]`.

### 5. Integration

- `lui/submit` keeps its registration and signature → `register_lui_commands!` unchanged →
  `liquers-py` and `liquers-axum` unaffected.
- SSR (`render_app_ssr`) output changes only by one extra attribute on the console input; the
  existing `webui_ssr.rs` assertions still hold.
- egui backend: `QueryConsoleElement::submit_query` sets `query_text` *before* sending, so
  `adopt_query` sees an unchanged query and does nothing — no double history entry (U3).

## Test Plan

### Unit tests

**File:** `liquers-lib/src/ui/widgets/query_console_element.rs` (`mod tests`)

| Test | Checks |
|---|---|
| `render_web_puts_action_on_input` (U1) | `render_web` output contains `data-lq-action` inside the `<input …>` tag and the `Apply` payload matches the Go control (`#[cfg(feature = "webui")]`) |
| `snapshot_query_updates_query_text` (U2) | snapshot with `query: "new-q"` → `query_text == "new-q"` |
| `snapshot_query_pushes_history_once` (U3) | history == `["new-q"]` after two identical snapshots |
| `expired_snapshot_refreshes_adopted_query` (U4) | `Status::Expired` snapshot with a new query sends `RequestAssetUpdates { query: "new-q" }` |
| `volatile_refresh_uses_adopted_query` (U5) | `Status::Volatile` snapshot with a new query → delayed refresh carries `"new-q"` |
| `snapshot_without_query_keeps_query_text` (U6) | `query: String::new()` leaves `query_text`/history untouched |

All 6 existing `AssetSnapshot` literals in this module gain `query: "q".to_string()` so current
expectations (which assume the console's own query) keep holding.

**File:** `liquers-lib/src/ui/widgets/markdown_element.rs` — 2 literals updated (`query: String::new()`),
no behavioural change (markdown ignores the query).

### Integration tests

**File:** `liquers-lib/tests/query_console_integration.rs` (extend)

| Test | Flow |
|---|---|
| `runner_stamps_query_into_snapshot` (I1) | register `hello`; `RequestAssetUpdates { handle, "hello" }`; run; assert the console's `query_text` is `"hello"` |
| `submit_command_updates_console_state` (I2) | console at `"hello"`; send `ApplyToInput { handle, input: "hello/ns-lui/markdown", query: "ns-lui/submit" }`; run; assert `query_text == "hello/ns-lui/markdown"` and the rendered `value="…markdown"` |

**File:** `liquers-lib/tests/ui_runner.rs` (extend)

| Test | Flow |
|---|---|
| `repaint_requested_after_sync_mutation` (I3) | submit a query that only mutates `AppState`; after `run`, `take_repaint_request()` is `true`, and `false` on the immediately following call |
| `repaint_request_false_on_idle_run` (I4) | `run` with no messages and no evaluations → `take_repaint_request()` is `false` |

### End-to-end (Playwright)

**File:** `liquers-lib/examples-web/ui_spec_demo/tests/webui.spec.ts` (extend)

1. `enter key submits in the query console` — click *Add Console*, fill the input, `press('Enter')`,
   expect the result text; assert zero `pageerror`.
2. `console keeps the submitted query` — submit a second query, expect the input's `value` to equal
   the submitted query after the re-render.
3. `synchronous menu action repaints` — trigger an action with no pending asset and expect the DOM
   to settle without any further interaction.

### Manual validation

```bash
# native unit + integration
cargo test -p liquers-lib --lib ui::widgets::query_console_element
cargo test -p liquers-lib --test query_console_integration --test ui_runner

# webui feature build (no egui/polars) + SSR tests
cargo test -p liquers-lib --no-default-features --features webui,image-support --test webui_ssr

# wasm build
rustup target add wasm32-unknown-unknown
cargo check -p liquers-lib --no-default-features --features webui --target wasm32-unknown-unknown

# browser e2e
cd liquers-lib/examples-web/ui_spec_demo && npm ci && npx playwright install chromium && npx playwright test
```

## Auto-Invoke: liquers-unittest Skill Output

Applying the project's unit-test conventions (`#[cfg(test)] mod tests` in-file, `#[tokio::test]`
for async, memory stores for fixtures, no `unwrap()` outside tests) the generated skeletons are:

```rust
// liquers-lib/src/ui/widgets/query_console_element.rs
#[test]
fn snapshot_query_updates_query_text() {
    let mut c = QueryConsoleElement::new("C".to_string(), "old-q".to_string());
    c.set_handle(UIHandle(1));
    let (ctx, _rx) = create_test_context();
    let snapshot = AssetSnapshot {
        query: "new-q".to_string(),
        value: Some(Arc::new(Value::from("v"))),
        metadata: Metadata::new(),
        error: None,
        status: Status::Ready,
    };
    c.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);
    assert_eq!(c.query_text, "new-q");
    assert_eq!(c.history, vec!["new-q".to_string()]);
}
```

```rust
// liquers-lib/tests/ui_runner.rs
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repaint_requested_after_sync_mutation() {
    let env = setup_env();
    let envref = env.to_ref();
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
    let (msg_tx, msg_rx) = app_message_channel();
    let ui_context = UIContext::new(app_state.clone(), msg_tx.clone());
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    ui_context.submit_root_query("hello/ns-lui/add-child");
    runner.run(&app_state).await.expect("runner.run");

    assert!(runner.take_repaint_request(), "a processed message must request a repaint");
    runner.run(&app_state).await.expect("runner.run");
    assert!(!runner.take_repaint_request(), "an idle run must not request a repaint");
}
```

**Query validation:** every query used above (`hello`, `ns-lui/submit`,
`dashboard/q/ns-lui/add-child`, `hello/q/ns-lui/query_console/add-child`,
`hello/ns-lui/add-child`, `hello/ns-lui/markdown`) contains no spaces or newlines, uses no `-R/`
resource part (so no store is required), and refers only to commands registered by
`register_lui_commands!` or by the test/example itself.
