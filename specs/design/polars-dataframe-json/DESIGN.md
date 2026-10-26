---
id: POLARS-DATAFRAME-JSON
kind: design
title: A polars DataFrame is written and read as JSON and NDJSON
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/polars, lib/value]
issues: [POLARS-DATAFRAME-JSON-NOT-SERIALIZABLE]
created: 2026-10-26
---
# A polars DataFrame is written and read as JSON and NDJSON

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

JSON is the format every web client reads. A `polars.DataFrame` value cannot be stored or served
as JSON, although the format parser already accepts the names and a record view of the same table
can. Implement `json` (an array of row objects) and `ndjson` / `jsonl` (one row object per line) for
DataFrames, writing the same JSON a record view writes.

### Problem Example

Verified at HEAD (`liquers-lib/src/polars/serde.rs`):

```
-R/data/sales.csv/-/ns-pl/head-5/data.json
```

The query is valid (`liquers-validate`: resource `data/sales.csv`, action `pl/head`, filename
`data.json`). Evaluated, the result is a `polars.DataFrame` whose declared format is `json`.
Today: `polars.DataFrame`'s `TypeInfo` (`liquers-lib/src/value/mod.rs`) declares only `csv`,
`csv:comma`, `csv_comma`, `parquet`, so `set_state` refuses it as undeclared, and an evaluated
asset's serialization (persistence, serving) fails in
`serialize_dataframe_to_writer` with "Polars serialization for data_format 'json' is not
implemented yet" (the parser maps `json` → `PolarsDataFormat::Json`, `ndjson`/`jsonl` →
`NdJson`, and both codec functions refuse them; polars' `json` feature is not enabled).
Should be: `[{"region":"N","amount":10.5}, …]`, five row objects, readable back into an equal frame.

### Scope and Acceptance Criteria

- **AC-1** JSON round trip
  WHEN a DataFrame with `Int64`, `Float64`, `String` and `Boolean` columns and a null cell is
  written as `json` and read back as `json`
  THEN the frame read back equals the original (`equals_missing`), column order and names included
- **AC-2** NDJSON round trip, both spellings
  WHEN the same frame is written and read as `ndjson` and as `jsonl`
  THEN it reads back equal, and the bytes are one JSON object per line
- **AC-3** Same JSON as a record view
  WHEN the frame and the record view converted from it (`to_record`) are both written as `json`
  THEN the two byte strings parse to equal JSON values (array of row objects, same keys and values)
- **AC-4** Declared and stored
  WHEN `type_info()` of a DataFrame is asked about `json`, `ndjson` and `jsonl`, and the problem
  example's query is evaluated as a keyed asset and reloaded through the fast track
  THEN all three are supported, and the reloaded value is a `polars.DataFrame` equal to the original
- **AC-5** Unsupported shapes refuse clearly
  WHEN a JSON payload that is not an array of objects (`{"a":1}`, `[1,2]`) is read as `json`
  THEN `deserialize_dataframe_from_reader` returns a `SerializationError` naming `json`

- **AC-6** A stored JSON file feeds `ns-pl`
  WHEN a store holds `data/rows.json` (an array of two row objects) and
  `-R/data/rows.json/-/ns-pl/head-1` is evaluated
  THEN the result is a one-row DataFrame (the input conversion `try_to_polars_dataframe` reads
  through the same `deserialize_dataframe_from_reader`)

Non-goals: `avro` (still refused; nothing uses it); column-oriented JSON (`{"col": [..]}`); exact
type preservation for temporal, decimal or nested columns (JSON has no such types: they read back
as strings or inferred types, as for a record view); logging that loss
(`serialization-lossy-write-warnings` could add a polars override later).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — enables a feature of the `polars` dependency and extends
  the formats declared in `polars.DataFrame`'s `TypeInfo`, a serialized-format change (rule 4).
- **Leading issue:** **Open design - implement or retract:** the issue leaves the choice open.
- **Explanation:** the parser, the dispatch and the `TypeInfo` slot exist; one dependency feature
  and two codec arms complete them. Phases 3-4 assume the recommended answers.
