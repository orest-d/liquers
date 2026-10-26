# Phase 4: Implementation Plan — record Decimal field type

## Overview

The plan builds the exact value type first, then the schema and column model, then the formats
one at a time, each with its tests. After that come the bridges (polars, browser), the cell
mapping, and the documents. Phase 1's recommended answers are assumed. Every step leaves the
workspace compiling, because exhaustive matches force each new arm in the same commit as its
variant (steps 2-3 add the variants together with every match arm, which at first only refuse).

## Progress

- [ ] Step 1: `Decimal128` value type
- [ ] Step 2: `FieldType::Decimal`, its serde, and refusal arms everywhere
- [ ] Step 3: `Column::Decimal`, `FieldValue::Decimal`, column operations
- [ ] Step 4: text formats (CSV/TSV, Markdown, HTML) and JSON/NDJSON
- [ ] Step 5: Arrow IPC read and write
- [ ] Step 6: Parquet write
- [ ] Step 7: polars bridge and `dtype-decimal`
- [ ] Step 8: lib cell mapping, `mem` / `search` arms, query-level tests
- [ ] Step 9: browser bridge, TypeScript declarations, JS view
- [ ] Step 10: documents and records
- [ ] Step 11: full checks and diff review

## Implementation Steps

### Step 1: value type
- Files / symbols: `liquers-records/src/decimal.rs` (new): `Decimal128` with `new`, `mantissa`,
  `precision`, `scale`, `parse`, `to_text`, `rescale`, `Display`, `Ord`. Export it from `lib.rs`.
- Depends on: none.
- Proof: Phase 3 tests 3-5 (`cargo test -p liquers-records decimal`).
- Rollback: delete the module.

### Step 2: schema type
- Files / symbols: `liquers-records/src/schema.rs`: the `FieldType::Decimal` variant,
  `FieldType::decimal`, and `#[serde(remote = "Self")]` with the lenient wrapper that `Analyzer`
  uses. In every crate, each exhaustive `FieldType` match gets a `Decimal` arm that refuses with
  `not_supported("Decimal …")` until its real step lands.
- Depends on: step 1.
- Proof: Phase 3 test 7; `cargo check` for records, lib and web (`bash scripts/check-build-matrix.sh`
  dry run lists the rows).
- Rollback: revert.

### Step 3: columns and scalars
- Files / symbols: `liquers-records/src/column.rs` (`Column::Decimal`, `FieldValue::Decimal`, the
  `len`/`get`/`push`/`compare`/`slice`/`concat`/`take` arms, `parse_text`) and
  `liquers-records/src/mutable.rs` (the `ColumnMut` arms).
- Depends on: step 2.
- Proof: Phase 3 tests 6 and 14.
- Rollback: revert.

### Step 4: text and JSON formats
- Files / symbols: `formats/csv.rs` `parse_scalar` and the writer; `formats/markdown.rs`;
  `formats/html.rs`; `formats/ndjson.rs` and `formats/shapes.rs` (string write, string-or-number
  read). Number text is parsed with `Decimal128::parse`, never through `f64`. `formats/infer.rs`
  is unchanged, and test 12 proves it.
- Depends on: step 3.
- Proof: Phase 3 tests 10-12.
- Rollback: revert the files.

### Step 5: IPC
- Files / symbols: `formats/ipc.rs`: `write_decimal_type`, the `build_field` arm, `encode_body`
  (16-byte LE values), and in `decode_arrow_field` a `TYPE_DECIMAL` (7) arm that reads
  `precision`, `scale`, `bitWidth` and refuses anything other than 128 or a negative scale. Also
  the column reader arm, using `read_fixed::<i128>`. Name the constant `TYPE_DECIMAL` and use it
  in `type_tag_name`.
- Depends on: step 3.
- Proof: Phase 3 tests 8 and 9; `cargo test -p liquers-records --features ipc`.
- Rollback: revert the file.

