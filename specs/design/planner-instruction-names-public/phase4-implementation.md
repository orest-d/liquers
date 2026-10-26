# Phase 4: Implementation Plan — planner-instruction-names-public

## Overview

Four code steps in dependency order (prerequisite, core, template, lib), then documents and records.
Each step has a proof command. The three crates land in one commit, so a revert is atomic.

## Progress

- [ ] 1. Prerequisite: `reserved-instruction-command-names` is merged
- [ ] 2. `liquers-core` public list and predicates
- [ ] 3. `liquers-template` uses them
- [ ] 4. `liquers-lib` uses them
- [ ] 5. Documents
- [ ] 6. Records

## Implementation Steps

1. **Prerequisite.** Confirm `liquers-core/src/query.rs` has `RESERVED_ACTION_NAMES` and
   `ActionRequest::is_v`. If not, carry out `reserved-instruction-command-names` Phase 4 steps 1-3
   exactly as written there. Proof: `cargo test -p liquers-core --lib reserved`.
2. **`liquers-core/src/query.rs`, `command_metadata.rs`.** Make `NS_INSTRUCTION`, `Q_INSTRUCTION`,
   `V_INSTRUCTION` `pub` with rustdoc. Rename `RESERVED_ACTION_NAMES` to `pub const
   PLANNER_INSTRUCTIONS`. Add `pub fn is_planner_instruction`. Make `ActionRequest::is_v` `pub` and
   add `ActionRequest::is_planner_instruction`. Update `CommandMetadata::check`. Rename the query test
   and add `is_planner_instruction_rejects_ordinary_names`. Depends on 1. Proof:
   `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib query::tests command_metadata`.
3. **`liquers-template/src/diagnostics.rs`.** Delete `is_instruction`. Call
   `liquers_core::query::is_planner_instruction` in `unknown_commands` and `template_commands`, and
   use `NS_INSTRUCTION` in `template_commands`. Add `instructions_are_not_unknown_commands`. Depends
   on 2. Proof: `cargo test -p liquers-template --lib`.
4. **`liquers-lib/src/utils.rs`.** Use `!a.is_planner_instruction()` in `find_next_presets`, in place
   of the private helper or `!a.is_ns() && !a.is_q()`. Add `test_trailing_v_is_skipped` if absent.
   Depends on 2. Proof: `cargo test -p liquers-lib --lib utils::tests`.

## Testing Plan

- `cargo fmt --check -p liquers-core -p liquers-template -p liquers-lib`
- `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`
- `cargo test -p liquers-template`
- `cargo test -p liquers-lib --lib --tests`
- `bash scripts/check-build-matrix.sh` (template and minimal-feature rows; `utils.rs` builds in
  every configuration)
- The Phase 3 source search prints nothing outside `query.rs` and test modules.
- Final diff review: four source files, three documents, the records; no debug output;
  `specs/command_registry.yaml` unchanged.

## Rollback Plan

Revert the commit. Steps 2-4 change three crates together, and a partial revert would leave
`liquers-template` or `liquers-lib` calling a missing item. Nothing is persisted, so there is no data
to migrate.

## Documentation Updates

5. `RECIPES_AND_PLANS.md` §Planner instructions (the public list and predicate);
   `DOC_02_QUERY_LANGUAGE_REFERENCE.md` §Planner instructions (one sentence); `TEMPLATE_LANGUAGE.md`
   diagnostics paragraph. History rows and `reviewed:` bumps. Proof: `python3 scripts/docs_index.py
   --check`.
6. Close `PLANNER-INSTRUCTION-NAMES-DUPLICATED` with a resolution note. If step 4 fixed the
   trailing-`v` part before `next-presets-follow-planner-rules` landed, add a line to that issue. Move
   this design to `complete`. Proof: `python3 scripts/docs_index.py` then `--check`.

## Phase 5 Entry Criteria

None: this is a four-phase bulk design. Implementation starts only after the Phase 1 decision is
answered and the prerequisite design has landed.
