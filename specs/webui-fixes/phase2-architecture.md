# Phase 2: Solution & Architecture - webui-fixes

## Overview

Three independent, small-surface changes inside `liquers-lib::ui`: (W1) the query input carries its
own `data-lq-action` and the delegated listener stops treating a click on a text field as an action
trigger; (W2) `AssetSnapshot` carries the query that produced the monitored asset, so
`QueryConsoleElement` reconciles `query_text`/history from the snapshot instead of guessing;
(W3) `AppRunner` tracks a "something changed" flag that the browser loop consumes to force a
re-render. W4 is documentation only.

## Data Structures

### Modified structs

```rust
// liquers-lib/src/ui/message.rs
#[derive(Clone, Debug)]
pub struct AssetSnapshot {
    /// Query the monitored asset was created from. Empty when unknown (e.g. a
    /// snapshot built before a query is known, or hand-built in tests).
    pub query: String,          // NEW — first field
    pub value: Option<Arc<Value>>,
    pub metadata: Metadata,
    pub error: Option<Error>,
    pub status: Status,
}
```

Ownership: an owned `String`. The snapshot is already cloned per delivery and must be
non-generic (it crosses the `dyn UIElement` boundary), so borrowing is not an option; the query is
short and cloned at most once per notification.

```rust
// liquers-lib/src/ui/runner.rs  (private)
struct MonitoredAsset<E: Environment> {
    asset_ref: AssetRef<E>,
    notification_rx: tokio::sync::watch::Receiver<AssetNotificationMessage>,
    query: String,              // NEW — source query, stamped into every snapshot
}

pub struct AppRunner<E: Environment> {
    // … unchanged fields …
    dirty: bool,                // NEW — set by any state-changing work in `run`
}
```

### New enums

```rust
// liquers-lib/src/ui/runner.rs  (private)
/// Result of pushing a snapshot at an element.
enum DeliveryOutcome {
    /// Element no longer exists — caller stops monitoring it.
    Missing,
    /// Delivered; carries the element's own repaint verdict.
    Delivered(UpdateResponse),
}
```

Replaces `deliver_snapshot`'s bare `bool`, so the element's `UpdateResponse` (currently discarded)
can drive `dirty`. Matches are exhaustive — no `_` arm, per the codebase convention.

### ExtValue extensions

None.

## Trait Implementations

No new traits and no trait-signature changes. `UIElement::update` keeps
`fn update(&mut self, &UpdateMessage, &UIContext) -> UpdateResponse`; only
`QueryConsoleElement`'s implementation body changes.

## Generic Parameters & Bounds

Unchanged. `AppRunner<E>` keeps `E: Environment<Value = Value>, E::Payload: UIPayload + From<SimpleUIPayload>`.
`AssetSnapshot` stays non-generic (required: it is delivered through `dyn UIElement`).

## Sync vs Async Decisions

| Function | Choice | Rationale |
|---|---|---|
| `dispatch_dom_event` | sync | DOM event callback; only sends channel messages |
| `lui/submit` | stays **sync** | With W2 it needs no `AppState` access, so no `.await`, no downcast |
| `AppRunner::take_repaint_request` | sync | Pure flag read/clear, called from the render loop |
| `AppRunner::run` | async (unchanged) | Already awaits evaluation |
| `QueryConsoleElement::update` | sync (unchanged) | Called by the runner between locks |

## Function Signatures

```rust
// liquers-lib/src/ui/web/app.rs  (wasm-only `browser` module)

/// True for controls that own their click (text entry): clicking into them must not fire an
/// action. Enter-key handling is unaffected.
fn is_text_entry(el: &web_sys::Element) -> bool;

fn dispatch_dom_event(ev: &web_sys::Event, ctx: &UIContext);   // unchanged signature, new guard

// liquers-lib/src/ui/runner.rs

impl<E> AppRunner<E> {
    /// True when message processing / evaluation / snapshot delivery changed anything since the
    /// last call. Clears the flag. Independent of `needs_repaint()`, which reports *pending*
    /// async work; this reports *completed* work.
    pub fn take_repaint_request(&mut self) -> bool;

    async fn build_snapshot(asset_ref: &AssetRef<E>, query: &str) -> AssetSnapshot;   // + query

    async fn deliver_snapshot(
        handle: UIHandle,
        snapshot: AssetSnapshot,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
        sender: &AppMessageSender,
    ) -> DeliveryOutcome;                                        // was `-> bool`
}

// liquers-lib/src/ui/widgets/query_console_element.rs

impl QueryConsoleElement {
    /// Adopt a query submitted elsewhere (web `lui/submit`, a preset, an init query): set
    /// `query_text`, append to history, and drop query-scoped caches. No message is sent —
    /// the caller is already monitoring the asset. Returns true when the query changed.
    fn adopt_query(&mut self, query: &str) -> bool;
}
```

