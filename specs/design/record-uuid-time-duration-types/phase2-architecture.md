# Phase 2: Solution & Architecture — UUID, time-of-day and duration record field types

## Overview

`liquers-records` gets three unit variants of `FieldType` — `Uuid`, `Time`, `Duration` — three
storage variants of `Column` and three `FieldValue` scalars. All are fixed-width and laid out as
Arrow lays them out, so an IPC export stays a pointer hand-off. Each table format maps them; the
polars and browser bridges map them; interval and zone-less timestamp remain non-types with a
documented `Text` rule.

Rejected:
- `Uuid` as `Binary` plus a schema annotation — filters would compare bytes against text
  literals and every writer would need the annotation.
- `Uuid` stored as `Buffer<u128>` — byte order would differ from Arrow's big-endian
  `FixedSizeBinary(16)`, costing a conversion per export.
- `Time` and `Duration` in nanoseconds — `Timestamp` is microseconds everywhere; one unit for all
  temporal columns keeps comparisons and the browser kinds uniform.
- An `Interval` type and a `LocalTimestamp` type (Phase 1 question 1).
- The `uuid` crate in records — 16 bytes and a hex formatter do not justify a dependency in a
  crate kept to core; `liquers-lib` may still use it for the extended scalar.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `RECORD-FIELD-TYPE-HAS-NO-DECIMAL` (`record-decimal-field-type`) | draft | P2 | same match sites, other arms | no | no | rebase second |
| `IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN` (`ipc-reader-polars-text-columns`) | draft | P2 | same `decode_arrow_field`, other arms | no | no | rebase second |
| `COLUMNMUT-VALIDITY-AND-VARIABLE-LENGTH-SET-ALWAYS-COPY` (`column-mut-zero-copy-validity`) | draft | P3 | `ColumnMut` gains arms | no | no | rebase second |
| `NO-RELATIONAL-DATABASE-ACCESS-LAYER` (`relational-database-access`) | draft | P2 | consumer | no | no | answers its question 3 for these types |
| `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON` | draft | P3 | no Python path | no | no | none |

## Interfaces

```rust
// liquers-records/src/schema.rs — unit variants, so the existing derive serializes them as
// `Uuid`, `Time`, `Duration`; no lenient reader needed.
pub enum FieldType { Bool, Int, UInt, Float, Text, Binary, Date, Timestamp, Vector,
    /// RFC 9562 UUID, 16 bytes in network (big-endian) order.
    Uuid,
    /// Time of day, microseconds since midnight, 0 ..= 86_399_999_999. No zone, no date.
    Time,
    /// Elapsed time in microseconds, signed. Not a calendar interval.
    Duration,
}

// liquers-records/src/column.rs
pub enum FieldValue { …, Uuid([u8; 16]), Time(i64), Duration(i64) }
pub enum Column { …,
    Uuid { validity: Option<Bitmap>, values: Buffer<[u8; 16]> },
    Time { validity: Option<Bitmap>, values: Buffer<i64> },
    Duration { validity: Option<Bitmap>, values: Buffer<i64> },
}

// liquers-records/src/temporal.rs (new, pub(crate)) — sync, chrono only
pub(crate) fn uuid_to_text(bytes: &[u8; 16]) -> String;              // hyphenated lowercase
pub(crate) fn uuid_from_text(text: &str) -> Result<[u8; 16], Error>;  // hyphenated or 32 hex, any case
pub(crate) fn time_to_text(micros: i64) -> String;                    // "10:15:30", "10:15:30.5"
pub(crate) fn time_from_text(text: &str) -> Result<i64, Error>;       // range-checked
pub(crate) fn duration_to_text(micros: i64) -> String;                // "PT90.5S", "-PT1S", "PT0S"
pub(crate) fn duration_from_text(text: &str) -> Result<i64, Error>;   // [-]P[nD][T[nH][nM][n[.f]S]]; Y and M refused
/// Exact unit scaling; `Err` naming `field` when a nanosecond value is not a whole microsecond.
pub(crate) fn to_micros(value: i64, unit: TimeUnit, field: &str) -> Result<i64, Error>;
```

`FieldValue::parse_text` routes the three types to these parsers (it is what filters use), and
`Column::compare` / sort compare `Uuid` bytewise and `Time` / `Duration` as `i64`.

Format mapping:

