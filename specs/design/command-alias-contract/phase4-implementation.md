# Phase 4: Implementation Plan — Command alias contract

## Overview

Core first, bottom up: errors, the step field, validation, planning, the scan and interpreter, then
registration; then the end-to-end tests; then the production alias in liquers-lib and the
regenerated registry; documentation last, written against the passing tests. No prerequisite issue.
Steps 1-2 are independent of each other; everything after depends on both.

## Progress

- [ ] Step 1: Typed alias errors in `error.rs`
- [ ] Step 2: `ActionOrigin` and the `origin` field on `Step::Action`
- [ ] Step 3: `CommandMetadataRegistry::alias_target`
- [ ] Step 4: Alias planning: `from_alias_action` and the `PlanBuilder` alias arm
- [ ] Step 5: Dependency scan and interpreter read `origin`
- [ ] Step 6: `CommandRegistry::register_alias`
- [ ] Step 7: End-to-end tests `liquers-core/tests/command_alias.rs`
- [ ] Step 8: `pl/head` becomes an alias; regenerate the registry
- [ ] Step 9: Documentation
- [ ] Step 10: Final validation

## Implementation Steps

### Step 1: Typed alias errors in `error.rs`
- Files / symbols: `liquers-core/src/error.rs` `impl Error`
- Change: add `alias_target_not_registered(alias: &CommandKey, target: &CommandKey)`
  (`ActionNotRegistered`), `alias_chain_not_supported(alias, target)` (`NotSupported`),
  `invalid_alias(alias, target, problem: &str)` (`ParameterError`), and
  `with_alias(self, alias: &CommandKey) -> Self`, which appends `" (via alias '<key>')"` unless the
  message already ends with it. Each message names both keys.
- Depends on: none
- Proof: `cargo test -p liquers-core --lib error` (unit tests for the four, including `with_alias`
  applied twice)
- Rollback: delete the functions; nothing else uses them yet

### Step 2: `ActionOrigin` and the `origin` field on `Step::Action`
- Files / symbols: `liquers-core/src/plan.rs` `ActionOrigin`, `Step::Action`; every construction
  and full-field pattern: `plan.rs` (builder, scan, tests), `interpreter.rs` (CWD rewrite in the
  step-resolve function, `apply_step`, `impl IsVolatile for Step`, `impl RequiresPayload for
  Step`, tests), `recipes.rs` tests, `validate/report.rs`, `liquers-core/tests/*.rs`,
  `liquers-lib/tests/plan_namespace_resolution.rs`
- Change: the enum and field exactly as Phase 2; every existing construction passes
  `ActionOrigin::Direct`; the CWD rewrite carries `origin` over
- Depends on: none
- Proof: `cargo check -p liquers-core --all-targets && cargo check -p liquers-lib --tests`;
  `plan::tests::action_origin_serialization` (Phase 3 test 17)
- Rollback: revert the commit; the field is additive

### Step 3: `CommandMetadataRegistry::alias_target`
- Files / symbols: `liquers-core/src/command_metadata.rs` `CommandMetadataRegistry::alias_target`
- Change: as Phase 2; returns `Ok(None)` for `Registered`, matches `CommandDefinition` explicitly
- Depends on: Step 1
- Proof: `cargo test -p liquers-core --lib alias_target` (five tests, Phase 3 test 13)
- Rollback: delete the function

### Step 4: Alias planning
- Files / symbols: `liquers-core/src/plan.rs` `ResolvedParameterValues::{from_action,
  from_alias_action}` (remove `from_action_extended`), `PlanBuilder::process_action`
- Change: extract today's loop and excess check into a private `resolve_own`; `from_alias_action`
  builds head values named by `target.arguments[i]` then appends `resolve_own(action, alias)`.
  The alias arm calls `alias_target`, runs the volatility / payload / expiration checks for the
  target key, resolves, and pushes the action with `origin: ActionOrigin::Alias`. Update the stale
  comment in the `v` branch that names `from_action_extended`.
- Depends on: Steps 2, 3
- Proof: `cargo test -p liquers-core --lib plan::tests` — tests 14, 15 and the rewritten 16 pass,
  and no existing plan test regresses
- Rollback: revert; the Registered arm is untouched

