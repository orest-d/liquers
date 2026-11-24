# Phase 1: High-Level Design — Update modes: on demand, live preview, apply immediately

## Purpose

Give the GUI query editors and the query console one model with three **update modes**: a GUI
edit always rewrites the query at once, and the mode decides what follows. **On demand** waits for
**✓** or Enter. **Live preview** evaluates the edit in a preview, so the impact on one recipe is
seen before the recipe changes and before its change cascades into costly dependents. **Apply
immediately** writes it, so the dependents expire and views of them recalculate. Typed text counts
only on Enter, when valid; Undo/Redo covers every mode; and the value display neither flickers on a
fast result nor leaves the person guessing on a slow one. This serves the interactive half of
`specs/reference/VISION.md`: queries and recipes edited in a UI, with results recomputed from
recipes as they change.

## Problem Example

**The console (a defect).** Query console with `-R/data/sales.csv/-/ns-pl/head-3`, editor panel
open (pencil). Set `n` to `7` in the panel:

- **Today**, with "Update immediately" on (the default), the text becomes `…/head-7` but the value
  is still that of `head-3`: the panel never submits. With it off, the edit is thrown away on the
  next frame and the checkbox turns itself back on
  (`QUERY-CONSOLE-EDITOR-LOSES-EDITS-WHEN-UPDATE-IMMEDIATELY-IS-OFF`, checked with a probe test on
  2026-11-24). The panel's browse button lists nothing
  (`QUERY-CONSOLE-EDITOR-CANNOT-BROWSE-THE-STORE`). A submit blanks the value, so even a fast result
  flickers.
- **Should**, in Apply immediately: the text becomes `…/head-7` and, after a short pause, the
  console evaluates it. The three rows stay until the seven rows are there; if that takes more than
  a moment, a status bar shows "Processing" with progress and the old rows are dimmed. In On
  demand: the text becomes `…/head-7`, the ✓ beside the query field lights up, and nothing is
  evaluated until ✓ or Enter.

**A recipe and its dependents (a gap).** An editor of recipe 0 of `reports/recipes.yaml`, and a
console showing a report that depends on it. Today an edit either rewrites `recipes.yaml` at once
(expiring the report, which the console then recomputes) or is held unseen. Should: in Live
preview the view the editor follows shows the recipe's new result in place, evaluated in
`reports` and framed as a preview, while the file and the report are unchanged; ✓ applies it, and the console of the report, in Apply
immediately, recalculates on the expiry; in On demand it dims the report and offers ✓.

## Scope and Acceptance Criteria

