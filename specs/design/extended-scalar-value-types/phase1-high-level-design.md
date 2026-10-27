# Phase 1: High-Level Design — extended scalar value types

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. The source is `L`. The design adds 15 `ExtValue` variants,
  `pub` `From` impls, a feature flag and two dependencies, and it changes the closed bare-identifier
  set (rules 1, 3 and 4).
- **Leading issue:** **Open design - JSON forms of the wide numbers.** The form `u64`, `i128`,
  `u128` and `decimal` take in JSON decides what every reader of the stored data and of HTTP
  responses sees.
- **Explanation:** the scalars are plain, `Copy`-sized payloads with obvious text forms. The
  generator and the conversion registry are specified in their own designs, so a complete plan
  can be written against them. The open choices are contract choices with recommended answers.
  Implementation waits for `value-type-definition-macro` and `value-conversion-registry`.
- **Open questions:**
  1. **Open design - JSON forms:** `i8`, `i16`, `u8`, `u16`, `u32` and `f32` are JSON numbers,
     with `f32` taking the `json_f64` tokens for non-finite values. `u64`, `i128`, `u128` and
     `decimal` are always JSON **strings** of their canonical text (`"18446744073709551615"`,
     `"12.30"`), and on read both a string and a number are accepted. Temporal values and `uuid`
     are strings: ISO 8601 / RFC 3339 and the hyphenated UUID. Recommended: yes. A number above
     2⁵³ silently loses precision in JavaScript, and making the form depend on the value's size
     would make it unpredictable.
  2. **Open design - decimal representation:** `rust_decimal::Decimal` (96-bit mantissa, scale
     ≤ 28), as the prior art proposes. It brings parsing, formatting and arithmetic. The record
     field type (`record-decimal-field-type`) stores Arrow `Decimal128` (38 digits), so a record
     decimal with more than 28 significant digits converts to `Decimal` with the **Fallible**
     class. Recommended: yes. The alternative, a Liquers-owned `i128` + scale type, matches Arrow
     exactly but has no arithmetic, and every command would have to re-implement it.
  3. **Open design - feature gating:** one `liquers-lib` feature, `ext-scalars`, on by default,
     for all fifteen. It adds `rust_decimal` and `uuid`, both pure Rust, both building for
     wasm32, and `uuid` is already in `Cargo.lock`. The issue's starting point was a two-way
     split, `ext-scalars` / `ext-temporal`. That buys little, because `chrono` is already
     non-optional, and it doubles the build-matrix rows. Recommended: one feature.
  4. **Open design - bare identifiers:** `I8`, `I16`, `I128`, `U8`, `U16`, `U32`, `U64`, `U128`,
     `F32`, `Decimal`, `Date`, `Time`, `DateTime`, `Duration`, `Uuid` are added to the closed bare
     set in `VALUE_TYPE_SYSTEM.md`, in the same CamelCase style as `I32`, `I64` and `F64`.
     Recommended: yes. Liquers owns these concepts, and "when unsure, prefix" does not apply to
     the fundamental scalars.
  5. **Proposed resolution - semantics:** `DateTime` is a UTC instant with nanosecond
     precision, the same meaning as the record `Timestamp` (`record-timestamp-utc`). `Duration`
     is an elapsed time (`chrono::TimeDelta`), not a calendar interval. `Time` is a time of day
     with no zone.
  6. **Proposed resolution - producers:** `From<T> for ExtValue` and `From<T> for Value`
     (lib), so a command can return `u64`, `NaiveDate` and so on, plus conversion edges and
     deserialization. `From<f32>` is not added for lib `Value`, because `SimpleValue` already
     maps `f32` to `F64`, and one Rust type must not produce two identifiers. Python and
     JavaScript bridges are out of scope and filed as `EXTENDED-SCALARS-NOT-MAPPED-BY-LANGUAGE-BRIDGES`.

## Purpose

Polars, Arrow, Python and SQL all have typed integers, `f32`, decimals, dates, times, instants,
durations and UUIDs. Liquers' base scalars do not, so these values cross the boundary as `I64` or
`Text` and lose their type. Prior art §9 shows each of the fifteen is distinct in at least five
of the nine target ecosystems. This design declares them as `ExtValue` types (VISION priority
area 6, extended value library).

## Problem Example

A command computes a 64-bit hash, `u64::MAX - 3`. Today it can return only `I64` (which fails
the range check) or `Text`, and in either case the stored type is not `u64`. A date computed by a
command is stored as `Text`, and a later command cannot tell it from any other string.

Once the scalars exist, `Value::from(18446744073709551612u64)` has identifier `U64`, is stored as
`"18446744073709551612"` in `data.json`, reads back as `U64`, and `convert-I64` on it fails with a
range error instead of wrapping. `Value::from(NaiveDate)` is a `Date`, stored as `"2026-10-26"`.

## Scope and Acceptance Criteria

- **AC-1** A wide unsigned value round-trips
  WHEN `Value::from(u64::MAX - 3)` is written as `json` and as `txt` and read back with identifier
  `U64`
  THEN the bytes are `"18446744073709551612"` (JSON) and `18446744073709551612` (text), and both
  read back as an equal `U64`
