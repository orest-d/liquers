# Phase 3: Examples & Use-cases — value-conversion-registry

Queries validated with `liquers-validate --command convert` (2026-10-20): each plans as
`GetAsset[…]` then `action convert(IDENTIFIER)`, and `ns-rec/head-5` after it as `rec/head(n=5)`.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | CSV to a record view, then `ns-rec` | explicit conversion in a query | AC-1, AC-2 |
| 2 | Example | Asking without converting | `can_convert`, `targets_from` | AC-4 |
| 3 | Test | `liquers_core::conversion::tests::scalar_edges_have_their_classes` | the scalar table | AC-6 |
| 4 | Test | `liquers_core::conversion::tests::automatic_policy_refuses_lossy_and_fallible` | policy | AC-5 |
| 5 | Test | `liquers_core::conversion::tests::can_convert_does_not_call_the_edge` | counter stays 0 | AC-4 |
| 6 | Test | `liquers_core::conversion::tests::duplicate_and_self_edges_are_refused` | registration | AC-7 |
| 7 | Test | `liquers-lib/tests/value_conversion.rs` `convert_csv_to_record_view` | end to end | AC-1, AC-2 |
| 8 | Test | `liquers-lib/tests/value_conversion.rs` `convert_text_to_i64_and_back` | scalar through a query | AC-6 |
| 9 | Test | `liquers-lib/tests/value_conversion.rs` `convert_without_edge_names_reachable_targets` | error | AC-3 |
| 10 | Test | `liquers-lib/tests/value_conversion.rs` `extended_registry_edge_is_used` | integration extension | AC-7 |
| 11 | Test | `liquers-lib/tests/value_conversion.rs` `convert_record_view_to_polars_is_lossy` (`polars`) | lossy, explicit | AC-1, AC-5 |

## Example 1: CSV to a record view

Store `data/sales.csv` (`region,amount\nnorth,10\nsouth,20\n`) with `data_format: csv`, then
evaluate:

```
-R/data/sales.csv/-/convert-RecordView/ns-rec/head-5
```

Result: a `RecordView` with 2 rows; the `convert` step's log has
`converted Bytes → RecordView (Fallible)`. `-R/data/sales.csv/-/convert-polars.DataFrame` gives a
2 × 2 frame (feature `polars`).

## Example 2: asking first

```rust
let reg = envref.get_conversion_registry();
assert_eq!(reg.can_convert("RecordView", "polars.DataFrame"), Some(ConversionClass::Lossy));
assert_eq!(reg.can_convert("I32", "I32"), Some(ConversionClass::Identical));
assert_eq!(reg.can_convert("Text", "Image"), None);
let targets: Vec<_> = reg.targets_from("Text").map(|(t, _)| t).collect(); // "Bool", "F64", …
```

## Edge and Error Cases

- **No edge** (`-R/data/n.txt/-/convert-Image`, `n.txt` stored as `Text`): `ConversionError`
  naming `Text`, `Image` and the reachable targets (test 9, AC-3).
- **Edge fails on the value** (`Text "x" → I64`): the parse error, typed `ConversionError` (test 8).
- **Out of range** (`I64(2^40) → I32`): refused, not truncated (test 3, AC-6).
- **Automatic policy** on `I64 → F64`: refused naming `Lossy` (test 4, AC-5).
- **Feature-gated edges**: present only with their feature; `ext_conversions_match_features`
  (part of test 11's file, gated) checks `can_convert("RecordView", "polars.DataFrame")` is `None`
  without `polars`.
- **Duplicate edge** in an extended registry: `register` errors (test 6, AC-7).

## Test Plan

- Unit (`liquers-core/src/conversion.rs`, core `Value`): `scalar_edges_have_their_classes` (AC-6):
  every edge in the table converts a sample and has its class; `automatic_policy_refuses_lossy_and_fallible`
  (AC-5); `can_convert_does_not_call_the_edge` (AC-4): an edge whose closure increments an
  `AtomicUsize`; `duplicate_and_self_edges_are_refused` (AC-7).
- Integration (`liquers-lib/tests/value_conversion.rs`, `DefaultEnvironment<Value>` built with
  `default_environment_builder`, an `AsyncMemoryStore` holding the files, core commands registered):
  `convert_csv_to_record_view` (AC-1, AC-2; `#[cfg(feature = "records")]`),
  `convert_text_to_i64_and_back` (AC-6: `-R/data/n.txt/-/convert-I64` then `…/convert-Text`),
  `convert_without_edge_names_reachable_targets` (AC-3), `extended_registry_edge_is_used` (AC-7: a
  builder with `with_conversion_registry` adding `Text → Bytes`), `convert_record_view_to_polars_is_lossy`
  (AC-1, AC-5; `polars` and `records`).
- Regression: `cargo test -p liquers-lib --test registry_export` after regenerating the registry;
  the existing `ns-rec` and `ns-pl` suites (helpers refactored).
- Commands: `cargo test -p liquers-core conversion`; `cargo test -p liquers-lib --test value_conversion`.

## Learning Log

- The two helpers that convert today (`to_record`, `try_to_polars_dataframe`) both key on the
  identifier and the declared data format — exactly `ConversionInput`.
