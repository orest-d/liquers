# Phase 4: Implementation Plan — query-editor-ergonomics

## Overview

The work is built bottom-up, so each step compiles and is tested before anything draws with it:
1. The renderer-neutral theme and icons.
2. The context and runner plumbing.
3. The editor model: layout, spans, keymap.
4. The egui renderer, then the browser renderer, then the console.
5. Command metadata, the hosts, and documentation last.

There are no prerequisites. The overlapping designs (`query-editor-live-validation`,
`template-syntax-highlighter`) have not landed their colour code, so this design owns it.

## Progress

- [x] Step 1 (60bf0acb): theme and icon model, YAML layers and composition, built-in theme, CSS
- [x] Step 2 (60bf0acb): Phosphor name list, `egui-phosphor` dependency, icon resolution
- [x] Step 3 (60bf0acb): `ThemeCell`, `UiConfig`, `IconCache`, `UIContext` and runner plumbing
- [x] Step 4: editor model: hints, realm, active presets, spans, layout, keymap, element mode/density
- [x] Step 5: egui theme application, icon drawing, themed query colours
- [x] Step 6: egui editor: cards, toolbar, form grid, chips, outline, shortcuts, jump
- [x] Step 7: browser editor, token classes, theme CSS in the mount
- [x] Step 8: query console Parameters panel in both modes
- [x] Step 9: command metadata (`hint icon`, presets, next) and registry regeneration
- [x] Step 10: hosts read the `ui` section (egui window, Python window, web mount)
- [ ] Step 11: documentation, API docs, issues

## Implementation Steps

### Step 1: theme model
- Files / symbols:
  - `liquers-lib/src/ui/theme/{mod.rs, model.rs, layer.rs, css.rs, builtin.yaml, tests.rs}`
  - `ui/mod.rs`, which gains `pub mod theme;`
- Change: Phase 2 *Theme*. Every type in it is added: `Rgba`, `Palette` and the role structs,
  `Spacing`, `Typography`, `Hue`, `SyntaxRole`, `Theme`, `ThemeLayer`, `ThemeMode` and `Density`.
  `Theme` gets `builtin`, `compose`, `with_layer`, `lookup`, `unknown_keys`, `palette`, `spacing`,
  `syntax`, `hue` and `to_css`. The icon types of Phase 2 *Icons* (`Icon`, `IconVariant`,
  `IconSource`, `IconBytes` with hand-written serde, the string forms) also land here, in
  `ui/theme/icon.rs`, because `IconTheme` holds them. The built-in values are those of the mockups
  (`mockups/source/gen.py` palette, including the `amber` hue).
- Depends on: none.
- Proof: `cargo test -p liquers-lib --lib ui::theme`, which runs tests 16, 20, 22, 24 (parse part)
  and 25 (string forms); `bash scripts/check-build-matrix.sh` (a new module in every configuration).
- Rollback: remove the module.

### Step 2: icons
- Files / symbols: `ui/theme/{icon.rs, icons.txt}`, `IconTheme.library`, `Theme::icon_for`,
  `ResolvedIcon`, `is_icon_name`; `Cargo.toml`: `egui-phosphor = { version = "0.14", optional =
  true }` and `"dep:egui-phosphor"` in the `egui` feature. `icons.txt` holds the kebab-case names
  converted from `egui_phosphor::regular::ICONS` (SCREAMING_SNAKE).
- Change: `lib:` expansion with cycle refusal, the precedence chain, the Unicode fallback.
- Depends on: Step 1. Proof: tests 11, 25 and 27; the egui build-matrix row. Rollback: revert.

### Step 3: context and runner plumbing
- Files / symbols:
  - `ui/theme/{cell.rs, config.rs}`: `ThemeCell`, `ThemeState`, `UiConfig`, `ModeSetting`,
    `ThemeSource`, `IconCache`, `IconLoad`.
  - `ui/ui_context.rs`: `with_theme`, `theme`, `icon_cache`.
  - `ui/message.rs`: `AppMessage::LoadIcon`.
  - `ui/runner.rs`: `theme`, `theme_watch`, `with_ui_config`, `theme_cell()` (public accessor),
    the `deliver_update` parameter, the `has_monitoring` exclusion, and the `LoadIcon` arm of the
    `AppMessage` match (`runner.rs:288`).