### Step 6: Parquet write
- Files / symbols: `formats/parquet.rs`: `encode_field` arm (FLBA, big-endian 16 bytes,
  statistics min/max as FLBA), `LogicalTypeSpec` decimal, `write_logical_type` DECIMAL
  (scale, precision), `write_leaf_schema_element` type_length 16 and converted type DECIMAL.
- Depends on: step 3.
- Proof: Phase 3 test 13; test 16 after step 7.
- Rollback: revert the file.

### Step 7: polars bridge
- Files / symbols: in `liquers-lib/Cargo.toml`, add polars feature `dtype-decimal`. In
  `liquers-lib/src/records/polars.rs`, update `polars_dtype_to_field_type`, the append function
  and the column → `Series` builder. Before writing them, check polars 0.55's `DataType::Decimal`
  signature (whether precision and scale are `Option<usize>`) and its `i128` physical access.
- Depends on: steps 3 and 6.
- Proof: Phase 3 tests 15-17 (`cargo test -p liquers-lib --test records_polars`).
- Rollback: revert, and drop the feature.

### Step 8: library arms and query tests
- Files / symbols: `liquers-lib/src/value/mod.rs` `record_view_cell_as_base` (`Decimal` →
  `Text`); the arms in `mem/{check,read,rules}.rs`, `search/{catalog,command_catalog,extract,md_split}.rs`
  and `records/{commands,table_ops}.rs`, which display decimals exactly and index them as text;
  `liquers-lib/tests/records_commands.rs` test 18.
- Depends on: steps 4 and 5.
- Proof: `cargo test -p liquers-lib --lib --tests`, and the same with `--no-default-features --features records`.
- Rollback: revert.

### Step 9: browser
- Files / symbols: `liquers-web/src/records.rs` (descriptor with `precision`/`scale`, and
  `columnCopy` returning strings), `liquers-web/src/typescript.rs` (`"Decimal128"` in the kind
  union, the two fields, doc comments), `liquers-web/src/records_column.js` (a `"Decimal128"` case
  that returns a `Uint8Array` view), `liquers-web/src/default_value.rs` (a single cell becomes a
  string), and the Node test 19.
- Depends on: step 3.
- Proof: after `cargo clean`, run
  `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles,records` and
  `./liquers-web/scripts/check-stubs.sh --build --records`.
- Rollback: revert the four files.

### Step 10: documents and records
- Files / symbols: `specs/reference/RECORD_STREAMS.md` and `specs/guides/RECORD_STREAM_GUIDE.md`,
  each with History and `reviewed:`; `liquers-web/README.md`. Close
  `RECORD-FIELD-TYPE-HAS-NO-DECIMAL`, and note in `relational-database-access` that its question 3
  is unblocked. Regenerate the index.
- Depends on: steps 1-9.
- Proof: `python3 scripts/docs_index.py --check`.
- Rollback: revert.

### Step 11: full checks
- Run the Phase 3 Test Plan commands and `bash scripts/check-build-matrix.sh`, which covers the
  records rows, the wasm32 rows and the lib rows. Review the diff: no step-2 refusal arms may
  remain, and there must be no `println!`, no `unwrap` in library code, and no `_ =>` over
  `FieldType`, `Column` or `FieldValue`.

## Testing Plan

The Phase 3 table: unit tests per format in `liquers-records`, bridge and query tests in
`liquers-lib`, the Node suite and the stub check in `liquers-web`, and the build matrix. Run the
browser loop separately, after `cargo clean`, as `CLAUDE.md` requires.

## Rollback Plan

Each step is one commit. Steps 4-9 can each be reverted alone, and the step-2 refusal arms then
come back to that format, so a reverted format refuses decimals as HEAD does today. Reverting
steps 1-3 removes the type entirely. No stored data predates the type.

## Documentation Updates

`specs/reference/RECORD_STREAMS.md`, `specs/guides/RECORD_STREAM_GUIDE.md`,
`liquers-web/README.md` and the generated declarations. The source issue is closed, and the
relational design's question 3 is updated.

## Phase 5 Entry Criteria

Progress is complete, every Phase 3 test passes in its feature configuration, the build matrix and
the web loops are green, no refusal arm from step 2 remains, and decisions 1-3 are recorded.
