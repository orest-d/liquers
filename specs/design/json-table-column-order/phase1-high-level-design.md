# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — changes the observable column order of schema-less reads
  under a new maintainer decision, not documented behaviour (rule 2)
- **Leading issue:** None
- **Explanation:** Decided (Maintainer decision, 2026-10-06): "If the column order is not specified […], it is irrelevant —
  collect column names and sort them to have a stable column order." JSON objects have no
  specified key order, so a schema-less read sorts its column names. This replaces the issue's
  premise (that file order should be kept). Today the order is *not* fully sorted either, so code
  changes too.
- **Open questions:** None. `serde_json/preserve_order` is not needed. The maintainer noted that
  the `metadata_version` change it would cause is acceptable, but sorting makes the order
  independent of that feature, so it stays off.

## Problem (re-evaluated at HEAD)

The issue says columns come out alphabetical. That holds only per object. The schema-less readers
collect names as the *union in first-appearance order* across rows, where each row's keys arrive
sorted (`serde_json::Map` is a `BTreeMap`). In `read_inferred_objects`
(`liquers-records/src/formats/ndjson.rs`) and in the `columns`/`index` shape readers
(`formats/shapes.rs`), `[{"b":1},{"a":2}]` therefore reads as `b, a`, while
`[{"a":2},{"b":1}]` reads as `a, b`. The same columns get different orders depending on row
order. That is neither file order nor a stable order.

Filed during `record-streams` (complete, frozen). The issue's `design:` field pointed at that folder; it now points here, because a frozen design cannot own new work.

## Expected behaviour and acceptance

1. Every schema-less JSON read (NDJSON, `json`, `records`, `columns`, `index` shapes) yields columns
   sorted by name (byte order of the UTF-8 name, i.e. `str` `Ord`), whatever the row order.
2. An index/id column produced by the `columns`/`index` shapes keeps its position first (it is
   structural, not a data key). Phase 4 verifies how `index_field_name` places it today, and keeps
   that.
3. With a declared schema, the order is the schema's (unchanged).
4. The order is the same with and without `serde_json/preserve_order` (it no longer depends on
   map order).
5. The reference states the rule.

## Scope

Schema-less JSON readers. CSV/Markdown have a header, which is a specified order, so they are
unchanged.

## Design Dependencies

None.

## Documentation assessment

- Reference: `specs/reference/RECORD_STREAMS.md`, JSON shapes: "without a schema, columns are
  sorted by name".

## Consolidated Findings

- The fix is a `sort()` of the collected name vector in each schema-less path (three collection
  sites: `read_inferred_objects`, and the columns-name collection of the `columns` and `index`
  shapes). Row-key ordering (`ordered_row_keys`) is unrelated and stays.
- The existing test `inferred_bool_and_text_columns` (ndjson.rs) has a comment about keys coming
  back sorted. Update it to state the rule.
