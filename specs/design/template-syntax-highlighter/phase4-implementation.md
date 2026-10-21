# Phase 4: Implementation Plan — template-syntax-highlighter

## Overview

Prerequisite: `query-editor-live-validation` Step 1 (core `SyntaxKind`, `SyntaxMark`,
`tile_syntax_marks`). Then the template tokens, the styles, the command and the egui helper.

## Progress

- [ ] Step 1: `liquers-template` `highlight` and `highlight_text`
- [ ] Step 2: render styles and `render_tokens`
- [ ] Step 3: `template/highlight` command and registry export
- [ ] Step 4: egui layout helper
- [ ] Step 5: documentation and source issue

## Implementation Steps

### Step 1: tokens
- Files / symbols: `liquers-template/src/highlight.rs` (new) `TemplateSyntaxKind`,
  `TemplateSyntaxToken`, `highlight`, `highlight_text`; `src/lib.rs` re-exports;
  `tests/highlight.rs` (tests 2-7).
- Change: AST walk tracking active delimiters; hygienic bodies to `SyntaxMark`s and
  `tile_syntax_marks`, slots overlaid; classic pieces to `Slot`/`Text`.
- Depends on: `query-editor-live-validation` Step 1.
- Proof: `cargo test -p liquers-template --test highlight`.
- Rollback: delete the module and test file.

### Step 2: styles
- Files / symbols: `highlight.rs` `TemplateRenderStyle`, `HtmlTemplateStyle`, `AnsiTemplateStyle`,
  `render_tokens` (tests 8, 9).
- Depends on: Step 1.
- Proof: `cargo test -p liquers-template --lib highlight`.
- Rollback: revert the file section.

### Step 3: command
- Files / symbols: `liquers-lib/src/template/commands.rs` `highlight` and its
  `register_command!` line in `register_template_commands!`; `liquers-lib/tests/template_commands.rs`
  (tests 10, 11); regenerate `specs/command_registry.yaml`
  (`cargo run -p liquers-lib --features cli --bin export-command-registry -- --format yaml -o specs/command_registry.yaml`)
  with a CHANGELOG line.
- Depends on: Step 2.
- Proof: `cargo test -p liquers-lib --test template_commands --test registry_export`.
- Rollback: remove the registration and regenerate.

### Step 4: egui helper
- Files / symbols: `liquers-lib/src/template/egui.rs` (new, `cfg(all(feature = "templates", feature = "egui"))`)
  `template_layout_job`; `egui/widgets.rs` `pub(crate) fn syntax_kind_color` if not yet present;
  `template/mod.rs` (test 12).
- Depends on: Step 1.
- Proof: `cargo test -p liquers-lib --lib template::egui`; `bash scripts/check-build-matrix.sh`.
- Rollback: delete the module.

### Step 5: records
- Files: `specs/reference/TEMPLATE_LANGUAGE.md` (§4, new §8, History, `reviewed:`),
  `specs/guides/TEMPLATE_GUIDE.md` §1 (History, `reviewed:`); `TEMPLATE-HAS-NO-HIGHLIGHTER` closed
  with resolution; `python3 scripts/docs_index.py`.
- Proof: `python3 scripts/docs_index.py --check`.

## Testing Plan

Per step; finally `cargo test -p liquers-template --lib --tests`, `cargo test -p liquers-lib --lib --tests`,
the wasm32 Node loop of `liquers-template` if it has one (`tests/node.rs`), and
`bash scripts/check-build-matrix.sh` (new `cfg` combination in Step 4).

## Rollback Plan

Steps 1-2 are additive in `liquers-template`; Step 3 is one command (revert and regenerate the
registry); Step 4 is additive.

## Documentation Updates

As Step 5; `specs/command_registry.yaml` regenerated in Step 3.

## Phase 5 Entry Criteria

None — four-phase design.
