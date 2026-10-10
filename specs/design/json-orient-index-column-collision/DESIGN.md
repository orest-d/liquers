---
id: JSON-ORIENT-INDEX-COLUMN-COLLISION
kind: design
title: A schema-less indexed JSON read does not let a data column named index overwrite the index
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
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
  - THEN the data column keeps `index` and the index is read into `index_1` (or the first free name
    `index_<n>` if `index_1` is also a data column), placed first
- **AC-2** No silent loss for `columns` and `index`
  - WHEN a `columns` or `index` document with a data column `index` is read without a schema
  - THEN the outcome is the same as AC-1
- **AC-3** No clash, no change
  - WHEN no data column is named like the index column, or a schema is declared
  - THEN the read is unchanged

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix in one file, `liquers-records/src/formats/shapes.rs`, with
  private helpers only; no `pub` change, no new structure, no command or format-spelling change
- **Leading issue:** None
- **Decided (Maintainer decision, 2026-10-10): rename the index.** On a clash the index column takes
  the first free name of `index`, `index_1`, `index_2`, …; the data column keeps `index`. Everything
  is read, and the index column's name depends on the data. (Refusing the read, and dropping the
  index, were rejected.)
- **Open questions:** None.

### Design Dependencies

- `overlaps` (weak) `ORDERED-JSON-ORIENT-COLUMN-ORDER` (implemented): same file and the same
  row-object construction. The `order` list `from_json_split` passes to
  `ndjson::objects_to_batch_ordered` must use the renamed index name.

## Phase 2: Architecture

### Solution

`liquers-records/src/formats/shapes.rs`: a private helper

```rust
/// The schema-less index column name: `index`, or the first of `index_1`, `index_2`, … that is not
/// a data column name.
fn free_index_name<'a>(data_columns: impl Iterator<Item = &'a str>) -> String
```

Used only when `schema` is `ReadSchema::Infer` (match on `Infer` itself, not on
`declared_id_field(schema)` being `None`: a declared schema without an `Id` field and with a payload
field `index` reads correctly today, and must not change — AC-3). With a declared schema,
`index_field_name(declared)` is used as now. The data column names it checks:

- `split`: `columns`, once; the result also goes first in the `order` list;
- `columns`: the outer keys;
- `index`: the union of every row's inner keys, collected before the rows are built (a name used in
  any row counts).

Rejected: refusing the read (maintainer decision), and keeping today's silent loss.

### Changes

`liquers-records/src/formats/shapes.rs` only: the private `free_index_name` helper and its use in
`from_json_split`, `from_json_columns`, `from_json_index`. No signature, command or document format
spelling changes. Document: `specs/reference/RECORD_STREAMS.md` (one sentence).

### Risks

A schema-less read whose data has a column `index` now yields one more column (`index_1`) and keeps
the data column. Writing that view back with the same orient does not restore the original document,
because `index_1` is not the id field of an inferred schema; that is today's round-trip behaviour for
every schema-less read and is out of scope. Certainty: high.

## Phase 3: Examples and Tests

### Examples

The Problem Example, read as `split` without a schema, gives columns `index_1, index, a` with
`index_1 = 0, 1` and `index = 50, 60`.

### Tests

In the `liquers-records/src/formats/shapes.rs` tests:

- `split_without_schema_renames_the_index_on_clash`: the Problem Example reads as
  `index_1 = 0, 1`, `index = 50, 60`, `a = 1, 2`, column order `index_1, index, a` — AC-1
- `split_without_schema_skips_taken_index_names`: columns `index`, `index_1` → index in `index_2` — AC-1
- `columns_and_index_without_schema_rename_the_index_on_clash`: both orients, including an `index`
  document where only the second row has an `index` key — AC-2
- `split_without_clash_keeps_index_name`: no data column `index` → index column is `index` — AC-3
- `split_with_declared_schema_accepts_a_column_named_index`: declared `Id` `order_id`, payload field
  `index` — AC-3
- `split_with_declared_schema_without_id_reads_its_index_field`: declared schema with no `Id` and a
  payload field `index`; unchanged — AC-3

## Phase 4: Implementation Plan

### Steps

- [x] 1. Decide on the clash behaviour (Phase 1): rename, maintainer decision 2026-10-10.
- [ ] 2. `formats/shapes.rs`: add `free_index_name` and use it in `from_json_split` (name and
  `order`), `from_json_columns` and `from_json_index` when the schema is `ReadSchema::Infer` —
  `cargo check -p liquers-records --all-features`
- [ ] 3. Add the tests above — `cargo test -p liquers-records --all-features --lib --tests`
- [ ] 4. In `specs/reference/RECORD_STREAMS.md`, add one sentence on the renamed index to the
  JSON-orient paragraph, a History row and a `reviewed:` bump; close the issue —
  `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-records --all-features --lib --tests` and `cargo test -p liquers-records
--lib --tests`.
