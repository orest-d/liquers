---
id: WEB-QUERY-EDITOR-NOT-LIVE
kind: feature
title: The browser query console checks and highlights a query only when it re-renders, not while typing
status: draft
priority: P3
complexity: M
area: [lib/ui]
design: web-query-editor-live-preview
created: 2026-10-21
github:
---
## Problem

`specs/design/query-editor-live-validation/` gives the egui console live analysis (syntax tokens
and the first parse or planning error while typing) and gives the browser console the same tokens
and diagnostic as **rendered** markup: a highlighted preview under the plain `<input>` produced by
`render_web`. `render_web` runs when the element is re-rendered (after a submit or a snapshot), so
in the browser the preview follows the submitted text, not the keystrokes.

Live analysis in the browser needs the typed text to reach the element (or a pure wasm-side
function) on every `input` event, and the preview node to be patched without re-rendering the
input, which would lose focus and caret. `specs/design/ui-events/` delivers user events through
`update()` but lists per-keystroke field sync as a non-goal.

## Problem Example

In `examples-web/ui_query_console_app`, the user edits the console input from `hello` to
`hello/q/ns-lui/query_c` without pressing Go. Once `query-editor-live-validation` is implemented, it
is still not implemented at `cd6d5da`. The preview will then keep showing `hello`, with no error.
`Action 'query_c' not registered…` (byte 15, `liquers-validate --command hello`) appears only after
Go. It should appear in the preview while typing, without the input losing focus.

## Impact

Browser users see a mistake only after submitting, as egui users did before the design. Low
urgency: the error is still positioned and highlighted after submission.

## Expected behaviour

While the user types in the browser console input, the preview shows the analysis of the current
text (tokens and the first diagnostic), as the egui console does, without losing focus or caret.
Options: an `input` listener in the wasm driver calling `analyse_query` and patching the preview
node directly (no `AppState` round trip), or a per-keystroke `UserEvent` once `ui-events` allows it.

## Discovery

Split out of `specs/design/query-editor-live-validation/` (bulk design, 2026-10-21), which keeps
browser highlighting at render time to stay within the current event model.