- **AC-1** GUI edit in Apply immediately
  WHEN an edit made in the GUI (the console's panel, or an editor) is accepted in Apply immediately
  THEN the query text shows it at once, and after a short pause the value is recalculated (an
  editor's accessor written); a burst of edits gives one recalculation
- ~~**AC-2** Console panel has no update switch~~ — retired 2026-11-24; replaced by AC-13
- **AC-3** Fast result without flicker
  WHEN a new value arrives within the quiet threshold (250 ms)
  THEN the old value is replaced by the new one with no empty or "loading" frame in between
- **AC-4** Slow result in a status bar
  WHEN the new value has not arrived after the quiet threshold
  THEN a status bar shows the status, progress and the latest log message, and the old value stays
  visible, dimmed and labelled as the previous result, however long it takes
- **AC-5** Error keeps the old value
  WHEN the new evaluation fails
  THEN the status bar shows the error, the query field highlights its position, and the previous
  value stays dimmed
- **AC-6** Out-of-order results
  WHEN a result of an earlier query arrives after a newer query was requested
  THEN it is ignored: it never replaces the newer value or status
- ~~**AC-7** Live preview in an editor~~ — retired 2026-11-24; restated as AC-17
- **AC-8** GUI edit On demand
  WHEN a GUI edit is accepted On demand
  THEN the query text shows it at once, marked pending; it is evaluated (an editor's accessor
  written) only on ✓ or Enter
- **AC-9** Apply immediately in an editor keeps AC-26
  WHEN an editor bound to an accessor is in Apply immediately
  THEN it writes the accessor as "Update immediately" does today (`query-pipeline-editor` AC-26),
  with AC-1's grouping of bursts
- **AC-10** Undo and Redo
  WHEN the person undoes (redoes) an edit with the toolbar buttons, Ctrl+Z or Ctrl+Shift+Z (Ctrl+Y)
  THEN the query returns to the previous (next) state and is handled as the current mode says
- ~~**AC-11** Preview where the value lives~~ — retired 2026-11-24; restated as AC-18
- **AC-12** Browser parity
  WHEN the console or an editor is rendered in the browser (`render_web`)
  THEN the other criteria hold there too: the same controls, status bar and dimming
- **AC-13** One set of controls
  WHEN the console is drawn
  THEN its toolbar has the mode and a ✓ icon button beside the query field, enabled while the text
  differs from the evaluated query; its editor panel shows no mode, Update or Revert of its own. An
  editor shows the same ✓ beside its own query field
- **AC-14** Typed text counts on Enter, when valid
  WHEN the person types in a query field (console or editor)
  THEN nothing is applied while typing; Enter (or ✓) applies the text if it validates (parses and
  names registered commands), and otherwise applies nothing, keeps the old value and highlights the
  error
- **AC-15** No edit is lost
  WHEN edits are pending, or the text is retyped
  THEN the panel keeps the edits and the mode; the mode is saved with the element
- **AC-16** The console's panel browses the store
  WHEN the browse button of a resource in the console's editor panel is pressed
  THEN the store is listed as in a standalone editor
- **AC-17** Live preview in an editor
  WHEN an editor bound to an accessor is in Live preview and accepts a GUI edit
  THEN after a short pause the edited query is evaluated and shown **in place** of the applied
  value, in the view the editor follows (or, with none, the editor's own value area); the accessor
  is not written and nothing that depends on it expires until ✓; Esc or Undo drops the preview and
  the view shows the applied value again. A value is never shown twice
- **AC-18** A recipe is previewed in its folder
  WHEN the previewed query is a recipe of a `recipes.yaml`
  THEN it is evaluated with that folder as its working directory, so it equals what the recipe
  would produce
- **AC-19** A view follows an expiry by its mode
  WHEN the asset a console shows expires (an applied upstream edit)
  THEN in Apply immediately the console recalculates it (as today); On demand it keeps the value,
  dimmed with a "changed upstream" note, and recalculates on ✓
- **AC-20** Mode per element
  WHEN a console or editor is created by `lui/query_console` or an editor command with a mode
  argument (a UI spec can open an expensive query On demand)
  THEN it starts in that mode; without one, a console starts in Apply immediately and an editor
  bound to an accessor in Live preview. The console offers two modes, Apply immediately and On
  demand, since nothing depends on its own query
- **AC-21** A slow evaluation suggests On demand
  WHEN an evaluation in Apply immediately or Live preview runs past a few seconds
  THEN the status bar offers a one-click switch to On demand; the mode never changes by itself
- **AC-22** Visual cues tell preview, stale and mode apart
  WHEN a view shows a preview, a stale value, or an element is in a given mode
  THEN a preview is framed in the theme's preview colour with the status bar line "Preview of
  unapplied edits — ✓ applies, Esc discards"; a stale value is dimmed (a slow preview is both); the
  mode is a segmented control of three icons (hand: On demand, eye: Live preview, lightning: Apply
  immediately), and the ✓ follows it: an outline that fills when edits are pending, the preview
  colour in Live preview, and in Apply immediately a lightning indicator instead, active while
  writing

Non-goals: live syntax feedback while typing (`query-editor-live-validation`,
`WEB-QUERY-EDITOR-NOT-LIVE`); cancelling an evaluation; undo across sessions; knowing a query's cost
in advance (`QUERY-COST-IS-UNKNOWN-TO-THE-UI`).

## Core Interactions

- **UI:** `ui::editor` (modes, undo history, preview request, ✓ and Enter), `ui::widgets`
  (`QueryConsoleElement`: panel effects, debounced evaluation, expiry by mode), both renderers, and
  a value display with the status bar, preview frame and dimming, shared by the console and the
  views an editor follows; a theme colour token for the preview (`ui::theme`).
- **Runner:** `AppRunner` (`ui::runner`): snapshots that name the request they answer (AC-6), a
  preview evaluation in a recipe's folder (AC-17, AC-18), and the console's directory requests.
- **Assets:** previews are ordinary evaluations with a cwd; expiry notifications reach views as
  today.
- **Commands:** `lui/query_console` and the editor commands (`query_editor`, `action_editor`,
  `resource_editor`, `key_editor`) gain a `mode` argument with a default;
  `specs/command_registry.yaml` is regenerated.

## Crate Placement

`liquers-lib` (`src/ui/`, `src/egui/`), behind the existing `egui` and `webui` features.
`liquers-core` only if Phase 2 finds no way to evaluate a query with a given cwd from the runner.

## Language Integration

- Integration guide: no change — existing Rust UI elements reached through `UIUSE` behave
  differently and two commands gain an argument; no value type or binding API is added.
- Python (`liquers-py`): not needed — its `query_editor` is the core `QueryEditor`, unchanged;
  `lui` commands are reached through queries as before.
- JavaScript (`liquers-web`): not needed — the browser console and editors are rendered by
  `liquers-lib`'s `webui` (AC-12); `liquers-web`'s `query_editor` is the core editor.

## Documentation Intent

- Reference: extend `specs/reference/UI_INTERFACE_FSD.md` (modes, ✓ and Enter, undo, in-place preview and its cues,
  expiry by mode, the console's panel and value display).
- Guide: none.
- Documents to update: `specs/reference/UI_INTERFACE_FSD.md`, `specs/command_registry.yaml`
  (generated).
- API docs: `ui::editor`, `ui::widgets::query_console_element`, `ui::commands`, and the module of
  the shared value display.
- User guides: `docs/guides/rust/11-ui.md`, `docs/guides/python/11-ui.md` and
  `docs/guides/web/11-ui.md`, where they describe "Update immediately" and the console's panel.

## Design Dependencies

- **Builds on:** `query-pipeline-editor` (AC-26) and `query-editor-ergonomics`, as changed by
  PRs #283 and #284; `recipe-dependency-versions` (PR #276: a write of `recipes.yaml` expires the
  recipe's dependents, which AC-19 relies on).
- **Fixes:** `QUERY-CONSOLE-EDITOR-LOSES-EDITS-WHEN-UPDATE-IMMEDIATELY-IS-OFF`,
  `QUERY-CONSOLE-EDITOR-CANNOT-BROWSE-THE-STORE`, `QUERY-EDITOR-HAS-NO-LIVE-PREVIEW`.
- **Overlaps:** `console-mounts-nested-element` (in review): it changes how the console holds a UI
  element result, which the dimmed previous value must also cover.
- **Files:** `QUERY-COST-IS-UNKNOWN-TO-THE-UI` (later: a command hint marking a query expensive,
  so defaults can follow it).

## Open Questions

Discussed with Orest on 2026-11-24; answers he agreed to are marked.

1. **Three modes** (agreed): On demand, Live preview, Apply immediately; typed text only on Enter.
2. **Where a preview shows** (agreed: never twice): in place, in the followed view, framed; the
   editor's own value area only when it follows no view. Visual cues (AC-22): frame and status
   line in a preview colour, dimming for stale, a three-icon mode control, and a ✓ that follows the
   mode; a tinted query field for pending text was considered and left out (the ✓ already shows it).
   Orest agreed to frame, ✓ by mode, mode control and status line on 2026-11-24.
3. **Defaults** (proposed): a `mode` argument with a default (AC-20); no automatic switch, only a
   suggestion on a slow evaluation (AC-21). Default for an editor bound to an accessor: Live
   preview (agreed at the gate, 2026-11-24).
4. **Timing** (agreed): quiet threshold 250 ms, debounce 300 ms; "a few seconds" for AC-21 is 3 s.
5. **A slow result** (agreed): the old value stays dimmed.
6. **Undo** (agreed): toolbar buttons and the keys. The console has no Revert; an editor keeps
   Revert, which reloads the accessor after a conflict.
7. **Enter on an invalid query** (agreed): applies nothing and highlights the error.
8. **Size** (agreed at the gate, 2026-11-24: two parts, one PR each). The design now spans editors, console, runner and two commands; it may be split at
   Phase 2 into the console part (AC-1 to AC-6, AC-13 to AC-16, AC-19) and the editor part
   (AC-9, AC-17, AC-18, AC-20, AC-21).

## References

- `specs/design/query-pipeline-editor/` (AC-26), `specs/design/query-editor-ergonomics/`,
  `specs/design/recipe-dependency-versions/`
- `specs/reference/UI_INTERFACE_FSD.md` (editors, the console's editor panel)
- `liquers-lib/src/ui/widgets/query_console_element.rs` (`panel_tree`, `parameters_tree`,
  `apply_parameter_commands`, `submit_query`, the `Status::Expired` branch of `update`),
  `liquers-lib/src/ui/editor/state.rs` (`commit`, `continuous`), `liquers-lib/src/ui/runner.rs`
  (`handle_request_asset_updates`), `liquers-lib/src/ui/commands.rs` (`query_console`)
