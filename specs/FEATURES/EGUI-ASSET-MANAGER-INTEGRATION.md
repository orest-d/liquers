# EGUI-ASSET-MANAGER-INTEGRATION

Status: Draft

## Summary
Replace temporary/implicit asset-manager coupling in egui widgets with a stable integration interface.

## Problem
Current widget integration includes temporary paths and direct assumptions about asset-manager access, making behavior brittle and harder to test.

## Current Situation (Concrete)
1. `TextEditor` pulls asset data by spawning a task and calling `envref.get_asset_manager().get(...).get()...`, and still carries a fallback TODO path (`liquers-lib/src/egui/widgets.rs`).
2. `AssetStatus` creates its own background notification task and directly subscribes to asset notifications (`liquers-lib/src/egui/widgets.rs`).
3. `AppRunner` already has centralized polling/monitoring (`liquers-lib/src/ui/runner.rs`), but widget-level code still duplicates parts of asset lifecycle handling.
4. Result: two integration styles coexist:
   1. centralized (`AppRunner` + `AssetSnapshot`),
   2. widget-owned async tasks (`AssetStatus`, `TextEditor`).

## Goals
1. Define a stable adapter boundary between widgets and asset manager.
2. Decouple widget logic from transient integration details.
3. Improve testability of update/poll/render workflow.

## Proposed Scope
1. Introduce adapter trait(s) for asset fetch/poll/status operations required by widgets.
2. Refactor widget code to depend on adapter, not direct manager internals.
3. Add integration tests for update cycle and state transitions.

## Examples

### Example A: TextEditor load (today vs target)
Today:
1. Widget owns async loading logic and channel.
2. Widget depends on `EnvRef<E>` and direct asset manager behavior.

Target:
1. Widget asks adapter for text by key:
   1. `adapter.request_text(key) -> RequestId`
   2. `adapter.poll_text(request_id) -> Pending | Ready(String) | Error`
2. Widget only renders based on adapter state.

### Example B: Asset status panel (today vs target)
Today:
1. Widget subscribes directly to `AssetRef` notifications and keeps its own task/lifecycle.

Target:
1. `AppRunner` (or adapter service) owns subscriptions and publishes normalized snapshots.
2. Widget receives `AssetSnapshot` updates only.
3. No direct `AssetRef`/subscription logic in widget.

### Example C: QueryConsole update redraw
Target behavior:
1. query submission produces a tracked handle/request id,
2. completion/error emits a terminal update event,
3. UI code requests repaint on terminal event (or on any changed snapshot).

## Concrete Solution Alternatives

### Alternative 1: Thin Adapter (Incremental, recommended)
Define a minimal trait used by widgets:
1. `request_asset(query|key) -> Handle`
2. `poll_snapshot(handle) -> Option<AssetSnapshot>`
3. `cancel(handle)`
4. `latest_snapshot(handle) -> Option<AssetSnapshot>`

Responsibility split:
1. `AppRunner` remains owner of real `AssetRef` and subscriptions.
2. Widgets become passive renderers over snapshots.
3. Existing message bus (`AppMessage`) remains primary transport.

Pros:
1. low migration risk,
2. minimal API surface,
3. easy to test with mock adapter.

Cons:
1. still tied to polling cadence,
2. fewer advanced controls (batching/priorities).

### Alternative 2: Asset Service + Event Stream
Introduce a dedicated UI asset service layer:
1. service owns requests, subscriptions, caching of latest snapshots,
2. widgets subscribe to stream-like updates by handle,
3. runner drives service ticks.

Pros:
1. clearer single ownership of asset lifecycle,
2. reusable for egui/websocket/other frontends.

Cons:
1. larger refactor,
2. extra state coordination layer.

### Alternative 3: Fully Message-Driven UI Store
Make app state the only source of truth:
1. all asset events are reduced into `AppState`,
2. widgets read only from state, never from adapter directly,
3. request APIs emit commands/events only.

Pros:
1. strongest decoupling and determinism,
2. excellent replay/testing model.

Cons:
1. highest implementation cost,
2. broader architectural change.

## Recommended Path
Use Alternative 1 now, design APIs so Alternative 2 stays possible:
1. introduce thin adapter trait and mock implementation,
2. migrate `TextEditor` and `AssetStatus` off direct asset manager/subscription usage,
3. standardize completion/error snapshot events for redraw,
4. add regression tests in `liquers-lib/tests/ui_runner.rs` and widget tests.

## Proposed Adapter Sketch
```rust
pub trait UiAssetAdapter: Send + Sync {
    fn request_query(&self, query: String, handle: UIHandle) -> Result<AssetRequestId, Error>;
    fn request_key(&self, key: Key, handle: UIHandle) -> Result<AssetRequestId, Error>;
    fn latest(&self, id: AssetRequestId) -> Option<AssetSnapshot>;
    fn poll_updates(&self) -> Vec<(UIHandle, AssetSnapshot)>;
    fn cancel(&self, id: AssetRequestId);
}
```

Notes:
1. trait is intentionally frontend-facing, not `AssetRef`-facing,
2. `AssetSnapshot` remains non-generic to keep UI wiring simple.

## Acceptance Criteria
1. Widget code no longer relies on temporary integration path.
2. Adapter-backed integration passes existing and new tests.
3. Poll/update/render loop remains functionally equivalent or improved.
4. `TextEditor` and `AssetStatus` no longer spawn direct asset-manager tasks/subscriptions.
5. Query completion/error reliably results in visible UI update (repaint trigger path verified by test).
