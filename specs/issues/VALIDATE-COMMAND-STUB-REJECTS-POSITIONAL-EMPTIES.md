---
id: VALIDATE-COMMAND-STUB-REJECTS-POSITIONAL-EMPTIES
kind: issue
title: A liquers-validate --command stub rejects a query with empty positional parameters
status: closed
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

## Resolution

2026-10-17, under [`design/variadic-empty-element-conversion/`](../design/variadic-empty-element-conversion/DESIGN.md).
`ParameterValue::pop_value` (`liquers-core/src/plan.rs`) now converts each string element of a
`multiple` argument with a copy of its `ArgumentInfo` that is neither `multiple` nor defaulted, so
an empty element gets its type's scalar fallback (`""` for `Any`, `null` for `IntegerOption` /
`FloatOption`, `false` for `Boolean`) and a required `Integer` / `Float` element fails with
"… argument … missing" at its own position. `liquers-validate --command echo_args --
'echo_args----20'` now exits 0 with four elements `"", "", "", "20"`.

Tests: `plan::tests::variadic_empty_element_uses_element_fallback`,
`plan::tests::variadic_empty_required_numeric_element_is_missing`, and
`validate::tests::permissive_command_accepts_any_argument_list` (now also `greet----20` and
`greet-a--b-`).
