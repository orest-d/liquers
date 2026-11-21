# Phase 1: High-Level Design — query-editor-ergonomics

## Purpose

The editors of `query-pipeline-editor` work but are hard to read and slow to use: one flat column,
ids instead of words, no focus on the common case, no keyboard model. This design redraws them as
action cards that read as human steps, with Simple and Advanced modes, icons, shortcuts and a jump
from the query text to a field, plus a minimal theme as a proof of concept for a later theme
design. Details and mockups: [`reference-design.md`](./reference-design.md).

## Problem Example

A user tunes `-R/data/sales.csv/-/ns-pl/gt-amount-1000/sort-amount-f/head-20` (checked with
`liquers-validate`) in the egui query editor and wants 20 rows instead of 5.

- **Today** ([mockup 1](./mockups/01-current-state.png)): the editor shows the resource row, the
  `ns` row and three `CollapsingHeader`s named `gt`, `sort`, `head`, each followed by a detached
  `⋯` row, with `link` and `↺` buttons repeated on every field. The `head` field is the seventh
  editable row; nothing distinguishes it, and the cursor in the query text cannot take the user
  there.
- **Should** ([mockup 2](./mockups/02-simple-mode.png)): in Simple mode the editor shows the query
  text, the steps "sales.csv › Greater than › Sort by column › Get first rows" as read-only chips,
  and one card, *Get first rows* (`pl/head`), with a "Number of rows" field, presets and next
  suggestions. Moving the text cursor onto `20` and pressing `Mod+J` focuses that field.

## Scope and Acceptance Criteria

- **AC-1** Simple mode edits the last action
  WHEN an editor in Simple mode shows the problem example's query
  THEN one expanded card, *Get first rows*, is editable; earlier steps are read-only chips named by
  label; no outline, nesting or link subtree is drawn
- **AC-2** Advanced mode edits every segment
  WHEN the editor is in Advanced mode
  THEN key part and query part are groups of cards, an outline shows them as a tree, and a link
  argument is nested under its field ([mockup 3](./mockups/03-advanced-mode.png))
- **AC-3** Switching mode keeps the work
  WHEN the mode is toggled (switch or `Mod+B`) with edits pending
  THEN text, pending edits and focus are kept, and the mode is saved with the element; every
  editor, console included, starts in Simple mode
- **AC-4** Cards read as human steps
  WHEN a card is collapsed
  THEN it is one framed line: icon, label, dimmed `namespace/name`, state marks; expanded,
  `[realm] namespace/name` sits above the label, the realm only when there are several
  ([mockup 4a](./mockups/04a-card-headers.png))
- **AC-5** Card toolbar
  WHEN a card is expanded
  THEN a toolbar offers Reset, Presets, Next, Move (Advanced only) and Delete, Delete last
- **AC-6** Form grid
  WHEN a card shows its fields
  THEN labels and editors align in two columns plus a faint per-field action column (value ↔ link,
  reset); descriptions and refused edits show under the field ([mockup 4](./mockups/04-card-anatomy.png))
- **AC-7** Presets as chips
  WHEN a command declares presets
  THEN up to three are chips, the rest under "+N", the matching one highlighted, `Alt+1`…`Alt+9`
  apply them ([mockup 5](./mockups/05-presets.png))
- **AC-8** Every card has an icon
  WHEN a card is drawn
  THEN the icon is the command's `hint icon`, else its namespace's icon and colour, else a
  monogram ([mockup 8](./mockups/08-icons.png)); a theme override for the command or namespace
  wins over both (AC-14); the renderer draws the first variant it can (AC-17)
- **AC-9** Keyboard navigation
  WHEN the editor has focus
  THEN the shortcuts of `reference-design.md` §3 work (fields, cards, regions, collapse, add action)
- **AC-10** Jump between text and field
  WHEN the text cursor is on a parameter and the user presses `Mod+J`
  THEN that card expands and the field takes focus; `Mod+J` returns with the parameter selected;
  in Simple mode a target outside the last action offers Advanced ([mockup 6](./mockups/06-query-jump.png))
- **AC-11** Theme proof of concept
  WHEN the editor is drawn in egui or the browser
  THEN colours, spacing and fonts come from one token set: dark and light, Comfortable and Compact
  (a per-editor setting, Compact by default in the console) ([mockups 7](./mockups/07-light-theme.png),
  [10](./mockups/10-compact.png)); egui is the main target, the browser gets the same structure
  (cards, modes, form grid) and the tokens as CSS, while its shortcuts and jump come later
