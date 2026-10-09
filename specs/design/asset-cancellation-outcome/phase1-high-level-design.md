# Phase 1: High-Level Design — asset-cancellation-outcome

## Purpose
Cancellation exists to save work, not to discard work that has already been done. Today an
evaluation that finishes after `cancel()` ends `Ready` but is silently not stored, and nothing lets a
command notice a cancel while it runs. This design makes the terminal status a single, deterministic
decision: a successful evaluation ends ready and is stored; an unfinished one ends `Cancelled`; and a
command can stop early by checking `Context::is_cancelled()` and returning a cancellation error.
A cancellation stays distinct from a failure as it cascades to dependents, and always names the
asset that was cancelled in the first place.

## Problem Example
`liquers-axum/tests/assets_api_endpoints.rs` `aae92_cancel_while_processing_reports_cancelled_deterministically`:
the keyed query `sleep_long` runs a synchronous command that blocks for 500 ms. It is submitted, polled
until `Processing`, then cancelled with `POST q/cancel`.

- **Today:** `AssetRef::cancel` sets the cancelled flag and sends `AssetServiceMessage::Cancel`. The
  sync command cannot be interrupted, returns `Ok`, and `AssetRef::evaluate` finalizes the asset
  through `finalize_status_with_version` unconditionally. The asset may briefly be `Cancelled` (if
  the service loop handled `Cancel` first) and then `Ready`, or go straight to `Ready`; either way
  `persist_with_status_tracking` skips the write because the flag is set. The client sees a `Ready`
  value that was never persisted; the test accepts either terminal status.
- **Wanted:** the command completed, so the asset ends `Ready` (or `Volatile` / `Expired` exactly as
  an uncancelled run would), its value is persisted, and `cancel()` returns `Ok(())`. Subscribers see
  one terminal status only. Had the command been `async` and still awaiting, or had it checked
  `context.is_cancelled()` and returned `Error::cancelled(..)`, the asset would end `Cancelled`.

## Scope and Acceptance Criteria
- **AC-1** Completed run wins over a late cancel
  WHEN a keyed asset is `Processing` a command that returns `Ok` after `cancel()` was requested
  THEN it ends in the status an uncancelled run would reach, and its value is persisted when storable
- **AC-2** Unfinished async run is cancelled
  WHEN `cancel()` is requested while an async command is suspended at an await point
  THEN the asset ends `Cancelled`, holds no value, records no error, and nothing is written for its key
- **AC-3** Cancel while waiting for dependencies
  WHEN `cancel()` is requested while the asset is `Dependencies`
  THEN the asset ends `Cancelled`, and its dependencies continue and finish normally
- **AC-4** Cancel before the job starts
  WHEN `cancel()` is requested while the asset is `Submitted`
  THEN the asset ends `Cancelled` and its command is never invoked
- **AC-5** Cooperative check
  WHEN a running command calls `context.is_cancelled()`
  THEN it returns `false` until `cancel()` is requested on that asset and `true` afterwards, from sync
  and async commands alike
- **AC-6** Cancellation error ends `Cancelled`
  WHEN a command returns an `ErrorType::Cancelled` error, whether or not its own asset's cancel was
  requested
  THEN the asset ends `Cancelled`, not `Error`, and nothing is persisted
- **AC-7** One terminal status per run
  WHEN cancellation races with completion
  THEN subscribers observe exactly one terminal status and one `JobFinished`
- **AC-8** Replacement is not overwritten
  WHEN `to_override` or `remove` replaces a cancelled asset whose sync command finishes later
  THEN the late result changes neither the replacement's status nor the store
- **AC-9** Cascade cancellation
  WHEN an asset waits for a dependency (`Dependencies`, or inside `context.evaluate`) and that
  dependency ends `Cancelled`
  THEN the waiting asset ends `Cancelled`, not `Error`, unless its command handles the cancellation
  error and returns `Ok`
- **AC-10** Root cause is named
  WHEN an asset is cancelled directly or by cascade, at any depth
  THEN the cancellation error read from it (`State::value_error`, its metadata) has `ErrorType::Cancelled`
  and its `query` field (and `key`, when keyed) names the asset whose `cancel()` was called
- **AC-11** Cascade is logged
  WHEN an asset ends `Cancelled` because of a dependency rather than its own `cancel()`
  THEN its log holds one warning naming the dependency it waited for and the root cause

Non-goals: preemptive interruption of synchronous code; cancelling an asset's dependencies downward
(they are shared with other dependents; cascade runs upward only); cancellation on the inline (`ImmediateAssetManager`/wasm) path beyond
what AC-5 gives (`WEB-CANCELLATION-INERT`); changing the `AssetRef::cancel` signature.

