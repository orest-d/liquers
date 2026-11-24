# Phase 1: High-Level Design — Apply immediately and Update

## Purpose

Make the GUI query editors and the query console behave the same simple way: a GUI edit always
rewrites the query at once; with **Apply immediately** on, the value is recalculated (and the
accessor written) at once too, and with it off, only on **✓** or Enter. Typed text counts only on
Enter, and only when it is valid. Undo/Redo covers both, and the value display neither flickers on
a fast result nor leaves the person guessing on a slow one. It serves the interactive half of
`specs/reference/VISION.md`: queries built and inspected in a UI, results recomputed from recipes.

## Problem Example

Query console with `-R/data/sales.csv/-/ns-pl/head-3`, editor panel open (pencil). Set `n` to `7`
in the panel:

- **Today**, with "Update immediately" on (the default), the text becomes `…/head-7`, but the value
  shown is still that of `head-3`: the panel never submits. With it off, the edit is thrown away on
  the next frame and the checkbox turns itself back on
  (`QUERY-CONSOLE-EDITOR-LOSES-EDITS-WHEN-UPDATE-IMMEDIATELY-IS-OFF`, checked with a probe test on
  2026-11-24). The panel's browse button lists nothing
  (`QUERY-CONSOLE-EDITOR-CANNOT-BROWSE-THE-STORE`). A submit blanks the value at once, so even a
  fast result flickers.
- **Should**, with Apply immediately on: the text becomes `…/head-7` and, after a short pause, the
  console evaluates it. The three rows stay on screen until the seven rows are there; if that takes
  more than a moment, a status bar shows "Processing" with progress and the old rows are dimmed.
  With it off: the text becomes `…/head-7`, the ✓ beside the query field lights up, and nothing is
  evaluated until ✓ or Enter.

## Scope and Acceptance Criteria

