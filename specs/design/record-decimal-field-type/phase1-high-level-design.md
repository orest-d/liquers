# Phase 1: High-Level Design — record Decimal field type

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. The source is `L`. The design adds `FieldType`,
  `FieldValue` and `Column` variants and a `pub` struct in `liquers-records`, changes the schema's
  serialized form, the browser column descriptor and a polars feature, and spans three crates
  (rules 1, 3, 4 and 6).
- **Leading issue:** **Open design - scope.** Whether this design delivers `Decimal` alone.
- **Explanation:** Arrow's `Decimal128` layout is a 16-byte two's-complement mantissa per cell
  with a column precision and scale. It fits the existing columnar model (`Buffer<i128>`,
  `bytemuck::Pod`) and every writer at HEAD, so a complete plan is possible. The open questions
  are contract choices with recommended answers.
- **Open questions:**
  1. **Open design - scope:** `Decimal` only. UUID, time of day, intervals and a zone-less
     timestamp move to the new issue `RECORD-FIELD-TYPE-HAS-NO-UUID-TIME-OR-INTERVAL`. Until that
     lands, those Arrow types stay refused as they are today, and the relational design reads them
     as `Text`. Recommended: yes. Decimal is the one type `relational-database-access` names as a
     prerequisite. Since `record-timestamp-utc`, `Timestamp` is already a UTC instant in every
     format. Adding all five would make this `XL`.
  2. **Open design - JSON / NDJSON cell form:** a JSON **string** holding the exact text
     (`"12.30"`), and reading also accepts a JSON number. Recommended: yes, matching
     `extended-scalar-value-types` question 1, because a JSON number is read as a double by most
     consumers.
  3. **Open design - browser form:** `RecordBatch.column` describes a decimal column as kind
     `"Decimal128"` (16-byte little-endian elements, as Arrow JS lays them out), with `precision`
     and `scale`. `columnCopy` returns an array of strings, and a single-cell view crosses as a
     string. Recommended: yes. JavaScript has no 128-bit typed array, and strings are exact.
  4. **Proposed resolution - parameters:** precision 1-38, scale 0 to precision. A negative
     scale (Arrow allows one) or a 256-bit decimal is refused on read with `not_supported`,
     naming the field.
  5. **Proposed resolution - no silent rounding:** text with more fraction digits than the
     scale, or a value that exceeds the precision, is a conversion error naming the field.
     Inference never produces `Decimal`. A text table gets one only from a declared schema.
  6. **Proposed resolution - polars:** turn on polars' `dtype-decimal` feature and map
     `DataType::Decimal` both ways. Parquet decimals then read through the bridge.

## Purpose

Money and other exact quantities are `DECIMAL(p, s)` in SQL, Parquet and Arrow. Records can hold
them only as `Float`, which loses precision silently, or as `Text`, which loses the type, and an
IPC or Parquet file with a decimal column is refused outright. A faithful database layer
(`relational-database-access`) and ordinary Parquet input need an exact type (VISION priority area
2, records).

## Problem Example

`finance/ledger.arrow` is an Arrow IPC file with `amount: Decimal128(10, 2)`.

```
-R/finance/ledger.arrow/-/ns-rec/head-5
```

The query validates. Evaluation fails today with "Arrow IPC: field 'amount' has unsupported
Arrow type Decimal (tag 7)" (`decode_arrow_field`, `liquers-records/src/formats/ipc.rs`). It
should give five rows with `amount` as `Decimal { precision: 10, scale: 2 }`, show `12.30` in CSV
and as `"12.30"` in JSON, and write back to IPC and Parquet as `Decimal128(10, 2)`.

## Scope and Acceptance Criteria

- **AC-1** IPC decimal columns are read and written
  WHEN an IPC file with a `Decimal128(10, 2)` column (nulls included) is read and written back
  THEN the schema has `Decimal { precision: 10, scale: 2 }`, every cell is equal, and the written
  file declares `Decimal128(10, 2)`
- **AC-2** Parquet decimal columns are written and read through the bridge
  WHEN a decimal view is written as Parquet and read back (feature `polars`)
  THEN the column is `DECIMAL(10, 2)` (`FIXED_LEN_BYTE_ARRAY(16)`) and reads back equal
- **AC-3** Text tables carry decimals under a declared schema
  WHEN `amount\n12.3\n-0.05\n` is read as CSV with schema `amount: Decimal(10,2)`
  THEN the cells are `12.30` and `-0.05`, written back as `12.30` and `-0.05`; JSON and NDJSON
  write `"12.30"`
