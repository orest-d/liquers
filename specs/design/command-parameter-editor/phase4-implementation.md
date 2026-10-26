# Phase 4: Implementation Plan — command-parameter-editor

## Overview

The work goes bottom-up: the core edit, then the model, then egui and the console, then the web half.
Steps 1-5 need `variadic-argument-slots` (its Steps 1-2) and Step 3 of
`query-editor-live-validation`. Steps 6-7 also need `ui-events` merged. Each step leaves the
workspace building, and the egui half can ship on its own.

## Progress

- [ ] Step 1: core `set_argument_parameter`
- [ ] Step 2: `ParameterForm`, `ParameterField`, `effective_gui`
- [ ] Step 3: egui `edit_parameter_form`
- [ ] Step 4: console egui panel and `show_parameters`
- [ ] Step 5: egui example
- [ ] Step 6: web `parameter_form_html` and console `render_web` / `update`
- [ ] Step 7: web example and Playwright case
- [ ] Step 8: documentation and issues

## Implementation Steps

### Step 1: core scalar edit
- Files / symbols: `liquers-core/src/plan.rs`: `set_argument_parameter`, which reuses the private
  padding and default-text helper that `variadic-argument-slots` introduces for
  `insert_variadic_element` (extract it if that design inlined it).
- Change: as in Phase 2. Matches over `CommandParameterValue` and `ActionParameter` are exhaustive.
- Depends on: `variadic-argument-slots` Steps 1-2.
- Proof: `cargo test -p liquers-core --lib set_argument_parameter` (tests 3-5).
- Rollback: additive; delete it.

### Step 2: model
- Files / symbols: new `liquers-lib/src/ui/parameter_form.rs` (`ParameterField`, `ParameterForm`,
  `effective_gui`); `ui/mod.rs` re-exports; `liquers-lib/src/utils.rs` makes `active_namespace`
  and `build_namespace_list` `pub(crate)`.
- Change: as in Phase 2. Lookup is plan first, then the presets fallback, then
  `resolve_global_enums`.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-lib --no-default-features --lib parameter_form` (tests 6-12).
- Rollback: delete the module and revert `utils.rs`.

### Step 3: egui renderer
- Files / symbols: new `liquers-lib/src/egui/parameter_form.rs` `edit_parameter_form`;
  `egui/mod.rs`.
- Change: the widget table of Phase 2. Link values use `egui::widgets::edit_query`, or
  `edit_query_live` when it exists. Per-field edit errors are kept in egui temp memory, keyed by the
  field name.
- Depends on: Step 2.
- Proof: `cargo test -p liquers-lib --no-default-features --features egui --lib parameter_form`
  (test 13).
- Rollback: delete the module.

### Step 4: console panel
- Files / symbols: `liquers-lib/src/ui/widgets/query_console_element.rs`: fields `show_parameters`
  (`#[serde(default)]`) and `parameter_form` (`#[serde(skip)]`), the `new` initialiser,
  `show_toolbar` (the toggle, the form under the toolbar, `ctx.command_registry()`), and a private
  `fn form_for_text(&mut self, registry)` that rebuilds the form only when the text changed.
- Depends on: Step 3; `query-editor-live-validation` Step 3 (`UIContext::command_registry`).
- Proof: `cargo test -p liquers-lib --features egui --lib query_console_element` (tests 14 and 15).
- Rollback: revert the file. Saved states keep loading, because the field is defaulted.

### Step 5: egui example
- Files / symbols: `liquers-lib/examples/ui_query_console_app.rs`, where the `UIContext` gets the
  runner's registry (if the parent design has not already done it). Add a `greet` fixture command
  with a string and a bool argument.
- Depends on: Step 4.
- Proof: `cargo build -p liquers-lib --example ui_query_console_app`, plus a manual run where a
  display exists: AC-10 by hand.
- Rollback: revert the example.

### Step 6: web half
- Files / symbols: new `liquers-lib/src/ui/web/parameter_form.rs` `parameter_form_html`;
  `ui/web/mod.rs`. In `query_console_element.rs`: `render_web` (the toggle button, and the form when
  it is on) and `update` (the `UpdateMessage::User(UserEvent { kind: Named(name), fields })` arms for
  `params-toggle`, `params-apply`, `params-add.*`, `params-remove.*`, `params-up.*` and
  `params-reset.*`; an unknown name answers `Unchanged`; no `_ =>` arm).
- Depends on: Step 4; `ui-events` merged.
- Proof: `cargo test -p liquers-lib --no-default-features --features webui --test webui_ssr`
  (test 16) and `--lib query_console_element` (test 17).
- Rollback: revert both files. The egui half is unaffected.

### Step 7: web example and browser test
- Files / symbols: `liquers-lib/examples-web/ui_query_console_app/src/lib.rs` (the `greet` fixture
  command), and `examples-web/tests/ui_query_console_app.spec.ts` "parameter form applies".
- Depends on: Step 6.
- Proof: after `cargo clean`,
  `cd liquers-lib/examples-web && npx playwright test --project ui_query_console_app` (test 18).
- Rollback: revert.

### Step 8: records
- Files: `specs/reference/UI_INTERFACE_FSD.md` "Parameter form", `specs/reference/RECIPES_AND_PLANS.md`
  "Argument slots" (`set_argument_parameter`), `specs/guides/COMMAND_REGISTRATION_GUIDE.md` (one
  sentence), each with a History row and `reviewed:`. Close `NO-COMMAND-PARAMETER-EDITOR` with a
  resolution note. In `UI-VARIADIC-ARGUMENT-LIST-EDITOR`, note that the list widget shipped here.
  Then `python3 scripts/docs_index.py`.
- Proof: `python3 scripts/docs_index.py --check`.

## Testing Plan

Run each step's proof as above. At the end run `cargo test -p liquers-core --lib --tests`,
`cargo test -p liquers-lib --lib --tests`, the `egui` and `webui` reduced configurations from
`CLAUDE.md`, and `bash scripts/check-build-matrix.sh`: the model must build with no features and on
wasm32, and new code sits under `cfg(feature = "egui")` / `"webui"`.
`cargo test -p liquers-lib --test registry_export` must pass unchanged.

## Rollback Plan

Step 1 is additive. Steps 2-5 form a self-contained egui slice, and reverting them leaves only the
core function. Steps 6-7 revert without touching the egui slice. Nothing is persisted except
`show_parameters`, which is defaulted, so a partial landing still reads saved app states.

## Documentation Updates

As in Step 8. `specs/command_registry.yaml` is unchanged. `specs/index.csv` is regenerated.

## Phase 5 Entry Criteria

None: this is a four-phase design.
