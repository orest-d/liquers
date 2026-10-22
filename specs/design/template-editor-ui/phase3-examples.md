# Phase 3: Examples & Use-cases — template-editor-ui

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | editing `templates/page.md` | the whole loop | AC-1, AC-2, AC-4, AC-5 |
| 2 | Test | `liquers-lib/tests/ui_template_editor.rs::editor01_command_builds_template_mode_editor` | element, text, mode | AC-1 |
| 3 | Test | `liquers-lib` `template/editor.rs` `analysis_marks_unknown_command` | diagnostic range | AC-2 |
| 4 | Test | `template/editor.rs` `model_reuses_analysis_for_same_text` | cache | AC-2 |
| 5 | Test | `template/editor.rs` (`egui`) `template_mode_layout_text_equals_buffer` | galley text | AC-3 |
| 6 | Test | `tests/ui_template_editor.rs::editor02_save_refreshes_evaluate_preview` | save → preview snapshot | AC-4, AC-5 |
| 7 | Test | `ui/widgets/text_editor_element.rs` `preview_query_forms` | evaluate and apply query text | AC-5, AC-6 |
| 8 | Test | `tests/ui_template_editor.rs::editor03_apply_preview_uses_input` | runner monitors the apply query | AC-6 |
| 9 | Test | `tests/ui_template_editor.rs::editor04_preview_error_is_shown` | broken template | AC-7 |
| 10 | Test | `egui/widgets.rs` `edit_query_live_marks_error` (from `query-editor-live-validation`, reused) | query field checking | AC-8 |
| 11 | Test | `text_editor_element.rs` `deserializes_without_new_fields` | compatibility | AC-1 |

## Example 1: editing a template

Store (`AsyncMemoryStore`): `templates/page.md` = `# {{title}}\n{{#$ -R-key/notes/-/ns-search/field_extrct-md}}- {{name}}{{/}}`.
A pending node with source `-R-key/templates/page.md/-/ns-template/editor` (validated with
`--command template/editor`) is run: the node holds a `TextEditorElement` in `Template` mode.
Its `init` requests `-R/templates/page.md/-/ns-template/expand` (validated). The layouter's
analysis has one error at `field_extrct`. The author fixes it to `field_extract`, saves; the
runner's `set_state` invalidates the expand asset; the next `run` pushes a fresh preview snapshot.

## Example 2: apply-mode preview

Preview input `-R/data/ctx.json` (a JSON map) → `preview_query()` =
`-R/data/ctx.json/-/ns-template/render-~X~-R/templates/page.md~E` (validated). Built with
`Query`/`Key` encoding, so a key with special characters is escaped by the encoder, not by hand.

## Edge and Error Cases

| Case | Expected | Test |
|---|---|---|
| Unclosed section while typing | one diagnostic, plain layout, no panic | 3 (row) |
| No registry in context | parse diagnostics only | 3 (row) |
| Editor without a handle | no preview request; editing and saving work | 7 (row) |
| Build without `templates` | `Template` mode lays out as plain; tests gated | 5 (gated) |
| Old serialized editor | `syntax = Plain`, `preview_input = ""` | 11 |

## Test Plan

- Unit: `liquers-lib/src/template/editor.rs` — 3, 4, 5 (`cfg(feature = "templates")`; 5 also `egui`).
- Unit: `liquers-lib/src/ui/widgets/text_editor_element.rs` — 7, 11.
- Integration: new `liquers-lib/tests/ui_template_editor.rs`
  (`#![cfg(all(feature = "templates", feature = "egui"))]`, harness of `tests/ui_invalidation.rs`
  with `register_template_commands!` and `register_egui_commands!`, multi-thread tokio) — 2, 6, 8, 9.
- Reused: 10 belongs to `query-editor-live-validation`.
- Commands: `cargo test -p liquers-lib --test ui_template_editor`, `cargo test -p liquers-lib --lib template::editor text_editor_element`,
  `cargo test -p liquers-lib --test registry_export`.

## Learning Log

- Previewing the saved template makes the dependency manager do the refresh: no editor-side
  timer or re-request is needed after a save.
