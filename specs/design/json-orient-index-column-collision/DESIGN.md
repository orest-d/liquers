---
id: JSON-ORIENT-INDEX-COLUMN-COLLISION
kind: design
title: A schema-less indexed JSON read does not let a data column named index overwrite the index
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [records]
issues: [SCHEMA-LESS-JSON-ORIENT-INDEX-COLUMN-OVERWRITTEN]
created: 2026-10-08
---
# A schema-less indexed JSON read does not let a data column named `index` overwrite the index

Filed through `liquers-project` triage (case 3b) as a spin-off of
`design/ordered-json-orient-column-order/`. Phases 1-4 are written but not approved, and nothing is
implemented.

## Phase 1: High-Level Design

### Purpose

The `split`, `columns` and `index` orients read their index into a column named `index` when there
is no schema. A data column with that same name now replaces the index silently. A read should never
drop data without saying so.

### Problem Example

```json
{"columns": ["index", "a"], "index": [0, 1], "data": [[50, 1], [60, 2]]}
```

Read as `split` without a schema, this gives `index, a` with `index` = `50, 60`. The index `0, 1` is
lost.

### Scope and Acceptance Criteria

- **AC-1** No silent loss for `split`
  - WHEN a `split` document whose `columns` contains `index` is read without a schema
  - THEN the result keeps both the index and the data column, or the read fails with an error that names the clash
- **AC-2** No silent loss for `columns` and `index`
  - WHEN a `columns` or `index` document with a data column `index` is read without a schema
  - THEN the outcome is the same as AC-1
- **AC-3** No clash, no change
  - WHEN no data column is named like the index column, or a schema is declared
  - THEN the read is unchanged

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. The fix changes what a document reads as, and the outcome is
  a behaviour choice that a maintainer has to make.
- **Leading issue:** None
- **Open design question (blocking): what does a clash do?**
  1. *Refuse* (recommended): return `Error::general_error` that names the orient and the column.
     This is the simplest option, never loses data, and has no naming convention to document. The
     cost: a pandas frame with a column named `index` needs a declared schema to read.
  2. *Rename the index*: use the first free name of `index`, `index_1`, …, for the index column.
     This reads everything, but the index column's name then depends on the data.
  3. *Prefer the data column and drop the index*: this is the current behaviour, made explicit.
     Rejected, because it is the loss this design exists to stop.
- **Open questions:** the one above.

### Design Dependencies

- `overlaps` (weak) `ORDERED-JSON-ORIENT-COLUMN-ORDER`: same file and the same row-object
  construction, but a different behaviour. Under option 2, the order that design passes for `split`
  must use the renamed index name.

## Phase 2: Architecture

### Solution

This assumes option 1 (refuse). `liquers-records/src/formats/shapes.rs`: in `from_json_split`,
`from_json_columns` and `from_json_index`, when `declared_id_field(schema)` is `None` and a data
column name equals `index_field_name(None)`, return an error before any row is built. For `split`
the check runs once over `columns`. For `columns` it runs over the outer keys. For `index` it runs
over each row's inner keys. The new checks are private and no signature changes.

Option 2 would add a private helper that picks the free name and passes it in place of
`index_field_name(None)` (and into the `split` order list).

### Risks

Under option 1, a document that read before, with its index lost, now fails. That is the intended
change. Certainty: high.

## Phase 3: Examples and Tests

In the `liquers-records/src/formats/shapes.rs` tests:

- `split_without_schema_refuses_a_data_column_named_index`: AC-1
- `columns_and_index_without_schema_refuse_a_data_column_named_index`: AC-2
- `split_with_declared_schema_accepts_a_column_named_index`: AC-3, with a declared `Id` named
  `order_id` and a payload field `index`

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Decide on the clash behaviour (Phase 1). This is a maintainer decision.
- [ ] 2. `formats/shapes.rs`: add the clash check (or the rename) to the three readers —
  `cargo check -p liquers-records --all-features`
- [ ] 3. Add the tests above — `cargo test -p liquers-records --all-features --lib --tests`
- [ ] 4. In `specs/reference/RECORD_STREAMS.md`, add one sentence on the clash to the JSON-orient
  paragraph, a History row and a `reviewed:` bump; close the issue —
  `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-records --all-features --lib --tests` and `cargo test -p liquers-records
--lib --tests`.
