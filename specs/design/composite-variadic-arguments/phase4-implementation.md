# Phase 4: Implementation Plan — composite-variadic-arguments

## Overview

Core type first, then plan, retrieval and overrides, then the macro, then the consumers that
`match` on `ArgumentType`. Adding the variant breaks every exhaustive match at once, so Step 2 adds
the explicit arms everywhere in one commit (each one a placeholder error or display). Later steps
give them meaning. Prerequisite: decisions on Phase 1 questions 1-3. `variadic-link-array-expansion`
should land first, or Step 5's link test is written against unexpanded elements.

## Progress

- [ ] Step 1: Record the decisions on questions 1-4
- [ ] Step 2: `ArgumentType::Tuple`, `arity`, explicit arms in every match
- [ ] Step 3: `ArgumentInfo::check` rules
- [ ] Step 4: Plan grouping and checks in `pop_value`
- [ ] Step 5: `FromParameterGroup` and `get_multiple_grouped`
- [ ] Step 6: Override conversion in `recipes.rs`
- [ ] Step 7: Macro support and rejections
- [ ] Step 8: MCP schema and display arms
- [ ] Step 9: Integration tests and matrix
- [ ] Step 10: Documents and records

## Implementation Steps

### Step 1: Record the decisions on questions 1-4
- Files / symbols: `phase1-high-level-design.md` Design Readiness
- Change: decisions replace questions. A "not now" on question 1 parks the design `approved`
- Depends on: none
- Proof: review
- Rollback: n/a

### Step 2: `ArgumentType::Tuple`, `arity`, explicit arms in every match
- Files / symbols: `command_metadata.rs` `ArgumentType`, `is_option`, `arity`; arms in `plan.rs`
  `from_string`, `recipes.rs` `convert_scalar` / `argument_type_name`, `command_declaration.rs`,
  `liquers-lib` (`commands.rs`, `search/command_catalog.rs`, `egui/widgets.rs`), `liquers-mcp`
  (`schema.rs`, `server.rs`), `liquers-macro` where it matches
- Change: the variant and arms (errors or display text, no `_ =>`)
- Depends on: Step 1
- Proof: `cargo check -p liquers-core -p liquers-macro -p liquers-mcp --tests`; `cargo check -p
  liquers-lib --tests`; `tuple_argument_type_round_trips`
- Rollback: revert; nothing uses the variant yet

### Step 3: `ArgumentInfo::check` rules
- Files / symbols: `command_metadata.rs` `ArgumentInfo::check`
- Change: `Tuple` needs `multiple`, 2-4 scalar components
- Depends on: Step 2
- Proof: `cargo test -p liquers-core --lib tuple_requires_multiple_and_scalar_components`
- Rollback: revert

### Step 4: Plan grouping and checks in `pop_value`
- Files / symbols: `plan.rs` `ParameterValue::pop_value` (multiple branch)
- Change: per-position element `ArgumentInfo`; trailing incomplete-group error when no link
- Depends on: Step 3
- Proof: `cargo test -p liquers-core --lib tuple_variadic_`; `scalar_variadic_plan_unchanged`
- Rollback: revert; scalar path untouched

### Step 5: `FromParameterGroup` and `get_multiple_grouped`
- Files / symbols: `commands.rs` `FromParameterGroup` (impls for 2-4 tuples),
  `CommandArguments::get_multiple_grouped`
- Change: grouping and run-time remainder error
- Depends on: Step 2
- Proof: `cargo test -p liquers-core --lib get_multiple_grouped`
- Rollback: revert

### Step 6: Override conversion in `recipes.rs`
- Files / symbols: `recipes.rs` the variadic branch of override conversion
- Change: flat or nested arrays, flattened one level, converted per component
- Depends on: Steps 2, 4
- Proof: `cargo test -p liquers-core --lib tuple_variadic_override`
- Rollback: revert

### Step 7: Macro support and rejections
- Files / symbols: `liquers-macro/src/registration.rs` (element type → `ArgumentType`, extractor
  choice, four rejections)
- Change: as Phase 2
- Depends on: Steps 2, 5
- Proof: `cargo test -p liquers-macro variadic_tuple`
- Rollback: revert; existing variadics untouched

### Step 8: MCP schema and display arms
- Files / symbols: `liquers-mcp/src/schema.rs` `type_schema`; display arms of Step 2 given text
- Change: tuple schema; readable labels
- Depends on: Step 2
- Proof: `cargo test -p liquers-mcp tuple_argument_schema`
- Rollback: revert

### Step 9: Integration tests and matrix
- Files / symbols: `liquers-core/tests/composite_variadic.rs`
- Change: Phase 3 integration tests
- Depends on: Steps 4-7
- Proof: `cargo test -p liquers-core --test composite_variadic`; `cargo test -p liquers-lib --lib
  --tests` (`registry_export` unchanged); `bash scripts/check-build-matrix.sh`
- Rollback: delete the file

### Step 10: Documents and records
- Files / symbols: `REGISTER_COMMAND_FSD.md`, `COMMAND_REGISTRATION_GUIDE.md`,
  `COMMAND_DECLARATION.md`, `RECIPES_AND_PLANS.md` (History rows, `reviewed:`); close
  `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS`; this design `complete`
- Change: as listed
- Depends on: Step 9
- Proof: `python3 scripts/docs_index.py` then `--check`
- Rollback: n/a

## Testing Plan

Unit tests per step; the integration file and the build matrix in Step 9 (the `egui` display arm
is feature-gated). The browser and wasm loops are not needed: `liquers-web` reads `ArgumentType`
only through core serde.

## Rollback Plan

Steps 2-8 are separate commits. Reverting Step 2 requires reverting everything after it. Until a
command declares a tuple argument, no observable behaviour depends on any step.

## Documentation Updates

`affects_docs` as in Phase 2. `specs/command_registry.yaml` changes only when a command adopts a
tuple argument; that change then also adds a changelog line.

## Phase 5 Entry Criteria

None — four-phase design.
