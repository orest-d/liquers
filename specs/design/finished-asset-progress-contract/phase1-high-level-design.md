# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The contract is decided (Maintainer decision, 2026-10-06). Progress exists to draw a progress bar, and a
  bar that will never move again is confusing. So once an asset is finished (any terminal status:
  `Ready`, `Error`, `Cancelled`, …), progress that was started is shown as done. Progress that
  never started stays absent. Finalizing after the service loop has drained makes the result
  deterministic.
- **Open questions:** None

## Decision

> Final done should only be reported if progress reporting has been started. If an asset is
> finished (ready, error, cancel…) then progress can be considered done. […] The final done may be
> reported if it simplifies the flow.

So:

| At finish | Primary progress afterwards |
|---|---|
| The command reported progress, and the last entry is already `done` | that entry, unchanged (the command's own final message is kept) |
| The command reported progress, and the last entry is not done (a tick, 3/10, …) | a `done` entry carrying the last entry's message |
| The command never reported progress | none (`ProgressEntry::off()`), and no bar is drawn |
| Cancelled | `done("Cancelled")`, which the cancel handler already writes (allowed, because it simplifies the flow) |

Secondary progress is cleared at finish. It is per-step detail with no meaning after the run.

## Problem

`AssetRef::finalize_primary_progress` (called by `run_with_future` / `run_inline_with_future` right
after the evaluation future returns) clears all progress. A command's own
`context.progress(ProgressEntry::done(..))` is sent to the service-message loop, which runs
concurrently, so whether the `done` entry or the clear wins depends on scheduling. A late update
after the status becomes finished is dropped by the loop's post-finish policy. So
`primary_progress()` of a finished asset is timing-dependent
(`interpreter::tests::test_evaluate_immediately` used to assert `is_done()` and broke when
`evaluate-path-consolidation` changed the timing).

## Expected behaviour and acceptance

1. The progress of a finished asset follows the table above and never depends on scheduling.
2. The same evaluation repeated 100 times gives the same `primary_progress()`, on both the native
   (spawned) and the inline (wasm/immediate) harness.
3. A finished asset never shows an unfinished bar: `primary_progress()` is either `is_done()` or
   `is_off()`.
4. The status remains the authoritative "is it finished" signal, and the reference says so.

## Scope and non-goals

Primary and secondary progress at the end of a run. A progress API redesign is not in scope.

## Affected systems

The `liquers-core` asset harness, and UI clients that draw progress (`liquers-lib` UI, the axum
websocket notifications).

## Design Dependencies

- `evaluate-path-consolidation` — **overlaps**. It exposed the race. This design changes only the
  harness tail.

## Documentation assessment

- Reference: `specs/reference/ASSETS.md` gains a short "Progress after completion" paragraph with
  the table.
- No guide.

## Consolidated Findings

- **Determinism:** call finalization after the service loop has finished (after `psm.await`
  natively, after `futures::join!` inline). The loop is FIFO and ends at `JobFinishing`, which is
  sent after the evaluation future, so every progress update the command sent is applied first.
  The loop also ends early on `Cancel` / `ErrorOccurred`, and finalization after it is still
  deterministic.
- **"Started" is observable without new state:** the metadata's progress list is non-empty
  exactly when some progress was applied (`MetadataRecord::primary_progress` returns `off()` for an
  empty list).
- Finalization writes the metadata to the store (`save_metadata_to_store`), as the loop's own
  progress updates do. Otherwise a stored keyed asset keeps an unfinished entry.
