---
id: COMMAND-SPECIALIZATION-BY-INPUT-TYPE-MISSING
kind: feature
title: One command name cannot have variants chosen by the type of its input
status: draft
priority: P3
complexity: L
area: [core/commands, core/plan]
design:
created: 2026-10-20
github:
---

## Problem

A command is found by `CommandKey { realm, namespace, name }` alone
(`CommandMetadataRegistry::find_command_in_namespaces`, `liquers-core/src/command_metadata.rs`,
marked `//TODO: implement command specialization by type`; `CommandKey` carries
`// TODO: specialization by type and type group`). Two implementations of `head` — one for a
Polars frame, one for a record view — must live in different namespaces (`ns-pl/head`,
`ns-rec/head`), and the query author has to know which one the input needs.

## Impact

Query authors and agents must track the input's type by hand and pick the namespace; generic
pipelines cannot be written once. Workaround: namespaces, or one command that dispatches on the
value's type internally.

## Expected behaviour

Several metadata entries may share a key and differ by a declared input type; the planner (or the
interpreter, when the type is only known at run time) picks one deterministically, and an
ambiguous or missing match is a reported error naming the candidates. This depends on input types
being declared at all (`COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED`) and on when the input's type is
known (`VALUE-CONVERSION-CAPABILITY`). Needs a design; `L`.

## Discovery

Split out of `COMMAND-METADATA-ENHANCEMENTS` (scope B, "specialization model") during the
2026-10-20 bulk design, which re-scoped that issue to global enums.
