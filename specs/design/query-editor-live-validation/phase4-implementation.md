# Phase 4: Implementation Plan — query-editor-live-validation

## Overview

Core tokens first (no dependents yet), then the backend-free model, the registry hand-off, the
egui adapter and console, then HTML. Each step leaves the workspace building. No prerequisite
issue. `template-syntax-highlighter` may start after Step 1.

## Progress

- [ ] Step 1: core `SyntaxKind`, `SyntaxToken`, `SyntaxMark`, `tile_syntax_marks`, `query_syntax_tokens`, `syntax_tokens`
- [ ] Step 2: `ui/query_editor.rs` model and analysis
- [ ] Step 3: registry in `UIContext` and `AppRunner`
- [ ] Step 4: egui adapter and console
- [ ] Step 5: HTML adapter, console `render_web`, `query_to_html`, web driver context
- [ ] Step 6: examples attach the registry; Playwright case
- [ ] Step 7: documentation, follow-up issue, source issue

## Implementation Steps

### Step 1: core tokens
- Files / symbols: `liquers-core/src/query.rs` — new `SyntaxKind`, `SyntaxToken`, `SyntaxMark`,
  `tile_syntax_marks`, `query_syntax_tokens`, `syntax_tokens`; new `liquers-core/tests/syntax_tokens.rs`.
- Change: as Phase 2; exhaustive matches over `QuerySegment`, `ActionParameter` (no `_ =>`).
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib syntax_tokens && cargo test -p liquers-core --test syntax_tokens` (tests 3-5).
- Rollback: additive; delete.

### Step 2: model
- Files / symbols: `liquers-lib/src/ui/query_editor.rs` (new), `ui/mod.rs` re-exports
  (`QueryEditorModel`, `QueryAnalysis`, `EditorDiagnostic`, `DiagnosticSeverity`, `analyse_query`).
- Change: `analyse_query` over `validate_query(text, 0, level, reg)`; range rule of Phase 2; model
  caches on `(text, registry.is_some())`.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-lib --lib query_editor` (tests 6-11).
- Rollback: delete module.

### Step 3: registry hand-off
- Files / symbols: `ui/ui_context.rs` `UIContext` (field, `with_command_registry`,
  `command_registry`); `ui/runner.rs` `AppRunner` (field `registry`, `new`, every `UIContext::new`
  site, `command_registry()`).
- Depends on: none (parallel with 2).
- Proof: `cargo test -p liquers-lib --test ui_runner runner_contexts_carry_registry` (test 14).
- Rollback: revert the two files.

### Step 4: egui adapter and console
- Files / symbols: `egui/widgets.rs` `analysis_to_layout_job`, `edit_query_live`, `edit_query`
  delegating; `ui/widgets/query_console_element.rs` field `editor`, `show_toolbar` (layouter uses
  the passed buffer), `update` (`set_execution_error`), remove `error_position`, replace its test
  with test 12.
- Depends on: Steps 2, 3.
- Proof: `cargo test -p liquers-lib --lib egui::widgets query_console_element` (tests 12, 13);
  manual run of `cargo run -p liquers-lib --example ui_query_console_app` where a display exists.
- Rollback: revert both files.

### Step 5: HTML
- Files / symbols: `ui/web/html.rs` `analysis_to_html`; `ui/web/widgets.rs` `query_to_html`;
  console `render_web` preview div; `ui/web/app.rs` builds `ctx` with the runner's registry;
  `examples-web/shared.css` `lq-tok-*` colours.
- Depends on: Steps 2, 3.
- Proof: `cargo test -p liquers-lib --no-default-features --features webui --lib --tests` (tests 15, 16).
- Rollback: revert.

### Step 6: examples and browser test
- Files / symbols: `liquers-lib/examples/ui_query_console_app.rs` (and other `ui_*` examples that
  host a console), `examples-web/ui_query_console_app/src/lib.rs`,
  `examples-web/tests/ui_query_console_app.spec.ts` (test 17).
- Depends on: Steps 4, 5.
- Proof: `cargo build -p liquers-lib --examples`; Playwright after `cargo clean` (`CLAUDE.md`).
- Rollback: revert examples.

### Step 7: records
- Files: `specs/reference/UI_INTERFACE_FSD.md`, `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md`
  (sections, History rows, `reviewed:`); new `specs/issues/WEB-QUERY-EDITOR-NOT-LIVE.md` if not
  already filed; source issue `closed` with resolution; `python3 scripts/docs_index.py`.
- Proof: `python3 scripts/docs_index.py --check`.

## Testing Plan

Per step as above; at the end `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests`,
and `bash scripts/check-build-matrix.sh` (new code under `cfg(feature = "egui")` and `"webui"`, and
the model must build with no features and on wasm32).

## Rollback Plan

Steps 1-2 are additive. Steps 3-5 revert per file; nothing is persisted (`editor` is
`serde(skip)`), so a partial landing leaves serialized app states readable.

## Documentation Updates

As Step 7; `specs/command_registry.yaml` unchanged; `specs/index.csv` regenerated.

## Phase 5 Entry Criteria

None — four-phase design.
