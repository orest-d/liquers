# Phase 2: Solution & Architecture — asset-cancellation-outcome

## Overview
A cancel becomes a **request** carried by a lock-free `CancellationRequest` that the asset shares with
its `Context`. The run splits evaluation into a cancellable *compute* (recipe, dependencies, command)
and a non-cancellable *completion* (install value, finalize, notify, persist, register). The run
races compute against the request (`select!`, compute polled first) and is the only writer of the
terminal status (D1, D2), apart from two status claims that need no run: cancelling a `Submitted`
asset (D8) and discarding a replaced one (AC-8). Every terminal transition goes through one guard: it
happens once, from an in-flight status. A cancellation error reaching `fail_asset` finalizes
`Cancelled` with its cause stored in metadata (D5, D10); `wait_for_dependency` passes a cancelled
dependency's stored cause up unchanged and logs the cascade (AC-9 to AC-11).

Rejected: keeping the service loop as a second status writer (the race in the source issue); a new
`ErrorType` (one exists and bindings already map it); `tokio::task::spawn_blocking` for sync commands
(cannot interrupt them either, and does not exist on wasm); a `cancelled_by` metadata field (D10).

## Known-Issue Preflight
| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `ERROR-WITH-KEY-SETS-QUERY-FIELD` | draft (design in review) | P2 | `with_key` writes `query`; we use `with_query` only | No | No | Linked (Phase 1 dependencies) |
| `EXTERNAL-MANAGER-CANNOT-NOTIFY-REPLACED-ASSET` | draft (design in implementation) | P3 | External managers need the new discard primitive too | No | No | `cancel_for_replacement` is `pub`; linked in Phase 1 |
| `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS` | draft | P2 | Unchanged: a cancel drops compute *inside* the run, the run future itself completes | No | No | None |
| `WEB-CANCELLATION-INERT` | accepted | P3 | Inline evaluation finishes before a handle exists; AC-5 only | No | No | None |
| `CORE-TOKIO-REMOVAL` | accepted | P3 | Adds one `tokio::sync::Notify` use on wasm, beside existing `mpsc`/`watch`/`RwLock` | No | No | Listed for that removal |
| `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` | draft | P3 | Same shape (late write after a key change); not fixed here | No | No | None |

Command namespaces involved: **none** (core only).

## Interfaces
New, `liquers-core/src/assets.rs` (sync methods are lock-free; `requested` is async):
```rust
/// Shared by an asset and every `Context` of its run. Never held across an await.
pub(crate) struct CancellationRequest {
    requested: AtomicBool,
    cause: std::sync::Mutex<Option<Error>>,   // set before `requested` (Release/Acquire)
    notify: tokio::sync::Notify,              // `sync` feature, available on wasm32
}
impl CancellationRequest {
    pub(crate) fn request(&self, cause: Error) -> bool;  // first cause wins; notify_waiters
    pub(crate) fn is_requested(&self) -> bool;
    pub(crate) fn cause(&self) -> Option<Error>;
    pub(crate) fn clear(&self);                           // after a successful finalization (D6)
    pub(crate) async fn requested(&self);                 // Notified::enable, then re-check
}
```
Changed: `AssetData.cancelled: bool` → `cancellation: Arc<CancellationRequest>`;
`AssetData::set_cancelled` removed (no use outside `assets.rs`). New `AssetRef` methods:
```rust
/// Request cancellation for a replacement: an in-flight asset ends `Cancelled` now and
/// its run's late result is discarded (AC-8). Used by set/set_state/remove/to_override.
pub async fn cancel_for_replacement(&self) -> Result<(), Error>;
pub(crate) async fn cancellation(&self) -> Arc<CancellationRequest>; // clone of the shared Arc
async fn finish_cancelled(&self, cause: Error) -> Result<(), Error>; // mirrors fail_asset
async fn compute(&self, payload: Option<E::Payload>) -> Result<RecipeEvaluation<E::Value>, Error>;
async fn complete_evaluation(&self, outcome: RecipeEvaluation<E::Value>) -> Result<(), Error>;
```
`finalize_status_with_version` returns `bool` (whether it finalized); `persist_with_status_tracking`
loses its `cancelled` parameter (callers: `evaluate`, the test-only `set_value`, three manager sites
passing `false`). `run_with_future{,_inline}` take the compute future.

`liquers-core/src/context.rs`: `Context` gains `cancellation: Arc<CancellationRequest>` (set in
`new`, copied by `with_volatile`, `clone_context`, `Clone`) and two sync methods:
```rust
pub fn is_cancelled(&self) -> bool;
/// `Err(cause)` once cancellation was requested, for `context.check_cancelled()?`.
pub fn check_cancelled(&self) -> Result<(), Error>;
```
`liquers-core/src/metadata.rs`: `MetadataRecord::with_cancellation(&mut self, cause: Error)` and the
`Metadata` wrapper: status `Cancelled`, `error_data = Some(cause)`, `is_error` stays `false`, an info
log entry (not `error(..)`, which would set `Error`).
Value types, traits, commands: none. Sync/async: as listed; nothing new blocks.

## Integration Points
`liquers-core/src/assets.rs`:
- `AssetRef::cancel`: `Submitted` → `finish_cancelled` at once (D8); `Dependencies`/`Processing`/
  `Partial` → `request(own cause)`; no `AssetServiceMessage::Cancel` sent; the 5 s wait stays (D7).
- `process_service_messages`: the `Cancel` arm only calls `request` (kept for external senders); it
  no longer sets a status or ends the loop.
