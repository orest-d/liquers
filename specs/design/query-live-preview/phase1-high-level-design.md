# Phase 1: High-Level Design — Query live preview

## Purpose

Let a person edit a query in the GUI and see what it produces before anything upstream changes:
a **live preview** that evaluates the edited query, an **Apply** that sets it, **Undo/Redo** over the
edits, and a value display that neither flickers on a fast result nor leaves the person guessing on
a slow one. It serves the interactive half of the vision (`specs/reference/VISION.md`: queries built
and inspected in a UI, results recomputed from recipes), where today each edit is either written
upstream at once (recomputing every dependent) or held unseen.

## Problem Example

**The console (a defect).** Query console with `-R/data/sales.csv/-/ns-pl/head-3`, editor panel
open (pencil). Set `n` to `7` in the panel:

- Today, with "Update immediately" on (the default), the text becomes `…/head-7`, but the value
  shown is still the value of `head-3`: the panel never submits. With it off, the edit is thrown
  away on the next frame and the checkbox turns itself back on
  (`QUERY-CONSOLE-EDITOR-LOSES-EDITS-WHEN-UPDATE-IMMEDIATELY-IS-OFF`, checked with a probe test on
  2026-11-24).
- Should: the panel has no such switch; shortly after the edit the console evaluates `…/head-7`.
  The three rows of `head-3` stay on screen until the seven rows are there. If that takes longer
  than a moment, a status bar shows "Processing" with progress, and the old rows are dimmed.

**A recipe's editor (a gap).** `lui/query_editor` bound to recipe 0 of `reports/recipes.yaml`,
beside a view of that recipe's result. Today every edit (with "Update immediately" on) rewrites
`recipes.yaml`, expires the recipe's dependents and recomputes every open view; with it off, the
person edits without seeing any result. Should: by default the edit is previewed, evaluated in the
recipe's folder and shown, while `recipes.yaml` and other viewers are unchanged until Apply.

## Scope and Acceptance Criteria

- **AC-1** Console edit re-evaluates
  WHEN an edit made in the console's editor panel is accepted
  THEN the console evaluates the new query after a short pause (debounced), without a Submit, and
  shows its value
- **AC-2** Console panel has no update switch
  WHEN the console's editor panel is drawn
  THEN it shows no "Update immediately", Update or Revert control, and no accepted edit is lost
- **AC-3** Fast result without flicker
  WHEN a new value arrives within the quiet threshold (proposed 250 ms)
  THEN the old value is replaced by the new one with no empty or "loading" frame in between
- **AC-4** Slow result in a status bar
  WHEN the new value has not arrived after the quiet threshold
  THEN a status bar shows the status, progress and the latest log message, and the old value stays
  visible but dimmed and labelled as the previous result
- **AC-5** Error keeps the old value
  WHEN the new evaluation fails
  THEN the status bar shows the error (and the query field highlights its position, as today),
  and the previous value stays dimmed
- **AC-6** Out-of-order results
  WHEN a result of an earlier query arrives after a newer query was requested
  THEN it is ignored: it never replaces the newer value or status
- **AC-7** Live preview in an editor
  WHEN an editor in Live preview mode accepts an edit
  THEN the edited query is evaluated and shown in its preview, the accessor is not written, and
  the editor shows the edit as pending, with Apply and Revert
- **AC-8** Apply and Revert
  WHEN the person presses Apply (Revert)
  THEN the working copy is written into the accessor (dropped and reloaded), as Update and Revert
  do today
- **AC-9** Apply immediately kept
  WHEN an editor is in Apply immediately mode
  THEN each accepted edit is written at once, as "Update immediately" does today
  (`query-pipeline-editor` AC-26)
- **AC-10** Undo and Redo
  WHEN the person undoes (redoes) an edit
  THEN the working copy returns to the previous (next) state; in Live preview the preview follows
  and nothing is written; in Apply immediately the restored value is written
- **AC-11** Preview where the value lives
  WHEN the edited query belongs to a recipe of a `recipes.yaml`
  THEN the preview is evaluated with that folder as its working directory, so it equals what the
  recipe would produce
- **AC-12** Browser parity
  WHEN the console or an editor is rendered in the browser (`render_web`)
  THEN AC-1 to AC-10 hold there too: the same status bar, dimming and controls

Non-goals: previewing while typing in a query text field (Enter or Go still submits;
`WEB-QUERY-EDITOR-NOT-LIVE` covers live syntax feedback); cancelling an evaluation on the server;
undo across sessions (the history is in memory); undo of the console's history navigation.

## Core Interactions