## Core Interactions
- **Assets:** `AssetRef::cancel`, the service loop's `Cancel` handling, `run_with_future` /
  `run_with_future_inline`, `evaluate`, `finish_run_with_result`, persistence after a cancel.
- **Commands:** `Context` gains `is_cancelled()` (and a `?`-friendly helper, Q3); commands may return
  `Error::cancelled`.
- **Errors:** `ErrorType::Cancelled` already exists (`liquers-core/src/error.rs`); its documented meaning
  widens from "value requested from a cancelled asset" to "evaluation was intentionally interrupted",
  and its existing `query`/`key` fields carry the root cause.
- **Dependencies:** `AssetManager::wait_for_dependency` cascades a dependency's `Cancelled` instead
  of failing the parent through `fail_due_to_dependency`.
- **Web/API:** `q/cancel` and `key/cancel` keep their contract; `aae92`/`aae38` tighten to one answer.
- **Bindings:** `liquers-py` exposes no `Context` cancel API today; no change planned.

## Crate Placement
`liquers-core` (assets, context, error) holds all behaviour. `liquers-axum` only tightens its tests.
No macro change: `context` is already injectable into sync and async commands.

## Documentation Intent
- Reference: extend `specs/reference/ASSETS.md` (cancellation path, Scenario 4, statuses) and the
  `liquers-core/src/assets.rs` module docs on cancellation.
- Guide: new `specs/guides/COMMAND-DESIGN-GUIDE.md` (name per request, see Q9): cooperative
  cancellation, what cancel guarantees, sync vs async commands, returning `Error::cancelled`.
- Other documents: link the new guide from `COMMAND_REGISTRATION_GUIDE.md` and `specs/README.md`.
- Documents to update: `specs/reference/ASSETS.md`, `specs/guides/COMMAND_REGISTRATION_GUIDE.md`.

## Open Questions
Q1-Q5 and Q10-Q12 are decided. The rest have a recommended answer, which Phase 2 assumes unless you
decide otherwise.

1. **Decided 2026-10-09 (recommended answer):** **Who decides the terminal status?** Today two writers race: the service loop's `Cancel` handler
   sets `Cancelled` and announces `JobFinished`, and `evaluate` later sets `Ready` regardless — the
   root cause of AC-1/AC-7. *Recommended:* the run owner is the only writer. While a run is in flight,
   `Cancel` only requests that the evaluation future be dropped; `run_with_future` sets `Cancelled`
   if that drop happened (or the command returned a cancellation error) and the normal ready status
   otherwise, through one guarded "finalize once" transition that refuses to leave a terminal status.
2. **Decided 2026-10-09 (recommended answer):** **Point of no return.** `tokio::select!` can drop the evaluation future *after* the value is
   installed, e.g. in the middle of `persist_with_status_tracking`, leaving a `Ready` asset half
   written. *Recommended:* once the command has returned `Ok`, finalization and persistence run
   outside the cancellable section; cancellation applies only up to that point.
3. **Decided 2026-10-09 (recommended answer):** **Spelling and helper.** The request says `is_canceled`; the code base uses *cancelled*
   throughout (`Status::Cancelled`, `ErrorType::Cancelled`, `AssetRef::is_cancelled`).
   *Recommended:* `Context::is_cancelled(&self) -> bool`, plus `Context::check_cancelled(&self) ->
   Result<(), Error>` returning `Error::cancelled(..)` for use with `?`.
4. **Decided 2026-10-09 (recommended answer):** **A synchronous check.** The flag lives inside the asset's async `RwLock`, which a sync command
   cannot await. *Recommended:* move it to a shared `Arc<AtomicBool>` read lock-free by `Context`.
5. **Decided 2026-10-09: cascade cancellation.** A cancellation is an intentional interruption, not a
   problem, so it stays distinct from `Error` all the way up. This implements the cascade rule
   `wp2-terminal-outcome` Phase 2 approved and the code never did (`wait_for_dependency` fails the
   parent today; test `dependency_failure_error_names_key` changes). The existing
   `ErrorType::Cancelled` is reused; any command returning it ends `Cancelled` (AC-6, AC-9). The
   error's `query` (and `key`) name the root cause and are copied unchanged at every level (AC-10);
   a cascaded asset logs one warning with the dependency and the root (AC-11). Consequence for
   command authors, for the guide: wrapping a cancellation in another error type (e.g.
   `Error::from_error(ErrorType::General, e)`) turns it back into an `Error`; propagate it as is.
