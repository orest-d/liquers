# Phase 3: Examples & Use-cases — plan-step-state-metadata

All tests are runnable. Core tests use a probe command `probe(state) -> result` that returns the
input state's `query`, `key`, `filename`, declared `data_format` and `title` as a JSON string, so
an assertion reads what the command received. Queries validated with `liquers-validate`
(`--command a --command b --command probe` for the probe commands): `a/b/probe`,
`-R/data/x.txt/-/probe`, `-R/data/x.txt/-/probe/out.csv`, `-R-bin/data/x.txt/-/probe`,
`a/b/out.csv`, `ns-pl/slice-0-2`, `-R/data/x.csv/-/ns-pl/slice-0-2`,
`-R/data/x.manifest.yaml/-/ns-rec/materialize` — all `Ok`, `encoded` as written.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A diagnostic step before an action | the source issue | AC-1, AC-9 |
| 2 | Example | `a/b/probe`, cut and expanded | the reference rule and its approximation | AC-2, AC-6, AC-11 |
| 3 | Test | `plan_step_state_metadata::pass_through_steps_keep_the_input_metadata` | `Info`/`Warning`/`Error`/`SetCwd`/`Filename` before `probe` | AC-1 |
| 4 | Test | `plan_step_state_metadata::cut_boundary_hands_on_the_predecessor_state` | `probe` input = asset `a/b`'s metadata | AC-2, AC-11 |
| 5 | Test | `plan_step_state_metadata::get_asset_hands_on_the_fetched_metadata` | stored `txt`, key, filename reach `probe` | AC-3 |
| 6 | Test | `plan_step_state_metadata::get_asset_binary_keeps_the_fetched_format` | `-R-bin/…` keeps `data_format` | AC-3 |
| 7 | Test | `plan_step_state_metadata::get_resource_hands_on_the_stored_metadata` | hand-built `[GetResource, probe]` | AC-4 |
| 8 | Test | `plan_step_state_metadata::output_filename_does_not_label_the_input` | `…/probe/out.csv` | AC-5, AC-10 |
| 9 | Test | `plan_step_state_metadata::expanded_plan_approximates_the_cut_plan` | `apply` vs `evaluate` of `a/b/probe` | AC-6, AC-11 |
| 10 | Test | `plan_step_state_metadata::bare_key_read_is_not_cut` | finalized plan shape, key at `probe` | AC-7 |
| 11 | Test | `plan_step_state_metadata::unnamed_query_declares_no_data_format` | asset `a/b`, `a/b/out.csv`, `as_bytes` | AC-8 |
| 12 | Test | `plan_step_state_metadata::asset_record_is_unchanged_by_step_states` | final asset's log, title, filename | AC-10 |
| 13 | Test | `plan_step_state_metadata::legacy_stored_metadata_degrades_to_a_warning` | edge | AC-4 |
| 14 | Test | `plan_step_state_metadata::recipe_with_overrides_records_no_prefix_query` | edge | AC-11 |
| 15 | Test | `liquers_core::plan::tests::action_steps_record_their_prefix_query` | `a`, `a/b`, `a/b/c` | AC-11 |
| 16 | Test | `liquers_core::plan::tests::prefix_queries_are_frozen_with_the_predecessor` | recipe CWD prologue | AC-11 |
| 17 | Test | `liquers_core::plan::tests::alias_step_records_the_alias_query` | alias name kept | AC-11 |
| 18 | Test | `liquers_core::plan::tests::plan_without_action_query_deserializes` | old serialized plan loads | AC-11 |
| 19 | Test | `liquers_core::plan::tests::cut_declines_a_bare_key_read` | `init_info` recorded, `a/b/c` still cut | AC-7 |
| 20 | Test | `liquers_core::recipes::tests::data_format_is_absent_without_a_filename` | `Recipe::data_format`, `get_asset_info` | AC-8 |
| 21 | Test | `liquers_core::recipes::tests::keyed_asset_takes_its_format_from_the_key` | key filename seeds `csv` | AC-8 |
| 22 | Test | `polars_commands::slice_after_info_step_keeps_csv_format` | the issue's failure | AC-1, AC-9 |
| 23 | Test | `polars_commands::slice_over_stored_csv_bytes_reads_the_stored_format` | stored untyped CSV | AC-9 |
| 24 | Test | `record_manifest_resource_key::materialize_plan_is_get_asset_then_action` | evaluated plan has no boundary | AC-7 |
| 25 | Test | `plan_step_state_metadata::applied_plan_is_marked_applied` | applied asset, its `AssetInfo` and `probe`'s input: `is_applied`; evaluated: not | AC-12 |
| 26 | Test | `liquers_core::metadata::tests::is_applied_round_trips_and_defaults` | serde skip/default, `AssetInfo` round trip, legacy JSON | AC-12 |