### Step 5: Dependency scan and interpreter read `origin`
- Files / symbols: `plan.rs` `scan_plan` (action arm); `interpreter.rs` `apply_step` error mapping,
  `impl IsVolatile for Step`, `impl RequiresPayload for Step`
- Change: scan inserts `for_command_metadata(alias)`; the error mapping calls `with_alias`; both
  impls also consult the alias's metadata (`volatile`, `payload_required`)
- Depends on: Steps 1, 2
- Proof: `cargo test -p liquers-core --lib` (whole suite)
- Rollback: revert; each change is local to its function

### Step 6: `CommandRegistry::register_alias`
- Files / symbols: `liquers-core/src/commands.rs` `CommandRegistry::register_alias`
- Change: as Phase 2. Builds the metadata from the target, calls `alias_target` on it before
  storing, checks the shape, stores with `add_command`, returns `get_mut`'s result mapped to an
  error rather than unwrapped
- Depends on: Steps 1, 3
- Proof: `cargo check -p liquers-core`; exercised by Step 7
- Rollback: delete the method

### Step 7: End-to-end tests
- Files / symbols: new `liquers-core/tests/command_alias.rs`
- Change: Phase 3 tests 3-12 with the environment described there. If `items: Vec<String> multiple`
  does not compile in the DSL, use the spelling `pl/select_columns` uses
- Depends on: Steps 4-6
- Proof: `cargo test -p liquers-core --test command_alias`
- Rollback: delete the file

### Step 8: `pl/head` becomes an alias
- Files / symbols: `liquers-lib/src/polars/selection.rs` `head`, `register_polars_selection_commands!`;
  `liquers-lib/tests/polars_commands.rs`; `liquers-lib/tests/registry_export.rs`;
  `specs/command_registry.yaml`
- Change: as Phase 3 Example 1; add tests 18 and 19; regenerate the registry with
  `cargo run -p liquers-lib --features cli --bin export-command-registry -- --format yaml -o
  specs/command_registry.yaml` and add a CHANGELOG line
- Depends on: Step 6
- Proof: `cargo test -p liquers-lib --test polars_commands --test registry_export`;
  `liquers-validate -- 'ns-pl/head' 'ns-pl/head-10'` now plans `slice` with two parameters
- Rollback: restore `head` and its `register_command!`, regenerate

### Step 9: Documentation
- Files / symbols: see Documentation Updates
- Depends on: Steps 1-8 (documents describe tested behaviour)
- Proof: `python3 scripts/docs_index.py --check`; every query in the new text through
  `liquers-validate`
- Rollback: revert the documentation commit

### Step 10: Final validation
- Proof: `cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
  `cargo test -p liquers-lib --no-default-features --lib --tests` (polars off: tests 18-19 compile
  out, the registry still builds); `cargo check -p liquers-py` if the toolchain has Python headers,
  else record it as not run
- Rollback: n/a

## Testing Plan

Each step's proof runs when the step lands. Full suites run at Step 10. No `cfg(feature)` or
optional dependency is added — the alias registration sits inside the already polars-gated macro —
so the full `scripts/check-build-matrix.sh` is replaced by the two liquers-lib configurations above,
which bracket the one gate involved.

## Rollback Plan

Steps 1-7 are additive in liquers-core except the removal of `from_action_extended`; reverting
them together restores today's behaviour. If Step 8 has not landed, liquers-lib is unaffected:
`pl/head` stays a registered command. If Step 8 is reverted alone, regenerate the registry.

## Documentation Updates

- New `specs/reference/COMMAND_ALIASES.md` (front matter per `DOCS_STRUCTURE_GUIDE.md`; contract,
  validation table, planning and dependencies, errors, History row).
- `specs/guides/COMMAND_REGISTRATION_GUIDE.md` and `specs/guides/COMMAND_DESIGN_GUIDE.md`: the
  sections in Phase 2's documentation architecture.
- `specs/reference/COMMAND_DECLARATION.md` §4.1, `specs/reference/POLARS_COMMAND_LIBRARY.md`,
  `specs/reference/api/DOC_08_RECIPES_PLANS.md`.
- Each changed reference or guide: a `## History` row and a `reviewed:` bump (§9.2).
- `specs/README.md`: *Command aliases* line points to the reference once the design completes.
- Regenerated: `specs/command_registry.yaml` (Step 8), `specs/index.csv` / `index.md`.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
