# Phase 4: Implementation Plan — automatic conversion of a command's state and link arguments

## Overview

Prerequisites: `value-conversion-registry` steps through its environment wiring, and
`command-io-type-declarations` steps 1-4, merged. Then one file changes (steps 1-3), tests follow
(step 4), documents last (step 5). Record the decision on Phase 1 question 1 before step 2.

## Progress

- [ ] Step 1: re-check the prerequisite signatures
- [ ] Step 2: `convert_to_declared` and `ConversionSite`
- [ ] Step 3: call it from the `Step::Action` arm
- [ ] Step 4: tests
- [ ] Step 5: documentation and final checks

## Implementation Steps

### Step 1: re-check the prerequisite signatures
- Files / symbols: `liquers-core/src/conversion.rs` (`ConversionRegistry::can_convert`, `convert`,
  `ConversionInput`, `ConversionClass::is_automatic`), `context.rs`
  (`Environment::get_conversion_registry`, `EnvRef`), `type_system.rs` (`TypeSet`),
  `command_metadata.rs` (`ArgumentInfo::accepts`).
- Change: none; adjust Phase 2's sketch if any landed differently, and record it here.
- Depends on: the two prerequisite designs.
- Proof: `cargo check -p liquers-core`.
- Rollback: n/a.

### Step 2: `convert_to_declared` and `ConversionSite`
- Files / symbols: `liquers-core/src/interpreter.rs` — private `ConversionSite` with a `Display`
  (exhaustive `match`), private `convert_to_declared` per Phase 2's algorithm; log through
  `context.info` / `context.warning`.
- Depends on: 1.
- Proof: compiles; exercised by step 4.
- Rollback: delete; unused until step 3.

### Step 3: call it from the `Step::Action` arm
- Files / symbols: `interpreter.rs` `do_step`, `Step::Action`: one `find_command` lookup; convert
  each link-resolved argument state before `arguments.set_value`; convert `input` before
  `ce.execute_async`. `match` on `Option` arms explicitly (no `_ =>`).
- Depends on: 2.
- Proof: `cargo test -p liquers-core` (existing suites unchanged).
- Rollback: revert the arm; behaviour returns to today's.

### Step 4: tests
- Files / symbols: new `liquers-core/tests/automatic_conversion.rs` with tests 3-10 of Phase 3.
- Depends on: 3.
- Proof: `cargo test -p liquers-core --test automatic_conversion`; `cargo test -p liquers-lib --lib
  --tests`; `bash scripts/check-build-matrix.sh` (no `#[cfg]` added, run for the declared lib commands).
- Rollback: n/a.

### Step 5: documentation and final checks
- Files / symbols: Phase 2's three documents, each with History and `reviewed:`; issue
  `VALUE-NOT-CONVERTED-AUTOMATICALLY` status per `DOCS_STRUCTURE_GUIDE.md` §4.3.
- Depends on: 4.
- Proof: `python3 scripts/docs_index.py --check`; diff review (no stdout printing, no stray edits).

## Testing Plan

Focused: `cargo test -p liquers-core --test automatic_conversion`. Crate: `cargo test -p
liquers-core`; `cargo test -p liquers-lib --lib --tests` (the declared library commands now pass
through the helper). Every AC-1…AC-8 has a test in Phase 3.

## Rollback Plan

Revert step 3 alone to switch the feature off; steps 2 and 4 then go with it. Nothing is persisted
by the conversion itself; a converted state that was cached keeps its converted value and metadata,
which is a valid value either way.

## Documentation Updates

`specs/reference/VALUE_TYPE_SYSTEM.md`, `specs/guides/COMMAND_REGISTRATION_GUIDE.md`,
`specs/guides/TYPE_SYSTEM_GUIDE.md` (step 5).

## Phase 5 Entry Criteria

Not applicable (no `workflow` marker). Done when the prerequisites are merged, question 1 is
decided, every proof passes and `docs_index.py --check` is clean.
