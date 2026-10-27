# Phase 3: Examples & Tests — planner-instruction-names-public

## Overview Table

| # | Kind | Scenario | Covers |
|---|---|---|---|
| 1 | Example | the public list and predicate in Rust | AC-1, AC-5 |
| 2 | Example | template and preset consumers | AC-3, AC-4 |
| 3 | Test | `planner_instructions_match_instruction_predicates` | AC-1 |
| 4 | Test | `is_planner_instruction_rejects_ordinary_names` | AC-5 |
| 5 | Test | `instruction_names_are_reserved_command_names` (existing after the prerequisite) | AC-2 |
| 6 | Test | `instructions_are_not_unknown_commands` | AC-3 |
| 7 | Test | `test_trailing_v_is_skipped` | AC-4 |
| 8 | Check | source search for literal instruction names | AC-2 |

## Example 1: the list and the predicate

```rust
use liquers_core::query::{is_planner_instruction, ActionRequest, PLANNER_INSTRUCTIONS};

assert_eq!(PLANNER_INSTRUCTIONS, ["ns", "q", "v"]);
assert!(is_planner_instruction("v"));
assert!(!is_planner_instruction("value"));
assert!(ActionRequest::new("q".to_string()).is_planner_instruction());
```

## Example 2: consumers

`validate_template("{{$ ns-lib/cmd/v}}", TemplateSyntax::Mustache, Some(&cmr))` with `cmd` in
`lib` reports no unknown command, before and after. `find_next_presets("text-Hello/uppercase/v",
…)` returns the `lowercase` preset after the change. The query was validated with
`liquers-validate --command uppercase --command text`: `Ok`, `v` emits no step.

## Edge and Error Cases

- Case: `V`, `NS`, `Q` are not instructions (names are case-sensitive, as the planner compares
  them). Test 4.
- Empty name `""`: not an instruction. Test 4.
- A template whose action name is a placeholder (`{{$ {{x}}/cmd}}`): not a literal, never compared;
  unchanged.
- Adding `cache` later: `PLANNER_INSTRUCTIONS` grows, and tests 3-4 must then include it (test 3
  iterates the list, so it adapts).

## Test Plan

| Test | Location | Asserts |
|---|---|---|
| `planner_instructions_match_instruction_predicates` (renamed) | `liquers-core/src/query.rs` `tests` | each listed name satisfies exactly one of `is_ns`, `is_q`, `is_v`, and `is_planner_instruction` |
| `is_planner_instruction_rejects_ordinary_names` | `liquers-core/src/query.rs` `tests` | `nsx`, `qq`, `value`, `V`, `""` → `false` |
| `instruction_names_are_reserved_command_names` | `liquers-core/src/command_metadata.rs` `validation_tests` | still green after the rename |
| `instructions_are_not_unknown_commands` | `liquers-template/src/diagnostics.rs` `tests` | `{{$ ns-lib/cmd/v}}` and `{{$ cmd/q}}` give no "unknown command" |
| `test_trailing_v_is_skipped` | `liquers-lib/src/utils.rs` `tests` | the preset after a trailing `v` (added here if `next-presets-follow-planner-rules` has not landed) |

Source check (AC-2):
`grep -nE '== "(ns|q|v)"|Some\("ns"\)|"ns" \| "q"' liquers-core/src liquers-template/src liquers-lib/src/utils.rs`
prints nothing outside `query.rs` and test modules.

Run:

```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib query::tests command_metadata
cargo test -p liquers-template --lib diagnostics
cargo test -p liquers-lib --lib utils::tests
```

Every Phase 1 scenario (AC-1 to AC-5) is cited above, and the Phase 2 rename risk is pinned by the
renamed core test.
