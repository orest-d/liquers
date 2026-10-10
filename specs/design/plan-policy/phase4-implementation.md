# Phase 4: Implementation Plan — plan-policy

## Overview

The build goes bottom-up so that each step compiles and is tested on its own:

1. the strategy type;
2. the command flag;
3. the plan (the flag and positional `v`);
4. the recipe field;
5. the manager options;
6. finalisation;
7. the asset's strategy;
8. registration in both managers;
9. the integration suite;
10. full verification.

No issue has to be fixed first (Phase 2 preflight). Documentation updates come in Phase 5. All
paths are in `liquers-core/src/` unless stated.

## Progress

- [x] Step 1: `CacheStrategy` module — b21d865
- [x] Step 2: `CommandMetadata::cached` and the `cached:` macro keyword — 2c23880
- [x] Step 3: Plan — `uncached_by`, the walk's third reason, positional `v`, markers retired — 4cb6a01
- [x] Step 4: `Recipe::cached` widened to a strategy; callers and records provider — bd9df3a
- [x] Step 5: `AssetManagerOptions` keys, trait accessors, `with_policies` — acbba81
- [x] Step 6: `finalize_plan` honours `cut_predecessors` — 13207db
- [x] Step 7: `AssetRef::cache_strategy` and `Context::cache_strategy` — 67d4928
- [x] Step 8: Registration rules in both managers, with metadata and log — be862b2
- [ ] Step 9: Integration suite `tests/cache_strategy.rs`
- [ ] Step 10: Full verification

## Implementation Steps

