# Phase 3: Examples & Use-cases — data-format vocabulary

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A typo caught offline | validator warning and write-error hint | AC-1, AC-7 |
| 2 | Example | A write-only table format | declaration and quiet recompute | AC-4, AC-5, AC-6 |
| 3 | Test | `liquers_core::data_format::tests::vocabulary_answers` | known / canonical / suggestion | AC-2 |
| 4 | Test | `liquers_core::data_format::tests::refinement_only_on_refinable_base` | `csv:x` known, `json:x` not | AC-2 |
| 5 | Test | `liquers_core::data_format::tests::table_has_no_duplicate_spelling` | every name and alias unique | AC-2 |
| 6 | Test | `liquers_core::value::tests::every_declared_core_format_is_known` | core descriptions | AC-3 |
| 7 | Test | `liquers-lib/tests/value_type_system.rs` `every_declared_lib_format_is_known` | lib descriptions, per build | AC-3 |
| 8 | Test | `liquers_core::type_system::tests::write_only_formats` | builder, `can_read_data_format`, registry | AC-4 |
| 9 | Test | `liquers_core::type_system::tests::type_info_serde_omits_empty_write_only` | serde shape | AC-9 |
| 10 | Test | `liquers-lib/tests/record_typeinfo.rs` `record_view_declares_html_write_only` | `RecordView` declaration | AC-4 |
| 11 | Test | `liquers-lib/tests/record_typeinfo.rs` `every_declared_record_view_format_round_trips_or_is_recorded_as_write_only` (changed) | the round-trip failures equal the declared write-only set | AC-4 |
| 12 | Test | `liquers-lib/tests/write_only_format_reload.rs` `write_only_view_reloads_by_recomputing` | fast track skips, recomputes | AC-5, AC-6 |
| 13 | Test | `liquers-lib/tests/write_only_format_reload.rs` `recovery_read_names_write_only_format` | `get_any_status` error | AC-5 |
| 14 | Test | `liquers-core/tests/type_consistency.rs` `set_rejects_an_unknown_format_with_a_suggestion` | write-path message | AC-7 |
| 15 | Test | `liquers-core/tests/validate_integration.rs` `unknown_filename_format_is_a_warning` | validator warning, exit 0 | AC-1 |
| 16 | Test | existing `vts7.1`-`vts7.4`, `markdown_is_text_only`, `SimpleValue::every_declared_format_round_trips` | unchanged after the constants | AC-8 |

## Example 1: a typo caught offline

```bash
cargo run -p liquers-core --features cli --bin liquers-validate -- -- '-R/data/sales.csv/-/ns-pl/head-5/data.jsonn'
# WARNING  [0] Unknown data format: data format 'jsonn' of filename 'data.jsonn' is not known; did you mean 'json'?
# exit 0
```

The query validates today with status `Ok` (checked with `liquers-validate`: resource
`data/sales.csv`, action `head-5` in namespace `pl`, filename `data.jsonn`). With `data.json` the
result stays `Ok` with no warning. If evaluated anyway, the write path refuses with
"Type 'polars.DataFrame' cannot be serialized as 'jsonn'; supported formats: [csv, …]; 'jsonn' is
not a known data format (did you mean 'json'?)".

## Example 2: a write-only table format

```rust
let info = registry.get("RecordView").unwrap();                // in a test
assert!(info.supports_data_format("html"));                     // writable: data.html is accepted
assert!(!info.can_read_data_format("html"));                    // not readable
assert!(info.can_read_data_format("csv:comma"));
```

Recipe used by tests 12-13 (`recipes.yaml` in an `AsyncMemoryStore`, key `views/table.html`):
query `-R/data/t.csv/-/ns-rec/to_record/table.html` (validated: resource `data/t.csv`, action
`to_record` in namespace `rec`, filename `table.html`). Test 12 seeds `data/t.csv`, evaluates
`-R/views/table.html` in one environment, then builds a second environment over the same store and
evaluates it again: the result is a `RecordView` equal to the first, so the second evaluation
recomputed rather than failing. Test 13 asks the second environment's asset manager for
`get_any_status(views/table.html)` and asserts the error names `html` as write-only, which shows
the degraded branch, not the deserializer, handled the entry (the deserializer's refusal would say
"not supported" from `read_table`).

## Edge and Error Cases

- Case: `JSON` (upper case) is not known (codecs are exact or lower-case themselves) but is
  suggested as `json` — test 3.
- A refinement on a non-refinable base (`json:pretty`) is unknown — test 4.
- `parquet` on `RecordView` is write-only only when `records-parquet` is on and `polars` off —
  test 11 runs in both configurations of the matrix rows `records-parquet` and
  `records-parquet,polars`.
- A type the registry does not know keeps the "not registered" degraded reason, checked before the
  write-only test — existing `degraded_reason` test extended in step 3.
- A filename without an extension produces no warning — test 15.

## Test Plan

- Unit, `liquers-core/src/data_format.rs`: tests 3-5 (AC-2).
- Unit, `liquers-core/src/value.rs`: test 6 (AC-3); existing tests unchanged (AC-8).
- Unit, `liquers-core/src/type_system.rs`: tests 8-9 (AC-4, AC-9).
- Integration, `liquers-core/tests/type_consistency.rs`: test 14 (AC-7), beside `vts8.2`
  `set_rejects_an_unsupported_format`, storing a `Text` with format `jsonn`.
- Unit, `liquers-core/src/assets.rs` `mod tests`: the existing `degraded_reason` assertions gain
  the write-only case (AC-5).
- Integration, `liquers-core/tests/validate_integration.rs`: test 15 (AC-1).
- Integration, `liquers-lib/tests/value_type_system.rs`: test 7 (AC-3).
- Integration, `liquers-lib/tests/record_typeinfo.rs` (`#![cfg(feature = "records")]`): tests
  10-11 (AC-4).
- Integration, new `liquers-lib/tests/write_only_format_reload.rs`
  (`#![cfg(feature = "records")]`, `DefaultEnvironment<Value>`, `AsyncMemoryStore`): tests 12-13
  (AC-5, AC-6).
- Commands: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`;
  `cargo test -p liquers-lib --test record_typeinfo --test value_type_system --test write_only_format_reload`;
  `cargo test -p liquers-lib --no-default-features --features records-parquet --lib --tests`.

## Learning Log

- `liquers-validate` already reports plan `Step::Warning`s; a validator-only warning keeps
  evaluation output unchanged.
- `&str` constants work as match patterns, so the arms can use the vocabulary without guards.