- **AC-1** GUI edit with Apply immediately on
  WHEN an edit made in the GUI (the console's panel, or a standalone editor) is accepted with Apply
  immediately on
  THEN the query text shows the edit at once, and the value is recalculated (a standalone editor's
  accessor written) after a short pause, a burst of edits giving one recalculation
- ~~**AC-2** Console panel has no update switch~~ — retired 2026-11-24 (Orest: the console keeps
  the setting); replaced by AC-13
- **AC-3** Fast result without flicker
  WHEN a new value arrives within the quiet threshold (250 ms)
  THEN the old value is replaced by the new one with no empty or "loading" frame in between
- **AC-4** Slow result in a status bar
  WHEN the new value has not arrived after the quiet threshold
  THEN a status bar shows the status, progress and the latest log message, and the old value stays
  visible, dimmed and labelled as the previous result, however long it takes
- **AC-5** Error keeps the old value
  WHEN the new evaluation fails
  THEN the status bar shows the error, the query field highlights its position (as today), and the
  previous value stays dimmed
- **AC-6** Out-of-order results
  WHEN a result of an earlier query arrives after a newer query was requested
  THEN it is ignored: it never replaces the newer value or status
- ~~**AC-7** Live preview in an editor~~ — retired 2026-11-24 (Orest chose Apply immediately
  on/off); deferred to `QUERY-EDITOR-HAS-NO-LIVE-PREVIEW`
- **AC-8** GUI edit with Apply immediately off
  WHEN a GUI edit is accepted with Apply immediately off
  THEN the query text shows the edit at once and is marked pending; the value is recalculated (the
  accessor written) only on ✓ or Enter
- **AC-9** Standalone editor keeps AC-26
  WHEN a standalone editor (`lui/query_editor`) is bound to an accessor
  THEN Apply immediately on writes the accessor as "Update immediately" does today
  (`query-pipeline-editor` AC-26), with AC-1's grouping of bursts; off holds the edits for ✓
- **AC-10** Undo and Redo
  WHEN the person undoes (redoes) an edit (Ctrl+Z, Ctrl+Shift+Z or the toolbar)
  THEN the query returns to the previous (next) state, and is applied as AC-1 or AC-8 say for the
  current setting
- ~~**AC-11** Preview where the value lives~~ — retired 2026-11-24 with AC-7
- **AC-12** Browser parity
  WHEN the console or an editor is rendered in the browser (`render_web`)
  THEN the other criteria hold there too: the same controls, status bar and dimming
- **AC-13** One set of controls
  WHEN the console is drawn
  THEN its toolbar has the Apply immediately setting and a ✓ icon button beside the query field,
  enabled while the text differs from the evaluated query; its editor panel shows no setting,
  Update or Revert of its own. A standalone editor shows the same ✓ beside its own query field
- **AC-14** Typed text counts on Enter, when valid
  WHEN the person types in a query field (console or editor)
  THEN nothing is applied while typing; Enter (or ✓) applies the text if it validates (parses and
  names registered commands), and otherwise applies nothing, keeps the old value and highlights the
  error in the field
- **AC-15** No edit is lost
  WHEN Apply immediately is off and GUI edits are pending, or the text is retyped
  THEN the panel keeps the edits and the setting; the setting is saved with the element
- **AC-16** The console's panel browses the store
  WHEN the browse button of a resource in the console's editor panel is pressed
  THEN the store is listed as in a standalone editor

Non-goals: a live preview that evaluates without writing (`QUERY-EDITOR-HAS-NO-LIVE-PREVIEW`);
live syntax feedback while typing (`query-editor-live-validation`, `WEB-QUERY-EDITOR-NOT-LIVE`);
cancelling an evaluation; undo across sessions (the history is in memory).

## Core Interactions

- **UI:** `ui::editor` (the setting, undo history, the ✓ and Enter rule), `ui::widgets`
  (`QueryConsoleElement`: panel effects, debounced evaluation, value display), both renderers,
  and a value display with the status bar shared by the console and the followed views.
- **Runner:** `AppRunner` (`ui::runner`): snapshots that name the request they answer (AC-6), and
  the console's directory requests (AC-16).
- **Commands:** none change; `lui/query_editor` takes no new argument.

## Crate Placement

`liquers-lib` only (`src/ui/`, `src/egui/`), behind the existing `egui` and `webui` features.

## Language Integration

- Integration guide: no change — existing Rust UI elements reached through `UIUSE` behave
  differently; no value type, command or binding API is added.
- Python (`liquers-py`): not needed — its `query_editor` is the core `QueryEditor`, unchanged.
- JavaScript (`liquers-web`): not needed — the browser console and editors are rendered by
  `liquers-lib`'s `webui` (AC-12); `liquers-web`'s `query_editor` is the core editor.

## Documentation Intent

- Reference: extend `specs/reference/UI_INTERFACE_FSD.md` (the setting, ✓ and Enter, undo, the
  console's panel and value display).
- Guide: none.
- Documents to update: `specs/reference/UI_INTERFACE_FSD.md`.
- API docs: `ui::editor`, `ui::widgets::query_console_element`, and the module of the shared value
  display.
- User guides: `docs/guides/rust/11-ui.md`, `docs/guides/python/11-ui.md` and
  `docs/guides/web/11-ui.md`, where they describe "Update immediately" and the console's panel.

## Design Dependencies

- **Builds on:** `query-pipeline-editor` (AC-26) and `query-editor-ergonomics`, as changed by
  PRs #283 and #284.
- **Fixes:** `QUERY-CONSOLE-EDITOR-LOSES-EDITS-WHEN-UPDATE-IMMEDIATELY-IS-OFF`,
  `QUERY-CONSOLE-EDITOR-CANNOT-BROWSE-THE-STORE` (folded in 2026-11-24: the same dropped effects).
- **Overlaps:** `console-mounts-nested-element` (in review): it changes how the console holds a UI
  element result, which the dimmed previous value must also cover.
- **Defers:** `QUERY-EDITOR-HAS-NO-LIVE-PREVIEW`.

## Open Questions

Discussed with Orest on 2026-11-24; the recommended answers are taken unless he disagrees.

1. **Update model.** Orest's: Apply immediately on (GUI edits recalculate at once) or off (on ✓
   or Enter); typed text only on Enter, when valid. Live preview deferred.
2. **A recipe editor with Apply immediately on writes every edit.** Accepted; bursts (a slider
   drag) are grouped into one write after a 300 ms pause (AC-1).
3. **Where a standalone editor's value shows.** In the view it follows (`with_follow`); no
   preview pane.
4. **Saving the setting.** Saved with each element (the console too); `lui/query_editor` takes no
   new argument; an element saved with `continuous` keeps it.
5. **Timing.** Quiet threshold 250 ms (AC-3), debounce 300 ms (AC-1).
6. **A slow result.** The old value stays dimmed rather than disappearing (AC-4).
7. **Undo keys and Revert.** Ctrl+Z, Ctrl+Shift+Z (and Ctrl+Y). The console has no Revert (Undo
   covers it); a standalone editor keeps Revert, which reloads the accessor after a conflict.
8. **Enter on an invalid query.** Applies nothing and highlights the error (AC-14).

## References

- `specs/design/query-pipeline-editor/` (AC-26), `specs/design/query-editor-ergonomics/`
- `specs/reference/UI_INTERFACE_FSD.md` (editors, the console's editor panel)
- `liquers-lib/src/ui/widgets/query_console_element.rs` (`panel_tree`, `parameters_tree`,
  `apply_parameter_commands`, `submit_query`), `liquers-lib/src/ui/editor/state.rs` (`commit`,
  `continuous`), `liquers-lib/src/ui/runner.rs` (`handle_request_asset_updates`)
