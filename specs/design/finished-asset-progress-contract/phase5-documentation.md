# Phase 5: Documentation

## Summary

Implemented 2026-10-07 as Wave 1 step 9 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`.

- `AssetRef::finalize_primary_progress` now runs **after** the service loop has drained, in both
  `run_with_future` (after `psm.await`) and `run_with_future_inline` (after `futures::join!`). Its
  body applies the private `finished_progress(&ProgressEntry) -> Option<ProgressEntry>`: done stays,
  started-but-unfinished becomes `done(last message)`, never started stays absent; secondary
  progress is cleared. It persists the metadata only when progress had started.
- **Addition:** a private `AssetData::progress_finalized` flag. The service loop's post-finish
  policy dropped progress updates once the status was finished, and `evaluate` sets `Ready`
  before `JobFinishing`, so a command's own last update could still be lost by scheduling. Until
  finalization, in-run progress updates are now applied even after the status flip; after it,
  late ones are dropped as before (`test_late_progress_messages_are_ignored_after_finish` still
  passes).
- **Addition:** finalization writes nothing when no progress started, so a keyed value that was
  never persisted (non-serializable) gains no metadata-only entry
  (`test_to_override_skips_store_write_when_nonserializable` caught this).
- Tests: `finished_progress_follows_the_contract`, `finished_run_progress_contract_{native,inline}`
  (E1–E4), `finished_progress_is_deterministic_{native,inline}` (×100, also run with
  `--test-threads=1`). `interpreter::tests::test_evaluate_immediately` asserts `is_done()` again,
  and `test_context_apply`, which asserted the old clear-everything result, asserts the command's
  own done entry.

## Conformance and deviations

Conforms to the decision and Phases 1–4, with the two additions above, both needed to meet
acceptance criteria 1–2 and to keep the existing "no metadata-only entry" guarantees.

## Documentation

`reference/ASSETS.md`: new §Progress after completion (table, "status is authoritative") and a
sentence on in-run progress in §Post-finish messages.

## New issues

None. Downstream clients (`liquers-lib` egui `display_progress`) already render a done entry as
"Done".
