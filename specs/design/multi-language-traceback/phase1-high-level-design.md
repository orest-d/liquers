# Phase 1: High-Level Design — Multi-language traceback

## Purpose

An error crossing several languages carries one flat traceback string, so a consumer cannot tell
where one language's stack ends, which language it is, or which command it belongs to. This design
adds ordered traceback **segments** beside the string, which stays as the display form, so a
notebook, log viewer or page (VISION: embedded in Python, browser) can show each stack apart.

## Problem Example

Python, at HEAD (`liquers-py/tests/test_ERROR_errors.py`
`test_ERROR_nested_python_tracebacks_are_concatenated`):

```python
@env.first_command
def inner_fails():
    raise ValueError("deep")

@env.first_command
def outer_calls(context):
    return context.evaluate("inner_fails")

env.evaluate("outer_calls")   # raises lq.ExecutionError
```

Today `raised.value.traceback` is the two stacks joined by a newline (`pyerr_to_error`,
`liquers-py/src/error.rs`), with no boundary, language or command; `ErrorPayload::traceback` is
`Option<String>`. Should be: `raised.value.traceback_segments` lists
`("python", "inner_fails", …)` then `("python", "outer_calls", …)`, and the string shows a header
line before each segment. Both queries validate (`--command outer_calls --command inner_fails`).

## Scope and Acceptance Criteria

- **AC-1** Nested Python segments
  WHEN the problem example runs
  THEN `traceback_segments` holds two `python` segments, innermost first, attributed to
  `inner_fails` and `outer_calls`, and `traceback` contains both, in that order
- **AC-2** One segment renders as today
  WHEN an error has exactly one segment
  THEN `traceback` is that segment's text, unchanged (no header)
- **AC-3** Several segments render with headers
  WHEN an error has two or more segments
  THEN `traceback` is each segment's text preceded by `--- <language> (<command>) ---`, innermost
  first; an empty language reads `unknown`, and ` (<command>)` is omitted when there is none
  (`--- unknown ---`, `--- python ---`)
- **AC-4** Persisted and backward compatible
  WHEN an error with segments is stored as `error_data` and as an error `LogEntry`, in JSON and YAML
  THEN both round-trip their segments; WHEN a record written before the field existed is read
  THEN it loads with no segments and its `traceback` unchanged
- **AC-5** A language error keeps segments across a round trip
  WHEN a Liquers error passes through a Python `LiquersError` (or a JavaScript `LiquersError`) and back
  THEN its segments are unchanged, and the language's own frames are appended as one more segment
- **AC-6** JavaScript segment (needs `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`)
  WHEN a JavaScript command throws an `Error` on the evaluation path
  THEN the error has one `javascript` segment holding the stack, attributed to the command
- **AC-7** Flat text still accepted
  WHEN code sets a traceback with `Error::with_traceback` and a language then appends a segment
  THEN the earlier text is kept as a first segment of unknown language

If that prerequisite is not done when steps 1-5 land, AC-6 and the JavaScript half of AC-5 are
deferred and spun off as an issue under Phase 4 step 7. `jsClass` and `jsStack` stay as the
prerequisite leaves them; `tracebackSegments` is independent of both, not derived from them.

Non-goals: native Rust backtraces; the Liquers path between segments (`error-with-key-field`); a
JavaScript class field; the HTTP body shape; an egui rendering.

## Core Interactions

Error (`error.rs`: type, field, rendering), metadata (`LogEntry`), plan (`interpreter.rs`
attributes segments), bindings (`liquers-py` errors and metadata, `liquers-web` errors and `.d.ts`).

## Crate Placement

`liquers-core` owns the type (every integration depends on it); bindings fill and expose it.

## Documentation Intent

- Reference: `specs/reference/PYTHON_INTEGRATION.md` §6. Guide: `specs/guides/PYTHON_GUIDE.md`,
  `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §ERROR and §6 Python.
- Other: both binding READMEs' `ERROR` rows. To update: these; `specs/README.md` row.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — size `L`; a `pub` struct and field in `liquers-core`; new
  Python and JavaScript attributes (rules 1, 3, 4).
- **Leading issue:** Q1, since the maintainer's position (2026-10-19) is "a flat string for now".
- **Explanation:** fully specified under the recommended answers; the choice changes a serialized
  format and two binding surfaces, so it is the maintainer's.
- **Open questions:**
  1. **Open design - shape.** (a) separator headers in the string only (consumers still parse
     text); (b) **recommended**: `traceback_segments: Vec<TracebackSegment { language, command,
     text }>`, additive, with `traceback` rendered from it using (a)'s headers; (c) a
     language-context field (declined by the maintainer). Under (a) a viewer splitting on `--- `
     misreads a frame that prints such a line; under (b) it reads data.
  2. **Open design - Rust frames.** Recommended none: no native backtrace; the Liquers path is
     the command attribution plus `error-with-key-field`'s frames.
  3. **Open design - binding surface.** Recommended: Python `LiquersError.traceback_segments`
     (tuple of named tuples `lq.TracebackSegment`) and `LogEntry.traceback_segments`; JavaScript
     `LiquersError.tracebackSegments` (array of `{language, command, text}`).
  4. **Proposed resolution - rendering.** As AC-2/AC-3; lowercase open-ended language names, empty = unknown.

## Design Dependencies

| Design / issue | Relation | Effect |
|---|---|---|
| `error-with-key-field` | overlaps | T3 (both extend `ErrorPayload` and the action error mapping); not merged: Phase 4 finished, merge `XL` (E2). Second to land rebases; `command` uses its `Action` frame label. |
| `error-type-taxonomy` | overlaps | Same file, independent; only surviving constructors are used. |
| `web-thrown-back-error-context` | overlaps | T3: both edit `js_value_as_liquers_error`; not merged (E1: it is eligible). Second to land rebases. |
| `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` (issue) | requires | Step 6 (AC-6, JS half of AC-5) waits for it. |
| `py-metadata-typed-errors` | requires | Edits `liquers-py/src/error.rs` and `metadata.rs`; it lands first, step 5 rebases. |
| `py-metadata-fields-as-properties` | overlaps | Makes `metadata.log()` a property; whichever lands later updates test 13's call. |
| `language-store-config-function` | overlaps | A handler's exception carries segments through the same bridges; no order. |

## Consolidated Findings

- `liquers-web` never fills `Error::traceback` (`js_error_to_liquers` appends to `message`): hence the `requires`.
- With segments present `traceback` is their rendering; `with_traceback` clears them.
- Attribution sits in the interpreter, so no binding needs its command.
- `js_value_as_liquers_error` drops `query`, `key`, `position`, `traceback`: filed as
  `WEB-THROWN-BACK-LIQUERSERROR-LOSES-CONTEXT` (`web-thrown-back-error-context`); step 6 restores
  the traceback only.
- Additive serde; no registry change.
