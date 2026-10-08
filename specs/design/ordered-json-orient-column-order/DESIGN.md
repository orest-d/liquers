---
id: ORDERED-JSON-ORIENT-COLUMN-ORDER
kind: design
title: Schema-less split and values JSON reads keep the document's column order
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [records]
issues: [SCHEMA-LESS-ORDERED-JSON-ORIENTS-LOSE-COLUMN-ORDER]
created: 2026-10-08
---
# Schema-less split and values JSON reads keep the document's column order

Produced under [`guides/autonomous_bulk_design.md`](../../guides/autonomous_bulk_design.md) by the
2026-10-08 backlog compaction: Phases 1-4, reviewed without phase approval. Not an approval and not
an implementation.

## Phase 1: High-Level Design

### Purpose

The 2026-10-06 maintainer decision sorts the columns of JSON whose column order is *not* specified
(`json-table-column-order`). The `split` and `values` orients do specify one; a schema-less read of
them should keep it.

### Problem Example

A schema-less read (`ReadSchema::Infer`) of the `split` document

```json
{"columns": ["z", "a"], "index": [0, 1], "data": [[1, "x"], [2, "y"]]}
```

today yields columns `a, index, z` (sorted). It should yield `index, z, a`. A `values` document
with eleven columns today yields `c0, c1, c10, c2, …`; it should yield `c0, c1, …, c10`.

### Scope and Acceptance Criteria

- **AC-1** `split` keeps `columns` order
  - WHEN a `split` document is read without a schema
  - THEN the columns are the index column first, then `columns` in document order
- **AC-2** `values` keeps positional order
  - WHEN a `values` document is read without a schema
  - THEN the columns are `c0 … c<n-1>` in position order, including for ten or more columns
- **AC-3** Unordered shapes still sort
  - WHEN `records`, `list`, NDJSON or the `json` format is read without a schema
  - THEN the columns are sorted by name, as the 2026-10-06 decision requires
- **AC-4** Declared schemas are unaffected
  - WHEN any orient is read with `ReadSchema::Declared`
  - THEN the schema's field order is used, as today

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix in `liquers-records/src/formats/{ndjson,shapes}.rs`;
  `objects_to_batch` is `pub(super)`, so no public signature, format or command changes.
- **Leading issue:** None
- **Explanation:** The decision text limits sorting to JSON "if the column order is not
  specified"; these two orients specify it, so the expected answer follows from the decision.
  Placing the `split` index column first is an implementation detail: the writer
  (`to_json_split`) takes the index from the id column, and `table` puts declared fields in order.
- **Open questions:** None

### Design Dependencies

- `overlaps` `JSON-TABLE-COLUMN-ORDER` (same change site, `read_inferred_objects`; that design is
  implemented and is not reopened).

## Phase 2: Architecture

### Solution

Give the schema-less reader an optional explicit column order. `objects_to_batch` keeps its
signature and calls a new `pub(super) fn objects_to_batch_ordered(items, schema, order:
Option<&[String]>)`; `read_inferred_objects` gains the `order` parameter and, when it is `Some`,
uses those names (followed by any other key seen, sorted, which cannot happen for these two
orients but keeps the function total) instead of sorting. `from_json_split` passes
`[index_name, columns…]` and `from_json_values` passes the `c<i>` names it already builds; both
only when `schema` is `ReadSchema::Infer`.

Rejected: enabling `serde_json/preserve_order` (a new feature flag, and the maintainer decision
explicitly keeps it off); building a `RecordSchema` in the orient readers (duplicates inference).

### Changes

- `liquers-records/src/formats/ndjson.rs`: `read_inferred_objects(objects, order:
  Option<&[String]>)`; new `pub(super) fn objects_to_batch_ordered`; `objects_to_batch` delegates
  with `None`.
- `liquers-records/src/formats/shapes.rs`: `from_json_split`, `from_json_values` call
  `objects_to_batch_ordered`.
- `specs/reference/RECORD_STREAMS.md`: one sentence in the JSON-orient section that `split` and
  `values` keep their order; `## History` row and `reviewed:` bump.

### Risks

Existing tests that expect sorted `split`/`values` columns change; they encode the defect. Certainty:
high.

## Phase 3: Examples and Tests

### Examples

The Problem Example is the primary example; the eleven-column `values` document is the secondary.

### Tests

In `liquers-records/src/formats/shapes.rs` tests:

- `split_without_schema_keeps_columns_order` — AC-1
- `values_without_schema_keeps_positional_order` — eleven columns; AC-2
- `records_and_list_without_schema_still_sort` — AC-3
- `split_with_declared_schema_uses_schema_order` — AC-4

Command: `cargo test -p liquers-records --all-features --lib shapes`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `formats/ndjson.rs` — `order` parameter and `objects_to_batch_ordered` — `cargo check -p
  liquers-records --all-features`
- [ ] 2. `formats/shapes.rs` — pass the order from `from_json_split` and `from_json_values` —
  `cargo check -p liquers-records --all-features`
- [ ] 3. Tests above; update any existing test that asserted the sorted order — `cargo test -p
  liquers-records --all-features --lib --tests`
- [ ] 4. `RECORD_STREAMS.md` sentence, History row; issue resolution and `status: closed`;
  `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-records --all-features --lib --tests`, `cargo test -p liquers-records --lib
--tests`, and `cargo test -p liquers-lib --lib --tests` (records consumers). Rollback: revert the
commit.
