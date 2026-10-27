---
id: MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING
kind: issue
title: An error crossing several languages carries one flat traceback string with no structure
status: draft
priority: P3
complexity: L
area: [core/error, py, web]
design: multi-language-traceback
created: 2026-10-19
github:
---

## Problem

`liquers_core::error::Error::traceback` is an `Option<String>`. A call chain can cross several
languages, for example Rust calling Python calling Rust calling Rhai. Each crossing has its own
stack, and today they are joined into that one string:

- `liquers-py` (`liquers-py/src/error.rs` `pyerr_to_error`) appends the Python frames a
  `LiquersError` was raised through to the traceback it already carries, innermost first.
  `test_ERROR_nested_python_tracebacks_are_concatenated` pins this.
- `liquers-web` puts the JavaScript stack in the same field.
- The Rust frames between the language stacks are not recorded at all.

The result is readable by a person but has no structure. A consumer cannot tell where one
language's stack ends and the next begins, which language each part came from, or which frame
belongs to which command.

The owner's position (2026-10-19): for now the traceback is a string concatenation of the stacks
it crossed, and the important property is that Python tracebacks are passed along. This issue
records the open problem so the shape is decided deliberately rather than by accretion.

## Problem Example

A Rust query calls Python command `outer`, which calls `context.evaluate("inner")`, where `inner` is
a Python command that raises `ValueError`. Today the caller's `LiquersError.traceback` is one string
with `inner`'s frames followed by `outer`'s, with nothing marking the boundary or naming either
command or language. Expected (whichever option is chosen): a consumer can list the segments —
`(python, inner, frames)`, `(python, outer, frames)` — or at least split the string at a documented
separator. Re-checked at HEAD 2026-10-20: `Error::traceback` is still `Option<String>`
(`liquers-core/src/error.rs`).

## Impact

No current user is blocked: Python tracebacks reach the caller, inner first. The cost appears
when a second embedded language (Rhai, Starlark) joins a chain, or when a UI wants to render or
filter stacks per language. Changing the field's type later touches serialization of `Error` and
`LogEntry` (metadata logs persist tracebacks), so it is cheaper to decide before stored logs
depend on the flat form.

## Expected behaviour

Open; options, not a choice:

1. Keep the string, with a documented separator and a per-segment header naming the language and
   command.
2. A structured list of segments, `Vec<TracebackSegment { language, command_key, text }>`, with
   the flat string derived for display. It would be serialized additively beside `traceback`.
3. Fold it into the language-context field proposed by `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`.

Each needs a decision on whether Rust frames between segments are recorded, and how a log entry
stores the result.

## Discovery

Raised by Orest while reviewing the `liquers-py` design (Phase 1, question 6), 2026-10-19, together
with the decision not to add a `language_context` field to core for now.
