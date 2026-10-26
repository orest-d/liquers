---
id: COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED
kind: feature
title: Command metadata does not declare what type a command accepts or returns
status: draft
priority: P2
complexity: L
area: [core/commands, core/value, macro]
design: command-io-type-declarations
created: 2026-10-20
github:
---

## Problem

`CommandMetadata` (`liquers-core/src/command_metadata.rs`, marked `// TODO: support input type` /
`// TODO: support output type`) describes the state argument as an `ArgumentInfo` with
`ArgumentType::Any`, and has no output type at all. `ArgumentType` describes how a query parameter
is parsed, not which value type a link or the state carries (`//TODO: add support for value with
type_identifier`). The type registry (`liquers-core/src/type_system.rs`) knows identifiers such as
`polars.DataFrame` and `Image`, but no command says which of them it takes or produces.
`register_command!` cannot infer them either: the state keyword (`state`, `value`, `text`) has no
Rust type, the function always returns `V`, and `TypeIdentifiedIn` has no impls
(`TYPE-IDENTIFIED-IN-HAS-NO-IMPLEMENTATIONS`).

## Problem Example

```
-R/data/sales.csv/-/ns-pl/head-5/ns-img/resize-100-100
```

`liquers-validate` reports `Ok`; evaluation fails inside `resize` with `Can't convert
'polars.DataFrame' to Image`. Nothing in the metadata says that `ns-pl/head` produces a frame or
that `ns-img/resize` needs an image, so the mismatch cannot be reported before running.

## Impact

Validation, the UI's "what can I apply next" and agents cannot tell a well-typed pipeline from an
ill-typed one before running it; command specialization and automatic conversion have nothing to
key on. Workaround: run it and read the error.

## Expected behaviour

Optional, serializable input and output type declarations on `CommandMetadata` (an ordered list of
type identifiers; groups such as "a table" later), given explicitly to `register_command!` and the
declaration format; checked at plan time where the input's declared type is known and reported
there as a warning, not enforced. Backward compatible: absent means any. Overlaps
`VALUE-CONVERSION-CAPABILITY` (the purpose axis) and `TYPE-REGISTRY-NOT-REALM-AWARE`; required by
`VALUE-NOT-CONVERTED-AUTOMATICALLY` and `COMMAND-SPECIALIZATION-BY-INPUT-TYPE-MISSING`.

## Discovery

Split out of `COMMAND-METADATA-ENHANCEMENTS` (scope C, "input/output typing") during the
2026-10-20 bulk design, which re-scoped that issue to global enums.