- **AC-2** Fifteen identifiers are registered
  WHEN the type registry of lib `Value` is built with `ext-scalars`
  THEN it holds the fifteen identifiers, each declaring `json` and the text formats, and the naming
  and description-completeness tests pass. Without the feature it holds none of them
- **AC-3** JSON forms are lossless
  WHEN every scalar in the Phase 3 table is written as JSON and read back
  THEN the bytes equal the table's and the value is equal, including `i128::MIN`, `Decimal`
  `"12.30"` (scale kept), a `DateTime` with nanoseconds, a negative `Duration` and an `F32` NaN
- **AC-4** Scalar hooks read through the base value
  WHEN `try_into_i64`, `try_into_f64`, `try_into_string` are called on `U8(7)`, `F32(1.5)`,
  `U64(u64::MAX)` and `Date(2026-10-26)`
  THEN they give `7`, `1.5`, the digit string and `"2026-10-26"`, and `U64(u64::MAX).try_into_i64`
  is a conversion error
- **AC-5** Conversions are classified
  WHEN the conversion registry is asked about `U8→I64`, `U64→I64`, `F64→F32`, `F32→F64`,
  `Decimal→F64`, `Text→Date`, `DateTime→Date`
  THEN the classes are Widening, Fallible, Lossy, Widening, Lossy, Fallible, Lossy
- **AC-6** Commands can return them
  WHEN a registered command returns `chrono::NaiveDate` through `Value::from`
  THEN the result's metadata has `type_identifier: Date` and the default filename `data.json`
- **AC-7** Feature off
  WHEN `liquers-lib` is built without `ext-scalars` and bytes with identifier `U64` are read
  THEN deserialization refuses with "Unsupported type identifier", and the build has no
  `rust_decimal`
- **AC-8** wasm32
  WHEN `liquers-lib` is checked for `wasm32-unknown-unknown` with `webui,ext-scalars`
  THEN it builds

Non-goals: arithmetic or comparison commands on the new scalars; Python and JavaScript bridge
mappings (follow-up issue); record cells becoming these scalars (a record view stays a view);
`f16`, complex, `char`, `isize`/`usize` (prior art rejected them); core `Value` changes; generic
YAML/CBOR forms (they come with `generic-serde-data-formats`).

## Core Interactions

Value types (`ExtValue` declarations, `TypeInfo`, scalar hooks); the conversion registry (edges);
assets (serialization by identifier). Commands only through `From` impls.

## Crate Placement

`liquers-lib` (`src/value/scalars.rs` and the `define_value_types!` block), as prior art §9
consequence 3 requires. `liquers-core` is unchanged apart from the reviewed bare-name list in its
documentation.

## Design Dependencies

- `value-type-definition-macro`: **requires**. The variants are declared through
  `define_value_types!`. They are never hand-written arms.
- `value-conversion-registry`: **requires**. The edges and classes of AC-5 use its
  `Conversion` / `ConversionClass` and `ValueExtension::conversions`.
- `value-interface-capability-split`: **requires**, through the macro.
- `generic-serde-data-formats`: **overlaps**. Once it lands, the scalars add
  `.with_serde_formats()`, and CBOR then keeps `u64`/`i128` native. Either order works.
- `record-decimal-field-type`: **overlaps**. It shares the decimal decision (question 2). The
  conversion from a record `Decimal` cell to `Decimal` is Fallible above 28 digits.
- `record-timestamp-utc` (complete): input. `DateTime` uses the same UTC-instant meaning.

## Documentation Intent

- Reference: `specs/reference/VALUE_TYPE_SYSTEM.md` §Registered identifiers, bare set
  (question 4).
- Guide: `specs/guides/TYPE_SYSTEM_GUIDE.md` (a scalar declaration as the worked small example);
  `specs/guides/SERIALIZATION_GUIDE.md` (JSON forms of the wide numbers).
- Documents to update: the three above; `CLAUDE.md` §Feature matrix lists `ext-scalars`
  (maintainer-owned, so propose the change in the PR).

## Consolidated Findings

- The macro's `accessor:` key requires an `Arc<T>` payload. Scalars are `Copy`, so they declare
  no accessor. Extraction goes through the scalar hooks, `TryFrom<&ExtValue>` for each Rust type,
  and the conversion registry.
- Each variant's hooks are generic functions over one private trait (`ExtScalar`), so fifteen
  declarations share one implementation of each hook.
- `as_base` returns the widest lossless base value: `I64` when the value fits, otherwise `Text`.
  `F32` becomes `F64`. Decimal, temporal values and UUID become their canonical `Text`.
- No stored data changes. The identifiers are new.
- Order: capability split → macro → conversion registry → this design. Adding
  `generic-serde-data-formats` afterwards is a one-line change per declaration.
- Decisions 1-4 change a contract (wire form, dependency, closed identifier set). Decisions 5-6
  are proposed resolutions.

## References

`specs/issues/VALUE-EXTENDED-SCALAR-TYPES-MISSING.md`; `specs/design/value-type-system/prior-art.md`
§9; `specs/design/value-type-definition-macro/phase2-architecture.md` (DSL keys);
`specs/design/value-conversion-registry/phase2-architecture.md` (interfaces).
