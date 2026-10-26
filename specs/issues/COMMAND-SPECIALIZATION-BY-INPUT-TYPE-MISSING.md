---
id: COMMAND-SPECIALIZATION-BY-INPUT-TYPE-MISSING
kind: feature
title: One command name cannot have variants chosen by the type of its input
status: draft
priority: P3
complexity: L
area: [core/commands, core/plan]
design: command-specialization-by-input-type
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

## Problem Example

```
-R/data/sales.csv/-/head-5
```

Fails to plan today (`Action 'head' not registered`): the author must write `ns-pl/head-5` or
`ns-rec/head-5` and know which one the input needs. Should be: a generic `head` runs `pl/head` for
CSV text or a frame and `rec/head` for a record view, choosing at plan time when the previous
command's declared output decides it, and otherwise at run time.

Note that `ns-pl/head` is itself an alias of `ns-pl/slice`, so a variant may be an alias.

## Impact

Query authors and agents must track the input's type by hand and pick the namespace; generic
pipelines cannot be written once. Workaround: namespaces, or one command that dispatches on the
value's type internally.

## Expected behaviour

A generic command lists variants (registered commands or aliases) that differ by declared input
type; the planner — or the interpreter, when the type is only known at run time — picks one
deterministically, an ambiguous declaration is refused at registration, and a missing match is a
reported error naming the candidates. Depends on input types being declared
(`COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED`); independent of conversion
(`VALUE-NOT-CONVERTED-AUTOMATICALLY` converts after a variant is chosen).

## Discovery

Split out of `COMMAND-METADATA-ENHANCEMENTS` (scope B, "specialization model") during the
2026-10-20 bulk design, which re-scoped that issue to global enums.
