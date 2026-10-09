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
  THEN the asset ends `Cancelled` (not `Error`), holds no value, and no value is written for its key
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
  WHEN `set`, `set_state`, `remove` or `to_override` replaces an asset whose sync command finishes later
  THEN the late result changes neither the replacement's status nor the store
- **AC-9** Cascade cancellation
  WHEN an asset waits for a dependency (`Dependencies`, or inside `context.evaluate`) and that
  dependency ends `Cancelled`
  THEN the waiting asset ends `Cancelled`, not `Error`, unless its command handles the cancellation
  error and returns `Ok`
- **AC-10** Root cause is named
  WHEN an asset is cancelled directly or by cascade, at any depth
  THEN the cancellation error read from it (`State::value_error`, its metadata) has `ErrorType::Cancelled`
  and its `query` field names the asset whose `cancel()` was called (its key, when keyed)
- **AC-11** Cascade is logged
  WHEN an asset ends `Cancelled` because of a dependency rather than its own `cancel()`
  THEN its log holds one warning naming the dependency it waited for and the root cause
- **AC-12** Cancelled stays visible until requested again 
  WHEN asset info is read for a keyed asset that ended `Cancelled`
  THEN it reports `Cancelled` with the root cause; a following `get` re-evaluates it

Non-goals: preemptive interruption of synchronous code; cancelling an asset's dependencies downward
(they are shared with other dependents; cascade runs upward only); cancellation on the inline
(`ImmediateAssetManager`/wasm) path beyond what AC-5 gives (`WEB-CANCELLATION-INERT`); changing the `AssetRef::cancel` signature.

## Core Interactions
- **Assets:** `AssetRef::cancel`, `run_with_future` / `run_with_future_inline`, `evaluate`,
  `finish_run_with_result`, `fail_asset`, the service loop's `Cancel` handling, persistence after a
  cancel, and the replacement paths (`set`, `set_state`, `remove`, `to_override`).
- **Dependencies:** `wait_for_dependency` cascades a dependency's `Cancelled` instead of failing the
  parent.
- **Commands:** `Context` gains `is_cancelled()` and `check_cancelled()`.
- **Errors:** the existing `ErrorType::Cancelled` now means "evaluation intentionally interrupted";
  its `query` field carries the root cause.
- **Web/API:** `q/cancel` and `key/cancel` keep their contract; tests `aae92`/`aae38` tighten.
- **Bindings:** none (`liquers-py` and `liquers-web` expose no cancel check to commands).

## Crate Placement
`liquers-core` (assets, context, metadata, state) holds all behaviour; `liquers-axum` only tightens
tests. No macro change: `context` is already injectable into sync and async commands.

## Documentation Intent
- Reference: update `ASSETS.md`, `ASSET_SET_OPERATION.md`, `DEPENDENCIES_STATUS.md`,
  `ASSET_LIFECYCLE.md`, `api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`, `WEB_API_SPECIFICATION.md`.
- Guide: new `specs/guides/COMMAND_DESIGN_GUIDE.md` (cooperative cancellation, what a cancel
  guarantees, sync vs async commands, propagating `Error::cancelled`); link it from
  `COMMAND_REGISTRATION_GUIDE.md`; update `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` and `WEB_API_GUIDE.md`.
- Other documents: `specs/README.md` link to the new guide.

## Open Questions
None. Decided with the maintainer on 2026-10-09 (each as recommended unless stated):

- **D1 Single status writer.** The run owner finalizes; `Cancel` only asks the run to drop its
  unfinished evaluation. Finalization happens once, from an in-flight status only.
- **D2 Point of no return.** Once the command returns `Ok`, finalizing and storing are not
  cancellable.
- **D3 API.** `Context::is_cancelled()` and `Context::check_cancelled()?` (spelling *cancelled*).
- **D4 Lock-free flag.** Shared with `Context`, readable from sync code.
- **D5 Cascade cancellation** (maintainer's choice over failing the dependent). Any command returning
  `ErrorType::Cancelled` ends `Cancelled`; the error's `query` names the root cause, copied unchanged
  at every level; a cascaded asset logs one warning. Implements the cascade rule `wp2-terminal-outcome`
  approved and the code never did. Wrapping a cancellation in another error type turns it into a failure.
- **D6 Flag after success.** The flag means "cancel requested"; a successful run ignores and clears
  it and is stored. A replaced asset's late result is discarded instead (AC-8).
- **D7 `cancel()` reports.** Signature kept; it returns `Ok(())` whether or not it took effect and
  waits at most 5 s. Documented.
- **D8 Queued assets.** A cancel of a `Submitted` asset ends it `Cancelled` at once; the job is never run.
- **D9 Guide name.** `COMMAND_DESIGN_GUIDE.md`, matching the other guides.
- **D10 Cause stored.** A `Cancelled` asset records its cancellation error in metadata (`error_data`,
  `is_error` stays false); `value_error` returns it.
- **D11 Across clients.** A shared dependency's cancel cascades to every waiter; documented.
- **D12 Two causes.** An asset's own cancel wins over a cascade, without a cascade warning.
- **D13 Visibility** (maintainer's question). Asset info of a cancelled keyed asset reports
  `Cancelled` with its cause, not `Recipe`, until the next `get` or `remove` (AC-12).

## Design Dependencies
- overlaps `error-with-key-field` (in review, T2 shared contract on error `query`/`key`; not merged,
  E2 size): this design uses only `query` for the root cause; `Error::with_key` currently writes
  `query` (`ERROR-WITH-KEY-SETS-QUERY-FIELD`), so a structured key is that design's to add.
- overlaps `external-manager-replacement-surface` (in implementation, E3): replacement now discards a
  late result (D6) through a new `pub` `AssetRef::cancel_for_replacement`, which an external manager
  calls in place of `cancel()`, beside `notify_removed`.
- overlaps `axum-assets-endpoints` (in implementation, E3): tests `aae92`/`aae38` tighten here.
- overlaps `WEB-CANCELLATION-INERT`: same contract, different cause; not merged.
- revisits `wp2-terminal-outcome` and `save-to-store-skip-outcome` (complete, E4): see D5, D6, D10.

## Scope Changes
- 2026-10-09: widened by the maintainer from the source issue to D1-D9 and the new guide; `M` → `L`.
- 2026-10-09: cascade cancellation chosen (D5, D10-D12); AC-9 to AC-11 added, AC-6 revised.
- 2026-10-09: D13 asked by the maintainer; AC-12 added. At the Phase 2 start AC-2 and AC-10 were
  aligned with D10 and with `Error::with_key`'s behaviour, and AC-8 names every replacement path.

## References
- `specs/issues/ASSET-CANCEL-DURING-PROCESSING-FINISHES-READY.md` (source)
- `specs/reference/ASSETS.md` — Scenario 4 and the cancellation path
- `liquers-core/src/assets.rs`, `context.rs`, `error.rs`, `state.rs`, `metadata.rs`
