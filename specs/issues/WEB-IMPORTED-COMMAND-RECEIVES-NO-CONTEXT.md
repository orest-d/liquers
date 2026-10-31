---
id: WEB-IMPORTED-COMMAND-RECEIVES-NO-CONTEXT
kind: issue
title: A JavaScript command bound to imported metadata cannot receive the context
status: draft
priority: P3
complexity: S
area: [web]
design:
created: 2026-10-31
github:
---

## Problem

`registerCommands(document, implementations)` binds a JavaScript function to command metadata
taken from a registry document (`JsCommandSpec::from_metadata`, `liquers-web/src/command/spec.rs`).
That metadata is a `CommandMetadata`, not a declaration, so it carries no `registration.context`
record, and the bound function never receives the `Context` that a declared command gets
(`specs/design/js-command-context/`). A metadata argument named `context` stays an ordinary query
argument.

## Impact

Only commands registered from imported metadata. A page that needs the context declares the command
with `registerCommand`, or with a declaration array, instead of a registry document.

## Expected behaviour

The implementation object accepts a context position (for example `{ run, context: 1 }`), or an
inferred parameter named `context` in `run` is honoured as it is for a declaration.

## Discovery

Noticed while implementing `js-command-context` (2026-10-31).
