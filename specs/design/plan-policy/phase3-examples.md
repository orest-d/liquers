# Phase 3: Examples & Use-cases — plan-policy

All tests are runnable. Counting commands observe what ran, following the pattern of
`liquers-core/tests/stored_cached_flags.rs`: a `tag: String` argument and a
`Mutex<HashMap<String, usize>>`, so parallel tests do not interfere. Integration scenarios are
written once, generically over the environment, and run on both managers. The environments are
built with `EnvironmentBuilder::<Value, (), Queued>` and `<…, Inline>`, with
`with_asset_manager_options`, the way `external_change_integration.rs` sets policies. The test
queries `seed-x/t1-x/t2-x/t3-x` are chains of registered fixture commands; each argument is the
counting tag.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A command that runs inline | `cached: false` moves the boundary | AC-1 |
| 2 | Example | A public service | strategies follow the origin | AC-3, AC-4, AC-6 |
| 3 | Test | `cache_strategy::tests::*` (unit) | serde forms, decisions | AC-5, AC-11 |
| 4 | Test | `plan::tests::*` (unit) | `uncached_by`, the walk, positional `v`, markers | AC-1, AC-9, AC-12 |
| 5 | Test | `tests/cache_strategy.rs` (integration, both managers) | registration, reuse, origin, metadata | AC-1–AC-8, AC-10 |
| 6 | Test | config, recipe and command-metadata round trips | old documents load unchanged | AC-5, AC-11 |
| 7 | Test | macro parse and a registered command | `cached:` keyword | AC-1, AC-11 |

## Example 1: A command that runs inline

`t2` is cheap and its output is large, so it is registered with `cached: false`:

```rust
register_command!(cr, fn t2(state, tag: String) -> result cached: false)?;
```