- **AC-12** The console offers both modes
  WHEN the query console's Parameters panel is open
  THEN it switches between Simple and Advanced over the console text, without submitting
  (absorbs `QUERY-CONSOLE-HAS-NO-QUERY-EDITOR-ON-DEMAND`)
- **AC-13** Commands show the feature
  WHEN the default command library is registered
  THEN common `pl`, `img` and `rec` commands declare `hint icon`, and a few declare presets and next
- **AC-14** A theme overrides icons and colours
  WHEN a theme configuration names an icon or colour hue for a namespace (`pl`) or a command
  (`pl/gt`, optionally with a realm)
  THEN the editor uses it instead of the registry's or the built-in value, in egui and in the
  browser; a name that does not resolve falls back as in AC-8
- **AC-15** The highlighted query follows the theme
  WHEN a query is shown highlighted (query field, console, editor text) in egui or the browser
  THEN each token kind the highlighter produces (today header, resource, separator, action,
  parameter, entity, highlight; namespace, link, filename and error with live validation's
  `SyntaxKind`) takes its colour from the theme's query-syntax tokens of the current mode,
  replacing the colours hard-coded in `egui/widgets.rs`
- **AC-16** The theme reaches elements and comes from configuration
  WHEN a host is started with a configuration document holding a `ui` section (`theme`, `mode`,
  `density`), or with none
  THEN every element reads the resolved theme from its `UIContext` without locking the app state;
  a theme given as a store key is reloaded when it changes; with no section the built-in theme
  applies ([`reference-design.md`](./reference-design.md) §4.4, §4.5); themes compose as nested
  dictionaries, a later layer changing existing fields and adding new ones
- **AC-17** An icon is a list of variants with a Unicode fallback (added 2026-11-21)
  WHEN an icon is declared in a command hint, a theme or code
  THEN it lists alternative variants (an icon-set reference such as Phosphor, an SVG, a raster
  image), each image given inline, embedded in code or by store key, plus a Unicode text that is
  always available; egui and the browser draw the first variant they can, a text-only surface the
  Unicode (absorbs the icon part of `ICONS-CANNOT-BE-ASSETS-OR-UNICODE`)

Non-goals: tool palettes (`COMMANDS-HAVE-NO-TOOL-PALETTE`), a TUI (`reference-design.md` §7); namespace metadata (`NAMESPACES-HAVE-NO-METADATA`); a full theme
design; editing inside nested pipelines (`EDITOR-CANNOT-EDIT-INSIDE-A-NESTED-PIPELINE`).

## Core Interactions

- UI: `ui::editor` (tree and view state: mode, focus, spans), the egui and browser renderers, the
  query console's Parameters panel.
- Query: source positions of actions and parameters (`ActionRequest.position`,
  `ActionParameter` positions) for AC-10.
- Commands: `hint icon`, `preset:` and `next:` on existing registrations; regenerated
  `specs/command_registry.yaml`.

## Crate Placement

`liquers-lib` only: `ui/editor/` (backend-free model), `egui/editor.rs`, `ui/web/editor.rs`, and
the theme in the UI part (`lui`): a renderer-neutral `ui/theme/` module (token model, YAML loading,
`to_css()`), applied by `egui/` and `ui/web/`, reached through `UIContext` (AC-16); and `commands`/`polars`/`image`/`records` registrations for AC-13. Phosphor enters as
an optional egui dependency (`egui-phosphor`) behind the `egui` feature and as a web font for the
browser renderer. Nothing in `liquers-core` changes unless Phase 2 finds that spans need a core
helper.

## Language Integration

- Integration guide: no change. This is UI rendering of an existing feature; no new
  language-visible capability.
- Python (`liquers-py`): not needed. Python opens the same egui window (`liquers-py-ui-window`) and
  gets the new look with no binding change.
- JavaScript (`liquers-web`): not needed. It exposes the core query editors
  (`query-builder-in-bindings`), not the UI renderers; the browser renderer lives in `liquers-lib`.

## Documentation Intent

- Reference: extend `specs/reference/UI_INTERFACE_FSD.md` §Editors (modes, cards, shortcuts, jump,
  theme tokens).
- Guide: none new; `specs/guides/COMMAND_REGISTRATION_GUIDE.md` gains the `hint icon` convention.
- Other documents: [`reference-design.md`](./reference-design.md) and [`mockups/`](./mockups/) in
  this folder.
