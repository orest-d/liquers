# Phase 3: Examples & Use-cases — query-editor-ergonomics

All queries below were checked with `liquers-validate` (ok):
`-R/data/sales.csv/-/ns-pl/gt-amount-1000/sort-amount-f/head-20` (the problem example, `P`),
`…/head-100`, `…/head-20/q`, `-R/data/sales.csv/-/ns-pl/select_columns-region-amount/head-5`, and
`L` = `-R/data/sales.csv/-/ns-pl/gt-amount-~X~-R/config/limit.txt~E/head-20` (a link argument).

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Tune the last step in Simple mode | Layout, spans, jump, preset | AC-1, AC-7, AC-10 |
| 2 | Example | A composed theme with a custom icon | Layers, icon variants, CSS | AC-11, AC-14, AC-16, AC-17 |
| 3 | Test | `ui::editor::tests::simple_layout_of_the_problem_example` | One card, three chips, no outline; the `ns` node is dropped; a link argument stays text (`L`) | AC-1 |
| 4 | Test | `ui::editor::tests::advanced_layout_groups_key_and_query_parts` | Groups, outline, link nested | AC-2 |
| 5 | Test | `ui::editor::tests::simple_layout_keeps_trailing_directives_as_chips` | `q` after the last action; `ns-pl` directly before the last action | AC-1 |
| 6 | Test | `ui::editor::tests::mode_switch_keeps_text_edits_and_focus` | Mode is view state; element serde default | AC-3 |
| 7 | Test | `ui::editor::tests::card_header_marks_and_realm` | `CardMarks`, `show_realm`; `hints["icon"]` parsed into `hint_icon`, a malformed hint gives `None` | AC-4, AC-8 |
| 8 | Test | `egui::editor::tests::expanded_card_offers_the_toolbar` | Reset, Presets, Next, Move only in Advanced, Delete; collapsed header order icon, label, dimmed `ns/name` | AC-4, AC-5 |
| 9 | Test | `egui::editor::tests::form_grid_aligns_labels_and_shows_refusal` | Two columns, action column, message under the field | AC-6 |
| 10 | Test | `ui::editor::tests::active_preset_matches_the_action` | `PresetEntry.active`, overflow after three | AC-7 |
| 11 | Test | `ui::theme::tests::icon_precedence` | theme command > theme ns > hint > ns > built-in > monogram | AC-8, AC-14 |
| 12 | Test | `ui::editor::tests::keymap_parses_and_has_no_duplicates` | Every keymap entry parses, unique, and collides with no shortcut of the bundled UISpec menus | AC-9 |
| 13 | Test | `egui::editor::tests::shortcuts_move_between_cards_and_toggle_mode` | Injected key events: `Ctrl+B`, `PageDown`, `F6`, `Ctrl+Shift+[`, `Ctrl+K`, `Alt+1` (preset index 0) | AC-3, AC-7, AC-9 |
| 13a | Test | `egui::editor::tests::ctrl_j_jumps_both_ways` | Cursor on `20` → field focused; back with the parameter selected; non-ASCII text before the cursor (char → byte offset) | AC-10 |
| 14 | Test | `ui::editor::tests::spans_of_the_problem_example` | Element, ns, argument and variadic item spans | AC-10 |
| 15 | Test | `ui::editor::tests::jump_round_trip_and_simple_offer` | `locate` / `span_of`; target outside Simple offers Advanced | AC-10 |
| 16 | Test | `ui::theme::tests::builtin_dark_light_and_densities` | Fixture equality, both modes, both densities | AC-11 |
| 17 | Test | `ui::web::editor::tests::cards_and_theme_css_in_html` | `lq-card`, `data-ns`, `data-density`, the form grid, header order, CSS vars; SVG sanitized (no `<script>`, no `on*`), inline raster as `data:` URI | AC-4, AC-6, AC-11, AC-17 |
| 17a | Test | `egui::theme::tests::apply_theme_sets_visuals_and_density` | Panel fill from the palette; Compact spacing smaller than Comfortable | AC-11 |
| 18 | Test | `ui::widgets::query_console_element::tests::parameters_panel_switches_mode_without_submit` | Starts in Simple, Compact default, switches mode, no submit | AC-12 |
| 19 | Test | `tests/command_icons.rs::bundled_commands_declare_icons_and_valid_presets` | `hint icon` on common `pl`/`img`/`rec`, names known, presets parse | AC-13 |
| 20 | Test | `ui::theme::tests::syntax_role_of_every_token` | `SyntaxRole` from each `StyledQueryToken` | AC-15 |
| 20a | Test | `egui::widgets::tests::layout_job_colours_follow_the_theme` | Layout job colours change with the mode | AC-15 |
| 21 | Test | `ui::web::widgets::tests::query_to_html_emits_token_classes` (new test module) | `lq-tok-<role>` spans; escaped plain text when the query does not parse | AC-15 |
| 22 | Test | `ui::theme::tests::layers_override_add_and_associate` | Override, addition, kept unknown keys, associativity | AC-16 |
| 23 | Test | `tests/ui_runner_theme.rs::theme_from_store_key_reloads` | Store key → `AppRunner::theme_cell()`; a bad edit keeps the old theme | AC-16 |
| 24 | Test | `ui::theme::tests::ui_section_parses_and_refuses_typos` | `take_section("ui")`, unknown key, unknown built-in | AC-16 |
| 25 | Test | `ui::theme::tests::icon_variants_and_unicode_fallback` | String forms, `lib:` expansion and cycle, Unicode always present | AC-17 |
| 26 | Test | `egui::theme::tests::egui_draws_first_drawable_variant` | Set via font, SVG via `resvg`, pending key skipped | AC-17 |
| 27 | Test | `ui::theme::tests::phosphor_name_list_matches_egui_phosphor` (feature `egui`) | `icons.txt` = `egui_phosphor::regular::ICONS` | AC-14, AC-17 |