`PlanBuilder` sets `uncached_by` on each candidate plan whose last action is `t2`. Evaluating
`seed-x/t1-x/t2-x/t3-x` walks back past `seed-x/t1-x/t2-x` ("its result is not cached: command
't2' declares cached: false") and cuts at `seed-x/t1-x`:

```text
Evaluate(seed-x/t1-x)  Action(t2)  Action(t3)
```

A second evaluation of a different tail, `seed-x/t1-x/t2-x/t4-x`, runs `seed` and `t1` zero more
times and `t2` once more. `seed-x/t1-x` is in `query_assets`; `seed-x/t1-x/t2-x` never is.

## Example 2: A public service

```yaml
assets:
  recipe_cache_strategy: all     # approved recipes cache results and intermediates
  query_cache_strategy: none     # guests' ad-hoc queries add nothing
```

1. The recipe `report.txt` (query `seed-y/t1-y/t3-y`) is evaluated. Its keyed asset has strategy
   `all` (from the default), so its boundary `seed-y/t1-y` is built with
   `recipe.cached = Some(All)` and is registered. That boundary's own boundary `seed-y` is
   registered too.
2. A guest evaluates `seed-y/t1-y/t4-y`. Its asset is top-level with strategy `none`, so it is not
   registered (`cached: Some(false)`, log: "not cached for reuse: query strategy 'none'").
3. Its boundary `Evaluate(seed-y/t1-y)` finds the recipe's asset and reuses it, so `seed` and
   `t1` do not run again.
4. A guest evaluates `seed-z/t1-z/t4-z`. Nothing is cached: each command runs once, and nothing
   new is left in `query_assets`.

## Edge and Error Cases

| Symptom risked | Expected | Test |
|---|---|---|
| A recipe with boolean `cached: false` (old file) | Loads as `none` | `recipe_cached_bool_and_words_deserialize` |
| `cached: default` in a recipe | `None`, i.e. the manager default | `recipe_cached_bool_and_words_deserialize` |
| `cached: some` | Parse error naming `none`, `result`, `all` | `unknown_strategy_word_is_rejected` |
| Old command metadata with legacy `"cache": false` | Ignored; `cached` stays `None` | existing `command_metadata_ignores_legacy_cache_field`, extended |
| `AssetManagerOptions::default()` built in code | Cuts predecessors (the `Default` trap) | `default_options_cut_predecessors` |
| `a/b/v` (empty tail) | `[Evaluate(a/b)]`; result volatile | `trailing_v_cuts_the_whole_prefix` |
| A link ending with an uncached command | Not registered, whatever the strategy | `uncached_link_is_not_registered` |
| A plan applied to an input state | Still not cut (unchanged rule) | existing `finalize_plan` input-state tests |
| Volatile prefix under `all` | Never registered (unchanged) | existing volatility tests |
| `ImmediateAssetManager` dependency path | Same decisions as queued | every integration test runs on both managers |

## Test Plan

**Unit, `liquers-core/src/cache_strategy.rs`:**
- `strategy_keeps_result_and_intermediates` (AC-5): truth table of `keeps_result` and
  `keeps_intermediates` over the three values.
- `recipe_cached_bool_and_words_deserialize` (AC-5, AC-11): YAML/JSON forms `true`, `false`,
  `none`, `result`, `all` and `default`, plus an absent field, map to the expected `Option`.
  Serialising emits the word.
- `unknown_strategy_word_is_rejected` (AC-5).

**Unit, `liquers-core/src/plan.rs`:**
- `uncached_command_sets_uncached_by_and_is_not_contagious` (AC-1): `a/u` → `Some(u)`; `u/a` →
  `None`; `a/u/out.txt` → `Some(u)`; `a/u/q` → `None`.
- `walk_steps_back_past_an_uncached_candidate` (AC-1): `a/b/u/d` cuts at `a/b`, with the Info line.
- `the_v_instruction_is_positional` (AC-9): replaces `the_v_instruction_is_declared`. `v` at any
  position gives `Positional`; recipe `volatile: true` gives `Declared`.
- `positional_v_cuts_before_itself` (AC-9): `a/b/v/c` → `Evaluate(a/b) c`; `v/a/b/c` → no cut.
- `trailing_v_cuts_the_whole_prefix` (AC-9): `a/b/v` → `[Evaluate(a/b)]`, and `is_volatile`.
- `declared_volatility_declines_before_the_walk`, re-expressed with a recipe's `volatile: true`
  (AC-9).
- `builder_policy_markers_are_retired` (AC-12): `include_str!("plan.rs")` contains none of the
  three `TODO: support` lines.

**Unit, `liquers-core/src/environment_builder.rs` / `environment_config.rs`:**
- `default_options_cut_predecessors` (AC-8, AC-11): `AssetManagerOptions::default()` gives
  `cut_predecessors()`.
- `cache_strategy_keys_round_trip_and_default` (AC-11): the three `assets:` keys parse, are absent
  when serialised at their defaults, and an empty document gives `all` / `all` / `true`.

**Unit, `liquers-macro/src/registration.rs`:** `parse_cached_statement` (AC-1): `cached: false`
parses to `Cached(false)`; `cached: "x"` is an error.

**Integration, `liquers-core/tests/command_declaration.rs`:**
- `cached_false_is_recorded_on_command_metadata` (AC-1, AC-11): `cm.cached == Some(false)` and
  `cm.cached()` is false; an undeclared command has `None`.

**Integration, `liquers-core/tests/cache_strategy.rs` (new):** each test is `<name>_default` and
`<name>_immediate`, calling one generic scenario.
- `uncached_command_runs_inline` (AC-1): Example 1, with counts and `query_assets` membership via
  `lookup_query_asset`.
- `uncached_command_ending_an_adhoc_query_is_not_reused` (AC-2): two requests, two runs, not
  registered, `is_volatile() == false`.
- `query_strategy_result_keeps_only_the_result` (AC-3): no boundary registered, the result
  registered.
- `query_strategy_none_keeps_nothing` (AC-3): neither.
- `existing_intermediate_is_reused_and_plan_unchanged` (AC-4): under query strategy `none`, with
  and without `seed/t1` pre-cached by first evaluating a keyed recipe `seed/t1/t3` (strategy
  `all`). The same plan steps both times, `t1` runs 0 or 1 times respectively, and no
  new entry is registered.
- `recipe_strategy_values` (AC-5): keyed recipes with `result`, `none`, `all`, `default` and
  absent, under `recipe_cache_strategy: result`. Result and boundary registration match the table.
- `boundary_follows_its_creator` (AC-6): Example 2.
- `keyed_result_ignores_the_command_flag` (AC-7): a recipe ending in an uncached command, under
  `all`, is registered.
- `cut_predecessors_false_expands_and_reuses_nothing` (AC-8): a pre-cached prefix runs again, the
  plan has no `Evaluate`, and the result is equal.
- `unregistered_result_is_visible_in_metadata` (AC-10): `MetadataRecord::cached == Some(false)`
  and `AssetInfo::cached == Some(false)`, with a log entry naming the reason, for AC-2 and AC-3.
- `uncached_link_is_not_registered` (AC-2): a link parameter whose query ends in an uncached
  command.

**Existing suites, unchanged, as the AC-11 proof:**
- `stored_cached_flags.rs`;
- `plan_cwd_freeze.rs` (including E16, where cut and expanded results are equal);
- `volatility_integration.rs`;
- `manager_parametric.rs`;
- the records provider tests.

Commands to run:

```bash
cargo test -p liquers-core --lib --tests
cargo test -p liquers-macro
cargo test -p liquers-records --all-features --lib --tests
cargo test -p liquers-lib --lib --tests
bash scripts/check-build-matrix.sh
```

## Learning Log

- A boundary's own plan is cut again when it is evaluated, so every prefix is a boundary. Every
  "intermediate" rule therefore applies recursively without extra code. Measured with counters
  (`DESIGN.md` notes).
- `AssetManagerOptions` and `CommandMetadata` both derive `Default`. Any "default true" flag on
  them must be an `Option` read through an accessor. `default_options_cut_predecessors` pins this.
- The plan stays a function of query, metadata and configuration. Reuse is decided where
  `Step::Evaluate` executes (`get_dependency_asset`), which already has the parent asset in hand.
