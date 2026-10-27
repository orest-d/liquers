---
id: VALUE-EXTENDED-SCALAR-TYPES-MISSING
kind: feature
title: Value types lack fifteen scalars most target ecosystems represent distinctly
status: draft
priority: P2
complexity: L
area: [lib/value, core/value]
design: extended-scalar-value-types
created: 2026-10-20
github:
---

## Problem

`liquers-core::value::Value` and `liquers-lib`'s `SimpleValue` carry only `None, Bool, I32, I64,
F64, Text, Bytes` among scalars. `specs/design/value-type-system/prior-art.md` §9 established that
`i8, i16, i128, u8, u16, u32, u64, u128, f32, decimal, date, time, datetime, duration, uuid` are
represented distinctly by at least five of the nine target ecosystems, and that they belong in
`liquers-lib::ExtValue` behind features, not in core (§"Consequences", point 3).

## Problem Example

A command that computes a 64-bit hash (`u64::MAX - 3`) can return only `I64`, which fails the
range check, or `Text`. A date computed by a command is stored as `Text` and cannot be told apart
from any other string. Should be: `Value::from(u64)` has identifier `U64`, is stored as
`"18446744073709551612"` in JSON, and reads back as `U64`. A `NaiveDate` is a `Date`.

## Impact

Values from Polars, records, Python and SQL lose their scalar type at the boundary; a `u64` above
`i64::MAX` cannot be represented; dates travel as text.

## Expected behaviour

Declare the fifteen as `ExtValue` variants through `define_value_types!`
(`specs/design/value-type-definition-macro/`), with: bare identifiers (Liquers owns them, a reviewed
change to the reference's bare set), JSON forms that do not lose precision (`u64`/`i128`/`decimal`
as strings or tagged), feature gating (starting proposal `ext-scalars` without dependencies,
`ext-temporal` with `rust_decimal`/`uuid`), scalar hooks reading through the base value, and
conversion edges classified by `value-conversion-registry` (`u8 → i64` widening, `u64 → i64`
fallible). Needs a design.

## Discovery

Designed in `specs/design/extended-scalar-value-types/` (bulk design, 2026-10-26). Python and
JavaScript bridge mappings are split off as `EXTENDED-SCALARS-NOT-MAPPED-BY-LANGUAGE-BRIDGES`.


Moved out of `VALUE-TYPE-DEFINITION-MACRO` by its bulk design (2026-10-20), which delivers the
generator first; the scalars need decisions the generator does not (JSON form, gating, conversion
classes). Requires `value-type-definition-macro` and `value-conversion-registry`.
