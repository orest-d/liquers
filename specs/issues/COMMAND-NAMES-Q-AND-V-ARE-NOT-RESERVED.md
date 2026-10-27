---
id: COMMAND-NAMES-Q-AND-V-ARE-NOT-RESERVED
kind: issue
title: A command named q or v registers but can never run
status: closed
priority: P2
complexity: S
area: [core/commands, core/query]
design: reserved-instruction-command-names
created: 2026-10-19
github:
---

## Problem

`CommandMetadata` validation (`liquers-core/src/command_metadata.rs`, around line 1222) reserves
only the name `ns`. The planner also treats `q` and `v` as query instructions
(`liquers-core/src/plan.rs`, around lines 1677 and 1791). A command named `q` or `v` is therefore
registered without an error or a warning, and a query `v` or `q` never calls it: from Python,
`@env.first_command def v(): return 7` followed by `env.evaluate("v")` returns `None`.

## Impact

A silently dead command: the author sees a successful registration and a `None` result, with
nothing pointing at the name. Affects every language (Rust macro, Python, JavaScript).

## Expected behaviour

Every name the planner intercepts is refused like `ns`, with a message naming it as reserved, and
the reserved list lives in one place that both the planner and the validation read.

## Discovery

Found by the Phase 5 review of `design/liquers-py/` (2026-10-19), probing names while checking
`guides/PYTHON_GUIDE.md`.

## Triage 2026-10-20

Reproduces at HEAD (18732dd): `CommandMetadata::check` still tests only `"ns"`
(`command_metadata.rs` ≈1222), `PlanBuilder::process_action` intercepts `"v"` by literal
(`plan.rs` ≈1677), and `liquers-validate --command v --command q -- v q` plans both as
instructions. Designed in `design/reserved-instruction-command-names/` (ready, eligible for
automatic fixing).

## Resolution

Fixed 2026-10-27 by `design/reserved-instruction-command-names/`. `CommandMetadata::check` refuses every name in the crate-private `RESERVED_ACTION_NAMES` (`ns`, `q`, `v`, in `liquers-core/src/query.rs`) with "Command name '…' is reserved: it is a query instruction", and the planner's instruction tests (`is_ns`, `is_q`, the new crate-private `is_v`, the `ns` action built in `plan.rs`) read the same constants. `EnvironmentBuilder::build` therefore refuses a registry holding a command named `q` or `v`. Evidence: `instruction_names_are_reserved_command_names`, `names_resembling_instructions_are_not_reserved`, `reserved_action_names_match_instruction_predicates`, and `build_refuses_a_command_named_like_an_instruction` in `liquers-core/tests/environment_builder.rs`.
