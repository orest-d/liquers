# Phase 1: High-Level Design — UUID, time-of-day and duration record field types

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds `FieldType`, `Column` and `FieldValue` variants in
  `liquers-records`, changes the schema's serialized vocabulary, the browser column descriptor and
  three crates (rules 1, 3, 4, 6).
- **Leading issue:** **Open design - which types become field types** (question 1).
- **Explanation:** all three proposed types are fixed-width (16 bytes, or one `i64` of
  microseconds) and fit the columnar model and every writer at HEAD as `Timestamp` and `Date` do;
  they are unit variants, so the schema's serde form needs no new reader. A complete plan is
  possible under the recommended answers.
- **Open questions:**
  1. **Open design - scope:** `Uuid`, `Time` (time of day, microseconds since midnight) and
     `Duration` (elapsed microseconds) become field types. A calendar **interval**
     (months, days, nanoseconds) and a **zone-less timestamp** do not: an interval has no total
     order and no exact conversion to a duration (`type-conversion-draft.md`: "Conditional, not
     automatic"), and `record-timestamp-utc` (complete) already decided that a naive timestamp is
     read as UTC with its value unshifted. Both are documented in `RECORD_STREAMS.md` as read as
     `Text` at a source that has them (the rule `relational-database-access` question 3 already
     proposes); Arrow `Interval` stays refused. Recommended. Alternative: an `Interval` type, a
     three-part cell that filters and sort would have to refuse.
  2. **Open design - text and JSON forms:** the forms `extended-scalar-value-types` chose for its
     scalars: a UUID as hyphenated lowercase (`"0190c2a4-7d3e-7b1a-9c3f-2a4b5c6d7e8f"`; reading
     also accepts upper case and the 32-digit form), a time as `"10:15:30"` with a fraction only
     when non-zero (`"10:15:30.5"`), a duration as ISO 8601 seconds (`"PT90.5S"`, `"-PT1S"`;
     reading also accepts `D`, `H` and `M` components but no years or months). JSON and NDJSON
     write these strings. Recommended: one text form per concept across scalars and cells.
  3. **Open design - UUID on the wire:** Arrow `FixedSizeBinary(16)` with the canonical extension
     name `arrow.uuid` in the field's metadata, and Parquet `FIXED_LEN_BYTE_ARRAY(16)` with the
     `UUID` logical type. On read, `FixedSizeBinary(16)` becomes `Uuid` only when it carries that
     extension name; any other fixed-size binary stays refused, as today. Recommended: a 16-byte
     column is not always a UUID.
  4. **Proposed resolution - no silent truncation:** a time or duration column in a coarser unit
     (seconds, milliseconds) is scaled exactly; one in nanoseconds is read when every value is a
     whole microsecond, and otherwise fails with a conversion error naming the field. Polars'
     `Time` is always nanoseconds, so this rule is what lets a polars-written column through.
  5. **Proposed resolution - never inferred:** CSV and other text inference never produces these
     types; a text table gets one only from a declared schema, as `record-decimal-field-type` does for
     `Decimal`.
  6. **Proposed resolution - polars and Parquet losses are documented:** polars has no UUID dtype,
     so a `Uuid` column becomes a polars `String` of the canonical text, and comes back as `Text`.
     Parquet has no duration logical type, so a `Duration` column is written as plain `INT64`
     microseconds and reads back as `Int` (values exact, type lost). Each loss is a line in
     `RECORD_STREAMS.md`'s per-format table.

## Purpose

Database keys are UUIDs; schedules hold times of day; logs and SLAs hold durations. Records can
carry them today only as `Text` or `Int`, losing the type, and an Arrow file containing them is
refused outright, so a faithful database layer (`relational-database-access`) and ordinary
Arrow/Parquet input need these types (VISION priority 2, records).

## Problem Example

`ops/jobs.arrow` is an Arrow IPC file with `id: FixedSizeBinary(16)` (extension `arrow.uuid`),
`starts: Time64(µs)` and `took: Duration(µs)`.

```
-R/ops/jobs.arrow/-/ns-rec/head-5
```

The query validates. At HEAD (`cd6d5da`) evaluation fails in `decode_arrow_field`
(`liquers-records/src/formats/ipc.rs`) with "Arrow IPC: field 'id' has unsupported Arrow type
FixedSizeBinary (tag 15)"; with `id` removed, the same for `Time` (tag 9), and then `Duration`
(tag 18). A polars frame with `Time` or `Duration` columns fails in `polars_dtype_to_field_type`
(`liquers-lib/src/records/polars.rs`). Should be: five rows with `id: Uuid`, `starts: Time`,
`took: Duration`, shown in CSV as `0190c2a4-…`, `10:15:30`, `PT90.5S`, and written back to IPC
with the same Arrow types.

## Scope and Acceptance Criteria

- **AC-1** IPC round trip
  WHEN an IPC file with a `FixedSizeBinary(16)`+`arrow.uuid`, a `Time64(µs)` and a `Duration(µs)` column (nulls included) is read and written back
  THEN the schema has `Uuid`, `Time`, `Duration`, every cell is equal, and the written file declares the same Arrow types and extension name
- **AC-2** Text tables under a declared schema
  WHEN a CSV with those three columns as text is read with schema `id: Uuid, starts: Time, took: Duration`
  THEN the cells parse, and CSV, JSON and NDJSON write them in the question 2 forms
- **AC-3** Bad text is refused
  WHEN a cell is `not-a-uuid`, `25:00:00` or `P1M` (a month)
  THEN reading fails with a conversion error naming the field
- **AC-4** Units are scaled exactly or refused
  WHEN an IPC `Time64(ns)` column holds whole microseconds, and another holds `1 ns`
  THEN the first reads as `Time` and the second fails naming the field; `Time32(ms)` and `Duration(s)` read scaled
- **AC-5** A fixed-size binary without the extension is still refused
  WHEN an IPC file has a `FixedSizeBinary(16)` with no extension name, or `FixedSizeBinary(8)`
  THEN reading fails with `not_supported` naming the field, as today
- **AC-6** Polars bridge
  WHEN a view with the three columns becomes a `polars.DataFrame` and back (feature `polars`)
  THEN `Time` and `Duration` keep their types and values, and `Uuid` becomes a `String` column of canonical text, read back as `Text`
- **AC-7** Parquet
  WHEN the view is written as Parquet and read back through the bridge
  THEN `id` is `FIXED_LEN_BYTE_ARRAY(16)` with logical type `UUID`, `starts` is `INT64` `TIME(MICROS, isAdjustedToUTC=false)`, `took` is plain `INT64`; `starts` reads back as `Time`, `took` as `Int` with equal values
- **AC-8** Filters and sort
  WHEN `ns-rec/gt-took-PT60S/ns-rec/sort-starts` runs over the jobs view
  THEN `PT90.5S` passes, `PT60S` does not, and rows are ordered by time of day; `ns-rec/eq-id-0190c2a47d3e7b1a9c3f2a4b5c6d7e8f` (the 32-digit form needs no `~_` escapes) selects by UUID
- **AC-9** Browser
  WHEN a batch with the three columns reaches JavaScript
  THEN `column("starts")` and `column("took")` are kinds `"Time"` and `"Duration"` over `BigInt64Array` microseconds, `column("id")` is kind `"Uuid"` over 16-byte rows, `columnCopy("id")` is an array of canonical strings, and a single-cell view of each is its text form
- **AC-10** Schemas serialize
  WHEN a schema with the three types is printed by `ns-rec/schema` and pasted into a YAML manifest
  THEN it reads back equal (`data_type: Uuid`, `Time`, `Duration`)
- **AC-11** Interval stays refused, documented
  WHEN an IPC file has an `Interval` column
  THEN reading fails with `not_supported` naming the field and suggesting a `Text` cast at the source

Non-goals: interval and zone-less timestamp types (question 1); arithmetic; a Python crossing
(`PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON`); the extended scalars (`extended-scalar-value-types`);
UUID generation.

## Core Interactions

Records (schema, columns, `FieldValue`, every table format, filters, sort, concat); the polars
bridge; the browser bridge; value cells in `liquers-lib` (`record_view_cell_as_base`).

## Crate Placement

`liquers-records` holds the model and the formats (core-only dependency; `chrono` is already a
dependency, so no new crate). `liquers-lib` holds the polars bridge and the cell-to-base mapping.
`liquers-web` holds the JavaScript descriptor.

## Documentation Intent

- Reference: `specs/reference/RECORD_STREAMS.md` §Schema and the per-format table (the three
  types; interval and zone-less timestamp read as `Text`; the polars and Parquet losses).
- Guide: `specs/guides/RECORD_STREAM_GUIDE.md` (declaring the types for a CSV).
- Other: `liquers-web/README.md` and the generated `.d.ts` (three kinds).

## Design Dependencies

- `record-decimal-field-type` — **overlaps** (order): both add arms at the same ~25 exhaustive
  matches; whichever lands second adds its arms beside the other's. T3 was judged weak: the arms
  are independent, this design's variants are unit variants (no lenient deserializer), and a
  merge would make an `XL` (E2). None of its open decisions changes this contract.
- `extended-scalar-value-types` — **overlaps**: shares the text forms (question 2); its `Uuid`,
  `Time` and `Duration` scalars are what these cells convert to.
- `relational-database-access` — **required-by**: its question 3 reads these SQL types as `Text`
  until this lands; `INTERVAL` and `TIMESTAMP WITHOUT TIME ZONE` stay `Text` afterwards.
- `ipc-reader-polars-text-columns` — **overlaps**, weak: edits other arms of `decode_arrow_field`.
- `record-timestamp-utc` (complete) — input: the zone-less timestamp decision (question 1).

## Consolidated Findings

- `FieldValue::Uuid([u8; 16])` is 16 bytes with alignment 1; `Time(i64)` and `Duration(i64)`
  match `Timestamp`, so `field_value_stays_reasonably_small` (≤ 32 bytes) holds.
- The IPC reader does not read a field's `custom_metadata` today; recognizing `arrow.uuid` needs
  it (the flatbuffer `Field` slot 6, a vector of `KeyValue`), and the writer must emit it.
- Polars writes `Time` in nanoseconds only; question 4's exact-scaling rule is what makes a
  polars-written file readable without silent truncation.
- 22 source files name `Timestamp` in a match today; each gains arms, listed by the compiler.
- Decisions 1-3 change a contract; 4-6 are proposed resolutions.

## References

`specs/issues/RECORD-FIELD-TYPE-HAS-NO-UUID-TIME-OR-INTERVAL.md`;
`specs/design/record-decimal-field-type/` (pattern); Arrow columnar format and canonical
extension types (`arrow.uuid`); Parquet `LogicalTypes.md` (UUID, TIME).
