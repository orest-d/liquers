---
id: VARIADIC-ARGUMENT-SLOTS
kind: design
title: Which action parameters belong to which argument, and list edits of a variadic argument
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/query, core/plan]
issues: [UI-VARIADIC-ARGUMENT-LIST-EDITOR]
affects_docs: [specs/reference/RECIPES_AND_PLANS.md]
created: 2026-10-21
---
# Which action parameters belong to which argument, and list edits of a variadic argument

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written on 2026-10-21, not
approved, nothing implemented. It designs the reusable core of the source issue (its fix-direction
steps 1 and 2). The list widget (step 3) has no in-tree parameter editor to live in and is filed as
`NO-COMMAND-PARAMETER-EDITOR`.

## Phase 1: High-Level Design

### Purpose

An editor of a query's action (a GUI form, a MIDI mapping, an agent tool) must know which written
parameters belong to which command argument, and must add, remove and reorder the elements of a
`multiple` argument without corrupting the arguments around it.

### Problem Example

Verified at HEAD (`8ecfd7f`): no in-tree code reads `ArgumentInfo::gui_info` (`grep` over
`liquers-lib`, `liquers-web`, `liquers-axum`, `liquers-py`); the only argument↔parameter mapping is
the private walk inside `ResolvedParameterValues::from_action` / `ParameterValue::pop_value`
(`liquers-core/src/plan.rs`), which yields values, not parameter positions.
`ActionRequest::parameters` is a `pub Vec`, so raw insert/remove exist; what is missing is doing
them at the right position.

Concrete case (validated with `liquers-validate`): `search/field_extract(extension = "md",
prefix = "", flatten = false, fields: multiple)`. The user's query is
`-R-key/specs/issues/-/ns-search/field_extract`, and the editor adds `status` to `fields`. A naive
append writes `…/field_extract-status`, which fails: `Can't convert 'status' to extension`. The
correct edit is `…/field_extract-md--false-status`: the omitted leading arguments are written out
with their defaults first. Likewise, for `ns-pl/is_in-name-a-b`, moving element 1 of `values` to
position 0 must give `ns-pl/is_in-name-b-a`, leaving `column` alone.

### Scope and Acceptance Criteria

- **AC-1** Slots match resolution
  WHEN `argument_slots` maps an action against its command metadata
  THEN each argument's parameter range holds exactly the parameters `ResolvedParameterValues::from_action`
  assigns to it (injected: empty; scalar: zero or one; multiple: the rest), and surplus parameters
  are reported as `excess`
- **AC-2** Append pads omitted arguments
  WHEN an element is inserted into `fields` of `field_extract` with no written parameters
  THEN the action encodes as `field_extract-md--false-status` and plans without error
- **AC-3** Reorder and remove stay inside the argument
  WHEN element 1 of `values` in `is_in-name-a-b` is moved to 0, or removed
  THEN the action is `is_in-name-b-a`, or `is_in-name-a`
- **AC-4** Refused edits
  WHEN the target argument is not `multiple`, an index is out of range, or an omitted earlier
  argument has no default
  THEN the edit returns an error and the action is unchanged
- **AC-5** Links are elements too
  WHEN a variadic element is a link parameter (`~X~…~E`)
  THEN it is moved and removed like a string element

