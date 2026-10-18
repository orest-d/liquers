---
id: VARIADIC-EMPTY-ELEMENT-CONVERSION
kind: design
title: An empty element of a variadic argument converts as an element, not as the whole list's default
status: complete
gh_pr: [148]
form: compact
readiness: ready
autofix: eligible
area: [core/plan, core/validate]
issues: [VALIDATE-COMMAND-STUB-REJECTS-POSITIONAL-EMPTIES]
created: 2026-10-17
---
# An empty element of a variadic argument converts as an element, not as the whole list's default

> **Complete on 2026-10-18**: implemented and merged in #148.

Produced under [`guides/autonomous_bulk_design.md`](../../guides/autonomous_bulk_design.md) by the
2026-10-17 backlog compaction: Phases 1-4, reviewed without phase approval. Not an approval and not
an implementation.

## Phase 1: High-Level Design

### Purpose

The issue reports it on the `liquers-validate --command` stub, but the cause is in the planner and
affects every `multiple` argument whose type is not `String`. `ParameterValue::pop_value`
(`liquers-core/src/plan.rs`) converts each element with `from_string(arginfo, s, pos)`, passing the
*variadic* argument's `ArgumentInfo`. For an empty `s`, the `Any`, `Integer`, `IntegerOption`,
`Float`, `FloatOption` and `Boolean` arms fall back to `from_arginfo(arginfo)`, which for a
`multiple` argument returns the whole list's default, `MultipleParameters(name, [])`; the element
loop then rejects it with "Multiple parameters not supported inside vector argument".

### Problem Example

```
liquers-validate --command echo_args -- 'echo_args----20'
```

Today: `ERROR [0] Multiple parameters not supported inside vector argument`. Should be: the plan
has one `MultipleParameters` with four elements `"", "", "", "20"` (an `Any` element keeps its
text, as a `String` element already does). A registered `fn f(state, xs: Vec<i64> multiple)` with
`f-1--2` today fails with the same confusing message; it should fail with
`Integer argument xs missing` at the empty element's position.

### Scope and Acceptance Criteria

- **AC-1** Permissive stub accepts empty slots
  - WHEN `echo_args----20` is validated with `--command echo_args`
  - THEN it plans, with four elements, the first three empty strings
- **AC-2** Element fallbacks apply per element
  - WHEN an empty element reaches a variadic of type `Any`, `IntegerOption`/`FloatOption`, or
    `Boolean`
  - THEN it becomes `""`, `null`, or `false` respectively — the fallback a scalar argument of that
    type without a default gets
- **AC-3** A required numeric element reports itself
  - WHEN an empty element reaches a variadic `Integer` or `Float`
  - THEN planning fails with that type's "argument … missing" error at the element's position
- **AC-4** Unchanged cases
  - WHEN a variadic receives no elements, or only non-empty ones, or is `String`
  - THEN the result is as today (`variadic-arguments-declaration` C1: an empty element is still an
    element)

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix in one function of `liquers-core/src/plan.rs`; no `pub`
  signature, command, format or registry change.
- **Leading issue:** None
- **Explanation:** The documented rule (`variadic-arguments-declaration` Phase 3 C1, Phase 5) is
  that an empty element is an element; the element fallbacks are those `from_string` already
  defines for a scalar with no default. The fix applies them, so `CLAUDE.md`'s "accepts any
  arguments" becomes true with no documentation change.
- **Open questions:** None.

### Design Dependencies

None.

### Consolidated Findings

The bug is wider than the issue's title (every non-`String` variadic); priority stays `P3`
because no registered variadic in `specs/command_registry.yaml` is non-`String` (`pl/select_columns`
and `pl/drop_columns` are `string`).

## Phase 2: Architecture

### Solution

In `ParameterValue::pop_value`, inside `if arginfo.multiple`, build the element's argument once
before the loop:

```rust
let mut element = arginfo.clone();
element.multiple = false;
element.default = CommandParameterValue::None;
```

and convert each string element with `Self::from_string(&element, s, pos)`. `element` keeps the
name, type and enum, so errors and `ParameterValue` names are unchanged. Links are untouched.

Rejected: special-casing `MultipleParameters` in the loop to mean "empty element" (types would
disagree on what an empty element is); changing `from_arginfo` (its list default is right for the
no-elements case).

Known-issue preflight: none. Performance: one `ArgumentInfo` clone per variadic argument per
plan, not per element; planning is not the hot path (`parse.rs` is).

### Changes

`liquers-core/src/plan.rs` `ParameterValue::pop_value` only, plus tests.

### Risks

A command with an `Option`/`bool` variadic that relied on the error: none registered. Certainty:
high. Recovery: revert.

## Phase 3: Examples and Tests

### Tests

- `liquers-core/src/validate/mod.rs` `permissive_command_accepts_any_argument_list`: add
  `"greet----20"` and `"greet-a--b-"` to its query list — AC-1
- `liquers-core/src/plan.rs` tests `variadic_empty_element_uses_element_fallback`: `Any`,
  `IntegerOption`, `Boolean` variadics with `cmd-1--` give `[1-or-"1", fallback, fallback]` — AC-2
- `variadic_empty_required_numeric_element_is_missing`: `Integer` variadic, `cmd-1--2` errors with
  "missing" and the second element's position — AC-3
- Existing `recipe_override_reaches_a_variadic_argument` and the `pl/select_columns` tests in
  `liquers-lib` — AC-4

Commands: `cargo test -p liquers-core --lib variadic` and `cargo test -p liquers-core --lib permissive` (Cargo takes one filter per run);
`cargo run -p liquers-core --features cli --bin liquers-validate -- --command echo_args -- 'echo_args----20'` (exit 0).

## Phase 4: Implementation Plan

### Steps

- [x] 1. `plan.rs` `pop_value` element `ArgumentInfo` — `cargo test -p liquers-core --lib`
- [x] 2. The three tests above — `cargo test -p liquers-core --lib variadic` and `cargo test -p liquers-core --lib permissive` (Cargo takes one filter per run)
- [x] 3. Problem Example through the CLI — exit 0; `cargo test -p liquers-lib --lib --tests`
- [x] 4. Close the issue — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`. Rollback: revert the commit.
