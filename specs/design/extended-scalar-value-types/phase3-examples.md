# Phase 3: Examples & Use-cases — extended scalar value types

Queries were checked on 2026-10-26 with `./target/debug/liquers-validate --command convert`.
`convert` comes from `value-conversion-registry` and is not registered yet. Both queries plan as
`GetAsset[data, …]`, then one or more `action convert(…)` steps, then a `Filename` where one is
given.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A 64-bit hash stored exactly | Text → `U64`, JSON string | AC-1, AC-5 |
| 2 | Example | A date, then an instant | chained conversion | AC-5 |
| 3 | Test | `liquers_lib::value::scalars::tests::json_forms_match_the_table` | the JSON-form table, including bounds | AC-3 |
| 4 | Test | `liquers_lib::value::scalars::tests::text_forms_round_trip` | `txt` for all fifteen | AC-1, AC-3 |
| 5 | Test | `liquers_lib::value::scalars::tests::json_reads_numbers_for_wide_types` | `U64` from `18446744073709551612` and from the string | AC-1, AC-3 |
| 6 | Test | `liquers_lib::value::scalars::tests::duration_iso_subset` | `PT90.5S`, `-PT1S`, `PT0S`; refuses `P1D` | AC-3 |
| 7 | Test | `liquers_lib::value::scalars::tests::base_forms` | the hook results of AC-4 | AC-4 |
| 8 | Test | `liquers-lib/tests/value_type_system.rs` `ext_scalars_are_registered` (`ext-scalars`) | fifteen identifiers, formats, bare-name rule | AC-2 |
| 9 | Test | `liquers-lib/tests/value_type_system.rs` `ext_scalars_absent_without_feature` (`not(ext-scalars)`) | no identifiers; `U64` bytes refused | AC-7 |
| 10 | Test | `liquers-lib/tests/value_conversion.rs` `ext_scalar_edges_have_their_classes` | the class table | AC-5 |
| 11 | Test | `liquers-lib/tests/value_conversion.rs` `u64_to_i64_out_of_range_is_refused` | Fallible edge error | AC-1, AC-5 |
| 12 | Test | `liquers-lib/tests/ext_scalars.rs` `command_returning_date_has_date_identifier` | `From<NaiveDate>`, metadata | AC-6 |
| 13 | Test | `liquers-lib/tests/ext_scalars.rs` `u64_round_trips_through_the_store` | write, reload, identifier | AC-1 |
| 14 | Check | `bash scripts/check-build-matrix.sh` (new rows, wasm32) | feature off and wasm | AC-7, AC-8 |

## Example 1: a 64-bit hash stored exactly

`data/hash.txt` holds `18446744073709551612`.

```
-R/data/hash.txt/-/convert-U64/hash.json
```

The result has `type_identifier: U64` and `data_format: json`, and its bytes are
`"18446744073709551612"`. Reading the key back gives `U64(18446744073709551612)`. Appending
`/convert-I64` fails with `ConversionError` "U64 18446744073709551612 does not fit I64".

## Example 2: a date, then an instant

`data/when.txt` holds `2026-10-26`.

```
-R/data/when.txt/-/convert-Date/convert-DateTime
```

The result is `DateTime(2026-10-26T00:00:00Z)`. It is stored as `"2026-10-26T00:00:00Z"`, and its
log records `Text → Date (Fallible)` and `Date → DateTime (Widening)`.

## JSON-form table (test 3)

| Value | JSON |
|---|---|
| `I8(-128)`, `U32(4294967295)` | `-128`, `4294967295` |
| `U64(u64::MAX)` | `"18446744073709551615"` |
| `I128(i128::MIN)` | `"-170141183460469231731687303715884105728"` |
| `F32(NaN)`, `F32(1.5)` | `"NaN"`, `1.5` |
| `Decimal(12.30)` | `"12.30"` |
| `Time(10:15:30.5)` | `"10:15:30.5"` |
| `DateTime(2026-10-26T10:00:00.000000001Z)` | `"2026-10-26T10:00:00.000000001Z"` |
| `Duration(-1 s)` | `"-PT1S"` |
| `Uuid` | `"67e55044-10b1-426f-9247-bb680e5fe0c8"` |

## Edge and Error Cases

- `"12.300"` read as `Decimal` keeps scale 3, and `Decimal` equality follows `rust_decimal`, so
  `12.30 == 12.300` but their text forms differ (test 3 asserts the text).
- `U8` from `"256"` is a conversion error that names `U8`.
- A `DateTime` with a non-UTC offset in its input (`+02:00`) is accepted and normalized to UTC.
  The output always has `Z`.
- `try_into_json_value` on `F32(NaN)` is a `ConversionError`, matching `json_f64::to_number`.
- A `Duration` beyond `TimeDelta`'s range is refused on read.

## Test Plan

```bash
cargo test -p liquers-lib --lib value::scalars
cargo test -p liquers-lib --test value_type_system --test value_conversion --test ext_scalars
cargo test -p liquers-lib --no-default-features --lib --tests          # feature off (test 9)
bash scripts/check-build-matrix.sh                                     # test 14
```
