---
id: COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED
kind: feature
title: Command metadata does not declare what type a command accepts or returns
status: draft
priority: P2
complexity: L
area: [core/commands, core/value, macro]
design:
created: 2026-10-20
github:
---

## Problem

`CommandMetadata` (`liquers-core/src/command_metadata.rs`) describes the state argument as an
`ArgumentInfo` with `ArgumentType::Any` in practice, and has no output type at all. The type
registry (`liquers-core/src/type_system.rs`) knows type identifiers such as `polars.DataFrame`, but
no command says which of them it takes or produces. `register_command!` sees the Rust types of the
function and the `-> result` / `-> value` form, and discards them.

Example: `-R/data/sales.csv/-/ns-img/resize-100-100` validates with `liquers-validate` and fails
only at run time, because nothing says `resize` needs an image.

## Impact

Validation, the UI's "what can I apply next" and agents cannot tell a well-typed pipeline from an
ill-typed one before running it; command specialization and automatic conversion have nothing to
key on. Workaround: run it and read the error.

## Expected behaviour

Optional, serializable input and output type descriptors on `CommandMetadata` (type identifiers,
possibly groups such as "a table"), filled by `register_command!` where it can infer them and by
declaration otherwise; checked at plan time where the input type is known and reported, not
enforced, where it is not. Backward compatible: absent means `Any`. Needs a design; `L`, and it
overlaps `VALUE-CONVERSION-CAPABILITY` (the purpose axis) and `TYPE-REGISTRY-NOT-REALM-AWARE`.

## Discovery

Split out of `COMMAND-METADATA-ENHANCEMENTS` (scope C, "input/output typing") during the
2026-10-20 bulk design, which re-scoped that issue to global enums.