- **Open questions:**
  1. **Open design - implement or retract:** implement through polars' `json` feature, or remove
     `Json` / `NdJson` from `PolarsDataFormat` so the parser stops accepting formats nothing
     writes. Recommended: implement. `polars-json` is already in `Cargo.lock` (resolved through
     `polars-io`), so no new crate is fetched; retracting leaves web clients with only the
     `ns-rec/to_record` detour.
  2. **Proposed resolution - orientation:** rows (`JsonFormat::Json`: `[{..}, ..]`), matching
     `liquers-records`' `JsonOrient::Records` default, so `data.json` means the same for both table
     types (AC-3). Column orientation would need a refinement (`json:columns`), not proposed here.
  3. **Proposed resolution - declared spellings:** declare `json`, `ndjson` and `jsonl` (every
     spelling the parser accepts, as the IPC design does for its aliases); the default format stays
     `csv`.

### Design Dependencies

- `polars-dataframe-ipc` — **overlaps** (T3, same change site: the refusing
  `Ipc | Json | NdJson | Avro` arms of both codec functions and the `TypeInfo` comment). Not
  attached: its Phase 4 is finished. No functional order; whichever lands second rebases the arms
  (if IPC lands first, the arm is `Json | NdJson | Avro` → this design leaves only `Avro`).
- `serialization-lossy-write-warnings` — **overlaps**, weak: its non-goals rely on polars JSON
  being unimplemented; once this lands, a polars override of `serialization_warnings` for
  non-finite floats and temporal columns becomes possible. No order.
- `data-format-vocabulary` — **overlaps**, weak: `json`/`ndjson`/`jsonl` are in its table; no order.

### Consolidated Findings

- `liquers-web` has a `polars` feature (`liquers-lib/polars`), so the dependency feature reaches
  `wasm32` when that feature is on; no build-matrix row checks it today. Step 1 checks it once.
- The `TypeInfo` gate is what makes AC-4 work: an undeclared format is refused before the codec.

## Phase 2: Architecture

### Solution

1. `liquers-lib/Cargo.toml`: add `"json"` to the `polars` dependency's features.
2. `liquers-lib/src/polars/serde.rs`:
   - `serialize_dataframe_to_writer`: `PolarsDataFormat::Json` →
     `JsonWriter::new(writer).with_json_format(JsonFormat::Json).finish(&mut df.clone())`;
     `NdJson` → the same with `JsonFormat::JsonLines`. Errors: `serialization_error("Failed to
     serialize JSON DataFrame: …")`, as the CSV arm does.
   - `deserialize_dataframe_from_reader`: `Json` → `JsonReader::new(reader).with_json_format(JsonFormat::Json).finish()`;
     `NdJson` → `JsonLines`. Errors name `json` / `ndjson` (AC-5).
   - The refusing arm keeps `Avro` (and `Ipc` until `polars-dataframe-ipc` lands). Explicit arms.
3. `liquers-lib/src/value/mod.rs`: `polars.DataFrame` `with_data_formats([... , "json", "ndjson",
   "jsonl"])`; the comment above it names only the formats still unimplemented.

