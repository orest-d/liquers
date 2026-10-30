# Phase 4: Implementation Plan - ui-events

## Progress

- [x] Step 1: Vocabulary types and the two new enum variants
- [x] Step 2: Runner delivery of `AppMessage::UIEvent`
- [x] Step 3: Key-event conversion, keyboard routing and markup helpers
- [x] Step 4: Query console handles `Submit` and declares its fields
- [x] Step 5: `UISpecElement` emits menu shortcuts
- [x] Step 6: Browser driver routing
- [x] Step 7: Playwright cases and the web example
- [x] Step 8: Reference, records and final checks

## Overview

**Feature:** ui-events, first increment (W1, W2, W5).

**Architecture:** `UIEvent` delivered through `update()` as `UpdateMessage::UI`, declared in
markup, routed in the browser by a pure keyboard function (Phase 2).

**Estimated complexity:** Medium; one crate (`liquers-lib`), no command or format change.

**Prerequisites:** the Phase 1 leading question answered (recommended: one variant, closed kind
enum). A different answer changes Steps 1, 2, 4 and 6 only in variant names.

## Implementation Steps

### Step 1: Vocabulary types and the two new enum variants

**Files:** new `liquers-lib/src/ui/event.rs`; `ui/mod.rs` (`pub mod event;`, re-export `UIEvent`,
`UIEventKind`); `ui/element.rs` `UpdateMessage::UI`; `ui/message.rs` `AppMessage::UIEvent`;
`ui/ui_context.rs` `UIContext::send_ui_event`.
**Change:** types and signatures exactly as Phase 2. Add the `UpdateMessage::UI(_) =>
UpdateResponse::Unchanged` arm to `AssetViewElement::update` (`element.rs`),
`MarkdownElement::update`, `QueryConsoleElement::update` (temporary, replaced in Step 4), the test
elements in `liquers-lib/tests/ui_invalidation.rs` and `tests/ui_runner.rs`, and
`liquers-lib/examples/ui_button_app.rs`, `liquers-lib/examples-web/ui_button_app/src/lib.rs`.
Add the `AppMessage::UIEvent` arm to `AppRunner::process_messages` as a no-op until Step 2.
Tests `ui_event_kind_from_name`, `ui_event_field_lookup`.
**Proof:** `cargo test -p liquers-lib --lib ui::event`; `cargo check -p liquers-lib --examples`.
**Rollback:** revert the step; nothing depends on it yet.

### Step 2: Runner delivery of `AppMessage::UIEvent`

**Files:** `liquers-lib/src/ui/runner.rs`.
**Change:** rename private `deliver_snapshot` to `deliver_update(handle, message: UpdateMessage,
…)`; its three callers pass `UpdateMessage::AssetUpdate(…)`. The `UIEvent` arm calls it and
ignores `Missing`. Tests `ui_event_reaches_element_and_records_repaint`,
`ui_event_for_missing_element_is_ignored` in `tests/ui_runner.rs`.
**Proof:** `cargo test -p liquers-lib --test ui_runner --test ui_invalidation --test
query_console_integration`.
**Rollback:** revert; Step 1 stands alone.

### Step 3: Key-event conversion, keyboard routing and markup helpers

**Files:** `ui/shortcuts.rs` (`KeyboardShortcut::from_key_event`: `Key::from_name(key)`, else
`Key::from_name(code)`; `None` for modifier names); new `ui/web/keys.rs` (`KeyPress`,
`KeyTarget`, `KeyRoute`, `route_keydown` with the Phase 2 rule order); `ui/web/mod.rs`
(`pub(crate) mod keys;`); `ui/web/html.rs` (`field_attr`, `event_attr`, `shortcut_attr`, each via
`escape_html`).
**Change:** pure functions only; no DOM access. Tests from Phase 3 for these three files.
**Proof:** `cargo test -p liquers-lib --features webui --lib -- ui::shortcuts ui::web`.
**Rollback:** revert; nothing outside these files calls them before Step 4.

### Step 4: Query console handles `Submit` and declares its fields

**Files:** `liquers-lib/src/ui/widgets/query_console_element.rs`.
**Change:** private `handle_ui_event(&mut self, event: &UIEvent, ctx: &UIContext) ->
UpdateResponse` as Phase 2 (`Submit`: set `query_text` from field `query` when present, then
`submit_query`; `Named(_)`: `Unchanged`). In `show_in_egui` (the `show_toolbar` Go button and the
Enter check), replace the direct `submit_query` calls with `handle_ui_event` and a `Submit` event
carrying field `query` (AC-8). `render_web`: field and event attributes on the input,
Go as `<button type="button" class="lq-go" data-lq-event="submit">`, the `UiAction::Apply` and its
`use` removed. Attributes come from the Step 3 helpers. Unit tests from Phase 3 for
this file, `egui_and_web_submit_share_outcome` (AC-8, `cfg(feature = "egui")`), and `test_ui_submit_event_evaluates_new_query` in
`tests/query_console_integration.rs`.
**Proof:** `cargo test -p liquers-lib --features webui --lib widgets::query_console_element`;
`cargo test -p liquers-lib --test query_console_integration`.
**Rollback:** revert this file; the console falls back to the `Apply` path.