### Step 1: `CacheStrategy` module
- Files / symbols: new `cache_strategy.rs`; `lib.rs` (`pub mod cache_strategy;`).
- Change: as in Phase 2 Interfaces, namely `CacheStrategy`, `keeps_result`,
  `keeps_intermediates`, `is_all`, `From<bool>`, `Display`, the hand-written `Deserialize`
  (through an untagged helper enum of `bool` / `String`), and `deserialize_optional_strategy`.
  The error names the accepted words. Matches are explicit, with no `_ =>`.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib cache_strategy` (`strategy_keeps_result_and_intermediates`,
  `recipe_cached_bool_and_words_deserialize`, `unknown_strategy_word_is_rejected`).
- Rollback: delete the module; nothing uses it yet.

### Step 2: `CommandMetadata::cached` and the `cached:` macro keyword
- Files / symbols:
  - `command_metadata.rs` `CommandMetadata` (field, `cached()`, `new`/constructors set `None`);
  - `liquers-macro/src/registration.rs` (`CommandSignatureStatement::Cached`, parse arm beside
    `"volatile"`, a `cached` field on the signature, codegen `cm.cached = Some(#b);`);
  - `liquers-core/tests/command_declaration.rs`.
- Depends on: none (parallel with Step 1).
- Proof:
  - `cargo test -p liquers-macro parse_cached_statement`;
  - `cargo test -p liquers-core --test command_declaration cached_false_is_recorded_on_command_metadata`;
  - `command_metadata_ignores_legacy_cache_field` extended (`cached` stays `None`).
- Rollback: revert both files; the field is serde-optional.

### Step 3: Plan — `uncached_by`, the walk's third reason, positional `v`, markers retired
- Files / symbols: `plan.rs`:
  - `Plan::uncached_by` (+ `Plan::new`, `split`, `check_consistent` unaffected);
  - `PlanBuilder` (set after each `Step::Action`, alias aware; cleared in `process_resource_query`
    and the `q` path), `mark_volatile` for `v` → `Positional`;
  - `VolatilitySource::Declared` doc; `cut_predecessor` (reason `candidate.uncached_by`, early
    guard allowing equality when `self.is_volatile`);
  - the three markers become builder documentation.
- Change: Info lines `"Result not cached: command '<key>' declares cached: false"` and
  `"Predecessor boundary expanded at '<q>': its result is not cached (command '<key>')"`.
- Depends on: Step 2.
- Proof: `cargo test -p liquers-core --lib plan::` with the Phase 3 plan tests. The three
  re-expressed tests are `the_v_instruction_is_positional`,
  `a_declared_source_survives_an_earlier_positional_one` (recipe-level) and
  `declared_volatility_declines_before_the_walk`.
- Rollback: revert `plan.rs`. `uncached_by` is serde-optional, so stored plans are unaffected.

### Step 4: `Recipe::cached` widened to a strategy; callers and records provider
- Files / symbols:
  - `recipes.rs` `Recipe::cached` (type, `deserialize_with`), `effective_cache_strategy`, removal
    of `cached()`, `get_asset_info`, tests;
  - `assets.rs` sites reading `recipe.cached` (≈2585 owner check, ≈3625 provider-recipe adoption,
    `ad_hoc_resource_recipe` both managers, `get_resource_asset` both managers). In this step they
    keep today's meaning: `effective(All).keeps_result()`;
  - `liquers-records/src/provider.rs` (two sites, `.into()`) and its tests.
- Depends on: Step 1.
- Proof:
  - `cargo test -p liquers-core --lib recipes`;
  - `cargo test -p liquers-core --test stored_cached_flags` (unchanged: boolean recipes);
  - `cargo test -p liquers-records --all-features --lib --tests`.
- Rollback: revert; the serialized form accepts booleans either way.

### Step 5: `AssetManagerOptions` keys, trait accessors, `with_policies`
- Files / symbols:
  - `environment_builder.rs` `AssetManagerOptions` (three fields, `cut_predecessors()`, three
    `with_*`), `AssetManagerKind::build` for `Queued` and `Inline`;
  - `assets.rs`: the `AssetManager` trait (three provided accessors), both managers'
    `with_policies(&AssetManagerOptions)` and the overriding accessors;
  - `environment_config.rs` tests.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-core --lib environment` (`default_options_cut_predecessors`,
  `cache_strategy_keys_round_trip_and_default`, existing `config_roundtrips_and_applies`).
- Rollback: revert. The defaults equal today's behaviour.

### Step 6: `finalize_plan` honours `cut_predecessors`
- Files / symbols: `interpreter.rs` `finalize_plan`.
- Change: `if !envref.get_asset_manager().cut_predecessors()` → `init_info("Predecessor boundary
  not cut: cut_predecessors is false")`, with no cut.
- Depends on: Steps 3, 5.
- Proof: `cargo test -p liquers-core --lib interpreter` (existing cut/expand equivalence tests).
- Rollback: one `if`.

### Step 7: `AssetRef::cache_strategy` and `Context::cache_strategy`
- Files / symbols: `assets.rs` `impl AssetRef` (reads `data.recipe.cached` and `data.key`; manager
  default by kind); `context.rs` `impl Context` (delegates).
- Depends on: Steps 4, 5.
- Proof: `cargo check -p liquers-core`; exercised by Step 9.
- Rollback: remove two methods.

### Step 8: Registration rules in both managers, with metadata and log
- Files / symbols, `assets.rs`:
  - `DefaultAssetManager`: `get_query_asset`, `get_dependency_asset`, `get_resource_asset`;
  - `ImmediateAssetManager`: `get_query_asset`, `get_resource_asset`, and a new
    `get_dependency_asset` override;
  - a private helper `fn uncached_query_asset(&self, query, strategy) -> AssetRef<E>` per manager
    (fresh, unregistered, recipe with `cached = Some(strategy)`, metadata `cached = Some(false)`,
    and one log entry with the reason).
- Change: the three decisions of Phase 2 (keyed / top-level / dependency). The dependency path
  first calls `lookup_query_asset`, and on a usable hit returns it, as the existing stale-terminal
  loop does. The plan for a non-keyed query is built once, via `make_plan`, and read for both
  `is_volatile` and `uncached_by`.
- Depends on: Steps 3, 4, 5, 7.
- Proof:
  - `cargo test -p liquers-core --test stored_cached_flags --test manager_parametric --test volatility_integration --test plan_cwd_freeze --test dependency_scheduling`
    (unchanged behaviour at defaults);
  - then Step 9.
- Rollback: revert `assets.rs` hunks. Steps 1–7 stand alone at default behaviour.

### Step 9: Integration suite `tests/cache_strategy.rs`
- Files / symbols: new `liquers-core/tests/cache_strategy.rs` with the Phase 3 scenarios, each as
  `_default` / `_immediate`.
- Depends on: Step 8.
- Proof: `cargo test -p liquers-core --test cache_strategy`.
- Rollback: delete the file.

### Step 10: Full verification
- Commands:
  - `cargo test -p liquers-core --lib --tests`;
  - `cargo test -p liquers-macro`;
  - `cargo test -p liquers-records --all-features --lib --tests`;
  - `cargo test -p liquers-lib --lib --tests`, including `registry_export`: no command declares
    `cached`, so no regeneration is expected;
  - `bash scripts/check-build-matrix.sh`, for the wasm row of the immediate manager.
- Depends on: Steps 1–9.
- Proof: all green; failures are fixed in the step that owns them.

## Testing Plan

- Each step runs its own proof before it is ticked.
- Step 8 also reruns the existing asset suites at defaults, which is the AC-11 proof.
- Step 10 is the proportionate final check.
- The build matrix runs once, because a new trait-method override and a module touch the wasm
  build.

## Rollback Plan

- Every step is additive behind defaults that reproduce today's behaviour, so a partial landing
  is safe at any step boundary.
- To roll the whole change back, revert the branch's commits.
- If Step 8 cannot land, Steps 1–7 still deliver the command flag's plan effect (AC-1 plan shape),
  positional `v` and the switch. The remainder becomes an issue (§5.6).

## Documentation Updates

Phase 5 updates every document in Phase 2's Documentation Architecture: `affects_docs` plus
`CLAUDE.md`, each with a `## History` row and a `reviewed:` bump.

- Regenerated: `specs/index.csv` and `specs/index.md`.
- `specs/command_registry.yaml` only if a command declares `cached` (none in this change).
- The two source issues are closed with resolution notes.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
