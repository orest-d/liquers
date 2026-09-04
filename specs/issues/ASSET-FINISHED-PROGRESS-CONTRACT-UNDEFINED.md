---
id: ASSET-FINISHED-PROGRESS-CONTRACT-UNDEFINED
kind: issue
title: What progress a finished asset reports is undefined, and was asserted by a race
status: draft
priority: P3
complexity: S
area: [core/assets]
design:
created: 2026-09-03
github:
---

## Problem

Two mechanisms disagree about what `primary_progress()` should report once an evaluation has
finished, and nothing decides between them.

- A command reports completion with `context.progress(ProgressEntry::done(...))`, which sends
  `AssetServiceMessage::UpdatePrimaryProgress` to the asset's service loop.
- The run harness ends with `AssetRef::finalize_primary_progress`
  (`liquers-core/src/assets.rs:1520`), which calls `metadata.remove_progress()` — it **clears**
  progress.

Both run at the end of an evaluation, and their order is not fixed: the service loop is a
concurrent task (`tokio::spawn` on native, `futures::join!` inline), so whether the `done` entry is
applied before or after the clear depends on scheduling.

`MetadataRecord::primary_progress` returns `ProgressEntry::off()` when the list is empty
(`metadata.rs:1331`), and `ProgressEntry::off().is_done()` is `false` (`:628`, `is_done` requires
`total > 0`). So the two outcomes are observably different: `is_done() == true` if the update won
the race, `false` if the clear did.

`interpreter::tests::test_evaluate_immediately` asserted
`state.metadata.primary_progress().is_done()` and passed for as long as the update happened to win.
Consolidating the evaluation body (`specs/design/evaluate-path-consolidation/`, Step 4) changed the
work done before the harness returns, the clear started winning, and the test failed — on an
assertion nothing in the design guarantees.

## Impact

Low, and diagnostic rather than functional: no value or status is wrong. But a client asking "is
this finished?" has two plausible sources — the status, which is authoritative, and the progress,
which is not — and the second answers differently depending on timing. Any UI that renders a
progress bar from `primary_progress()` will occasionally show a completed asset as having no
progress at all, or a stale in-flight entry, with no way to tell which.

The test has been changed to assert completion through `Status::is_finished()`, which is
deterministic. That removes the flaky assertion but does not decide the contract.

## Expected behaviour

Decide, and state it in the reference:

1. **A finished run carries no progress** — `finalize_primary_progress`'s current intent. Then the
   clear must happen *after* the service loop has drained, so it is deterministic rather than a
   race, and clients are told to use the status.
2. **A finished run carries a terminal `done` entry** — friendlier to a progress-rendering client.
   Then the harness must synthesize that entry rather than clearing, and a command's own `done`
   report must not be discarded.

Either is defensible; the current state is neither, and is timing-dependent.

## Discovery

Found on 2026-09-03 while implementing Step 4 of `evaluate-path-consolidation`. The consolidated
evaluation body does more work before returning to the harness (status finalization, notification,
dependency-manager registration), which changed the scheduling enough to flip the outcome. The
assertion had been passing for reasons no document states.