## Integration Points

| File | Change | Issue |
|---|---|---|
| `liquers-lib/src/ui/widgets/query_console_element.rs` | `render_web`: action attr on the `<input>`; `update`: `adopt_query` from the snapshot | W1, W2 |
| `liquers-lib/src/ui/web/app.rs` | `dispatch_dom_event` click guard; render loop consumes `take_repaint_request()` | W1, W3 |
| `liquers-lib/src/ui/message.rs` | `AssetSnapshot::query` | W2 |
| `liquers-lib/src/ui/runner.rs` | `MonitoredAsset::query`, `dirty` + `take_repaint_request`, `DeliveryOutcome` | W2, W3 |
| `liquers-lib/examples/ui_*.rs` (5 egui apps) | `request_repaint` also on `take_repaint_request()` | W3 |
| `liquers-lib/examples-web/ui_spec_demo/src/lib.rs` | menu entry creating a query console (enables e2e) | W1, W2 |
| `specs/ISSUES.md` | W1–W3 → Resolved; W4 → Resolved (async-wasm-refactor) | all |

## Relevant Commands

### New commands

None. This is deliberate: the alternative W2 design (mutating the console inside `lui/submit`)
would have forced `submit` to become `async` and to downcast `dyn UIElement`.

### Relevant existing namespaces

- `lui` — `submit` (web Apply target), `query_console`, `ui_spec`, `add-*`/navigation words.
- `egui` namespace and the `pl`/image namespaces are unaffected.

Queries used by the fixes and their tests are the already-registered ones:
`ns-lui/submit`, `hello`, `dashboard/q/ns-lui/add-child`, `text-hello/ns-lui/query_console/add-child`.

## Web Endpoints

None (`liquers-axum` is untouched).

## Error Handling

No new error paths. Existing behaviour is preserved:

- `dispatch_dom_event` returns silently on a malformed/absent action (unchanged).
- `AppRunner::handle_request_asset_updates` builds an error snapshot on evaluation failure; it now
  also stamps the query into that snapshot, so the console still adopts the query it failed on
  (the user sees what they typed, with the error).
- `adopt_query` cannot fail — it is pure state assignment.
- All errors continue to use `liquers_core::error::Error` typed constructors; no new `ErrorType`.

## Serialization Strategy

- `AssetSnapshot` is `#[derive(Clone, Debug)]` only — not serialized, so adding a field is not a
  wire-format change.
- `QueryConsoleElement`'s persistent fields (`query_text`, `history`, `history_index`, `data_view`)
  are unchanged; `adopt_query` writes exactly those, so a serialized console now restores the
  query the user last submitted rather than the constructor's initial one. Typetag round-trip is
  unaffected.
- `dirty` lives on `AppRunner`, which is not serializable.

## Concurrency Considerations

- `dirty` is a plain `bool` behind `&mut self`; `AppRunner` is single-owner (one render loop).
- `deliver_snapshot` keeps the existing extract-update-replace discipline: the `AppState` lock is
  never held across `update()`.
- Delivery order is unchanged, so W2 has no new race: the console adopts the query of whichever
  asset the runner is monitoring, which is by construction the last `RequestAssetUpdates` for that
  handle.
- wasm remains single-threaded; `try_lock` in `render_roots_into` is unchanged.

## Compilation Validation

- Adding `AssetSnapshot::query` breaks every struct literal. There are 10 (verified with
  `grep -rn "AssetSnapshot {"`): 6 in `query_console_element.rs` tests, 2 in
  `markdown_element.rs` tests, 2 in `runner.rs` (error snapshot + `build_snapshot`) — all updated
  in the same change (Phase 4, Step 2).
- `deliver_snapshot`'s new return type has exactly two call sites, both in `runner.rs`.
- `take_repaint_request` is additive; existing `run`/`needs_repaint` callers keep compiling.
- The W1 guard uses `web_sys::HtmlInputElement`/`HtmlTextAreaElement`; `HtmlInputElement` is
  already in the `web-sys` feature list — `HtmlTextAreaElement` must be added to
  `liquers-lib/Cargo.toml` (or the check restricted to `tag_name()`, which needs no new feature).
  **Decision: use `tag_name()` + `get_attribute("type")`, no new web-sys feature.**

## References to liquers-patterns.md

- Explicit match arms everywhere (`DeliveryOutcome`, `UpdateResponse`, `UiAction`) — no `_` arms.
- No `unwrap()`/`expect()` in library code; DOM lookups stay `Option`-based.
- Errors via typed constructors on `liquers_core::error::Error`.
- Async only where I/O or evaluation happens; the render loop stays sync-friendly.
- Rich UI behaviour stays in `liquers-lib`; `liquers-core` untouched.
