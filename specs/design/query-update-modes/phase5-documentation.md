# Phase 5: Documentation - query-update-modes

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (Codex review of PR #285: three findings, fixed)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (Part A in #285, Part B in its own PR)

## Implementation Summary

Both parts are implemented as approved, in two PRs. **Part A** (PR #285): `UpdateMode` (On demand,
Live preview, Apply immediately) and a 100-step Undo / Redo in the editor core; the mode control,
Undo, Redo and a ✓ beside the query field in both renderers; `ValueDisplay` (no flicker, a status
bar and dimming after 250 ms, an On demand suggestion after 3 s, the old value kept on error, a
framed preview); `schedule_event` timers; and the console on top of them: panel edits evaluated
after a 300 ms debounce in Apply immediately or on ✓ / Enter On demand, typed text applied only
when it parses and plans, Browse in the panel, expiry followed by mode. **Part B**: the editor
element's `update_mode` (old `continuous` still loads), Live preview through
`AppMessage::Preview` evaluated in the recipe's folder and shown in place in the followed view (or
the editor's own area), ended by a write or Revert, and a `mode` argument on `lui/query_console`
and the four editor commands.

Deviations, each recorded in `phase4-implementation.md` (Implementation Notes): rendering stays in
the console rather than on `ValueDisplay`; the browser always draws ✓; the mode name is
`on_demand` (a dash separates query arguments); Live preview is offered by query editors only;
`ValueDisplay` adopts a query it did not ask for (a re-point from outside) and ignores only its own
late requests; a recipe's followed view ends a preview through `lq:preview-end`; Esc as Discard was
not added (filed). AC-18 held, so Phase 2's fallback was not needed.

## Documentation Delivered

### New Reference Documents
None: the behaviour belongs in `UI_INTERFACE_FSD.md` and `THEME.md`.

### New Guide Documents
None: nothing repeatable to instruct beyond the reference and the user guides.

### Existing Documents Reviewed or Updated
- `specs/reference/UI_INTERFACE_FSD.md`: editor update modes, Undo / Redo, editor element modes and
  Live preview, console update modes and value display (History rows for Part A and Part B).
- `specs/reference/THEME.md`: `state.preview` (History row, `reviewed:` 2026-11-24).
- `specs/command_registry.yaml`: regenerated with the `mode` arguments, CHANGELOG line.

### Module Docs
`ui::editor`, `ui::widgets::value_display`, `ui::widgets::query_console_element`, `ui::commands`,
`ui::runner`, `ui::message`: reviewed; `scripts/check_api_docs.py` reports 0 warnings, 0 errors
for each.

### User-Guide Chapters
`docs/guides/{rust,python,web}/11-ui.md` share `docs/guides/_shared/ui-editors.md`, which gains
"When edits take effect" (the three modes, ✓, Enter, Undo, the status line, the `mode` argument);
reviewed, `HISTORY.md` rows added in each guide.

### Language Integration
Python and JavaScript: not needed, as decided in Phase 1, and that held: no binding API changed,
and the browser console and editors are `liquers-lib`'s `webui` rendering.

### Links and Capability Map
`specs/README.md`: the design's entry is marked complete.

## Issues Filed

- `PREVIEW-IS-FRAMED-ONLY-IN-THE-QUERY-CONSOLE`: other followed views show a preview unframed and
  ignore its end.
- `EDITOR-HAS-NO-KEY-TO-DISCARD-A-PREVIEW`: the planned Esc Discard key.
- Earlier, from Phase 1: `QUERY-COST-IS-UNKNOWN-TO-THE-UI`, `NO-RECIPE-OR-RECIPES-EDITOR-UI`.
- Closed: `QUERY-CONSOLE-EDITOR-LOSES-EDITS-WHEN-UPDATE-IMMEDIATELY-IS-OFF`,
  `QUERY-CONSOLE-EDITOR-CANNOT-BROWSE-THE-STORE`, `QUERY-EDITOR-HAS-NO-LIVE-PREVIEW`.

## Important Learning

- The lost-edit bug came from rebuilding the panel's editor whenever its working copy differed from
  the text; the panel's editor must never hold pending edits, and only applied text rebuilds it.
- A preview with a recipe's folder as `cwd` (`Recipe::cwd`, `apply_queued`) resolves relative keys
  exactly as the recipe does, which makes in-place recipe previews cheap and side-effect free.
- Enum values of command arguments must not contain `-`, the argument separator; `liquers-validate`
  found it at once.
- Timers sent as UI events make time-dependent UI behaviour deterministic in tests.

## Conformance and Remaining Work

All 22 scenarios (less the three retired) are implemented and tested, with the deviations above.
Nothing remains in this design; the two follow-ups are filed issues.

## Validation

`cargo test -p liquers-lib --lib --tests` (50 suites pass), `--features webui` tests,
`bash scripts/check-build-matrix.sh` (49 configurations OK), `cargo run -p liquers-docs -- check`
(0 errors), `python3 scripts/docs_index.py --check` (0 errors), `check_api_docs.py` on each module,
`liquers-validate` on `-R/data/sales.csv/-/ns-pl/head-3/ns-lui/query_console-on_demand`.