- Documents to update: `specs/reference/UI_INTERFACE_FSD.md`,
  `specs/guides/COMMAND_REGISTRATION_GUIDE.md`, `specs/command_registry.yaml`.
- API docs: `liquers-lib/src/ui/editor/mod.rs`, `liquers-lib/src/egui/editor.rs`,
  `liquers-lib/src/ui/web/editor.rs`, the new theme module.
- User guides: the UI chapter of each guide, `docs/guides/{rust,python,web}/11-ui.md`, together with
  `USER-GUIDES-DO-NOT-DESCRIBE-THE-EDITORS` (decided at Phase 2 whether it is absorbed).

## Design Dependencies

- `query-pipeline-editor` (complete): the editors this redesigns.
- `query-editor-live-validation` (in implementation): **overlaps** (T2, the token model). Its
  source-span tokens of the query text would give AC-10 its spans; Phase 2 decides whether to wait
  for it or derive spans from parse positions.
- `QUERY-CONSOLE-HAS-NO-QUERY-EDITOR-ON-DEMAND`: covered by AC-12 (T4), joins this design.
- `keyboard-shortcuts` (complete): UISpec menu shortcuts; AC-9 must not collide with them.
- `query-editor-live-validation`: also **T3** for AC-15: both change the egui query colours in
  `styled_query_to_layout_job`; the theme keys follow its `SyntaxKind`.
- `NAMESPACES-HAVE-NO-METADATA`: **required-by**; when it lands, the registry's namespace metadata
  slots into AC-14's precedence between the command hint and the built-in table.
- `UI-NOT-SEPARATED-BY-CRATE-AND-TECHNIQUE`: **weak overlap**; the theme module may move with the
  UI split later.

## Decisions at the Phase 1 gate (Orest, 2026-11-21)

1. Namespace metadata lives in the command metadata registry; it is its own issue
   (`NAMESPACES-HAVE-NO-METADATA`: label, doc, icon, colour hue). This design adds theme overrides
   keyed by namespace and command (AC-14) and a built-in table for the bundled namespaces.
2. Browser: structure and theme only; egui is the main target (AC-11).
3. Compact is a setting, Compact by default in the console (AC-11).
4. Simple is always the default mode (AC-3).
5. (2026-11-21) Theme format: YAML authoring over one Rust token model, CSS generated for the
   browser, DTCG later for interchange ([`theme-formats.md`](./theme-formats.md)). The theme
   subset covers the editors and the query syntax (AC-15) and is named for reuse by other elements.

## Design Readiness

Decision log under `proceed all` (pre-approved after Phase 2 on 2026-11-21).

- **Readiness:** ready
- **Automatic fixing:** not-eligible (complexity L, new UI behaviour, design-level decisions)
- **Leading issue:** None
- **Explanation:** Phase 2 resolved the two Phase 1 questions: the theme is reached through a
  `ThemeCell` in `UIContext`, sourced from the `ui` host section, with a store key watched by the
  runner. A per-document theme is deferred to `UI-DOCUMENT-CANNOT-NAME-ITS-THEME`.
- **Decided at the Phase 2 gate (`proceed all`):** AC-15 covers the token kinds today's
  highlighter produces. `namespace`, `link`, `filename` and `error` follow
  `query-editor-live-validation`'s `SyntaxKind`.
- **Open questions:** None

## Scope Changes

- 2026-11-21: themes compose as nested dictionaries (Orest). AC-16 extended; Phase 2 `ThemeLayer`;
  size unchanged. Approved by request.
- 2026-11-21: icons are lists of variants with a Unicode fallback (Orest). AC-8 amended, AC-17
  added, icon part of `ICONS-CANNOT-BE-ASSETS-OR-UNICODE` absorbed, palettes split off as
  `COMMANDS-HAVE-NO-TOOL-PALETTE`; adds about two implementation steps. Approved by request.

## References

- [`reference-design.md`](./reference-design.md), [`mockups/`](./mockups/)
- `specs/archive/2026-11-19-egui-craft-suite-survey.md` §4 (theme layer)
- `specs/design/query-pipeline-editor/`, `specs/design/query-editor-live-validation/`
- `NAMESPACES-HAVE-NO-METADATA`, `ICONS-CANNOT-BE-ASSETS-OR-UNICODE`,
  `QUERY-CONSOLE-HAS-NO-QUERY-EDITOR-ON-DEMAND`, `USER-GUIDES-DO-NOT-DESCRIBE-THE-EDITORS`
