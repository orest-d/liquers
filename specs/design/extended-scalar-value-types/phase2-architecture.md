# Phase 2: Solution & Architecture — extended scalar value types

## Overview

There are fifteen `define_value_types!` declarations, one per scalar. Each is gated by `cfg:
feature = "ext-scalars"`, and each points its hooks at generic functions in a new module,
`liquers-lib/src/value/scalars.rs`. A private trait, `ExtScalar`, carries the per-type facts:
identifier, text form, JSON form, base form and parse. The hooks are written once over that
trait. Conversion edges are returned from `ExtValue`'s `ValueExtension::conversions`.

Rejected:
- One `ExtValue::Scalar(ExtScalar)` variant holding a sub-enum. It breaks "one identifier per
  variant" (`CLAUDE.md` §Adding a Value Type) and the macro's one-declaration-per-identifier
  model.
- Adding the scalars to core `Value`. Prior art §9, consequence 3, rules that out, and the
  untagged form makes `7` ambiguous.
- Hand-written arms. The source issue requires declaration through the generator.
- Per-family features (Phase 1, question 3).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `VALUE-TYPE-DEFINITION-MACRO` (`value-type-definition-macro`) | draft | P2 | supplies `define_value_types!` | yes | yes (ordered) | Land first |
| `VALUE-CONVERSION-CAPABILITY` (`value-conversion-registry`) | draft | P2 | supplies `Conversion`, `ConversionClass` | yes | yes (ordered) | Land first |
| `CORE-VALUE-INTERFACE-CAPABILITY-SPLIT` | accepted | P2 | the macro emits `ValueType` | yes (via macro) | yes (ordered) | Land first |
| `SERDE-TYPES-HAVE-NO-GENERIC-DATA-FORMATS` (`generic-serde-data-formats`) | draft | P2 | optional generic formats | no | no | Add `.with_serde_formats()` when present |
| `RECORD-FIELD-TYPE-HAS-NO-DECIMAL` (`record-decimal-field-type`) | draft | P3 | shares the decimal decision | no | no | Edge class recorded |

## Interfaces

Declarations (one shown; the other fourteen differ only in names, type and `cfg`):

```text
U64(u64) {
    identifier: "U64", type_name: "u64",
    defaults: ("json", "json", "application/json", "data.json"),
    formats: ["json", "txt", "html", "css", "js", "py", "rs"],
    cfg: feature = "ext-scalars",
    rust_type: u64,
    encode: crate::value::scalars::encode::<u64>,
    decode: crate::value::scalars::decode::<u64>,
    as_base: crate::value::scalars::as_base::<u64>,
    json: crate::value::scalars::json::<u64>,
},
```

| Variant | Payload | Base form (`as_base`) | JSON | Text (`txt`) |
|---|---|---|---|---|
| `I8`, `I16`, `U8`, `U16`, `U32` | `i8` … `u32` | `I64` | number | decimal |
| `U64`, `I128`, `U128` | `u64`, `i128`, `u128` | `I64` if it fits, else `Text` | string | decimal |
| `F32` | `f32` | `F64` (exact) | number; `json_f64` tokens | Rust `Display` (`NaN`, `inf`) |
| `Decimal` | `rust_decimal::Decimal` | `Text` | string, scale kept (`"12.30"`) | same |
| `Date` | `chrono::NaiveDate` | `Text` | `"2026-10-26"` | same |
| `Time` | `chrono::NaiveTime` | `Text` | `"10:15:30.5"` (fraction only when non-zero) | same |
| `DateTime` | `chrono::DateTime<Utc>` | `Text` | RFC 3339, `Z`, `SecondsFormat::AutoSi` | same |
| `Duration` | `chrono::TimeDelta` | `Text` | ISO 8601 seconds form `"PT90.5S"`, `"-PT1S"` | same |
| `Uuid` | `uuid::Uuid` | `Text` | hyphenated lowercase | same |

When reading, the JSON readers also accept a number for `U64`, `I128`, `U128` and `Decimal`, and
`null` / tokens for `F32`, as `json_f64` does.

```rust
// liquers-lib/src/value/scalars.rs  (#![cfg(feature = "ext-scalars")]) — all synchronous
pub(crate) trait ExtScalar: Copy + Sized + 'static {
    const IDENTIFIER: &'static str;
    fn to_text(&self) -> String;
    fn from_text(s: &str) -> Result<Self, Error>;           // conversion_error_with_message on failure
    fn to_json(&self) -> serde_json::Value;                   // per the table
    fn from_json(v: &serde_json::Value) -> Result<Self, Error>;
    fn as_base(&self) -> SimpleValue;
}
pub(crate) fn encode<T: ExtScalar>(v: &T, data_format: &str) -> Result<Vec<u8>, Error>; // json | text formats
pub(crate) fn decode<T: ExtScalar>(b: &[u8], data_format: &str) -> Result<T, Error>;
pub(crate) fn as_base<T: ExtScalar>(v: &T) -> Result<SimpleValue, Error>;
pub(crate) fn json<T: ExtScalar>(v: &T) -> Result<serde_json::Value, Error>;           // F32 non-finite: ConversionError, as json_f64::to_number
pub(crate) fn duration_to_iso(d: &chrono::TimeDelta) -> String;
pub(crate) fn duration_from_iso(s: &str) -> Result<chrono::TimeDelta, Error>;      // the subset written above

// Public producers and extractors, per payload type T (f32 excluded from From for lib Value):
impl From<T> for ExtValue;   impl From<T> for crate::value::Value;
impl TryFrom<&ExtValue> for T;   // Err: Error::conversion_error(identifier, IDENTIFIER)
```