Rejected: retract the variants (Q1); hand-writing JSON through `serde_json` over the columns
(duplicates polars' writer, and the read side would need its own type inference); converting through
a record view inside the serializer (couples `polars` to `records`, which are independent features).

Known-issue preflight: `polars-dataframe-ipc` (same arms, see dependencies). Nothing blocks.

### Changes

- `liquers-lib/Cargo.toml`, `liquers-lib/src/polars/serde.rs`, `liquers-lib/src/value/mod.rs`.
- Tests: `serde.rs` `mod tests`; `liquers-lib/tests/polars_value_serde.rs` (already
  `#![cfg(feature = "polars")]`).
- Documents: `specs/reference/POLARS_COMMAND_LIBRARY.md` (§ deserialization from binary: `json`,
  `ndjson`, `jsonl`, and the "Supported: csv, parquet" message text; History row, `reviewed:`).
- No command or registry change (`specs/command_registry.yaml` lists commands, not type formats).

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | the three above, two test files, one reference |
| Existing tests | `polars_type_info_*` tests that list the declared formats gain three entries |
| Dependency | polars `json` may pull `simd-json`; confirm `cargo tree -p liquers-lib -e features -i polars-io` shows no new crate, and `cargo check -p liquers-web --target wasm32-unknown-unknown --features polars` |
| Data | JSON loses temporal/decimal types (documented non-goal); NaN behaviour follows polars' writer and is pinned by a test rather than assumed |
| Compile time | one more polars-io module; measured in step 1 |
| Recovery | revert the commit; the feature line goes with it |
| Certainty | Medium-high: exact polars 0.55 builder names are confirmed at step 1 |

## Phase 3: Examples and Tests

### Examples

```rust
let df = df!("region" => ["N", "S"], "amount" => [Some(10.5), None])?;
let mut buf = Vec::new();
serialize_dataframe_to_writer(&df, "json", &mut buf)?;   // [{"region":"N","amount":10.5},{"region":"S","amount":null}]
let back = deserialize_dataframe_from_reader(Cursor::new(buf), "json")?;
assert!(back.equals_missing(&df));
```

Query (validated with `liquers-validate`, status `Ok`): `-R/data/sales.csv/-/ns-pl/head-5/data.json`.

### Tests

Unit, `liquers-lib/src/polars/serde.rs` `mod tests`:

- `json_roundtrip_keeps_the_frame` — the four column types and a null: AC-1
- `ndjson_roundtrip_both_spellings` — `ndjson`, `jsonl`; line count equals row count: AC-2
- `json_payload_that_is_not_rows_is_refused` — `{"a":1}`, `[1,2]`: `SerializationError` naming
  `json`: AC-5
- `json_non_finite_float_behaviour_is_pinned` — a `NaN` cell: assert whatever polars writes and
  reads back, so a polars upgrade that changes it fails visibly (part of AC-1's contract)

Integration, `liquers-lib/tests/polars_value_serde.rs`:

- `polars_type_info_declares_json_spellings` — `supports_data_format` for the three: AC-4
- `polars_value_json_roundtrip_through_value` — `Value::as_bytes` / `deserialize_from_bytes(..,
  "polars.DataFrame", "json")`: AC-1, AC-4
- `polars_and_record_view_write_the_same_json` — `#[cfg(feature = "records")]`, the frame and its
  `to_record` view: equal parsed JSON: AC-3
- `keyed_json_dataframe_reloads_through_fast_track` — a memory store, recipe
  `data/head.json` ← `-R/data/sales.csv/-/ns-pl/head-5/data.json`, evaluate, new environment over
  the same store, `get`; command counter unchanged and frame equal: AC-4

- `stored_json_file_feeds_ns_pl` — memory store with `data/rows.json`; the AC-6 query (validated,
  `Ok`); one row, both columns: AC-6

Run: `cargo test -p liquers-lib --lib polars::serde` and
`cargo test -p liquers-lib --test polars_value_serde`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-lib/Cargo.toml` — add polars `json`; `cargo check -p liquers-lib`, `cargo tree` check, `cargo check -p liquers-web --target wasm32-unknown-unknown --features polars`; confirm the `JsonWriter`/`JsonReader`/`JsonFormat` names. Rollback: drop the feature.
- [ ] 2. `liquers-lib/src/polars/serde.rs` — the four codec arms and unit tests — `cargo test -p liquers-lib --lib polars::serde`. Depends on 1.
- [ ] 3. `liquers-lib/src/value/mod.rs` — declare the three formats and update the comment; integration tests — `cargo test -p liquers-lib --test polars_value_serde`. Depends on 2.
- [ ] 4. `POLARS_COMMAND_LIBRARY.md` (History row, `reviewed:`); close `POLARS-DATAFRAME-JSON-NOT-SERIALIZABLE` with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-lib --lib --tests`;
`cargo test -p liquers-lib --no-default-features --features polars --lib --tests`;
`bash scripts/check-build-matrix.sh`; diff review: no change outside the named files.
