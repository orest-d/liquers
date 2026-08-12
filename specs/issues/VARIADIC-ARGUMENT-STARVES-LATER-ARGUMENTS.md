---
id: VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS
kind: issue
title: An argument declared after a multiple argument is unreachable and unchecked
status: draft
priority: P2
complexity: S
area: [core/commands, core/plan, macro]
design: excess-action-parameters-error
created: 2026-08-12
github:
---
## Problem

A `multiple` argument consumes every remaining action parameter, so any argument declared after it
can never receive one. Nothing rejects, or even warns about, such a declaration.

`ParameterValue::pop_value` (`liquers-core/src/plan.rs:679-729`) drains the
`ActionParameterIterator` for a `multiple` argument. `ResolvedParameterValues::from_action_extended`
(`:884`) then continues its loop over the remaining declared arguments against an exhausted
iterator, so each subsequent argument reaches the `None` branch at `:737`. The outcome depends on
whether that argument has a default, and both outcomes are wrong:

| Later argument | Result |
|---|---|
| has a default | silently takes the default, forever — the value the caller wrote was swallowed by the variadic argument |
| has no default | `ArgumentMissing`: "Missing argument" for an argument the caller did supply |

The second is actively misleading: the query names a value, the error says it is missing.

## Why it is latent today

`register_command!` cannot declare a variadic argument — it hardcodes `multiple: false`
(`liquers-macro/src/registration.rs:718`, `:2336`, `:2406`) — so no command in the workspace sets
it, and `multiple: true` appears nowhere outside one `plan.rs` unit test. The hazard becomes
reachable the moment the flag is declarable, which is planned in
`specs/design/excess-action-parameters-error/`. Filing separately because the constraint belongs to
the command model, not to that design's subject.

## Expected behaviour

A command whose argument list places a `multiple` argument anywhere but last is rejected at
registration, with a message naming the command and the starved argument.

## Fix direction

`CommandMetadata::check()` (`liquers-core/src/command_metadata.rs:946`) is the natural home: it
already walks `self.arguments` and emits `CommandRegistryIssue::error(...)`. The check is that no
argument before the last has `multiple` set.

Two obstacles sit in that path and are worth knowing before starting:

- `CommandMetadata::check()` has no caller anywhere in the workspace, so adding a rule to it
  changes nothing until registration (or the registry exporter) invokes it and acts on the result.
- `CommandRegistryIssue::warning` and `::error` transpose two fields — see
  `COMMAND-REGISTRY-ISSUE-NAMESPACE-NAME-SWAPPED`. Any message emitted through them is misattributed
  until that is fixed.

Injected arguments are unaffected: they consume no query parameters, so an injected argument after a
variadic one is legitimate and must keep working.

## Discovery

Found while designing `PLAN-EXCESS-ACTION-PARAMETERS-DROPPED`, which makes `multiple` the sanctioned
way to accept a variable-length parameter list and therefore turns this latent constraint into a
live one.
