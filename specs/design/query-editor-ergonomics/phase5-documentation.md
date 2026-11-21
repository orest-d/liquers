# Phase 5: Documentation - query-editor-ergonomics

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #278)

## Implementation Summary

The query, action, resource and key editors and the query console's Parameters panel now draw a
query as **action cards** in two modes, from one backend-free layout (`ui/editor/layout.rs`) that
egui (`egui/editor.rs`) and the browser (`ui/web/editor.rs`) both render.

- **Simple** (the default) shows step chips, one card for the last action and "Continue with"
  chips from `next`; **Advanced** shows every step as a card with an outline, nested link
  arguments and Collapse / Expand all. The mode and density are saved with the element.
- A **card** has a header (icon, label, dimmed `namespace/name`, marks, position, menu), a toolbar
  (Reset, presets as chips with "+N", Next, Filename, Move, Delete) and a three-column form grid
  with per-field actions and the refused edit's message.
- **Keyboard** (egui): the table of `ui/editor/keys.rs` with scopes, and **Ctrl+J** between the
  query text and the field at the cursor, through source spans computed on rebuild.
- **Theme PoC** (`ui/theme/`): a token model composed from YAML layers as nested dictionaries
  (later layers override and add keys, unknown keys kept), dark and light palettes, query-syntax
  colours (AC-15: the highlighted query in egui and the browser follows them), two densities, hues,
  `to_css()`; reached through `UIContext::theme()` from a `ThemeCell` the runner owns, and
  configured by a `ui:` section (`run_window`, `NativeDriver`, the browser mount, the Python
  window). A theme given as a key reloads when it changes.
- **Icons**: an `Icon` is a list of variants (Phosphor or another set, a library icon, SVG or
  raster inline, embedded or by key) with an always-present Unicode text; egui draws Phosphor
  glyphs, SVG (resvg) and rasters; the browser draws Phosphor CSS with a Unicode fallback and
  inline images as sanitized data URIs. 110 `pl`, `img` and `rec` commands declare `hint icon`;
  `pl/head`, `rec/head`, `img/blur` and `img/rotate` gained presets, `pl/gt`, `pl/sort`,
  `rec/sort` and `pl/head` next presets.

