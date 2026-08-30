---
id: COMMAND-ALIAS-DEFINITION-UNTESTED
kind: issue
title: CommandDefinition::Alias has no test and no user, and its head-parameter semantics are unexercised
status: draft
priority: P2
complexity: M
area: [core/plan, core/commands]
design:
created: 2026-08-29
github:
---
## Problem

`CommandDefinition::Alias { command, head_parameters }` is a supported variant of a public enum that
the planner acts on (`liquers-core/src/plan.rs:1575`) and the documentation generator reports
(`liquers-lib/src/commands.rs:165`). Nothing else in the repository mentions it:

- no test constructs one, in any crate;
- `register_command!` has no statement that produces one;
- `specs/command_registry.yaml` contains none (`grep -c 'Alias'` returns 0).

So a code path that rewrites an action to a different command, prefixing parameters, has never been
executed.

## Why it matters

The semantics are not obvious and are easy to get wrong in either direction.
`ResolvedParameterValues::from_action_extended` (`plan.rs:995-1011`) zips `head_parameters` against
**the alias command's own `arguments`**, then resolves the remainder:

```rust
let mut values = head_parameters.iter()
    .zip(command_metadata.arguments.iter())
    .map(|(x, arginfo)| ParameterValue::from_command_parameter_value(&arginfo.name, x))
    .collect_vec();
let n = values.len();
for a in command_metadata.arguments.iter().skip(n) { … }
```

This means an alias must declare the full argument list including the positions its head parameters
fill — correct for a genuine alias (`head` = `slice` with the first argument pre-filled), but it is
an unstated invariant. Several things follow that nobody has had to confirm:

- what happens when `head_parameters` is longer than the alias's declared `arguments`
  (`zip` truncates silently, so the extra head parameters are **discarded without error**);
- whether an alias whose target has a different state-argument shape plans and executes correctly;
- whether `accepted_parameter_count(metadata, n)` reports the arity a user would expect in the
  `too_many_parameters` error raised at `plan.rs:1021`;
- whether an alias onto a `multiple` argument resolves, and onto an alias (chaining) terminates.

## Why it is being filed now

`design/command-declaration` evaluated using `Alias` as a dispatch mechanism for declared Python and
JavaScript commands — a per-runtime `pycall` command with the callable's identifier as a head
parameter. That evaluation concluded the mechanism does not fit as-is (Phase 2 §The `run` field), but
it also surfaced that the variant is unexercised, which is a defect independent of that design.

The related opportunity: an `Alias`-based binding is the only *serializable* record of which
implementation a declared command has. `CommandDefinition::Registered` says nothing about it, so an
exported registry cannot reconstruct a host-declared command. That makes `Alias` relevant to
`POST-INIT-COMMAND-REGISTRATION` and to the `snapshot_declaration` cleanup — but only once it is
specified and tested.

## Fix direction

1. Decide and document the head-parameter contract: whether an alias's `arguments` include the
   head-filled positions (as the code assumes today), and what an over-long `head_parameters` means —
   silent truncation is almost certainly wrong; an error at registration is the natural answer.
2. Add tests: a plain alias with one head parameter; arity error messages; an alias with a `multiple`
   argument; a chained alias; a mismatched-arity alias.
3. Register one real alias in `liquers-lib` so the path has a production user and appears in
   `specs/command_registry.yaml`, giving the round-trip test coverage of the variant.

## Related

- `COMMAND-DECLARATION-FORMAT` — surfaced this; does not depend on it under the recommended
  branch-1-only resolution.
- `POST-INIT-COMMAND-REGISTRATION` — would benefit from a serializable binding.
- `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS` — adjacent parameter-resolution semantics.

## Verification

Tests covering each bullet above, and a non-zero `Alias` count in `specs/command_registry.yaml`.
