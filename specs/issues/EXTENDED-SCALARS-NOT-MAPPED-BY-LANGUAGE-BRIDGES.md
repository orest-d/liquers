---
id: EXTENDED-SCALARS-NOT-MAPPED-BY-LANGUAGE-BRIDGES
kind: feature
title: Python and JavaScript bridges do not map dates, decimals, wide integers and UUIDs to typed values
status: draft
priority: P3
complexity: M
area: [py, web, lib/value]
design: language-bridge-extended-scalars
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

## Problem Example

```python
@env.first_command(volatile=True)
def emit(n: int = 0):
    return [datetime.date(2026, 10, 26), 2**63][n]

env.evaluate("emit-0/to_text")   # today: ConversionError, the date is an opaque py.Object
env.evaluate("emit-1")           # today: ConversionError, 2**63 is outside i64
# expected once the extended scalars exist: "2026-10-26" (a Date), and 9223372036854775808 (a U64)
```

Checked at HEAD (2026-10-27): `liquers-py/src/value.rs` `py_to_simple` refuses `2**63` and
`ext_to_py` refuses every extended variant but `Foreign`, `PolarsDataFrame` and `Image`;
`liquers-web/src/bridge.rs` refuses a `Date` and a `BigInt` outside `i64`.

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