- Change: Phase 2 *Context*. Every match over `AppMessage` gets an explicit arm.
- Depends on: Step 2. Proof: tests 23 (`tests/ui_runner_theme.rs`) and 24.
- Rollback: the cell defaults to the built-in theme, so callers that do not set it are unaffected.

### Step 4: editor model
- Files / symbols:
  - `ui/editor/tree.rs`: `TextSpan`, `SpanTarget`, `EditorTree.spans`, `show_realm`, `locate`
    and `span_of`. `EditorNode::Action` gains `realm` and `hint_icon`; `PresetEntry` gains
    `active`.
  - `ui/editor/state.rs`: `build_nodes`, `rebuild` and a new `spans_of`.
  - `ui/editor/layout.rs` (new): `editor_layout`, `StepChip`, `CardGroup`, `Card` and `CardMarks`.
  - `ui/editor/keys.rs` (new): `EditorKey` (`ApplyPreset(n)` from `Alt+n` is preset index `n-1`),
    `editor_keymap`.
  - `ui/editor/view.rs`: `FocusTarget` and the new `EditorView` fields.
  - `ui/editor/element.rs`: `mode`, `density`, `set_mode`, `set_density` and the cached
    `ThemeState`.
- Change: Phase 2 *Editor model*. The layout drops the `Namespaces` node from Simple chips.
- Depends on: Step 3 for the element's theme cache; the rest needs only Step 2.
- Proof: tests 3–7, 10, 12, 14 and 15, plus the existing `ui/editor/tests.rs` passing.
- Rollback: the new fields are additive.

### Step 5: egui theme
- Files / symbols:
  - `egui/theme.rs` (new): `apply_theme`, `color`, `phosphor_glyph`, `draw_icon` (with the `resvg`
    and `image` texture cache).
  - `egui/widgets.rs`: `styled_query_to_layout_job(&StyledQuery, &ThemeState)` and its callers.
- Depends on: Steps 1–3. Proof: tests 17a, 20a and 26; the existing `widgets.rs`
  tests; `cargo check -p liquers-lib --no-default-features --features egui`.
- Rollback: revert; the colours return to the constants.

### Step 6: egui editor
- Files / symbols: `egui/editor.rs` (`show_editor`, `EditorOutput`, `card`, `card_header`,
  `toolbar`, `form_grid`, `preset_chips`, `outline`, `handle_keys`, `jump`), and every caller of
  `show_editor`: `ui/editor/element.rs` `show_in_egui` (l.349), the console
  (`query_console_element.rs:605`, Simple and Compact until Step 8) and the `egui/editor.rs` tests.
- Change: drawing follows `reference-design.md` §2. Density is applied in `ui.scope`.
- Depends on: Steps 4–5. Proof: tests 8, 9, 13 and 13a, and a screenshot of `egui-native-app` compared
  with mockups 2 to 4 (manual; recorded in Phase 5).
- Rollback: the old `show_editor` body stays in history. This step is one commit.

### Step 7: browser editor
- Files / symbols:
  - `ui/web/editor.rs` `editor_html`.
  - `ui/web/widgets.rs` `query_to_html`.
  - `ui/web/app.rs`: the mount creates the cell first and writes `<style id="lq-theme">`;
    `MountOptions.ui`.
  - `ui/web/widgets.rs`: `query_to_html` tokenizes with `StyledQuery` and falls back to escaped
    text when the query does not parse; a new test module.
  - `liquers-web/src/ui.rs:858` builds `MountOptions` as a literal: it gains `ui: None`. The
    TypeScript `MountOptions` (`liquers-web/src/typescript.rs:451`) is unchanged; the theme is not
    exposed to JavaScript in this design.
  - `examples-web/ui_spec_demo/index.html` links the Phosphor web font.
- Depends on: Steps 4 and 1–3. Proof: tests 17 and 21;
  `cargo check -p liquers-web --target wasm32-unknown-unknown`;
  `cargo test -p liquers-lib --no-default-features --features webui --lib --tests`; the Playwright
  demo after `cargo clean`.
