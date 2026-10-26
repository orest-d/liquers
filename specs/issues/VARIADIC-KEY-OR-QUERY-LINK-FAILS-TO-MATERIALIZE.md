---
id: VARIADIC-KEY-OR-QUERY-LINK-FAILS-TO-MATERIALIZE
kind: issue
title: A key or query link inside a multiple argument fails, because Key and Query values have no JSON form
status: draft
priority: P3
complexity: S
area: [core/plan, core/value]
design:
created: 2026-10-26
github:
---

## Problem

Links inside a `multiple` (variadic) argument are materialized by
`materialize_nested_parameter` → `materialize_link_json` (`liquers-core/src/interpreter.rs`), which
converts the linked state with `try_into_json_value`. A link whose value is a key (`-R-key/…`) or a
query (`…/q`) evaluates to core `Value::Key` / `Value::Query` (and the `SimpleValue` equivalents),
which have no JSON arm, so the whole action fails.

Found during the 2026-10-26 bulk design of `sql-several-tables` (cluster I), which plans the fix as
its AC-7. Filed separately so it can be fixed on its own if `sql-several-tables` waits on its
decisions: the fix is one function, with no interface change.

## Problem Example

A test command `join_text(state, parts: Vec<String> multiple)` fed `…/join_text-a-~X~-R-key/data/x.csv~E`
fails with a "cannot convert Key to JSON" error instead of receiving the text `data/x.csv`.

## Expected behaviour

A key or query link inside a variadic argument arrives as its encoded text.
