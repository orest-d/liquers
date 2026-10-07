# Phase 5: Documentation - Schema-less JSON reads use a stable, sorted column order

**Status: executed 2026-10-07**, after implementation (Wave 4 step 26 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`). Awaiting approval.

## Completion Preconditions

- [x] Implementation is finished and validated (Phase 4)
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (none yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR

## Documentation Plan

### New Reference Documents

None.

### New Guide Documents

None.

### Existing Documents to Update

| Document | Change |
|---|---|
| `reference/RECORD_STREAMS.md` | JSON shapes: without a schema, columns are sorted by name; `index` sorts among them; declared schema and `table` keep their order; link to the `split`/`values` issue. History row |

### Candidates Considered and Discarded

`guides/RECORD_STREAM_GUIDE.md`, which does not discuss column order.

### Issues to Close

`SCHEMA-LESS-JSON-READS-SORT-COLUMNS-ALPHABETICALLY`.

## Implementation Summary

- `liquers-records/src/formats/ndjson.rs`, `read_inferred_objects`: `names.sort()` after the
  union, with the rule stated in a comment. **Deviation from Phase 2:** this is the one site, not
  three. The `columns` and `index` shapes, and every other orient, build row objects and go
  through `objects_to_batch` → `read_inferred_objects`, so one sort covers all of them. Sorting in
  `shapes.rs` as well would have been dead code.
- The `index` column keeps the position it had before this change: sorted among the names
  (`a, index, z`), because each row's `Map` was already a sorted `BTreeMap`.
- Tests:
  - `schema_less_json_columns_are_sorted` (T1, T3)
  - `schema_less_json_column_order_ignores_row_order` (T2)
  - `declared_json_columns_keep_the_schema_order` (T6)
  - `columns_shape_sorts_columns` (T4)
  - `index_shape_sorts_columns` (T5)

  The comment in `inferred_bool_and_text_columns` now states the rule.

## Documentation Delivered

As planned above.

## Issues Filed

- [`SCHEMA-LESS-ORDERED-JSON-ORIENTS-LOSE-COLUMN-ORDER`](../../issues/SCHEMA-LESS-ORDERED-JSON-ORIENTS-LOSE-COLUMN-ORDER.md)
  (P3). `split` and `values` documents specify a column order, and a schema-less read sorts it
  away. This already happened before the change, through `BTreeMap`. With eleven or more columns,
  `values` reads as `c0, c1, c10, c2, …`.

## Important Learning

`serde_json` has no `preserve_order` anywhere in the dependency graph (checked with
`cargo tree -e features -i serde_json`). Every per-row map was therefore already sorted, and the
only order that varied was the union across rows.

## Conformance and Remaining Work

Conforms to Phase 1, except for the site count noted above. The ordered orients are tracked by
the filed issue.

## Validation

- `cargo test -p liquers-records --all-features --lib`: 400 passed
- Full lib loop and build matrix run with the wave
