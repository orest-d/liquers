# Phase 2: Solution & Architecture — planner-instruction-names-public

## Overview

`liquers-core` makes the instruction names it already holds (after
`reserved-instruction-command-names`) public, adds one predicate, and the two other crates replace
their copies with it. No behaviour changes in core or template. In lib, a trailing `v` stops hiding
the presets.

Starting point, once `reserved-instruction-command-names` has landed, in `liquers-core/src/query.rs`:

```rust
pub(crate) const NS_INSTRUCTION: &str = "ns";
pub(crate) const Q_INSTRUCTION: &str = "q";
pub(crate) const V_INSTRUCTION: &str = "v";
pub(crate) const RESERVED_ACTION_NAMES: [&str; 3] = [NS_INSTRUCTION, Q_INSTRUCTION, V_INSTRUCTION];
impl ActionRequest { pub fn is_ns(&self); pub fn is_q(&self); pub(crate) fn is_v(&self); }
```

with `plan.rs` and `CommandMetadata::check` already using them.

## Known-Issue Preflight

| Item | Status | Effect |
|---|---|---|
| `COMMAND-NAMES-Q-AND-V-ARE-NOT-RESERVED` (`reserved-instruction-command-names`) | draft, design ready | hard prerequisite; Phase 4 step 1 does its steps if it has not landed |
| `NEXT-PRESETS-MISREAD-PLANNER-INSTRUCTIONS` (`next-presets-follow-planner-rules`) | draft, design ready | soft; its private helper is replaced here |
| `TEMPLATE-DIAGNOSTICS-RESOLVE-NAMESPACES-FROM-THE-WHOLE-QUERY` | draft, filed with this design | same file, different function; independent |
| `query-cache-directives` | design, needs-decision | adds `cache`; whichever lands second extends the list |

## Interfaces / Data Structures

`liquers-core/src/query.rs`:

```rust
/// The `ns` planner instruction: selects the namespaces the following actions are looked up in.
pub const NS_INSTRUCTION: &str = "ns";
/// The `q` planner instruction: quotes the query before it as a value.
pub const Q_INSTRUCTION: &str = "q";
/// The `v` planner instruction: makes the plan volatile from its position on.
pub const V_INSTRUCTION: &str = "v";
/// Action names the plan builder consumes itself. They are never looked up as commands, and a
/// command may not be registered under one (`CommandMetadata::check`).
pub const PLANNER_INSTRUCTIONS: [&str; 3] = [NS_INSTRUCTION, Q_INSTRUCTION, V_INSTRUCTION];

/// Whether `name` is a planner instruction rather than a command name.
pub fn is_planner_instruction(name: &str) -> bool { PLANNER_INSTRUCTIONS.contains(&name) }

impl ActionRequest {
    pub fn is_v(&self) -> bool { self.name == V_INSTRUCTION }
    /// Whether this action is a planner instruction (`ns`, `q`, `v`) rather than a command.
    pub fn is_planner_instruction(&self) -> bool { is_planner_instruction(&self.name) }
}
```

`RESERVED_ACTION_NAMES` is renamed `PLANNER_INSTRUCTIONS`. No new struct, enum or trait.

Rejected: predicate only, with no list (callers that report the names would rebuild it); a
`PlannerInstruction` enum with `FromStr` (more structure than three names need, and every `match`
changes when `cache` arrives); copies guarded by a cross-crate test (the test is one more copy).

## Integration Points

- `liquers-core/src/command_metadata.rs` `CommandMetadata::check`: `RESERVED_ACTION_NAMES` →
  `PLANNER_INSTRUCTIONS`. The `query.rs` test `reserved_action_names_match_instruction_predicates` is
  renamed `planner_instructions_match_instruction_predicates`.
- `liquers-template/src/diagnostics.rs`: delete `fn is_instruction`; `unknown_commands` and
  `template_commands` call `liquers_core::query::is_planner_instruction(&name)`. `template_commands`
  compares a template literal with `liquers_core::query::NS_INSTRUCTION` instead of `"ns"`.
- `liquers-lib/src/utils.rs` `find_next_presets`: `a.is_planner_instruction()` replaces the private
  helper from `next-presets-follow-planner-rules`, or `!a.is_ns() && !a.is_q()` if that design has
  not landed (which also fixes its trailing-`v` part, not its namespace or realm parts).
- Bindings: `liquers-py` and `liquers-web` expose no `ActionRequest` predicates and register
  through `CommandMetadata::check`. No change.
- `query-builder` (finished Phase 4, not edited) can call `is_planner_instruction` when it
  validates action names.

## Error Handling

None: constants and pure predicates. The reserved-name error in `CommandMetadata::check` keeps the
message `reserved-instruction-command-names` defines.

## Relevant Commands

No command is added, removed or changed; `specs/command_registry.yaml` is untouched. The reserved
names stay refused as command names.

## Documentation Architecture

- `specs/reference/RECIPES_AND_PLANS.md` §Planner instructions: the action-shaped names are
  `liquers_core::query::PLANNER_INSTRUCTIONS`; tools that skip instructions call
  `is_planner_instruction`.
- `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` §Planner instructions: one sentence.
- `specs/reference/TEMPLATE_LANGUAGE.md`, diagnostics paragraph: instruction names come from core.
- Each gets a History row and a `reviewed:` bump.

## Risks

| Assessment | Finding |
|---|---|
| Likely files | `liquers-core/src/query.rs`, `command_metadata.rs`; `liquers-template/src/diagnostics.rs`; `liquers-lib/src/utils.rs`; three documents |
| Affected workflows / crates | planning (no change), template validation (no change), query console presets (bug fix) |
| Existing-test impact | one core test renamed; template and `utils.rs` tests unchanged |
| Compatibility | additive public API (three constants promoted, one constant and two functions added, `is_v` made public); nothing public removed |
| Data / concurrency / performance / security | none |
| Recovery | revert the single commit spanning the three crates |
| Certainty | high, given the prerequisite lands as written |

Open question carried from Phase 1: **Proposed resolution - public shape and name**. Settled
implementation detail: `NS_INSTRUCTION` must be public too, because `template_commands` compares a
template literal with it.