### Step 5: `UISpecElement` emits menu shortcuts

**Files:** `liquers-lib/src/ui/widgets/ui_spec_element.rs` `render_top_level_item_web`,
`render_menu_item_web`.
**Change:** append `shortcut_attr(s)` to `Button` items with `Some(s)`; `Menu` / `Submenu` labels
unchanged. Test `render_web_emits_shortcut_attribute`.
**Proof:** `cargo test -p liquers-lib --features webui --lib widgets::ui_spec_element`;
`cargo test -p liquers-lib --no-default-features --features webui,image-support --test webui_ssr`.
**Rollback:** revert the two functions.

### Step 6: Browser driver routing

**Files:** `liquers-lib/src/ui/web/app.rs` (`browser` module); `liquers-lib/Cargo.toml` `web-sys`
features `NodeList`, `HtmlTextAreaElement`.
**Change:** replace `dispatch_dom_event` with `dispatch_click`, `dispatch_keydown(ev, ctx, root)`,
`send_field_event` and `run_action` as Phase 2; the keydown closure in `mount_web` captures a
clone of `root` and is attached to the document (implemented deviation: see `DESIGN.md`). Read `is_composing`, `key`, `code`, `ctrl_key || meta_key`, `alt_key`,
`shift_key` from `KeyboardEvent`; `editable` / `single_line` from the target's tag and input
`type`; `event` from its `data-lq-event`. `prevent_default` only after a dispatch.
**Proof:** `cargo check -p liquers-lib --target wasm32-unknown-unknown --no-default-features
--features webui`; Step 7's Playwright run.
**Rollback:** revert `app.rs` and `Cargo.toml`; native behaviour is unaffected.

### Step 7: Playwright cases and the web example

**Files:** `liquers-lib/examples-web/tests/ui_query_console_app.spec.ts`;
`liquers-lib/examples-web/ui_query_console_app/src/lib.rs` (menu YAML: `New Console` shortcut
`Alt+N`, with a comment that Chrome reserves Ctrl+N).
**Change:** replace the "known gaps" block and update the header comment; add the five Phase 3
cases.
**Proof:** after `cargo clean`: `cd liquers-lib/examples-web && npx playwright test --project
ui_query_console_app`, then the full `npx playwright test` (menu clicks in the other examples
must still dispatch exactly once).
**Rollback:** revert the spec and the YAML line.

### Step 8: Reference, records and final checks

**Files:** `specs/reference/UI_INTERFACE_FSD.md` §Event Handling (History row, `reviewed:`);
the three source issues (`status: closed`, resolution note); `specs/design/ui-events/DESIGN.md`
(status, Phase Status); `specs/index.csv` via `python3 scripts/docs_index.py`.
**Change:** document the vocabulary, the `data-lq-*` attributes, keyboard precedence and the
reserved-shortcut limit.
**Proof:** `python3 scripts/docs_index.py --check`; `bash scripts/check-build-matrix.sh` (new
`cfg(feature = "webui")` code and a wasm-only driver change); `cargo test -p liquers-lib --lib
--tests`; `cargo fmt --check`; `git diff --stat` reviewed for debug output, `println!`, unrelated
files, and an unchanged `specs/command_registry.yaml`.
**Rollback:** documentation only.

## Testing Plan

### Unit Tests
After Steps 1, 3, 4 and 5: `cargo test -p liquers-lib --features webui --lib`.

### Integration Tests
After Steps 2 and 4: `cargo test -p liquers-lib --lib --tests`; `webui_ssr` with
`--no-default-features --features webui,image-support`.

### Manual Validation
After Step 7, in a separate `cargo clean`ed build: the Playwright suite. The browser loop and the
native loop must not share a run (disk limit, `CLAUDE.md`).

## Agent Assignment

One implementer, in order; Step 5 may run in parallel with Step 4. Apply
`rust-best-practices` at Steps 1, 2 and 6 and `liquers-unittest` for every test step.

## Rollback Plan

Each step reverts independently in reverse order. Nothing persisted, serialized or registered
changes, so no data migration exists; reverting Steps 1-8 restores today's behaviour exactly.

## Documentation Updates

`specs/reference/UI_INTERFACE_FSD.md` (Step 8). No guide, `CLAUDE.md` or `PROJECT_OVERVIEW.md`
change: no query, key, command or core concept changes.

## Phase 5 Entry Criteria

Not applicable: this design has no `workflow: liquers-project` and ends after Phase 4. It is
complete when every Progress item is ticked, the source issues are closed, and the reference is
updated.

## Validation

Design validation run on 2026-10-19: `validate_phase.py ui-events 1..4`,
`docs_index.py --check`, and `liquers-validate` on the example queries.
