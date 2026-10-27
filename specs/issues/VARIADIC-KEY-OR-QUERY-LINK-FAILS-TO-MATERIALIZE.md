---
id: VARIADIC-KEY-OR-QUERY-LINK-FAILS-TO-MATERIALIZE
kind: issue
title: A key or query link inside a multiple argument fails, because Key and Query values have no JSON form
status: draft
priority: P3
complexity: S
area: [core/plan, core/value]
design: variadic-key-query-link-text
created: 2026-10-26
github:
---

## Problem

Links inside a `multiple` (variadic) argument are materialized by
`materialize_nested_parameter` → `materialize_link_json` (`liquers-core/src/interpreter.rs`), which
converts the linked state with `try_into_json_value`. A link whose value is a key (`-R-key/…`) or a
query (`…/q`) evaluates to core `Value::Key` / `Value::Query` (and the `SimpleValue` equivalents),
which have no JSON arm, so the whole action fails.

The failure is deliberate today: `plan-relative-resolution` (complete) recorded that such a link
"returns the existing `ConversionError`. It is not silently stringified", and
`multiple_parameter_non_json_link_reports_positioned_conversion_error` (`interpreter.rs`) pins it
with a `-R-key/./target` link. Changing it reverses that decision, so it needs one.

## Problem Example

A test command `join_text(state, parts: Vec<String> multiple)` fed `…/join_text-a-~X~-R-key/data/x.csv~E`
fails with a "cannot convert Key to JSON" error instead of receiving the text `data/x.csv`.

## Impact

A command cannot take a list of keys or queries given as links (`Vec<Key> multiple`), and
`sql-several-tables` cannot pass its tables as `/q` links in a variadic list. Workaround: write the
key or query text literally instead of linking it.

## Expected behaviour

A key or query link inside a variadic argument arrives as its encoded text; other values without a
JSON form keep failing. Designed in `design/variadic-key-query-link-text/` (decision pending).

## Discovery

2026-10-26, bulk design of `sql-several-tables`, which planned the fix as its AC-7; split out so it
can be decided and fixed on its own. Re-verified by reading `interpreter.rs` and `value.rs` at
`70b9005`.
