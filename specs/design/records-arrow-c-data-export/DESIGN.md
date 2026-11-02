---
id: RECORDS-ARROW-C-DATA-EXPORT
kind: design
title: Record batches cross to Arrow consumers through the C Data Interface, without copying
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [records, py]
issues: [RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT]
affects_docs: [specs/reference/RECORD_STREAMS.md, specs/reference/PYTHON_INTEGRATION.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md]
created: 2026-10-27
---
# Record batches cross to Arrow consumers through the C Data Interface

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written and
reviewed without phase approval. Nothing is implemented. This is the unbuilt remainder of the
complete design `record-streams` (§"Arrow interoperability"), which is frozen and not extended.

Two PRs, one contract: Part A, the export and import in `liquers-records` behind the feature
`cdata`. Part B, the `liquers-py` transport to `pyarrow`, after `py-record-values-to-python`.

**Note, 2026-11-01:** `py-record-values-to-python` is superseded by `liquers-py-records`, where a
view reaches Python as an `lq.RecordView` wrapper and pyarrow is reached through `.to_arrow()`.
Part B, its AC-7 and AC-8 now target `.to_arrow()` instead of `ext_to_py`, and its question 3 /
AC-1 must carry origins (`liquers.origins`, `liquers-py-records` question 8). The phases below
still name the old design and are revised when this design is resumed.
Leading decision: whether `liquers-records` may hold this `unsafe` module (Phase 1, question 1).

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
