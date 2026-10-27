# Phase 1: High-Level Design — Multi-language traceback

## Purpose

An error that crosses several languages (Rust → Python → Rust → Python, later Rhai or Starlark)
carries one flat traceback string, so a consumer cannot tell where one language's stack ends, which
language it came from, or which command it belongs to. This design adds an ordered list of
traceback **segments** beside the string, which stays as the display form. It serves VISION's
"embedded in Python" and browser uses: a notebook, a log viewer or a page can show, filter or fold
each language's stack.

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

Today `raised.value.traceback` is `inner_fails`'s formatted stack followed by `outer_calls`'s
frames, joined by a bare newline (`pyerr_to_error` in `liquers-py/src/error.rs`). Nothing marks the
boundary or names a language or command; `Error::traceback` is `Option<String>`
(`liquers-core/src/error.rs` `ErrorPayload`). Should be: `raised.value.traceback_segments` lists
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
  THEN `traceback` is each segment's text preceded by `--- <language> (<command>) ---`, innermost first
- **AC-4** Persisted and backward compatible
  WHEN an error with segments is stored as `error_data` and as an error `LogEntry`, in JSON and YAML
  THEN both round-trip their segments; WHEN a record written before the field existed is read
  THEN it loads with no segments and its `traceback` unchanged
- **AC-5** A language error keeps segments across a round trip
  WHEN a Liquers error passes through a Python `LiquersError` (or a JavaScript `LiquersError`) and back
  THEN its segments are unchanged, and the language's own frames are appended as one more segment
- **AC-6** JavaScript segment
  WHEN a JavaScript command throws an `Error` on the evaluation path
  THEN the error has one `javascript` segment holding the stack, attributed to the command
- **AC-7** Flat text still accepted
  WHEN code sets a traceback with `Error::with_traceback` and a language then appends a segment
  THEN the earlier text is kept as a first segment of unknown language

Non-goals: native Rust backtraces; the Liquers-level path between segments (queries, keys, action
frames: `error-with-key-field`); a JavaScript exception class field; HTTP body shape (axum keeps the
derived string as lines); an egui rendering.

## Core Interactions

- Error (`liquers-core/src/error.rs`): segment type, payload field, rendering, attribution.
- Value/metadata (`liquers-core/src/metadata.rs` `LogEntry`): segments copied into error log entries.
- Plan (`liquers-core/src/interpreter.rs`): the action error path attributes segments to its command.
- Bindings: `liquers-py` (`error.rs`, `metadata.rs`, `python/liquers/_errors.py`) and `liquers-web` (`error.rs`, `.d.ts`).

## Crate Placement

The type lives in `liquers-core`, which every integration already depends on; each binding only
fills and exposes it. Nothing goes in `liquers-lib`.

## Documentation Intent

- Reference: extend `specs/reference/PYTHON_INTEGRATION.md` §6 Errors (segments replace "open").
- Guide: extend `specs/guides/PYTHON_GUIDE.md` (error fields table) and `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §ERROR and §6 Python.
- Other documents: `liquers-web/README.md` `ERROR` row, `liquers-py/README.md` `ERROR` row.
- Documents to update: the four above; `specs/README.md` gains this design's row.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — size `L`; adds a `pub` struct and a `pub` field to
  `liquers-core` and new attributes to the Python and JavaScript bindings (rules 1, 3 and 4).
- **Leading issue:** **Open design - traceback shape** (Q1), because the maintainer's recorded
  position (2026-10-19) is "a flat string for now".
- **Explanation:** a working solution is fully specified under the recommended answers; Phases 3-4
  follow it. The choice changes a serialized format and two binding surfaces, so it is the
  maintainer's to make.
- **Open questions:**
  1. **Open design - traceback shape.** (a) *Documented separator only*: the string gains headers,
     nothing structured; cheapest, but a consumer still parses text. (b) *Segments plus derived
     string* (**recommended**): `traceback_segments: Vec<TracebackSegment { language, command, text }>`,
     serialized additively; `traceback` stays and is rendered from the segments with (a)'s headers,
     so every current consumer keeps working. (c) *Fold into a language-context field*
     (`LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`'s option): the maintainer declined that field for
     now. Incompatible example: under (a) a viewer splitting on `--- ` misreads a Python frame that
     prints such a line; under (b) it reads data.
  2. **Open design - Rust frames between segments.** Recommended: record none. A native
     `std::backtrace` is costly, empty in most builds and meaningless to a language user; the
     Liquers path between segments is the command attribution here plus `error-with-key-field`'s
     action frames.
  3. **Open design - binding surface.** Recommended: Python `LiquersError.traceback_segments` (a
     tuple of `lq.TracebackSegment(language, command, text)`, a named tuple) and
     `LogEntry.traceback_segments`; JavaScript `LiquersError.tracebackSegments` (an array of
     `{language, command, text}`).
  4. **Proposed resolution - rendering.** One segment renders as its bare text (AC-2), so the
     single-language output does not change; headers appear only from two segments on. Language
     names are lowercase strings (`python`, `javascript`), open-ended so a new language needs no
     core change; an empty name means unknown.

## Design Dependencies

| Design / issue | Relation | Effect |
|---|---|---|
| `error-with-key-field` | overlaps | Shared change site (T3): both add a list to `ErrorPayload` and edit the action error mapping in `interpreter.rs`. Not merged: its Phase 4 is finished (bulk-design guide §1) and the merge would be `XL` (E2). Whichever lands second rebases; a segment's `command` uses its `Action` frame's label form. |
| `error-type-taxonomy` | overlaps | Same file, independent; lists structured tracebacks as a non-goal. Only surviving constructors are used. |
| `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` (issue) | requires | Moves the JavaScript stack from `message` to `traceback`; Phase 4 step 6 waits for it, steps 1-5 do not. |

## Consolidated Findings

- **Premise corrected:** `liquers-web` does not fill `Error::traceback`; `js_error_to_liquers`
  appends the stack to `message`. Hence the `requires` above.
- **Invariant:** with segments present, `traceback` is their rendering; `with_traceback` clears them.
- **Attribution** happens in the interpreter's action error mapping, so no binding needs to know its command.
- **Gap found, not in scope:** `liquers-web` `js_value_as_liquers_error` drops `query`, `key`,
  `position` and `traceback` of a thrown-back `LiquersError`; step 6 restores the traceback only.
  Reported for filing.
- **Compatibility:** additive serde; records without segments are unchanged; no registry change.
