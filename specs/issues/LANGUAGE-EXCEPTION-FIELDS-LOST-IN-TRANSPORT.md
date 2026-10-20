---
id: LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT
kind: issue
title: Language exception class and stack are lost in transport
status: accepted
priority: P3
complexity: M
area: [core/error, web, py]
design: 
created: 2026-08-08
github:
---
## Problem

`liquers-web` exposes `LiquersError.jsClass` and `LiquersError.jsStack`, and its error bridge
populates them via `LiquersError::from_thrown`. **The evaluation path does not go through that
constructor.** When a JavaScript command throws, the adapter converts the exception to a
`liquers_core::error::Error` so it can travel through the planner and the asset lifecycle; that
type has fields for message, position, query, key and command key, but nothing for
language-specific context. The eventual `Promise` rejection is rebuilt from the `Error`, so
`jsClass` and `jsStack` are `null`.

Measured, on the real path — a command doing `throw new TypeError('bang')`:

```
errorType = "execution_error"
jsClass   = undefined
jsStack   = undefined
message   = "TypeError: bang\n    at eval (...)\n    at ..."
```

Nothing is *lost* — the class and the stack are both in the message — but they are no longer
separately addressable, so a page cannot branch on the class or render the stack on its own.

## Impact

Small, and the same for every *integrated language*: Python's exception type and traceback, and
Starlark's call stack, hit exactly the same wall. It is a property of the transport, not of the
JavaScript bridge.

## Intended solution

Give `liquers_core::error::Error` a place for language context — for example
`language_context: Option<serde_json::Value>` with `#[serde(default, skip_serializing_if)]`, which
is additive and keeps every existing implementor compiling. The bridge would fill it on the way in
and read it on the way out, and `liquers-py` could use the same field for a traceback.

Additive to core, but it is a core type change touching serialization, so it wants a moment's
design rather than being folded into an integration milestone.

## Discovery

Raised by review on PR #19 and confirmed by probing the real evaluation path. Worth noting how it
survived: `ERROR03` tested the error bridge *in isolation*, where `from_thrown` does populate the
fields — the test was true and the shipped path was not. A conformance test for a bridge should
exercise the route production traffic takes, not the most convenient entry point.

**Fixed alongside it:** the message contained its first line twice ("TypeError: bang\nTypeError:
bang\n at ..."), because a JavaScript `stack` conventionally begins with the "Name: message" line
that had already been prepended.

## Progress (2026-10-11)

`design/metadata-error-traceback/` added the traceback half of the core field: `Error` now carries
an optional rendered `traceback` string (`Error::with_traceback`), which reaches the metadata log
and the axum error body. What remains here is the exception class and having `liquers-web` and
`liquers-py` fill the traceback on the way in instead of appending the stack to `message`.

## Related (2026-10-19)

`liquers-py` follows the same route as `liquers-web`: the Python class goes in the message and the
stack in `Error::traceback`, and no `language_context` field was added (owner decision, design
`liquers-py` Phase 1 question 6). How several languages' stacks share that one field is
`MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING`.
