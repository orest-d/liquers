---
id: VALIDATE-COMMAND-STUB-REJECTS-POSITIONAL-EMPTIES
kind: issue
title: A liquers-validate --command stub rejects a query with empty positional parameters
status: draft
priority: P3
complexity: S
area: [core/validate]
design: variadic-empty-element-conversion
created: 2026-10-12
github:
---

## Problem

`liquers-validate --command NAME` declares a permissive command "that accepts any arguments"
(`liquers-core/src/bin/liquers_validate.rs:90`, `RegistryBuilder::add_permissive_command`,
`liquers-core/src/validate/registry.rs:107`). It is built as one variadic `Any` argument
(`permissive_command_takes_multiple_any_arguments`, `registry.rs:268`). A query with empty
positional slots then fails to plan:

```
liquers-validate --command echo_args -- 'echo_args----20'
ERROR [0] Multiple parameters not supported inside vector argument
```

The same query validates once the real signature is supplied with `--registry-file`.

## Problem Example

`liquers-validate --command echo_args -- 'echo_args----20'` today fails with
`Multiple parameters not supported inside vector argument`; it should plan with four elements
`"", "", "", "20"`. Checked 2026-10-17: the cause is in `ParameterValue::pop_value`
(`liquers-core/src/plan.rs`), which converts an empty element with the variadic argument's own
`ArgumentInfo`, so the `Any`/numeric/`bool` fallbacks return the whole list's default. Every
non-`String` `multiple` argument is affected, not only the stub.

## Impact

Low. The `--command` route exists for designing commands that do not exist yet, which is when
empty slots (`cmd---x`, reaching a later argument) are most likely to be written. The workaround
is a registry overlay, which needs `commands`, `default_namespaces` and `global_enums` and is not
documented for this purpose. `CLAUDE.md`'s table says `--command` "accepts any arguments", which
this contradicts.

## Expected behaviour

A permissive command accepts every query an arbitrary signature could accept, empty positional
parameters included — or the documentation states the limit and points to the overlay.

## Discovery

Validating fixture queries for `design/axum-query-named-arguments/` (Phase 3, 2026-10-12).