Non-goals: rendering the list (`NO-COMMAND-PARAMETER-EDITOR`); editing alias head parameters
(an alias's own metadata is what the user writes, so slots are computed against it); recipe
argument overrides (`Recipe::arguments`).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — new `pub` structs and functions in `liquers-core` (rules 3, 4).
- **Leading issue:** **Proposed resolution - padding:** inserting into a variadic argument whose
  preceding arguments are omitted writes those arguments' defaults explicitly (AC-2), and refuses
  when one has none (AC-4). Alternative: refuse every such insert, leaving editors to pad.
  Recommended: pad, since only core knows the default's text form.
- **Explanation:** the rules are those of `pop_value`, which is already tested; the design exposes
  them as positions and adds three checked edits. Scope is `liquers-core` only.
- **Open questions:**
  1. **Proposed resolution - padding:** as the leading issue.
  2. **Proposed resolution - location:** `liquers-core/src/plan.rs`, beside `from_action`, so the
     two rules sit together; `from_action` is not rewritten (AC-1 test guards their agreement).
  3. **Proposed resolution - default text:** a `CommandParameterValue::Value` default is written
     with `ActionParameter::new_string` of its scalar text; a `Query` default as a link parameter.
  4. **Proposed resolution - issue scope:** the source issue's renderer step moves to
     `NO-COMMAND-PARAMETER-EDITOR`; the issue closes when this design ships.

### Design Dependencies

- `variadic-arguments-declaration` (complete): origin; frozen.
- `query-builder` (in review): **overlaps** — builds queries from scratch; no shared symbol.
- `NO-COMMAND-PARAMETER-EDITOR` (issue, no design): **required-by**.

### Consolidated Findings

- The issue's obstacle 2 ("insert/remove/move do not exist") is only half true: `parameters` is a
  public `Vec`; the real gap is the position (AC-2's padding).
- Out-of-tree `orest-d/egui-midi-test` remains the only `gui_info` consumer; its three obstacles
  are answered by `argument_slots` (1, 3) and the checked edits (2).

## Phase 2: Architecture

### Solution

A pure function walks `command_metadata.arguments` exactly as `from_action` does, recording index
ranges instead of converting values. Three edit functions compute slots, check the request,
materialise defaults for omitted earlier arguments when the target range starts past the end, and
edit `action.parameters`. Rejected: a method on `ActionRequest` (core `query.rs` must not depend on
command metadata); refactoring `from_action` onto slots (larger change to a hot path; the test
suffices).

### Changes

```rust
// liquers-core/src/plan.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgumentSlot { pub argument: usize, pub parameters: std::ops::Range<usize> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgumentSlots { pub slots: Vec<ArgumentSlot>, pub excess: std::ops::Range<usize> }

pub fn argument_slots(action: &ActionRequest, metadata: &CommandMetadata) -> ArgumentSlots;
pub fn insert_variadic_element(action: &mut ActionRequest, metadata: &CommandMetadata,
    argument: &str, index: usize, value: ActionParameter) -> Result<(), Error>;
pub fn remove_variadic_element(action: &mut ActionRequest, metadata: &CommandMetadata,
    argument: &str, index: usize) -> Result<ActionParameter, Error>;
pub fn move_variadic_element(action: &mut ActionRequest, metadata: &CommandMetadata,
    argument: &str, from: usize, to: usize) -> Result<(), Error>;
```

Arguments are named (`argument: &str`), matching `override_value`. Sync, no allocation beyond the
result. Errors: unknown or non-`multiple` argument and out-of-range index use
`Error::general_error`; an omitted argument without a default uses `Error::missing_argument(i, name, &action.position)`
(`liquers-core/src/error.rs`). Excess parameters make every edit fail (the action does not plan).
Documents: `specs/reference/RECIPES_AND_PLANS.md` gains "Argument slots" (History,
`reviewed:`); the source issue's body records the re-scope.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Slots drift from `pop_value` | AC-1 test over every registry command and generated actions | high |
| Padding writes a default the user did not type | documented; the only way to reach the slot | medium |
| A default not representable as text (array) | refused (AC-4) | high |

Recovery: additive; revert.

## Phase 3: Examples and Tests

### Examples

`-R-key/specs/issues/-/ns-search/field_extract` → `…/field_extract-md--false-status` and
`ns-pl/is_in-name-a-b` → `ns-pl/is_in-name-b-a` (all validated).

### Tests

Unit, `liquers-core/src/plan.rs` `mod tests` (metadata built inline, as the existing variadic
tests do):

- `argument_slots_agree_with_from_action` — injected, scalar with/without default, multiple, excess, for several shapes: AC-1
- `insert_pads_omitted_defaults` — the `field_extract` shape: AC-2
- `move_and_remove_stay_in_argument` — the `is_in` shape: AC-3
- `variadic_edit_refusals_leave_action_unchanged` — non-multiple, out of range, missing default, excess: AC-4
- `variadic_edit_moves_link_elements` — AC-5

Integration, `liquers-core/tests/variadic_slots.rs`: `edited_actions_plan_against_committed_registry`
— loads `specs/command_registry.yaml` with `ValidationRegistryBuilder`, applies AC-2 and AC-3 edits,
and checks `validate_query` at Plan level is `Ok`: AC-2, AC-3.

Run: `cargo test -p liquers-core --lib variadic && cargo test -p liquers-core --test variadic_slots`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/plan.rs` `ArgumentSlot`, `ArgumentSlots`, `argument_slots` + `argument_slots_agree_with_from_action` — `cargo test -p liquers-core --lib argument_slots`. Rollback: delete.
- [ ] 2. `plan.rs` `insert_variadic_element`, `remove_variadic_element`, `move_variadic_element` + the four edit tests — `cargo test -p liquers-core --lib variadic`.
- [ ] 3. `liquers-core/tests/variadic_slots.rs` — `cargo test -p liquers-core --test variadic_slots`.
- [ ] 4. `specs/reference/RECIPES_AND_PLANS.md` "Argument slots" (History, `reviewed:`); close `UI-VARIADIC-ARGUMENT-LIST-EDITOR` pointing at `NO-COMMAND-PARAMETER-EDITOR` — `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests` (core API is
additive, nothing should change); diff review limited to `plan.rs`, the new test and the document.