## Example 1: A diagnostic step before an action

A caller applies `ns-pl/slice-0-2` to a CSV text state whose metadata declares `data_format: csv`,
through a plan with an `Info` step first (the shape the alias planner used to emit).

```rust
let mut plan = PlanBuilder::new(parse_query("ns-pl/slice-0-2")?, cmr).build()?;
plan.steps.insert(0, Step::Info("before".into()));
let context = Context::new(AssetRef::new_temporary(envref.clone()), false).await;
let value = apply_plan(plan, csv_state("a\n1\n2\n3"), context, envref).await?;
```

Today `slice` receives `data_format: bin` and fails with *"Unsupported polars data_format
'bin'"*. After: `Info` hands its input state on, `slice` reads `csv` and returns two rows.

## Example 2: `a/b/probe`, cut and expanded

`a` returns `"A"`, `b` appends `"B"` and calls `context.info("b ran")`, `probe` reports its input.

- **Evaluated** (cut): the plan is `Evaluate(a/b), Action(probe)`. `probe` receives asset `a/b`'s
  state: `query: a/b`, no key, no filename, no `data_format`, status `Ready`, and `a/b`'s log
  (which holds "b ran").
- **Applied** to a non-empty input state (expanded): the plan is `Action(a), Action(b),
  Action(probe)`; `b`'s step records `query: a/b`. `probe` receives the asset's record corrected
  to `query: a/b`, no key, no filename, no `data_format` — the same description as the cut. The
  asset's log holds "b ran" too.

Today both report `query: a/b/probe` (the final asset) and `data_format: bin`.

## Edge and Error Cases

| Symptom | Expected behaviour | Test |
|---|---|---|
| Stored entry with legacy JSON metadata read by `GetResource` | value handed on with `prefix_metadata(None)` + key; one warning in the asset log; no error | 13 |
| Recipe with argument overrides | no action step records a query; `probe` sees `query: None` | 14 |
| Plan serialized before this change (no `query` on actions) | deserializes, `query: None` | 18 |
| Keyed asset whose stored metadata has no key (`set_manifest` in `record_manifest_resource_key.rs`) | `GetAsset` sets the key; manifest chunks stay keyed | existing `materialize_keys_the_chunks_of_a_manifest_fetched_as_a_resource` |
| Volatile prefix that is not a bare key read | walk-back unchanged | existing `predecessor`-cut tests in `plan.rs` |
| Alias in the prefix | query keeps the alias name | 17 |

## Test Plan

- Integration: `liquers-core/tests/plan_step_state_metadata.rs` (new) — tests 3-14 and 25 over
  `SimpleEnvironment<Value>` with `AsyncMemoryStore` and the default recipe provider; stored
  entries use `Text` with declared `txt` (the core `Value` cannot load `csv`).
- Unit: `liquers-core/src/plan.rs` tests 15-19; `liquers-core/src/recipes.rs` tests 20-21;
  `liquers-core/src/metadata.rs` test 26.
- Integration: `liquers-lib/tests/polars_commands.rs` tests 22-23 (file already
  `#![cfg(feature = "polars")]`); `liquers-lib/tests/record_manifest_resource_key.rs` test 24
  (file already `#![cfg(feature = "records")]`).
- Updated, not new: tests asserting `bin` or the boundary shape of `-R/<key>/-/ns-x/…`; the
  `fetched_key_honours_the_resource_header` unit test is deleted with `fetched_key`.
- Commands:
  `cargo test -p liquers-core --lib --tests` ·
  `cargo test -p liquers-lib --lib --tests` ·
  `cargo test -p liquers-lib --no-default-features --features records --lib --tests` ·
  `bash scripts/check-build-matrix.sh` (the egui widget caller).

## Learning Log

- Measured, not assumed: the throwaway probe of Phase 1 showed the output filename labelling the
  input (`…/fmt/out.txt` → `txt`). Test 8 keeps that measurement.
- The core `Value` cannot load a stored entry declared `csv`; core tests use `txt` and leave CSV to
  `liquers-lib`.
- `record_manifest_resource_key.rs`'s `set_manifest` stores no key in the metadata, so the
  `GetAsset` key rule (Phase 2) is load-bearing for the records tests, not a nicety.
- `liquers-lib/tests/resolver_dependency_recording.rs` works around `bin` with a `RecordView`
  fixture; once AC-8 holds the workaround is unnecessary. Removing it is optional and not a step.
