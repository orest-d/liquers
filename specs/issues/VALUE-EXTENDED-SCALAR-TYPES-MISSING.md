---
id: VALUE-EXTENDED-SCALAR-TYPES-MISSING
kind: feature
title: Value types lack fifteen scalars most target ecosystems represent distinctly
status: draft
priority: P2
complexity: L
area: [lib/value, core/value]
design:
created: 2026-10-20
github:
---

## Problem

`liquers-core::value::Value` and `liquers-lib`'s `SimpleValue` carry only `None, Bool, I32, I64,
F64, Text, Bytes` among scalars. `specs/design/value-type-system/prior-art.md` §9 established that
`i8, i16, i128, u8, u16, u32, u64, u128, f32, decimal, date, time, datetime, duration, uuid` are
represented distinctly by at least five of the nine target ecosystems, and that they belong in
`liquers-lib::ExtValue` behind features, not in core (§"Consequences", point 3).

Example: a Polars `u64` column cell or a Python `datetime` crosses into Liquers as `I64` or `Text`,
losing its type.

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

Moved out of `VALUE-TYPE-DEFINITION-MACRO` by its bulk design (2026-10-20), which delivers the
generator first; the scalars need decisions the generator does not (JSON form, gating, conversion
classes). Requires `value-type-definition-macro` and `value-conversion-registry`.
