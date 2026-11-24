# Phase 4: Implementation Plan — Update modes

## Overview

Part A first (one PR): the editor core's modes and undo, then `ValueDisplay`, snapshots and timers,
then the console, then the renderers and the theme. Part B second (one PR, on top of A): the
editor element's modes and preview path, the runner's `AppMessage::Preview`, the `mode` arguments.
Each step compiles and its tests pass before the next. No prerequisite issue must be fixed first
(Phase 2 preflight).

## Progress

- [ ] Step 1: `UpdateMode`, undo history and the new commands and effects in the editor core
- [ ] Step 2: `ValueDisplay`, `AssetSnapshot.query`/`preview`, and `schedule_event`
- [ ] Step 3: the console on `ValueDisplay`: panel effects, debounce, ✓, Enter, expiry by mode
- [ ] Step 4: renderers: egui and web toolbars, keys, the theme's preview colour
- [ ] Step 5: Part A checks, docs, PR
- [ ] Step 6: `EditorElement` modes, legacy `continuous`, preview effects
- [ ] Step 7: the runner's `AppMessage::Preview` and the binding tests
- [ ] Step 8: the `mode` arguments, the registry, Part B checks, docs, PR

## Implementation Steps

### Step 1: editor core
- Files / symbols: `liquers-lib/src/ui/editor/{state,command,tree,view,keys,mod}.rs` —
  `EditorState::{commit, apply, on_written}`, `EditorCommand`, `EditorEffect`, `EditorTree`,
  `EditorView::from_ui_event` (the `continuous` field event becomes `mode`), `EditorKey`.
- Change: `UpdateMode` (Phase 2), `SetUpdateMode`, `Undo`, `Redo`, `Preview`, `ClearPreview`;
  `continuous` → `update_mode`; history; `EditorTree::{update_mode, can_undo, can_redo,
  host_controls}`. Every `match` on the changed enums stays explicit. `element.rs`,
  `query_console_element.rs`, `egui/editor.rs` and `web/editor.rs` follow the rename only; until
  Step 6, `EditorElement` maps `continuous` to Apply immediately / On demand and treats `Preview`
  and `ClearPreview` as no-ops.
- Depends on: none.
- Proof: `cargo test -p liquers-lib --lib ui::editor` — `commit_follows_the_update_mode`,
  `undo_and_redo_restore_the_working_copy`, `update_mode_names_round_trip`.
- Rollback: revert the commit; nothing else depends on it yet.

### Step 2: value display, snapshots, timers
- Files / symbols: `liquers-lib/src/ui/widgets/value_display.rs` (new), `widgets/mod.rs`,
  `ui/message.rs` `AssetSnapshot`, `ui/runner.rs` `MonitoredAsset`, `build_snapshot`,
  `handle_request_asset_updates`, `poll_monitored_assets`, `ui/mod.rs` `schedule_event`.
- Change: as Phase 2; the 14 `AssetSnapshot` literals gain `query`, `preview: false`.
- Depends on: none (parallel with Step 1).
- Proof: `cargo test -p liquers-lib --lib ui::widgets::value_display` — the six tests.
- Rollback: revert; the console still has its own fields until Step 3.

### Step 3: the console
- Files / symbols: `liquers-lib/src/ui/widgets/query_console_element.rs` — `panel_tree`
  (`host_controls`), `parameters_tree` (rebuild only on an applied text), `apply_parameter_commands`
  (every effect), `submit_query`, `handle_ui_event` (`qc:update`, `qc:mode`, `qc:undo`, `qc:redo`,
  `lq:timer`), `update` (`Directory`, the `Expired` branch), `schedule_volatile_refresh` onto
  `schedule_event`.
- Depends on: Steps 1, 2.
- Proof: the nine console tests of Phase 3 and the existing console tests, updated where they
  asserted a cleared value or that a panel edit submits nothing.
- Rollback: revert Steps 3–4 together.

### Step 4: renderers and theme
- Files / symbols: `liquers-lib/src/egui/editor.rs` `toolbar`, `query_text`, key handling;
  `QueryConsoleElement::show_toolbar`, `show_content` and `render_web`;
  `liquers-lib/src/ui/web/editor.rs` toolbar; `ui/theme/{model,builtin,css}.rs`, `builtin.yaml`
  (`StateRoles::preview`, `--lq-preview`); icons `hand-tap`, `eye`, `lightning`, `check`,
  `arrow-u-up-left`, `arrow-u-up-right` (all in `ui/theme/icons.txt`).
