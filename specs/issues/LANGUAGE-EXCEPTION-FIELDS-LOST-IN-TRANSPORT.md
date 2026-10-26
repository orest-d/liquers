---
id: LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT
kind: issue
title: Language exception class and stack are lost in transport
status: accepted
priority: P3
complexity: S
area: [web]
design: 
created: 2026-08-08
github:
---
## Problem

When a JavaScript command throws, `js_error_to_liquers` (`liquers-web/src/error.rs`) converts the
exception to a `liquers_core::error::Error` with the class prefixed to the message and the stack
**appended to `message`**; it never sets `Error::traceback`. The rejection the page receives is
rebuilt from that `Error`, so `LiquersError.jsClass` and `LiquersError.jsStack` are `null` on the
evaluation path (they are filled only when a `LiquersError` is built directly with
`from_thrown`), and `LiquersError` has no `traceback` getter.

Status at HEAD (2026-10-26) — this is now a `liquers-web`-only gap:

- **Core: done.** `Error::traceback: Option<String>` and `Error::with_traceback`
  (`liquers-core/src/error.rs`, `design/metadata-error-traceback/`); it reaches the metadata log
  and the axum error body.
- **`liquers-py`: done.** `pyerr_to_error` puts class and message in `message` and the formatted
  stack in `traceback` (owner decision, `design/liquers-py/` Phase 1 question 6; `ERROR` row
  `COMPLETE` in `liquers-py/README.md`).
- **`liquers-web`: not done**, as above.

## Problem Example

```javascript
liquers.registerCommand({ name: "bang", run: () => { throw new TypeError("bang"); } });
try { await liquers.evaluate("bang"); } catch (e) {
  e.message;    // today: "TypeError: bang\n    at run (...)\n ..."   expected: "TypeError: bang"
  e.traceback;  // today: undefined (no getter)                       expected: the frames
  e.jsClass;    // today: null
}
```

## Impact

A page cannot render the stack separately from the message or branch on the exception class
without parsing text, and Python and JavaScript errors look different to a consumer reading
`Error::traceback` (a log viewer, the axum error body).

## Expected behaviour

Follow the `liquers-py` route: `message` is `"<Class>: <message>"`, the frames go to
`Error::traceback`, `LiquersError` gains a `traceback` getter (and the `.d.ts` declares it), and
`ERROR03` asserts `traceback` on the **evaluation path**, not on the bridge in isolation.
`jsStack` becomes an alias of `traceback` or is removed; `jsClass` stays `null` on the evaluation
path until a structured language field exists (`MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING`, and
the error-schema decision of `ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED`).

## Discovery

Raised by review on PR #19 and confirmed by probing the real evaluation path (2026-08-08). `ERROR03`
tested the bridge in isolation, where `from_thrown` fills the fields — the test was true and the
shipped path was not. Progress noted 2026-10-11 and 2026-10-20; tightened and re-sized `M` → `S`,
area narrowed to `web`, on 2026-10-26 (project-groups plan).