## Example 1: Tune the last step in Simple mode

A user opens the problem example in an editor. Simple mode shows one card, and the cursor in the
text jumps to it.

```rust
let mut state = EditorState::new(EditorKind::Query, Arc::new(ergonomics_registry()));
state.on_snapshot(&Value::from_query(&parse_query(P)?), Some(fp(1)))?;
let tree = state.tree();

let layout = editor_layout(tree, EditorMode::Simple);
let chips: Vec<_> = layout.before.iter().map(|c| c.label.as_str()).collect();
assert_eq!(chips, ["sales.csv", "Greater than", "Sort by column"]);   // the layout drops the `ns` node
assert_eq!(layout.groups.len(), 1);
assert!(matches!(layout.groups[0].cards[0].node, EditorNode::Action { label, .. } if label == "Get first rows"));
assert!(!layout.outline);

let at = tree.text.rfind("20").unwrap();                              // the cursor on `20`
let span = tree.locate(at).unwrap();
assert_eq!(span.target, SpanTarget::Argument { arg: "n".into(), item: None });
assert_eq!(&tree.text[span.range.clone()], "20");
assert_eq!(tree.span_of(&span.path, &span.target), Some(span.range.clone()));

// Preset "Top 100" (AC-7, 0-based index; `Alt+3` sends the same): the action is replaced.
let head = layout.groups[0].cards[0].node.path().clone();
state.apply(EditorCommand::ApplyPreset { path: head, index: 2 })?;
assert!(state.tree().text.ends_with("head-100"));
```

The cursor on `1000` is in `gt`, outside the last action, so in Simple mode the jump sets
`view.simple_offer` instead of moving focus (AC-10).

## Example 2: A composed theme with a custom icon

A host is started with `ui: { theme: -R/config/theme.yaml, density: compact }`. The store file
changes only what it needs:

```yaml
dark: { surface: { card: "#1b1f27" } }
icons:
  library:
    brush: { unicode: "🖌", variants: [ { svg: { inline: "<svg viewBox='0 0 16 16'><path d='M2 14L14 2'/></svg>" } } ] }
  commands:
    pl/gt: { icon: "ph:funnel-simple", hue: amber }   # `amber` is a built-in hue
    img/draw: { icon: "lib:brush" }                   # a library icon, not a Phosphor name
editor: { card: { elevation: 2 } }       # unknown to the model: kept, readable by path
```

