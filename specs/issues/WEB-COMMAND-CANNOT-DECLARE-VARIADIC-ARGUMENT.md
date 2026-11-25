---
id: WEB-COMMAND-CANNOT-DECLARE-VARIADIC-ARGUMENT
kind: feature
title: A JavaScript command cannot declare a variadic argument or its shape
status: draft
priority: P3
complexity: M
area: [web]
created: 2026-11-25
github:
---
## Problem

`LiquersArgument` (`liquers-web/src/typescript.rs`), the argument of a `registerCommand`
declaration, has `name`, `type` and `default` only. It has no `multiple`, so a JavaScript command
cannot take every remaining parameter of its action, and therefore none of the shapes a variadic
argument can have since `design/composite-variadic-arguments/`: tuples, a dictionary
(`shape: map`) or a JSON value (`shape: json`). Core accepts both fields in a declaration
(`COMMAND_DECLARATION.md` §3, `val03_shape_and_columns_are_declared_and_checked`); the JavaScript
declaration does not carry them, and the binder has no rule for how the values arrive (spread, or
one array or object).

## Impact

A browser command that needs a list of values, or key and value options such as layout data, has
to take one string and parse it itself, or be written in Rust. Commands registered in Rust with a
shape are unaffected: their metadata reaches JavaScript as it is.

## Expected behaviour

`LiquersArgument` gains `multiple?: boolean`, `shape?` and `columns?`, passed through the
declaration; the function receives a list for a plain variadic, an array of arrays for tuples, an
object for a dictionary and the value itself for a JSON value. `LANGUAGE-INTEGRATION_GUIDE.md`
`COMMAND04` covers the binding.

## Discovery

Step 8 of `design/composite-variadic-arguments/`, 2026-11-25: its Phase 1 expected the TypeScript
declarations to gain the new metadata, and they turned out to have no variadic argument at all.
The Python side of the same gap is `PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA`.