- **AC-4** No silent rounding
  WHEN `12.345` is read into `Decimal(10, 2)`, or `123456789012` into `Decimal(10, 2)`
  THEN reading fails with a conversion error naming `amount`
- **AC-5** Polars bridge
  WHEN a decimal view becomes a `polars.DataFrame` and back
  THEN the dtype is `Decimal(10, 2)` and the values are equal
- **AC-6** Filters and sorting are exact
  WHEN `ns-rec/gt-amount-10/ns-rec/sort-amount` runs over the ledger
  THEN `10.01` passes the filter, `10.00` does not, and the order follows the exact values
- **AC-7** Browser
  WHEN a decimal batch reaches JavaScript
  THEN `column("amount")` has kind `"Decimal128"`, `precision` 10, `scale` 2;
  `columnCopy("amount")` is `["12.30", null, …]`; a single-cell view is the string `"12.30"`
- **AC-8** Schemas serialize
  WHEN a schema containing a decimal field is printed by `ns-rec/schema` and pasted into a YAML
  manifest
  THEN it reads back equal (`data_type: {Decimal: {precision: 10, scale: 2}}`)
- **AC-9** Unsupported decimal shapes are refused clearly
  WHEN an IPC file holds `Decimal256` or a negative scale
  THEN reading fails with `not_supported`, naming the field and the shape

Non-goals: decimal arithmetic or aggregation; UUID, time, interval and zone-less timestamp
(question 1); a Python crossing (records do not cross to Python yet,
`PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON`); the extended scalar `Decimal` value
(`extended-scalar-value-types`), which converts from these cells once it exists.

## Core Interactions

Records (schema, columns, scalar `FieldValue`, every table format, filters, sort, concat); the
polars bridge; the browser bridge; value cells in `liquers-lib` (`record_view_cell_as_base`
gives a decimal cell as `Text`).

## Crate Placement

`liquers-records` holds the model and the formats (it depends on core only). `liquers-lib` holds
the polars bridge and the cell-to-base mapping. `liquers-web` holds the JavaScript descriptor.

## Design Dependencies

- `relational-database-access`: **required-by**. Its question 3 waits on this design.
- `extended-scalar-value-types`: **overlaps**. They share the decimal and JSON-form decisions.
  A record `Decimal` cell becomes the `Decimal` scalar with class Fallible above 28 digits.
- `ipc-reader-polars-text-columns`: **overlaps**, weak. Both edit `decode_arrow_field`, but in
  different arms, and whichever lands second rebases.
- `column-mut-zero-copy-validity`: **overlaps**, weak. `ColumnMut` gains a decimal arm.
- `record-timestamp-utc` (complete): input. Timestamps are no longer part of this gap.

## Documentation Intent

- Reference: `specs/reference/RECORD_STREAMS.md` §Schema (the type and its mapping per format).
- Guide: `specs/guides/RECORD_STREAM_GUIDE.md` (declaring a decimal schema for a CSV).
- Other: `liquers-web/README.md` and the generated `.d.ts` (`"Decimal128"` kind).

## Consolidated Findings

- `FieldValue` must stay at most 32 bytes (`field_value_stays_reasonably_small`). An `i128`
  field would raise its alignment to 16 and its size to 48. The cell type therefore stores the
  mantissa as two 8-byte words (`Decimal128 { lo, hi, precision, scale }`, 24 bytes), which keeps
  `FieldValue` at 32.
- `FieldType` stays `Copy`. Its serde shape gains a struct variant, so it needs the same lenient
  YAML reader that `Analyzer` and `IndexKind` already use.
- Every exhaustive match over `FieldType`, `Column` or `FieldValue` (about 30 files) gains an arm, and
  the compiler lists them. The non-format ones (`mem`, `search`) treat a decimal like `Float`
  for display and like `Text` for search, so they never round.
- Parquet reading goes through polars, so AC-2 and AC-5 need `polars`. A build without polars
  writes Parquet decimals but cannot read them, as for every other Parquet type today.
- Decisions 1-3 change a contract. Decisions 4-6 are proposed resolutions.

## References

`specs/issues/RECORD-FIELD-TYPE-HAS-NO-DECIMAL.md`; `specs/design/record-streams/engine-survey.md`
§3 gap 2; `specs/design/relational-database-access/phase1-high-level-design.md` question 3;
Arrow columnar format (Decimal128); Parquet `LogicalTypes.md` (DECIMAL).