- `run_with_future{,_inline}`: if requested before start, skip compute; `select!` (biased: compute,
  then `wait_to_finish`, then `requested`); `complete_evaluation` runs after the select (D2).
- `evaluate` → `compute` + `complete_evaluation`; the `Err` branch's direct status write goes.
- One guard, `is_in_flight(status)` (`None`, `Recipe`, `Submitted`, `Dependencies`, `Processing`,
  `Partial`), in `finalize_status_with_version`, `fail_asset` and `finish_cancelled`.
  `complete_evaluation` notifies, persists and registers only if it finalized, then `clear()`s.
- `finish_run_with_result`: on `Err(e)` with `e.is_cancelled()` and an own request, use the own cause
  (D12); a cancellation without `query` gets this asset's query; then `fail_asset`.
- `fail_asset`: a cancellation error → `finish_cancelled`.
- `save_to_store`: the two flag checks become "status is `Cancelled`" (defensive; `SaveOutcome::Skipped`).
- `submitted()`: no-op on a finished asset, so a re-park cannot revive a cancelled one.
- `DefaultAssetManager::wait_for_dependency`: `Cancelled` → cause = `stored_error()` if a
  cancellation, else built for the dependency; warn on the parent unless its own cancel is requested;
  `Err(cause)`. `fail_due_to_dependency` calls removed (D1: the run decides; an unhandled failure ends
  `Error` with the same cause via `fail_asset`); the function is deleted.
- `AssetManager::wait_for_dependency` (default, used by `ImmediateAssetManager`): same cascade when
  `get()` returns a `Cancelled` state.
- Replacement sites (`remove`, `set`, `set_state`, both managers' `to_override`, `AssetRef::to_override`):
  `cancel()` → `cancel_for_replacement()`.
- `no_binary_error` `Cancelled` arm and module docs (`# Expiration, recovery, and removal`).

`liquers-core/src/state.rs` `State::value_error`: for `Cancelled`, the stored cancellation error if
any, else today's synthesized one. `liquers-axum/tests/assets_api_endpoints.rs`: `aae92`, `aae38`
assert one status each.

## Error Handling
| Case | Error |
|---|---|
| Own cancel | `Error::cancelled(format!("Asset {subject} was cancelled")).with_query(&query)` |
| Replaced while in flight | `Error::cancelled(format!("Asset {subject} was replaced")).with_query(&query)` |
| Cancelled dependency with no stored cause | `Error::cancelled(format!("Dependency {subject} was cancelled")).with_query(&dep_query)` |
| Command's own `Error::cancelled(..)` without query | `.with_query(&own_query)` added; otherwise unchanged |
| Cascade | the dependency's cause, unchanged; warning `"Dependency {dep} was cancelled; root cause: {root}"` |

`subject` is `expiry_subject()` (key or query). A missing query (`get_query` fails) leaves `query`
unset rather than failing. No `unwrap`; the cause mutex recovers a poisoned lock with `into_inner`.

## Relevant Commands
None new. Any command may take `context` and call `is_cancelled()` / `check_cancelled()?`.

## Documentation Architecture
| Path | Kind | Change |
|---|---|---|
| `guides/COMMAND_DESIGN_GUIDE.md` | guide, new, area `core/commands` | Cooperative cancellation; what `cancel()` guarantees; sync vs async commands; propagating `Error::cancelled`; dependencies and cascade |
| `guides/COMMAND_REGISTRATION_GUIDE.md` | guide | Link from Quick Reference and "Waiting for dependencies" |
| `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` | guide | `cancel_for_replacement` in the primitives table; `cancel()` row |
| `guides/WEB_API_GUIDE.md` | guide | `q/cancel` may leave a finished query `Ready` |
| `reference/ASSETS.md` | reference | Scenario 4, cancellation path diagram, `Cancelled` description |
| `reference/ASSET_SET_OPERATION.md` | reference | Replace the "cancelled flag" mechanism with discard |
| `reference/DEPENDENCIES_STATUS.md` | reference | Flow D: cascade; no `fail_due_to_dependency` |
| `reference/ASSET_LIFECYCLE.md` | reference | Persistence-outcome row for cancelled assets |
| `reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`, `reference/WEB_API_SPECIFICATION.md` | reference | Best-effort cancel, cause in `error_data` |

Proposed `affects_docs`: the nine existing paths above. `specs/README.md`: the new guide under
commands.

## Risks
| Assessment | Finding |
|---|---|
| Files likely to change | `assets.rs` (large), `context.rs`, `metadata.rs`, `state.rs`; axum tests |
| Crates and workflows affected | `liquers-core`; `liquers-axum` tests; wasm build (inline path) |
| Existing tests likely to change | `assets.rs` save-skip tests (fixture sets `Status::Cancelled` instead of the flag), `dependency_failure_error_names_key` (cause type), `asset_failure_contract.rs` (cancelled `value_error` may carry a cause), `aae92`, `aae38` |
| New validation | Phase 3 tests per AC; `scripts/check-build-matrix.sh` for wasm |
| Compatibility | `AssetData::set_cancelled` removed; `ErrorType::Cancelled` reaches clients for cascades that were `Error`; a dependency failure no longer flips the parent to `Error` before its run ends (D1) — a command that handles it now ends ready, consistently with AC-9 |
| Concurrency | A late persist of an already-`Ready` asset can still race a replacement write, as today (window between finalize and `store.set`); not widened |
| Recovery | Revert the PR; no stored format change (`error_data` on a `Cancelled` record is an existing field) |
| Certainty and open questions | High for the run/finalize split; medium for the inline path until Phase 3 tests run on wasm. None open. |
