# Phase 1: High-Level Design — asset-cancellation-outcome

## Purpose
Cancellation exists to save work, not to discard work that has already been done. Today an
evaluation that finishes after `cancel()` ends `Ready` but is silently not stored, and nothing lets a
command notice a cancel while it runs. This design makes the terminal status a single, deterministic
decision: a successful evaluation ends ready and is stored; an unfinished one ends `Cancelled`; and a
command can stop early by checking `Context::is_cancelled()` and returning a cancellation error.

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
  WHEN a command returns an `ErrorType::Cancelled` error after its asset's cancel was requested
  THEN the asset ends `Cancelled`, not `Error`, with no error recorded and nothing persisted
- **AC-7** One terminal status per run
  WHEN cancellation races with completion
  THEN subscribers observe exactly one terminal status and one `JobFinished`
- **AC-8** Replacement is not overwritten
  WHEN `to_override` or `remove` replaces a cancelled asset whose sync command finishes later
  THEN the late result changes neither the replacement's status nor the store

Non-goals: preemptive interruption of synchronous code; cancelling an asset's dependencies (they are
shared with other dependents); cancellation on the inline (`ImmediateAssetManager`/wasm) path beyond
what AC-5 gives (`WEB-CANCELLATION-INERT`); changing the `AssetRef::cancel` signature.

## Core Interactions
- **Assets:** `AssetRef::cancel`, the service loop's `Cancel` handling, `run_with_future` /
  `run_with_future_inline`, `evaluate`, `finish_run_with_result`, persistence after a cancel.
- **Commands:** `Context` gains `is_cancelled()` (and a `?`-friendly helper, Q3); commands may return
  `Error::cancelled`.
- **Errors:** `ErrorType::Cancelled` already exists (`liquers-core/src/error.rs`); its documented meaning
  widens from "value requested from a cancelled asset" to also "a command stopped on request".
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
Each has a recommended answer; Phase 2 assumes it unless you decide otherwise.

1. **Who decides the terminal status?** Today two writers race: the service loop's `Cancel` handler
   sets `Cancelled` and announces `JobFinished`, and `evaluate` later sets `Ready` regardless — the
   root cause of AC-1/AC-7. *Recommended:* the run owner is the only writer. While a run is in flight,
   `Cancel` only requests that the evaluation future be dropped; `run_with_future` sets `Cancelled`
   if that drop happened (or the command returned a cancellation error) and the normal ready status
   otherwise, through one guarded "finalize once" transition that refuses to leave a terminal status.
2. **Point of no return.** `tokio::select!` can drop the evaluation future *after* the value is
   installed, e.g. in the middle of `persist_with_status_tracking`, leaving a `Ready` asset half
   written. *Recommended:* once the command has returned `Ok`, finalization and persistence run
   outside the cancellable section; cancellation applies only up to that point.
3. **Spelling and helper.** The request says `is_canceled`; the code base uses *cancelled*
   throughout (`Status::Cancelled`, `ErrorType::Cancelled`, `AssetRef::is_cancelled`).
   *Recommended:* `Context::is_cancelled(&self) -> bool`, plus `Context::check_cancelled(&self) ->
   Result<(), Error>` returning `Error::cancelled(..)` for use with `?`.
4. **A synchronous check.** The flag lives inside the asset's async `RwLock`, which a sync command
   cannot await. *Recommended:* move it to a shared `Arc<AtomicBool>` read lock-free by `Context`.
5. **New error type or the existing one?** `ErrorType::Cancelled` exists, and `State::value_error`
   already synthesizes it when a *dependency* was cancelled. Mapping every `Cancelled` error to
   `Status::Cancelled` would turn "my dependency was cancelled by someone else" into a retryable
   cancel of this asset. A precedent conflicts here: `wp2-terminal-outcome` Phase 2 approved
   *cascade-cancelling* a parent whose dependency is cancelled, but the code fails the parent instead
   (`wait_for_dependency`, test `dependency_failure_error_names_key`). *Recommended:* reuse the
   existing type; a command's `Cancelled` error yields `Status::Cancelled` only when this asset's
   cancel was requested, and is an ordinary `Error` otherwise (AC-6), which matches today's code and
   retires the unimplemented cascade rule explicitly. A distinct variant is the alternative.
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

## Design Dependencies
- overlaps `axum-assets-endpoints` (in implementation, PR #73; exclusion E3): its tests `aae92` and
  `aae38` accept either status and cite the source issue; this design tightens them after it lands.
- overlaps `WEB-CANCELLATION-INERT`: same contract, different cause (inline evaluation); not merged.
- revisits `wp2-terminal-outcome` (complete, E4): keeps "`Cancelled` stores no error"; Q5 decides
  its unimplemented dependency cascade-cancel rule.
- revisits `save-to-store-skip-outcome` (complete, frozen, E4): its "cancelled ⇒ `NotPersisted`" rule
  now applies only to runs that end `Cancelled`.

## Scope Changes
- 2026-10-09: widened by the maintainer from the source issue (which allowed either "end
  `Cancelled`" or "document best-effort") to: successful runs win and are stored, cooperative
  `Context::is_cancelled`, cancellation errors map to `Cancelled`, and a new command design guide.
  Complexity raised from `M` to `L`, hence the full form.

## References
- `specs/issues/ASSET-CANCEL-DURING-PROCESSING-FINISHES-READY.md` (source)
- `specs/reference/ASSETS.md` — Scenario 4 and the cancellation path
- `liquers-core/src/assets.rs` `AssetRef::cancel`, `process_service_messages`, `run_with_future`,
  `evaluate`, `finalize_status_with_version`, `persist_with_status_tracking`, `save_to_store`
- `liquers-core/src/error.rs` `ErrorType::Cancelled`; `liquers-core/src/state.rs` `State::value_error`