```rust
let layer = ThemeLayer::from_yaml(TEXT)?;
let theme = Theme::compose(&[layer])?;
assert_eq!(theme.palette(ThemeMode::Dark).surface.card, Rgba([0x1b, 0x1f, 0x27, 0xff]));
assert_eq!(theme.palette(ThemeMode::Dark).surface.panel, Theme::builtin().palette(ThemeMode::Dark).surface.panel);
assert_eq!(theme.lookup("editor.card.elevation"), Some(&json!(2)));
assert_eq!(theme.unknown_keys(), ["editor"]);          // the top-level path outside the model

let draw = CommandKey::new("", "img", "draw");
let icon = theme.icon_for(&draw, "Draw", None, None);
assert!(matches!(icon.variants[0], IconVariant::Svg(IconSource::Inline(_))));
assert_eq!(icon.unicode, "🖌");
assert!(theme.to_css().contains("--lq-card:"));
```

## Edge and Error Cases

| Symptom | Expected behaviour | Test |
|---|---|---|
| Theme YAML has a bad colour or a known key of the wrong type | `Error::configuration_error`; the cell keeps the previous theme | 22, 23 |
| `ui` section misspells `density` | Host start fails with `ConfigurationError` | 24 |
| `lib:a` → `lib:b` → `lib:a` | Compose refuses the layer | 25 |
| Icon names a Phosphor glyph that does not exist | Next variant, then Unicode (both renderers) | 11, 17, 26 |
| Key variant not loaded yet | Renderer skips it; `LoadIcon` sent once | 26 |
| Text typed but not committed when `Ctrl+J` is pressed | `SetText` then the jump after rebuild | 15 |
| Stale text (parse error) | `spans` empty; jump does nothing | 14 |
| Saved element without `mode` / `density` | Loads as Simple, density from the cell | 6 |
| `webui` build without `egui` | Theme, icon names and CSS compile; matrix row | `scripts/check-build-matrix.sh` |

## Test Plan

- Unit: `liquers-lib/src/ui/editor/tests.rs`. A second registry `ergonomics_registry()` holds
  `pl/gt`, `pl/sort`, `pl/head` and `pl/select_columns`. It carries tests 3–7, 10, 12, 14 and 15.
- Unit: `liquers-lib/src/ui/theme/tests.rs`. It carries tests 11, 16, 20, 22, 24, 25 and 27.
- Unit with an egui headless context (`ctx.run_ui(RawInput { events, .. }, …)`, as the existing
  `egui/editor.rs` tests do): `egui/editor.rs` (8, 9, 13, 13a), `egui/theme.rs` (17a, 26) and
  `egui/widgets.rs` (20a).
- Unit: `ui/web/editor.rs` (17), `ui/web/widgets.rs` (21), `query_console_element.rs` (18).
- Integration: `liquers-lib/tests/ui_runner_theme.rs` (23, with a memory store).
- Integration: `liquers-lib/tests/command_icons.rs` (19), gated on `image-support`, `polars` and
  `records` (`is_icon_name` needs no egui).
- Commands to run:
  - `cargo test -p liquers-lib --lib --tests`
  - `cargo test -p liquers-lib --test registry_export`
  - `cargo test -p liquers-lib --no-default-features --features webui --lib --tests`
  - `bash scripts/check-build-matrix.sh`

## Learning Log

- `Pipeline::try_from` folds `ns` into the following actions, so the `ns` span must come from the
  `ActionRequest` positions of `parse_query`, not from pipeline elements (Phase 2 review).
- `KeyboardShortcut` spells the command modifier `Ctrl`, `Cmd` or `Meta`, never `Mod`. The keymap
  uses `Ctrl`.
- `resvg`, `usvg` and `image` are already non-optional dependencies of `liquers-lib`, so SVG and
  raster icons cost no new crate. `egui-phosphor` 0.14 targets egui 0.36.