- Rollback: revert.

### Step 8: console
- Files / symbols: `ui/widgets/query_console_element.rs` (`mode`, `density`, the Parameters panel
  and its `render_web`).
- Depends on: Steps 6–7. Proof: test 18; the existing console tests.
- Rollback: revert.

### Step 9: command metadata
- Files / symbols:
  - `polars/{selection,sorting,filtering,aggregation,info}.rs`, `image/commands.rs`,
    `records/commands.rs`.
  - `specs/command_registry.yaml`, with a changelog line.
  - `tests/command_icons.rs` (new).
- Change: `hint icon:` on the common commands, and `preset:` / `next:` on `pl/head`, `pl/sort`,
  `pl/gt` and a few `img` / `rec` commands, every one validated. `pl/head` is a `register_alias`
  (`polars/selection.rs:142`), so it gets its hint, presets, next, label "Get first rows" and the
  argument label "Number of rows" through the returned `&mut CommandMetadata` (`with_hint`).
- Depends on: Step 2 (`is_icon_name`). Proof: test 19, and `cargo test -p liquers-lib --test
  registry_export`; the build matrix.
- Rollback: revert and regenerate.
- Can run in parallel with Steps 5–8.

### Step 10: hosts
- Files / symbols: `egui/app.rs` (`WindowOptions.ui`, `run_window`: `apply_theme` on a generation
  change, `System` resolved), `ui/native.rs` (`NativeDriver` passes the cell), and the Python
  window (`liquers-py/src/ui.rs:658`; `src/config.rs` takes the `ui` section beside `python`).
- Depends on: Steps 3 and 5. Proof: `cargo test -p liquers-lib --lib`,
  `bash scripts/test-liquers-py.sh`, and `egui-native-app` started with a `ui` section.
- Rollback: revert; hosts fall back to the built-in theme.

### Step 11: documentation
- Files: see Documentation Updates. Proof: `python3 scripts/check_api_docs.py` on every touched
  module, `python3 scripts/docs_index.py --check`, and `cargo run -p liquers-docs -- check`.

## Testing Plan

- After each step, run the step's tests, then `cargo test -p liquers-lib --lib --tests`.
- After Steps 1, 2, 5, 7, 9 and 10, which add a module, an optional dependency or a `cfg`, run
  `bash scripts/check-build-matrix.sh`.
- After Step 7, run the browser tests in their own pass after `cargo clean`.
- After Step 9, run `registry_export`.
- Final checks:
  - the full `liquers-lib` loop
  - the build matrix
  - `liquers-py`'s script
  - `liquers-docs` check
- Failing tests are re-run alone while fixing.

## Rollback Plan

Each step is a commit on `claude/project-thread-jp0vnv` and can be reverted on its own.

If only Steps 1–4 land, the model and theme exist but nothing draws them. Steps 5–8 can then follow
in a later PR, and the remainder becomes an issue. If Step 9 is reverted, the commands lose their
icons and the editor shows monograms.

## Documentation Updates

- `specs/reference/UI_INTERFACE_FSD.md` §Editors: modes, cards, keymap, jump, theme and icons, and
  the `ui` section. Add a History row and bump `reviewed:`.
- `specs/guides/COMMAND_REGISTRATION_GUIDE.md`: the `hint icon` string forms, and presets shown as
  chips. Add a History row and bump `reviewed:`.
- `docs/guides/{rust,python,web}/11-ui.md`: the editors and the `ui` section (closes
  `USER-GUIDES-DO-NOT-DESCRIBE-THE-EDITORS`).
- Module docs: `ui/theme/mod.rs` gets all five parts. `ui/editor/mod.rs`, `egui/editor.rs`,
  `ui/web/editor.rs` and `ui/ui_context.rs` get the parts named in Phase 2.
- Regenerated: `specs/command_registry.yaml`, `specs/index.csv` and `specs/index.md`.
- Issues to close: `QUERY-CONSOLE-HAS-NO-QUERY-EDITOR-ON-DEMAND` and
  `ICONS-CANNOT-BE-ASSETS-OR-UNICODE` (icon part).

## Phase 5 Entry Criteria
- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