| Format | Uuid | Time | Duration |
|---|---|---|---|
| CSV/TSV, Markdown, HTML | text form | text form | text form |
| JSON, NDJSON | string | string | string |
| IPC write | `FixedSizeBinary(16)` + field metadata `ARROW:extension:name = arrow.uuid` | `Time(MICROSECOND, 64)` | `Duration(MICROSECOND)` |
| IPC read | `FixedSizeBinary(16)` with that name; else refused | `Time` any unit, scaled exactly (question 4) | `Duration` any unit, scaled exactly |
| Parquet write | `FIXED_LEN_BYTE_ARRAY(16)`, `LogicalType::UUID` | `INT64`, `TIME(isAdjustedToUTC=false, MICROS)` | `INT64`, no logical type |
| Parquet read (polars) | as polars yields it (`Binary`) | `Time` | `Int` |
| polars | `String` (canonical text) → back as `Text` | `DataType::Time` (ns ↔ µs) | `DataType::Duration(Microseconds)`; other units scaled exactly |
| Browser | kind `"Uuid"`, `Uint8Array` of `len*16`; copy → strings | kind `"Time"`, `BigInt64Array` | kind `"Duration"`, `BigInt64Array` |
| `record_view_cell_as_base` | `Text` (canonical) | `Text` | `Text` |

## Integration Points

`liquers-records`:
- `schema.rs` (variants), `column.rs` (variants, `len`, `get`, `push`, `compare`, `slice`,
  `concat`, `take`, null handling, `parse_text`), `mutable.rs` (`ColumnMut` arms), new
  `temporal.rs` (+ `mod temporal;` in `lib.rs`).
- `formats/csv.rs` (`parse_scalar`, cell writing), `markdown.rs`, `html.rs`, `ndjson.rs`,
  `shapes.rs`; `formats/ipc.rs` — `decode_arrow_field` (tags 9, 15, 18; read `custom_metadata`,
  Field slot 6), the writer's type tables and field metadata, buffer read/write arms;
  `formats/parquet.rs` (schema elements and value encoding); `infer.rs` unchanged.
- `batch.rs`, `views.rs`, `sources.rs`, `search/*`: exhaustive-match arms (search indexes `Uuid`
  as a keyword text, `Time`/`Duration` as integers).

`liquers-lib`:
- `records/polars.rs` — `polars_dtype_to_field_type` (`Time`, `Duration(_)`),
  `append_series_values`, the reverse builder; `Cargo.toml` polars features `dtype-time`,
  `dtype-duration` if `temporal` does not already enable them (checked in step 6).
- `value/mod.rs` `record_view_cell_as_base`; `mem/*`, `search/*`, `records/{commands,table_ops}.rs`:
  arms.

`liquers-web`:
- `records.rs` (descriptor and copy arms), `typescript.rs` (kind union), `records_column.js`
  (`"Uuid"` rows to strings), `default_value.rs` (single cell → text form).

## Error Handling

Bad text, out-of-range time, a month or year in a duration, and a non-whole nanosecond value are
`Error::conversion_error_with_message(text, "<Type>", reason)`, naming the field where the caller
knows it. Unsupported Arrow shapes (`FixedSizeBinary` without the extension or of another width,
`Interval`) are `Error::not_supported` with the field name; `Interval`'s message suggests a `Text`
cast at the source. No `unwrap`.

## Relevant Commands

No new commands, no signature changes; `specs/command_registry.yaml` unchanged. Existing `ns-rec`
filters (`eq`, `gt`, …) and `sort` gain the types through `parse_text` and `compare`.

## Documentation Architecture

| Path | Change |
|---|---|
| `specs/reference/RECORD_STREAMS.md` | §Schema: the three types; per-format table; the interval and zone-less timestamp rule; the polars and Parquet losses; History, `reviewed:` |
| `specs/guides/RECORD_STREAM_GUIDE.md` | declaring the types for a CSV; History, `reviewed:` |
| `liquers-web/README.md` | the three kinds |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | about 25 across `liquers-records`, `liquers-lib`, `liquers-web` (match arms), plus `temporal.rs` |
| Crates and workflows affected | records, lib (polars, mem, search), web; every table read and write |
| Existing tests likely to change | none semantically; exhaustive matches in tests gain arms; the IPC test asserting tag 15 is refused is narrowed to "without the extension" (AC-5) |
| New validation | Phase 3; `cargo test -p liquers-records --all-features`; polars bridge tests; web Node suite with `records`; `check-stubs.sh --records`; `scripts/check-build-matrix.sh` |
| Compatibility / data | `FieldType` serde gains three unit names; existing schemas read unchanged; files refused today now read. A `.d.ts` consumer switching on `kind` sees three new kinds |
| Performance | fixed-width columns, no per-cell allocation; `FieldValue` stays ≤ 32 bytes |
| Polars | the exact `Duration` time-unit enum and the `Time` feature name in polars 0.55 are checked in step 6 |
| Recovery | revert per step; nothing persisted before release uses the types |
| Certainty | Medium-high; IPC field metadata reading is new code, covered by AC-1 / AC-5 |
