---
id: RECORD-BATCH-FIELDS-CAN-BE-MUTATED-PAST-VALIDATION
kind: issue
title: RecordBatch validates its columns in new, but its public fields can be changed afterwards; InMemorySource's materialize exposes a placeholder chunk id
status: draft
priority: P3
complexity: M
area: [records]
design: record-streams
created: 2026-09-27
github:
---
# `RecordBatch` validates its columns in `new`, but its public fields can be changed afterwards

## Problem

Since the implementation review, `RecordBatch::new` and deserialization run `Column::validate`, so
an inconsistent batch cannot be built through them. The fields stay `pub`, though
(`liquers-records/src/batch.rs`). Code that sets `len`, `rows`, `columns` or `schema` afterwards
can reintroduce every inconsistency validation rules out. `place_chunk` does this deliberately to
re-stamp `rows`, and the checked kernels then report an error rather than panic, but nothing
enforces the invariant.

A second, related leak: `InMemorySource::materialize` of a single view now yields a batch whose
`chunk_id` is the synthetic placeholder `in_memory_view-0` (`sources.rs`). Before, it was `None`.
That placeholder then appears in a serialized batch although it identifies nothing outside the
source.

## Expected behaviour

Make the fields private, with accessors and a crate-internal re-stamp for `place_chunk`, or document
the invariant as the caller's. Keep placeholder chunk ids out of materialized batches.

## Discovery

Found 2026-09-27 while fixing the implementation review's data-model findings.
