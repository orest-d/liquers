# Phase 4: Implementation Plan — template-editor-ui

## Overview

Starts only after `ui-asset-snapshot-boundary` (Steps 1-3), `text-editor-save` (all),
`query-editor-live-validation` (Steps 1-4) and `template-syntax-highlighter` (Steps 1, 4) have
landed. Then: analysis, element mode and preview, command, records.

## Progress

- [ ] Step 1: `template/editor.rs` analysis and model
- [ ] Step 2: `TextEditorElement` mode, layout and preview
- [ ] Step 3: `template/editor` command and registry export
- [ ] Step 4: integration tests
- [ ] Step 5: documentation and source issue

## Implementation Steps

### Step 1: analysis
- Files / symbols: `liquers-lib/src/template/editor.rs` (new) `TemplateAnalysis`,
  `analyse_template`, `TemplateEditorModel`; `template/mod.rs`.
- Change: `highlight_text` + `validate_template(text, Mustache, registry)`; map to `EditorDiagnostic`.
- Depends on: prerequisites.
- Proof: `cargo test -p liquers-lib --lib template::editor` (tests 3-5).
- Rollback: delete module.

### Step 2: element
- Files / symbols: `ui/widgets/text_editor_element.rs` `EditorSyntax`, fields, `with_syntax`,
  `preview_query`, `init`, `update`, `show_in_egui` (template layout, preview input via
  `edit_query_live`, preview pane), `render_web`.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-lib --lib text_editor_element` (tests 7, 11);
  `bash scripts/check-build-matrix.sh` (gated field).
- Rollback: revert the file.

### Step 3: command
- Files / symbols: `template/commands.rs` `editor` and its line in `register_template_commands!`;
  regenerate `specs/command_registry.yaml` with a CHANGELOG line.
- Depends on: Step 2.
- Proof: `cargo test -p liquers-lib --test registry_export`.
- Rollback: remove and regenerate.

### Step 4: integration tests
- Files: `liquers-lib/tests/ui_template_editor.rs` (tests 2, 6, 8, 9).
- Depends on: Steps 2, 3.
- Proof: `cargo test -p liquers-lib --test ui_template_editor`.
- Rollback: delete the file.

### Step 5: records
- Files: `TEMPLATE_LANGUAGE.md` §4, `UI_INTERFACE_FSD.md` "Editors", `TEMPLATE_GUIDE.md` §1 (History,
  `reviewed:`); `NO-TEMPLATE-EDITOR-UI` closed with resolution (query half: `query-editor-live-validation`);
  `python3 scripts/docs_index.py`.
- Proof: `python3 scripts/docs_index.py --check`.

## Testing Plan

Per step; finally `cargo test -p liquers-lib --lib --tests`, the `templates`-only and `egui`-only
rows (`cargo test -p liquers-lib --no-default-features --features templates --lib --tests`, and
`--features egui`), and `bash scripts/check-build-matrix.sh`.

## Rollback Plan

Step 1 is additive. Step 2 adds `#[serde(default)]` fields; after a revert, states saved with them
still load, because the element does not use `deny_unknown_fields`. Step 3 is one command.

## Documentation Updates

As Step 5; `specs/command_registry.yaml` in Step 3.

## Phase 5 Entry Criteria

None — four-phase design.
