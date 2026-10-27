# Phase 4: Implementation Plan — computed enum alternatives

## Overview

Prerequisite: `global-enum-registration` implemented (`add_global_enum`), and the answers to Phase 1
questions 1-2 recorded there. Then metadata (step 1),
planner tolerance (step 2), providers (step 3), the two checks (steps 4-5), validation and listing
(steps 6-7), documents (step 8). Each step compiles and passes on its own.

## Progress

- [ ] Step 1: `EnumProviderScope` and `EnumArgument::computed`
- [ ] Step 2: planner accepts unlisted values of a computed enum
- [ ] Step 3: provider trait, registry storage, `CommandExecutor::enum_provider`
- [ ] Step 4: environment check in `make_plan_with_cwd`
- [ ] Step 5: input check in the `Step::Action` arm
- [ ] Step 6: `enum_provider_issues` in `EnvironmentBuilder::validate`
- [ ] Step 7: `enum_alternatives` listing
- [ ] Step 8: documents and final checks

## Implementation Steps

### Step 1: metadata
- Files / symbols: `liquers-core/src/command_metadata.rs` `EnumProviderScope`,
  `EnumArgument::computed` field and builder, `EnumArgument::new` sets `None`;
  `liquers-macro/src/registration.rs` `to_argument_type_expression` emits `computed: None`.
- Proof: test 10; `cargo test -p liquers-macro`.
- Rollback: revert both files.

### Step 2: planner tolerance
- Files / symbols: `plan.rs` `ParameterValue::from_string`, `ArgumentType::Enum` arm, the
  `CommandParameterValue::None` branch: `if e.others_allowed || e.computed.is_some()`.
- Depends on: step 1. Proof: test 8.

### Step 3: providers
- Files / symbols: new `liquers-core/src/enum_alternatives.rs` (trait, `EnumAlternatives`,
  `EnumProviderFn` closure adapter), `lib.rs` `pub mod`; `commands.rs` `CommandExecutor::
  enum_provider` (default `None`), `CommandRegistry` field `enum_providers`,
  `register_enum_provider` (calls `command_metadata_registry.add_global_enum`), the override.
- Depends on: step 1 and `global-enum-registration`. Proof: a unit test that registers and looks
  up a provider. Rollback: the trait method is defaulted, so removing the override is safe.

### Step 4: environment check
- Files / symbols: `enum_alternatives.rs` `check_environment_enums`; `interpreter.rs`
  `make_plan_with_cwd` calls it after `analyze_plan_dependencies`. Matches on `Step` and
  `ParameterValue` are exhaustive (no `_ =>`).
- Depends on: steps 2-3. Proof: tests 3, 5, 7 (plan half), 11.

### Step 5: input check
- Files / symbols: `enum_alternatives.rs` `check_input_enums`; `interpreter.rs` `Step::Action`
  arm, before `execute_async`; errors pass through the existing `map_err`.
- Depends on: step 3. Proof: tests 4, 5, 7 (execution half).
- Containment: the call is skipped unless the command has an `Input`-computed argument, so
  ordinary actions do one metadata lookup more and nothing else.

### Step 6: builder warning
- Files / symbols: `commands.rs` `enum_provider_issues`; `environment_builder.rs` `validate`.
- Proof: test 9.

### Step 7: listing
- Files / symbols: `enum_alternatives.rs` `enum_alternatives`.
- Proof: test 6.

### Step 8: documents
- `REGISTER_COMMAND_FSD.md`, `COMMAND_DECLARATION.md`, `COMMAND_REGISTRATION_GUIDE.md` with History
  rows and `reviewed:` bumps; remove `// TODO: support for dynamic global enums`; close the issue,
  this design to `complete`; `python3 scripts/docs_index.py` then `--check`.

## Testing Plan

`CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-macro`;
`cargo test -p liquers-lib --lib --tests` (every library still registers and plans);
`cargo test -p liquers-lib --test registry_export` unchanged; `cargo test -p liquers-mcp`;
`bash scripts/check-build-matrix.sh` (wasm32 row: the trait uses `?Send` there).

## Rollback Plan

Steps 4-5 are the only behavioural changes for computed enums and can be reverted alone, leaving
computed enums unchecked (AC-6 behaviour everywhere). With no computed enum registered, the system
behaves as at HEAD after any step.

## Documentation Updates

As step 8. `specs/command_registry.yaml` does not change: no built-in command uses a computed enum.

## Phase 5 Entry Criteria

All tests in Phase 3 pass; the workspace loop and the build matrix are green; Phase 1 questions 1-2
have recorded answers; the `ns-pl` follow-up is filed as an issue if wanted.
