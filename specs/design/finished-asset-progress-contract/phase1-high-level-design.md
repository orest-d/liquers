# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question — what a finished run's progress is.** Either it
  carries no progress (`ProgressEntry::off()`), or it carries a terminal `done` entry.
- **Explanation:** Both contracts can be made deterministic with the same mechanism: finalize
  progress after the service-message loop has drained. Only the value written at the end differs,
  so Phases 3–4 are complete around the recommended answer.
- **Open questions:**
  1. **Proposed resolution — terminal entry:** a run that ends `Ready`/`Source`/`Override`/
     `Volatile`/`Expired` (data-bearing) carries a `done` entry. If the command's last primary
     progress was already `done`, its message is kept; otherwise `ProgressEntry::done("Finished")`
     is written. `Error` clears progress. `Cancelled` keeps the `done("Cancelled")` the cancel
     handler already writes. Recommended because cancellation already uses a terminal `done`
     entry, and a progress-bar client then never sees a finished asset at 0%. Alternative: always
     clear. That is cheaper to explain ("ask the status"), but discards a command's own final
     message.

## Problem

`AssetRef::finalize_primary_progress` (called by `run_with_future` and `run_inline_with_future`
right after the evaluation future returns) clears all progress. A command's own
`context.progress(ProgressEntry::done(..))` is sent to the service-message loop, which runs
concurrently (`tokio::spawn` natively, `futures::join!` inline). Whether the `done` entry is applied
before or after the clear depends on scheduling. A late update that arrives after the status
becomes finished is dropped by the loop's post-finish policy. So `primary_progress().is_done()`
on a finished asset is timing-dependent. `interpreter::tests::test_evaluate_immediately` used to
assert it and broke when `evaluate-path-consolidation` changed the timing.

## Expected behaviour and acceptance criteria

1. The progress of a finished asset is a function of its final status and the messages the command
   sent. It never depends on scheduling.
2. Running the same evaluation 100 times yields the same `primary_progress()` every time (stress
   test, both harnesses).
3. The contract is stated in `specs/reference/ASSETS.md`.
4. A status, not progress, remains the authoritative "is it finished" signal. The reference says
   so.

## Scope and non-goals

- Primary progress only. Secondary progress is cleared together today and stays cleared (it is
  per-step detail).
- Not in scope: persisting progress for stored assets differently, or a progress API redesign.

## Affected systems

`liquers-core` asset harness (`run_with_future`, `run_inline_with_future`, the service loop),
UI clients that render progress (`liquers-lib` UI, the axum websocket notifications).

## Design Dependencies

- `evaluate-path-consolidation` — **overlaps**. It exposed the race. This design changes only the
  harness tail, and does not touch the consolidated body.

## Documentation assessment

- Reference: extend `specs/reference/ASSETS.md` with a short "Progress after completion"
  paragraph.
- Guide: none.
- Other documents: none.
- Updates: the source issue.

## Consolidated Findings

- The determinism fix does not depend on the decision. Move finalization to after the service
  loop has finished: after `psm.await` in `run_with_future`, and after `futures::join!` in
  `run_inline_with_future`. The loop processes messages FIFO and ends at `JobFinishing`, which is
  sent after the evaluation future. Every progress update the command sent is therefore applied
  first.
- The service loop can also end early, on `Cancel` or `ErrorOccurred`. Finalization then still
  runs after it, which is still deterministic.
- Finalization must write the metadata to the store, as the loop's own progress updates do
  (`save_metadata_to_store`). Otherwise a stored asset keeps a stale entry. This is a new
  obligation, because the current clear is in-memory only and is then overwritten or persisted by
  `finish_run_with_result`. Phase 4 checks which.
- Decision needed: the terminal-entry vs. clear contract (above).