- Depends on: Step 3.
- Proof: `toolbar_controls_follow_the_mode`, `render_web_shows_mode_status_and_dimming`,
  `panel_shows_no_controls_of_its_own`; the theme tests (`ui/theme/tests.rs`).
- Rollback: revert with Step 3.

### Step 5: Part A checks and docs
- Change: module docs of `ui::editor`, `ui::widgets::value_display`,
  `ui::widgets::query_console_element`; `UI_INTERFACE_FSD.md` and `THEME.md` (History rows,
  `reviewed:`); the three `11-ui.md` chapters; close
  `QUERY-CONSOLE-EDITOR-LOSES-EDITS-WHEN-UPDATE-IMMEDIATELY-IS-OFF` and
  `QUERY-CONSOLE-EDITOR-CANNOT-BROWSE-THE-STORE`.
- Proof: `cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh`;
  `python3 scripts/check_api_docs.py <each module>`; `cargo run -p liquers-docs -- check`;
  `python3 scripts/docs_index.py --check`.
- Rollback: the PR is reverted as a whole.

### Step 6: editor element modes
- Files / symbols: `liquers-lib/src/ui/editor/element.rs` — fields, `with_update_mode`,
  `ensure_state`, `run_effect` (`Preview`, `ClearPreview`), `update` (`AssetUpdate` into its own
  display), `show_in_egui`, `render_web`.
- Depends on: Part A merged.
- Proof: `live_preview_clears_on_write_and_revert`, `legacy_continuous_loads_as_a_mode`.
- Rollback: revert the commit.

### Step 7: preview in the runner
- Files / symbols: `ui/message.rs` `AppMessage::Preview`; `ui/runner.rs` a handler beside
  `handle_request_asset_updates`; `tests/ui_editor_binding.rs`.
- Depends on: Step 6.
- Proof: `live_preview_of_a_recipe_leaves_recipes_yaml_unchanged` (checks AC-18 first: if the cwd
  does not resolve as the recipe's own evaluation, stop and take Phase 2's fallback, which voids
  the pre-approval and returns to the Phase 2 gate), `apply_expires_a_dependent_view`,
  `on_demand_editor_writes_only_on_update`.
- Rollback: revert Steps 6–7.

### Step 8: commands and Part B checks
- Files / symbols: `liquers-lib/src/ui/commands.rs` `query_console`, `query_editor`,
  `action_editor`, `resource_editor`, `key_editor` and their `register_command!` lines;
  `specs/command_registry.yaml` (`export-command-registry`, a CHANGELOG line).
- Proof: `mode_argument_sets_the_element_mode`; `cargo test -p liquers-lib --test registry_export`;
  the Step 5 checks again; `liquers-validate` on `-R/data/sales.csv/-/ns-pl/head-3/ns-lui/query_console-on-demand`.
- Rollback: revert; the registry file reverts with it.

## Testing Plan

During each step only its own tests (and the files it changed) run. At Step 5 and Step 8 the
crate loop runs once (`cargo test -p liquers-lib --lib --tests`) and the feature matrix once,
because both steps change code under `cfg(feature = "egui")` / `"webui"`. Failing tests are re-run
alone while fixing. The browser end-to-end loops of `CLAUDE.md` are not needed: the HTML is
asserted in unit tests and no `liquers-web` code changes.

## Rollback Plan

Part A and Part B are separate PRs; reverting B leaves A working (editors keep On demand and Apply
immediately through `SetUpdateMode`, with Live preview behaving as On demand until B). Reverting A
requires reverting B first.

## Documentation Updates

As Phase 2's table: `specs/reference/UI_INTERFACE_FSD.md` and `specs/reference/THEME.md` (each with
a History row and `reviewed:` bump in the same commit), `docs/guides/{rust,python,web}/11-ui.md`
(`USER_DOCS_EDITING_GUIDE.md`), module docs checked with `scripts/check_api_docs.py`,
`specs/index.csv` regenerated, `specs/command_registry.yaml` regenerated in Step 8.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
