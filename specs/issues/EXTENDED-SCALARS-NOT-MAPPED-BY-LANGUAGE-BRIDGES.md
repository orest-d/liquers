---
id: EXTENDED-SCALARS-NOT-MAPPED-BY-LANGUAGE-BRIDGES
kind: feature
title: Python and JavaScript bridges do not map dates, decimals, wide integers and UUIDs to typed values
status: draft
priority: P3
complexity: M
area: [py, web, lib/value]
design:
created: 2026-10-26
github:
---

## Problem

`liquers-py` and `liquers-web` convert values crossing the language boundary using only the core
scalar set (`None`, `Bool`, `I32`, `I64`, `F64`, `Text`, `Bytes`). A Python `datetime.date`,
`datetime.datetime`, `datetime.timedelta`, `decimal.Decimal`, `uuid.UUID` or an `int` above
`i64::MAX`, and a JavaScript `bigint` or `Date`, cannot arrive as a typed value. Going the other
way, the extended scalars that `specs/design/extended-scalar-value-types/` adds (`U64`, `I128`,
`Decimal`, `Date`, `DateTime`, `Duration`, `Uuid`, …) would have no native form in either language.

## Impact

Once the extended scalars exist, they are usable from Rust commands only. A Python or JavaScript
command still has to pass these values as text. Workaround: text forms (ISO 8601, decimal
strings).

## Expected behaviour

A mapping table per bridge, in both directions: Python `date`/`time`/`datetime`/`timedelta`/
`Decimal`/`UUID`/big `int`, and JavaScript `bigint` (for `U64`/`I128`/`U128`) and `Date` (for
`DateTime`, with the millisecond precision loss stated). Everything else stays as today. Record
the mapping in `PYTHON_INTEGRATION.md` and `LANGUAGE-INTEGRATION_GUIDE.md`. This needs
`extended-scalar-value-types` implemented first.

## Discovery

Split off by the 2026-10-26 bulk design of `VALUE-EXTENDED-SCALAR-TYPES-MISSING`, which keeps
bridge mappings out of scope (Phase 1, question 6) so its design stays `L`.
