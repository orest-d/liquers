# Phase 2: Solution & Architecture — record Decimal field type

## Overview

`liquers-records` gets one logical type, `FieldType::Decimal { precision, scale }`, one storage
variant, `Column::Decimal` (an `i128` buffer, which is Arrow's `Decimal128` layout), and one scalar,
`FieldValue::Decimal(Decimal128)`. The scalar is a small exact value type with parse and format,
and it carries its own precision and scale so a filter literal is self-describing. Each format
maps the type. Text formats use the exact decimal text, IPC uses Arrow `Decimal` (bitWidth 128),
and Parquet uses `FIXED_LEN_BYTE_ARRAY(16)` with the `DECIMAL` logical type. The polars bridge
maps it to `DataType::Decimal`, and the browser bridge to a `"Decimal128"` descriptor.

Rejected:
- `Decimal` as `Text` plus a schema annotation. Filters and sort would compare strings.
- `Decimal` stored as `Float` with a scale hint. That is the silent loss this issue exists to stop.
- Using `rust_decimal` in records. It has 28 digits, not Arrow's 38, and would add a dependency to
  a crate that has kept to core only.
- A 256-bit decimal. No consumer needs it, and it is refused clearly (AC-9).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN` (`ipc-reader-polars-text-columns`) | draft | P2 | same `decode_arrow_field` | no | no | Rebase second |
| `COLUMNMUT-VALIDITY-AND-VARIABLE-LENGTH-SET-ALWAYS-COPY` (`column-mut-zero-copy-validity`) | draft | P3 | `ColumnMut` arms | no | no | Rebase second |
| `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON` | draft | P3 | no Python path to change | no | no | None |
| `NO-RELATIONAL-DATABASE-ACCESS-LAYER` (`relational-database-access`) | draft | P2 | consumer | no | no | Unblocks its question 3 |

## Interfaces

```rust
// liquers-records/src/schema.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self")]                       // + the lenient Deserialize wrapper Analyzer uses
pub enum FieldType {
    Bool, Int, UInt, Float, Text, Binary, Date, Timestamp, Vector,
    /// Exact decimal: `precision` 1..=38 significant digits, `scale` 0..=precision fraction digits.
    Decimal { precision: u8, scale: u8 },
}
impl FieldType {
    pub fn decimal(precision: u8, scale: u8) -> Result<FieldType, Error>; // validates the ranges
}

// liquers-records/src/decimal.rs (new; pub use in lib.rs) — sync, no dependency
/// A decimal cell. The mantissa is stored as two words so `FieldValue` keeps 8-byte alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Decimal128 { lo: u64, hi: i64, precision: u8, scale: u8 }
impl Decimal128 {
    pub fn new(mantissa: i128, precision: u8, scale: u8) -> Result<Self, Error>; // |mantissa| < 10^precision
    pub fn mantissa(&self) -> i128;
    pub fn precision(&self) -> u8;
    pub fn scale(&self) -> u8;
    /// Exact parse at the given scale: "12.3" → 1230 at scale 2. Errors on excess fraction or
    /// overflow; never rounds.
    pub fn parse(text: &str, precision: u8, scale: u8) -> Result<Self, Error>;
    /// Exactly `scale` fraction digits: "12.30", "-0.05", "7" at scale 0.
    pub fn to_text(&self) -> String;
    /// Same value at another scale, exactly, or an error.
    pub fn rescale(&self, precision: u8, scale: u8) -> Result<Self, Error>;
}
impl std::fmt::Display for Decimal128;  impl Ord for Decimal128;   // value order; compares across scales

// liquers-records/src/column.rs
pub enum FieldValue { …, Decimal(Decimal128) }
pub enum Column { …, Decimal { validity: Option<Bitmap>, precision: u8, scale: u8, values: Buffer<i128> } }
```

Format mapping:

| Format | Write | Read |
|---|---|---|
| CSV/TSV, Markdown, HTML | `to_text()` | `Decimal128::parse` at the declared scale; never inferred |
| JSON, NDJSON | JSON string `"12.30"` | string or number (number text parsed exactly, not through `f64`) |
| IPC | `Type::Decimal { precision, scale, bitWidth: 128 }`, 16-byte LE values | the same; `bitWidth` other than 128 or negative scale → `not_supported` |
| Parquet (write) | `FIXED_LEN_BYTE_ARRAY(16)`, big-endian two's complement, `LogicalType::DECIMAL(scale, precision)` + converted type `DECIMAL` | via the polars bridge |
| polars | `Series` of `DataType::Decimal(precision, scale)` | `DataType::Decimal(p, s)` → `Decimal`; an unknown precision becomes 38 |
| Browser | descriptor `{kind: "Decimal128", ptr, len, precision, scale, validity}`; copy → strings | — |

Filters: `ns-rec` comparison commands parse their literal with
`FieldValue::parse_text(FieldType::Decimal{..}, text)` at the column's scale. A literal that is
not exact at that scale is an error, and the comparison is on the mantissa. Sort compares
mantissas. `Column::concat` requires equal precision and scale, and otherwise fails with an error
naming both.

## Integration Points

`liquers-records`:
- `schema.rs`: the variant, `FieldType::decimal`, the lenient deserializer.
- `decimal.rs` (new); `lib.rs` `pub mod decimal; pub use decimal::Decimal128;`.
- `column.rs`: variants, `len`, `get`, `push`, `compare`, `slice`, `concat`, `take`, null
  handling, `FieldValue::parse_text`.
- `mutable.rs`: `ColumnMut` arms.
- `formats/{csv,markdown,html,ndjson,shapes,ipc,parquet}.rs`: as in the table. `infer.rs` is
  unchanged: it never produces `Decimal`.
- `batch.rs`, `views.rs`, `sources.rs`, `search/*`: exhaustive-match arms.

`liquers-lib`:
- `Cargo.toml`: polars feature `dtype-decimal`.
- `records/polars.rs`: `polars_dtype_to_field_type`, the append and the reverse build.
- `value/mod.rs` `record_view_cell_as_base`: `FieldValue::Decimal(d)` → `SimpleValue::Text { d.to_text() }`.
- `mem/*`, `search/*`, `records/{commands,table_ops}.rs`: match arms.

`liquers-web`:
- `records.rs`: descriptor and copy arms.
- `typescript.rs`: the kind union and the `precision` / `scale` fields.
- `records_column.js`: a `"Decimal128"` view.
- `default_value.rs`: a single cell becomes a string.

## Error Handling

Out-of-range parameters and values are `Error::conversion_error_with_message(text,
"Decimal(p,s)", reason)`, naming the field where the caller knows it. Unsupported Arrow shapes are
`Error::not_supported`. Parquet and IPC structural errors keep their existing `general_error`
style. No `unwrap`.

## Relevant Commands

No new commands, and no signature changes. `specs/command_registry.yaml` is unchanged. Existing
`ns-rec` filters gain decimal support through `parse_text`.

## Documentation Architecture

| Path | Change |
|---|---|
| `specs/reference/RECORD_STREAMS.md` | §Schema: `Decimal`; the per-format mapping table; History, `reviewed:` |
| `specs/guides/RECORD_STREAM_GUIDE.md` | declaring a decimal column for a CSV; History, `reviewed:` |
| `liquers-web/README.md` | the `"Decimal128"` descriptor |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | about 30 in `liquers-records` and `liquers-lib` (match arms), plus 4 in `liquers-web` |
| Crates and workflows affected | records, lib (polars, mem, search), web; every table read and write |
| Existing tests likely to change | none semantically. Tests matching `FieldType` exhaustively gain arms. `field_value_stays_reasonably_small` must still pass |
| New validation | Phase 3; `cargo test -p liquers-records --all-features`; the polars-bridge tests; the web Node suite with `records`; `check-stubs.sh --records` |
| Compatibility / data | `FieldType` serde gains a struct variant, and existing schemas read unchanged. Files that used to be refused now read |
| Performance | `Column::Decimal` is fixed-width with no per-cell allocation, and `FieldValue` stays at 32 bytes |
| Polars | `dtype-decimal` adds compile time. Polars 0.55's exact `Decimal` signature (`Option` precision) is checked in step 7 |
| Recovery | Revert per step. Nothing stored before release depends on the type |
| Certainty | Medium-high. The layouts are standard. The polars API shape, and reading an IPC file polars wrote, are checked by the Phase 3 interop tests |