- **UI:** `ui::editor` (`EditorState` modes, undo history, preview request), `ui::widgets`
  (`QueryConsoleElement`: panel effects, debounced evaluation, value display), the egui and web
  renderers of both, and a shared value-display piece with the status bar.
- **Runner:** `AppRunner` (`ui::runner`): a preview evaluation for an editor, and snapshots that
  name the request they answer (AC-6).
- **Assets:** previews are ordinary evaluations; a recipe's preview needs its folder as cwd (AC-11).
- **Commands:** `lui/query_editor` and friends may gain a mode argument (Open Question 3).

## Crate Placement

`liquers-lib` only (`src/ui/`, `src/egui/`), behind the existing `egui` and `webui` features.
`liquers-core` is unchanged unless Phase 2 finds that a recipe preview needs a core entry point to
evaluate a query in a given cwd (it would go in `liquers-core/src/assets.rs` or `context.rs`).

## Language Integration

- Integration guide: no change. This changes the behaviour of existing Rust UI elements reached
  through `UIUSE`; it adds no value type, command or API a binding exposes.
- Python (`liquers-py`): not needed — Python has no UI elements of its own; its `query_editor` is
  the core `QueryEditor`, which this does not change.
- JavaScript (`liquers-web`): not needed — the browser console and editors are rendered by
  `liquers-lib`'s `webui`; `liquers-web`'s `query_editor` is the core editor. AC-12 covers the
  browser rendering inside this design.

## Documentation Intent

- Reference: extend `specs/reference/UI_INTERFACE_FSD.md` (editor modes, undo, console panel and
  value display).
- Guide: none.
- Documents to update: `specs/reference/UI_INTERFACE_FSD.md`.
- API docs: `ui::editor` (module doc: modes, undo, preview), `ui::widgets::query_console_element`
  (panel and value display), and the module of the shared value display.
- User guides: `docs/guides/rust/11-ui.md`, `docs/guides/python/11-ui.md` and
  `docs/guides/web/11-ui.md` where they describe "Update immediately" and the console's editor panel.

## Design Dependencies

- **Builds on:** `query-pipeline-editor` (AC-26 becomes AC-7 to AC-9 here) and
  `query-editor-ergonomics` (the console's panel), as changed by PRs #283 and #284.
- **Overlaps:** `QUERY-CONSOLE-EDITOR-CANNOT-BROWSE-THE-STORE` (S): the same function,
  `apply_parameter_commands`, drops the editor's effects; recommended to fold it in, since this
  design reworks that effect handling. `console-mounts-nested-element` (in review): it changes how
  the console holds a UI element result, which the dimmed previous value must also cover.
- **Fixes:** `QUERY-CONSOLE-EDITOR-LOSES-EDITS-WHEN-UPDATE-IMMEDIATELY-IS-OFF`,
  `QUERY-EDITOR-HAS-NO-LIVE-PREVIEW`.

## Open Questions

1. **Update model** — A (Update immediately + Undo), B (Live preview + Apply only) or C (Live preview
   by default, Apply immediately as an option, local Undo/Redo in both). Recommended: C. Asked on
   2026-11-24; this draft is written on C.
2. **Where an editor's preview is shown.** Recommended: in the followed view when the editor has
   one (`EditorElement::with_follow`), otherwise in a compact preview pane under the editor, drawn
   by the same value display as the console.
3. **Mode in the command.** Recommended: no new argument for `lui/query_editor`; the mode is saved
   with the element, Live preview for a new one, and an element saved with `continuous: true`
   loads as Apply immediately.
4. **Thresholds.** Recommended: a quiet threshold of 250 ms (AC-3) and a 300 ms debounce (AC-1).
5. **Long waits.** Recommended: the previous value stays dimmed however long it takes, rather than
   disappearing or blurring (blur is not cheap in egui; dimming is cheap in both backends).
6. **Undo keys.** Recommended: Ctrl+Z and Ctrl+Shift+Z (Ctrl+Y too) in the editor, plus toolbar
   buttons; in the console's panel they undo panel edits only.

## References

- `specs/design/query-pipeline-editor/` (AC-26), `specs/design/query-editor-ergonomics/`
- `specs/reference/UI_INTERFACE_FSD.md` (editors, the console's editor panel)
- `liquers-lib/src/ui/widgets/query_console_element.rs` (`panel_tree`, `parameters_tree`,
  `apply_parameter_commands`, `submit_query`), `liquers-lib/src/ui/editor/state.rs` (`commit`,
  `continuous`), `liquers-lib/src/ui/runner.rs` (`handle_request_asset_updates`)