It conforms to the approved design and both gate requests (composable themes; icons as variant
lists). Departures that do not change an acceptance criterion are listed in
[Phase 2 §Implementation Changes](./phase2-architecture.md#implementation-changes): the extra
`ui_ctx` parameter of `show_editor`, key scopes, `Alt+Shift+N` for Next, `icons.defaults`, the
console panel over the whole query, and the browser's missing keyboard model and keyed icon
images (which AC-11 already deferred). Screenshots: [`screenshots/`](./screenshots/).

## Documentation Delivered

### New Reference Documents
None. The editor is described in `UI_INTERFACE_FSD.md` §Editors, where the editor element
already lives.

### New Guide Documents
None.

### Existing Documents Reviewed or Updated
- `specs/reference/UI_INTERFACE_FSD.md`: §Editors gains Modes, Cards, Keyboard (egui), Theme and
  icons, and the console panel; §Hosts gains the `ui` section. History row, `reviewed: 2026-11-21`.
- `specs/guides/COMMAND_REGISTRATION_GUIDE.md` §5: `hint icon` forms, presets as chips, `next` as
  Simple mode's "Continue with". History row, `reviewed: 2026-11-21`.
- `specs/command_registry.yaml`: regenerated, changelog line of 2026-11-21.

### Module Docs
Every module the design created or changed, 44 in all, including the command modules that only
gained hints (`polars/{aggregation,filtering,info,io,selection,sorting}.rs`, `image/commands.rs`,
`records/commands.rs`, which had no or a partial module doc), `ui/theme/{mod,icon}.rs`,
`ui/ui_context.rs`, `egui/{editor,widgets,app}.rs`, `ui/web/{app,editor,widgets}.rs`,
`ui/widgets/query_console_element.rs` and `liquers-web/src/ui.rs`:
`python3 scripts/check_api_docs.py <the 44 files>` → `44 modules, 0 warnings, 0 errors`. The
examples run as doctests (`cargo test -p liquers-lib --doc`, and with `--features webui` for
`ui::web`).

### User-Guide Chapters
`docs_index.py chapters-for` lists `{rust,python,web}/11-ui.md`, `{rust,python,web}/05-commands.md`
and `rust/15-troubleshooting.md`.
- `11-ui.md` ×3: new section "Editing queries" (11.7 Rust and Python, 11.8 Web) from the shared
  fragment `docs/guides/_shared/ui-editors.md`, plus a short host part each (Rust: `with_ui`,
  `with_mode`; Python: the `ui` section of `lq.Environment`; Web: the Phosphor stylesheet and the
  built-in theme). The reference section is renumbered and its cross-references updated.
- `rust/05-commands.md`: the options table gains `hint icon`.
- `python/05-commands.md`, `web/05-commands.md`, `rust/15-troubleshooting.md`: still true.
Each has a `HISTORY.md` row and `reviewed: 2026-11-21`; no `CHECK:` markers.

### Language Integration
- Integration guide: no change (Phase 1).
- Python: not needed for the window, which gets the new editor unchanged; the `ui` section of the
  configuration was wired (Step 10). Gap found here: a Python `@command` cannot declare an icon,
  presets or next (`PY-COMMAND-CANNOT-SET-ICON-PRESETS-OR-NEXT`).
- JavaScript: not needed for the editor; `mountUI` cannot pick a theme
  (`WEB-MOUNTUI-HAS-NO-THEME-OPTION`).

### Links and Capability Map
`specs/README.md` regenerated blocks only; no new link (the design folder was already listed).

## Issues Filed

- `WEB-EDITOR-HAS-NO-KEYBOARD-MODEL`: the browser editor binds no keys and skips icon images by key.
- `WEB-MOUNTUI-HAS-NO-THEME-OPTION`: `mountUI` passes no `ui` section.
- `PY-COMMAND-CANNOT-SET-ICON-PRESETS-OR-NEXT`: the Python decorators lack `hints`, `presets`, `next`.
- `COMMAND-MACROS-NEED-CALLER-IMPORTS`: `register_records_commands!` needs `Context` and `State`
  imported by its caller.
- Earlier in the design: `COMMANDS-HAVE-NO-TOOL-PALETTE` (split from the icon issue).

Closed: `QUERY-CONSOLE-HAS-NO-QUERY-EDITOR-ON-DEMAND` (AC-12), `ICONS-CANNOT-BE-ASSETS-OR-UNICODE`
(AC-17), `USER-GUIDES-DO-NOT-DESCRIBE-THE-EDITORS`.

## Important Learning

- **One layout, two renderers.** Putting card order, header parts, toolbar items and form rows in
  a backend-free module made egui and the browser agree by construction and made the layout
  testable without a renderer; the renderers only draw.
- **egui shortcuts need scopes and order.** `consume_shortcut` matches with extra modifiers, so a
  keymap must be tried most specific first, and header keys must not fire inside a text field.
  The bundled menus' shortcuts are scanned by a test so the editor cannot collide with them.
- **A node is not an element.** The `ns` node shares its element path with the following action;
  code that maps between tree nodes, spans and query positions must use `node.path()`.
- **Theme application is per generation.** egui visuals are applied once per theme generation
  and need a repaint; hosts paint their own background, or a light theme shows on black.
- **Theme precedence needs a floor.** A built-in namespace table must sit *below* command hints
  (`icons.defaults`) while a theme's namespace entry sits above them.

## Conformance and Remaining Work

| AC | Status |
|---|---|
| AC-1 … AC-8, AC-12 … AC-17 | implemented in egui and the browser; tests 3–21, 26 |
| AC-9, AC-10 | implemented in egui; the browser part was deferred by AC-11 itself (`WEB-EDITOR-HAS-NO-KEYBOARD-MODEL`) |
| AC-11 | implemented: one token set, dark/light, Comfortable/Compact, Compact default in the console |

Nothing requested is left without an issue. The design moves to `complete` when the owner
accepts these results.

## Validation

- `cargo test -p liquers-lib --lib --tests`: 50 suites, all passing (537 unit tests).
- `cargo test -p liquers-lib --doc` (40 passed) and `--features webui -- ui::web` (3 passed).
- `xvfb-run cargo test -p liquers-lib --test egui_window`: 5 cases passing, the new
  `ui_section_sets_the_window_theme` included.
- `cargo test -p liquers-lib --test registry_export`: 7 passed after regeneration.
- `bash scripts/test-liquers-py.sh`: pytest 688 passed (35 skipped), Rust unit tests 51 passed.
- `bash scripts/check-build-matrix.sh`: see the PR (run after Step 11).
- `cargo run -p liquers-docs -- check --release`: 0 errors, 0 warnings; `cargo test -p liquers-docs`
  passes.
- `python3 scripts/docs_index.py --check`: 0 errors (26 warnings, none from this design).
- Preset queries validated with `liquers-validate` (`ns-pl/head-5`, `ns-rec/head-10`,
  `ns-img/blur-3`, `ns-img/rotate-90`, `-R/data/sales.csv/-/ns-pl/head-10`).
- Not run in this session: the Playwright browser tests (`trunk` is not installed here); the wasm32 lib build is part of the build matrix.