Conversion edges, from `impl ValueExtension for ExtValue { fn conversions() }` and lifted by
`CombinedValue`:

| Edge | Class |
|---|---|
| `I8`, `I16`, `U8`, `U16`, `U32` → `I64`; `I8`, `I16`, `U8`, `U16` → `I32`; `F32` → `F64`; `I64` → `I128`; `Date` → `DateTime` (midnight UTC) | Widening |
| `I64` → `I8`, `I16`, `U8`, `U16`, `U32`, `U64`; `U64`, `I128`, `U128` → `I64`; `I32` → `U8`, `U16`, `U32`; `Text` → every scalar; `F64` → `Decimal` (non-finite refused) | Fallible |
| `F64` → `F32`; `Decimal` → `F64`; `DateTime` → `Date`; `U64`, `I128`, `U128` → `F64` | Lossy |
| every scalar → `Text`; `I64` → `Decimal` (scale 0) | Exact |

## Integration Points

- `liquers-lib/src/value/mod.rs`: fifteen declarations inside the `define_value_types!` block;
  `ExtValue::conversions` gains the edges (a `#[cfg(feature = "ext-scalars")]` extend).
- `liquers-lib/src/value/scalars.rs` (new), `mod scalars;` in `value/mod.rs`.
- `liquers-lib/Cargo.toml`: `rust_decimal = { version = "1", optional = true }`,
  `uuid = { version = "1", optional = true }`; `ext-scalars = ["dep:rust_decimal", "dep:uuid"]` in
  `default`.
- Consumers with an explicit `match` over `ExtValue` outside the value module
  (`ui/web/html.rs`, `egui/mod.rs`, `liquers-web/src/default_value.rs`) get a cfg-gated arm. The
  UI matches render the scalar's `to_text`. `default_value.rs` returns what its neighbouring
  `ExtValue` arms return (`Ok(None)`). The macro design keeps those matches hand-written, and the
  compiler lists them.
- `scripts/check-build-matrix.sh`: `LIB_CONFIGS` gains `--no-default-features --features ext-scalars --tests`
  and a wasm32 row with `webui,ext-scalars`.
- `liquers-validate` and `specs/command_registry.yaml`: no change, because no command is added.

## Error Handling

A parse failure is `Error::conversion_error_with_message(text, IDENTIFIER, reason)`. An
unsupported format is `SerializationError` "Serialization to {fmt} not supported by {type_name}",
the macro's standard refusal. A narrowing edge out of range is `ConversionError`, naming both
identifiers. No `unwrap`, and no `Error::new`.

## Relevant Commands

None added. `convert` (from `value-conversion-registry`) reaches the new identifiers through the
edges. `specs/command_registry.yaml` is unchanged.

## Documentation Architecture

| Path | Change |
|---|---|
| `specs/reference/VALUE_TYPE_SYSTEM.md` | bare set and registered identifiers (feature `ext-scalars`); History, `reviewed:` |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` | the scalar declaration as the small worked example; History, `reviewed:` |
| `specs/guides/SERIALIZATION_GUIDE.md` | "Wide numbers in JSON" subsection (question 1); History, `reviewed:` |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-lib/{Cargo.toml, src/value/mod.rs, src/value/scalars.rs, src/ui/web/html.rs, src/egui/mod.rs}`, `liquers-web/src/default_value.rs`, `scripts/check-build-matrix.sh` |
| Crates and workflows affected | lib, web (render arms); any command returning the new Rust types |
| Existing tests likely to change | `ext_value_type_descriptions_complete` (it gains 15 descriptions with the feature); no other test asserts the absence of these types |
| New validation | Phase 3 tests; matrix rows; wasm32 |
| Compatibility / data | Additive. No stored data or identifier changes, and JSON forms are new |
| Performance | `ExtValue` size is unchanged: the largest new payload, `Decimal` / `i128` at 16 bytes, is smaller than existing payloads (`Arc` fat pointers are 16 too) |
| Security | Parsing bounded strings only |
| Recovery | Turn off `ext-scalars`; revert the declarations |
| Certainty | Medium-high. The mechanics are simple, and the dependence on two unimplemented designs is the main schedule risk. If the macro changes its DSL, only the declaration syntax here changes |
