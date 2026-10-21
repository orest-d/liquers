---
id: RECORD-BATCH-FIELDS-CAN-BE-MUTATED-PAST-VALIDATION
kind: issue
title: RecordBatch validates its columns in new, but its public fields can be changed afterwards; InMemorySource's materialize exposes a placeholder chunk id
status: draft
priority: P3
complexity: M
area: [records]
design: record-batch-private-fields
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

## Triage 2026-10-21

Persists at HEAD 18732dd: `RecordBatch`'s six fields are `pub` (`liquers-records/src/batch.rs`);
`InMemorySource::materialize` places its one view with `Some(in_memory_chunk_id(0))`
(`sources.rs`). The leak is wider than filed: `RecordStreamExt::materialize` keeps the `chunk_id` of a
single streamed batch (`RecordBatch::concat`'s one-input rule), so a filter, `head` or
`select_columns` over a one-view `InMemorySource` also yields `in_memory_view-0`. Origin design
`record-streams` (complete); design: `specs/design/record-batch-private-fields/`.

## Problem Example

Outside the crate, `batch.len = 10;` or `batch.columns.pop();` compiles after `RecordBatch::new`
validated the batch; and `InMemorySource::new(vec![view]).materialize(resolver, 1000)` returns a batch
whose serialized `chunk_id` is `in_memory_view-0`. Should: the assignments do not compile, and the
batch's `chunk_id` is the view's own (`None` for a fresh batch).