6. **What "cancelled flag" means after a successful finish.** It now blocks store writes
   (`save_to_store`, `persist_with_status_tracking`, and the closed design `save-to-store-skip-outcome`
   records the skip as `NotPersisted`). *Recommended:* the flag means "cancel requested for this run";
   a run that finalizes ready ignores it and clears it, and the orphan-write guard becomes "this run
   lost the terminal transition" (which is what AC-8 needs).
7. **What `cancel()` reports.** It returns `Ok(())` whether or not it took effect, and gives up after
   5 s, so a sync command longer than that is still `Processing` when it returns. *Recommended:* keep
   the signature (axum already answers with `AssetInfo`), document both facts; a status-returning
   variant can be a later issue.
8. **Queued assets.** The service loop is spawned only when the run starts, so a `Cancel` sent to a
   `Submitted` asset waits in the channel, and `cancel()` may time out before the job is picked up.
   *Recommended:* `run` checks the flag before invoking the command (AC-4); Phase 2 verifies the
   `JobQueue` path.
9. **Guide file name.** Guides are named `UPPER_SNAKE_GUIDE.md` except
   `LANGUAGE-INTEGRATION_GUIDE.md`. *Recommended:* `COMMAND_DESIGN_GUIDE.md` for consistency, unless
   you want the requested `COMMAND-DESIGN-GUIDE.md` kept as is.
10. **Decided 2026-10-09 (recommended answer): where a `Cancelled` asset keeps its root cause.** `wp2-terminal-outcome` made `Cancelled` store
    no error, and `State::value_error` synthesizes a bare `Error::cancelled("Asset was cancelled")`.
    The root cause must survive persistence and several levels of cascade, so it needs a home.
    A `Cancelled` asset records its cancellation `Error` in its metadata, as `Error` assets do, and
    `value_error` returns that instead of synthesizing one. Status alone still decides error-ness, so
    `Cancelled` remains distinct from `Error`. Rejected: a dedicated `cancelled_by` metadata field.
11. **Decided 2026-10-09 (recommended answer): cascade across clients.** A dependency is shared, so one client's `cancel()` on it cancels every
    asset waiting on it, including other clients' requests. Today those fail with `Error` anyway;
    after this design they end `Cancelled`, a cache miss, so a retry re-evaluates. Accepted and
    documented; "cancel only if nobody else waits" is a separate feature, filed only if wanted.
12. **Decided 2026-10-09 (recommended answer): two causes at once.** An asset may be cancelled directly while its dependency is also being
    cancelled. Its own `cancel()` wins: the root cause is the asset itself, with no cascade warning.

## Design Dependencies
- overlaps `axum-assets-endpoints` (in implementation, PR #73; exclusion E3): its tests `aae92` and
  `aae38` accept either status and cite the source issue; this design tightens them after it lands.
- overlaps `WEB-CANCELLATION-INERT`: same contract, different cause (inline evaluation); not merged.
- revisits `wp2-terminal-outcome` (complete, E4): implements its approved but unimplemented
  dependency cascade-cancel rule (Q5) and replaces its "`Cancelled` stores no error" rule with
  "`Cancelled` stores its cancellation cause" (Q10).
- revisits `save-to-store-skip-outcome` (complete, frozen, E4): its "cancelled ⇒ `NotPersisted`" rule
  now applies only to runs that end `Cancelled`.

## Scope Changes
- 2026-10-09: widened by the maintainer from the source issue (which allowed either "end
  `Cancelled`" or "document best-effort") to: successful runs win and are stored, cooperative
  `Context::is_cancelled`, cancellation errors map to `Cancelled`, and a new command design guide.
  Complexity raised from `M` to `L`, hence the full form.
- 2026-10-09: maintainer chose cascade cancellation over failing the dependent (Q5): a dependency's
  cancellation cancels its waiting dependents, the cancellation error's `query`/`key` name the root
  cause, and a cascaded asset logs it as a warning. Adds AC-9 to AC-11 and Q10 to Q12; revises AC-6.
  Size stays `L`. Q1-Q4 and Q10-Q12 then decided as recommended.

## References
- `specs/issues/ASSET-CANCEL-DURING-PROCESSING-FINISHES-READY.md` (source)
- `specs/reference/ASSETS.md` — Scenario 4 and the cancellation path
- `liquers-core/src/assets.rs` `AssetRef::cancel`, `process_service_messages`, `run_with_future`,
  `evaluate`, `finalize_status_with_version`, `persist_with_status_tracking`, `save_to_store`,
  `AssetRef::fail_due_to_dependency`, `AssetManager::wait_for_dependency`
- `liquers-core/src/error.rs` `ErrorType::Cancelled`; `liquers-core/src/state.rs` `State::value_error`
