---
id: COMMAND-ENUM-ALTERNATIVES-CANNOT-BE-COMPUTED
kind: feature
title: An enum argument's alternatives are fixed at registration and cannot be computed
status: draft
priority: P3
complexity: L
area: [core/commands, core/plan]
design: computed-enum-alternatives
created: 2026-10-20
github:
---

## Problem

`EnumArgument` (`liquers-core/src/command_metadata.rs`) holds a fixed `values` list, and a
`GlobalEnum(name)` reference resolves to a fixed entry of
`CommandMetadataRegistry::global_enums`. Nothing can supply alternatives at run time: the list of
stored tables, the columns of the input frame, the fonts installed. `command_metadata.rs` carries
`// TODO: support for dynamic global enums` above `CommandMetadataRegistry`.

An argument that depends on the input frame (`ns-pl/sort`'s `column`) cannot be checked at plan
time at all, because the planner has no input state; it can be checked just before the command
runs. An argument that depends only on the environment (stored tables) can be checked at plan time.

## Problem Example

`-R/data/sales.csv/-/ns-pl/sort-amont` validates (`liquers-validate` status `Ok`) and fails only
inside Polars on the unknown column. Should: with `column` declared as a computed enum of the input
frame's columns, the action is refused before the command runs, naming `region, amount, date`, and
a UI can ask for those three names.

## Impact

UIs and agents cannot be offered the valid choices for data-dependent arguments, and plan-time
validation of such arguments is impossible. Workaround: a static enum with `others: true`, or a
plain string checked in the command.

## Expected behaviour

A provider contract for enum alternatives that are computed, with the decisions the original
umbrella left open: whether a provider may depend on the input state (which the planner does not
have) or only on the environment; sync or async; caching and the metadata version (a cached result
must not survive a change of the alternatives it was validated against); and what the planner does
when the provider fails. Needs a design; `L`.

## Discovery

Split out of `COMMAND-METADATA-ENHANCEMENTS` (milestone 2, "dynamic enum provider contract") during
the 2026-10-20 bulk design, which re-scoped that issue to global enums
(`design/global-enum-registration/`).
