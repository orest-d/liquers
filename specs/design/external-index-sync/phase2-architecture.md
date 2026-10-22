# Phase 2: Solution & Architecture — Asset expiration events observable by scope

## Overview

`DependencyManager` gains a `tokio::sync::broadcast::Sender<AssetEvent>`. A private helper
`announce_expiry` sends one `AssetEvent::Expired` beside each of the three `record_expiry` call
sites. A provided `AssetManager::subscribe_asset_events(scope)` wraps a receiver in an
`AssetEventSubscription`, which filters by scope and turns `RecvError::Lagged` into an explicit
`Lagged(n)`. No required trait method is added. Every manager owns a `DependencyManager` through
`DependencyManagerAccess`, so the external manager emits events unchanged.

Rejected alternatives:
- **Generalise the per-asset `watch` channel.** It must keep coalescing for UIs, and it exists only
  for live assets.
- **Emit inside the default `record_expiry`.** A manager overriding it (as the guide's §8 example
  does) would silently lose events.
- **A new required `AssetManager` method returning the sender.** It breaks external managers, and
  the dependency manager is already the one per-manager object they must own.
- **Unbounded channel per subscriber (`mpsc`).** A slow observer would grow memory without bound.
  A bounded broadcast with a lag signal is what the issue asks for.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED` | closed | P2 | listing changes now cascade, so dependents' expiries are announced | no | no | none |
| `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` | draft | P3 | an announced stored-only expiry may be overwritten by an in-flight write; the observer then sees a later `Ready` by reconciliation | no | no | stated in the reference |
| `EVALUATION-KEEPS-DROPPED-DEPENDENCY-EDGES` | draft | P3 | may announce an expiry through a stale edge (conservative) | no | no | none |

## Interfaces

```rust
// liquers-core/src/assets.rs (re-exported where AssetNotificationMessage is)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetEventSubject { Key(Key), Query(Query) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetEvent {
    /// An asset or a stored-only key became `Expired`.
    Expired {
        subject: AssetEventSubject,
        reason: crate::metadata::ExpiryReason,
        /// The version it held before expiring, when its metadata recorded one.
        version: Option<crate::metadata::Version>,
        /// `true` for a live asset, `false` for a stored-only key.
        live: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetEventScope { All, Under(Key) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetEventReceive { Event(AssetEvent), Lagged(u64), Closed }

pub struct AssetEventSubscription { /* rx: broadcast::Receiver<AssetEvent>, scope */ }
impl AssetEventSubscription {
    /// Next event in scope. `Lagged(n)`: n events were dropped for this subscriber, so
    /// resynchronize. `Closed`: the manager is gone.
    pub async fn recv(&mut self) -> AssetEventReceive;
    /// Non-blocking form; `None` when nothing is pending.
    pub fn try_recv(&mut self) -> Option<AssetEventReceive>;
    pub fn scope(&self) -> &AssetEventScope;
}

// AssetManager<E>, provided
fn subscribe_asset_events(&self, scope: AssetEventScope) -> AssetEventSubscription {
    AssetEventSubscription::new(self.dependency_manager().subscribe_events(), scope)
}
```

`liquers-core/src/dependencies.rs` `DependencyManager`: a field `events: broadcast::Sender<AssetEvent>`
created in `new()` with capacity `ASSET_EVENT_BUFFER = 1024`; `pub(crate) fn subscribe_events(&self)
-> broadcast::Receiver<AssetEvent>`; `pub(crate) fn announce(&self, event: AssetEvent)`, which
ignores the "no receivers" error. Sync: `subscribe` and `send` are sync, `recv` async and
runtime-free (wasm32 safe). No serde on `AssetEvent` in this slice; the WebSocket follow-up maps it.

## Integration Points

`liquers-core/src/assets.rs`:
- `AssetData::record_expiry`: after `manager.record_expiry(..)`, build the subject from `self.key`,
  else `self.query`, and skip when both are absent. Version is `self.metadata.version()`, and
  `live: true`. Call `manager.dependency_manager().announce(..)`.
- `expire_stored_copy`: after the `record_expiry` call, only when `store.set_metadata` succeeded:
  `Key(key)`, `metadata.version()`, `live: false`.
- `AssetManager::expire`, stored branch: after `store.set_metadata(key, &metadata).await?`, the
  same, with `Direct { Explicit }`.
- `AssetManager::subscribe_asset_events`: provided, as above.

The order in AC-4 is a property of the existing code: `expire_with_reason` marks the root (which
sends) before `cascade_expire_dependents` (whose expiries send next).

## Error Handling

No new errors. Sending never fails visibly. A receive returns `Closed` rather than an `Err` when
the `DependencyManager` (and so the manager) is dropped. Lag is a value (`Lagged`), not an error,
because it is an expected condition for which the documented response is to resynchronize.

## Relevant Commands

None. No command or registry change. A later `ns-...` command or HTTP route would consume the
subscription; that is the out-of-process follow-up.

## Documentation Architecture

- `specs/reference/ASSETS.md` §Communication Channels: new "Manager event stream (broadcast,
  lossless or told)" beside the service and notification channels: the type, the scope, ordering
  (Phase 1 Q3), `live`, the lag contract and the store-follows-manager note.
- `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`: `subscribe_asset_events` in the API
  list.
- `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` §8: drop "or forward the event"; events are
  sent regardless of overrides.
- `affects_docs: [reference/ASSETS.md, reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md,
  guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md]`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/assets.rs` (three sites, new types, provided method), `liquers-core/src/dependencies.rs` (field, two methods), `liquers-core/src/lib.rs` if a re-export is needed |
| Crates and workflows affected | `liquers-core`; every expiry path gains one sync send |
| Existing tests likely to change | none; `record_expiry` and cascade tests keep their assertions |
| New validation | `liquers-core/tests/asset_events.rs` on three managers; `scripts/check-build-matrix.sh` wasm rows |
| Compatibility / data / concurrency / performance / security | Additive API. Sending under the asset write lock is sync and bounded. One clone of `ExpiryReason` per event per send (not per subscriber; broadcast clones on receive). Events reveal key names to in-process code only |
| Recovery | Revert; nothing else depends on the stream yet |
| Certainty and open questions | High; Q1, Q2, Q3 and Q5 are contract choices to confirm |
