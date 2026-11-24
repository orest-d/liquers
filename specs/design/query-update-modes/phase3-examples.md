# Phase 3: Examples & Use-cases — Update modes

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A console edit, Apply immediately and On demand | Debounced evaluation, no flicker, ✓ | AC-1, AC-3, AC-8, AC-13 |
| 2 | Example | Live preview of a recipe, then ✓ | Preview in place, nothing written, cascade on apply | AC-17, AC-18, AC-19 |
| 3 | Tests | `ui::editor::tests` (4) | Modes, undo, names, legacy load | AC-8, AC-9, AC-10, AC-15, AC-20 |
| 4 | Tests | `ui::widgets::value_display::tests` (6) | Fast, slow, error, stale, hint, cues | AC-3, AC-4, AC-5, AC-6, AC-21, AC-22 |
| 5 | Tests | `ui::widgets::query_console_element::tests` (9) | The console end to end with a test context | AC-1, AC-8, AC-10, AC-12 to AC-16, AC-19, AC-22 |
| 6 | Tests | `egui::editor::tests` (1) | Toolbar controls by mode | AC-13, AC-22 |
| 7 | Tests | `tests/ui_editor_binding.rs` (3), `ui::commands::tests` (1) | Preview through the runner, apply and cascade, `mode` argument | AC-9, AC-17, AC-18, AC-19, AC-20 |

## Example 1: a console edit (Part A)

A console on `-R/data/sales.csv/-/ns-pl/head-3` (validated), panel open, Apply immediately.

1. The panel's `SetField n=7` → the console's `EditorState` (always `ApplyImmediately`) returns
   `Write`; `query_text` becomes `-R/data/sales.csv/-/ns-pl/head-7` (validated); a `debounce` timer
   with generation *g* is scheduled (300 ms). A second edit within 300 ms schedules *g+1*; the *g*
   timer is then ignored.
2. The *g+1* timer arrives → `display.request(query, false)` and `AppMessage::RequestAssetUpdates`;
   a `quiet` timer (250 ms) and a `slow` timer (3 s) are scheduled for the same generation.
3. A `Ready` snapshot for `…/head-7` within 250 ms → `on_snapshot` returns `Settled`; the old
   value was on screen until then (no empty frame); no status bar was ever drawn.

The same edit **On demand**: step 1 rewrites the text, schedules nothing, and the ✓ is enabled
(text ≠ `applied`); pressing ✓ (or Enter) runs step 2.

```rust
let mut c = QueryConsoleElement::new("C".into(), "-R/data/sales.csv/-/ns-pl/head-3".into());
c.set_update_mode(UpdateMode::OnDemand);
c.apply_parameter_commands(vec![set_n("7")], &ctx);
assert_eq!(c.query_text, "-R/data/sales.csv/-/ns-pl/head-7");
assert!(c.check_mark_enabled() && no_message(&mut rx)); // nothing evaluated yet (AC-8)
c.update(&UpdateMessage::UI(named("qc:update")), &ctx);  // ✓
assert_eq!(requested(&mut rx), Some("-R/data/sales.csv/-/ns-pl/head-7".to_string()));
```

## Example 2: Live preview of a recipe (Part B)

`reports/recipes.yaml` holds recipe 0, `-R/data.txt/-/upper` (validated with `--command upper`;
`-R/data.txt` is relative to the recipe's folder, so it reads `reports/data.txt`). A
`lui/query_editor` bound to
`-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query` (validated), Live preview (its
default), follows a console on the recipe's result.

1. The edit appends `/lower`. `EditorState::commit` → `pending`, `Preview(value)`.
2. The element sends `AppMessage::Preview { handle: follow, query: "-R/data.txt/-/upper/lower",
   cwd: Some("reports") }`. The runner applies `Recipe::from(query)` with `cwd = "reports"` through
   `apply_queued`; the console gets snapshots with `preview: true` and frames the value
   (AC-22). `recipes.yaml` is unchanged and nothing expires.
3. ✓ → `Write` → the accessor writes `recipes.yaml` → the recipe's dependents expire
   (`recipe-dependency-versions`) → `on_written` yields `ClearPreview` and the console is
   re-pointed at the applied query, unframed. A second console showing a dependent report, in Apply
   immediately, recalculates on the expiry; On demand it dims with "changed upstream" (AC-19).

## Edge and Error Cases

- **Out of order** (AC-6): a snapshot whose `query` is not `requested` → `Ignored`; covered for the
  console and for a preview followed by an apply.
- **Enter on bad text** (AC-14): `-R/data/sales.csv/-/ns-pl/nosuch` → `PlanBuilder` error; nothing
  sent, `typed_error` set, value unchanged. Also a parse error (`-R/a/-/head-~`).
- **Typing then a GUI edit:** unentered typed text is replaced by the edited applied query; the
  typed text is not applied (documented in `UI_INTERFACE_FSD.md`).
- **Undo past the start:** `Undo` with an empty history → `EditorEffect::None`, buttons disabled.
- **Serialization:** a console and an editor round-trip their `update_mode`; an editor saved with
  `continuous: false` loads `OnDemand`, `true` → `ApplyImmediately`, neither → `LivePreview`.
- **Feature flags:** `ValueDisplay` builds with neither `egui` nor `webui`;
  `scripts/check-build-matrix.sh` covers `--no-default-features` and wasm32.
- **wasm32:** no `Instant`; `schedule_event` compiles to `gloo_timers` under `webui`.

## Test Plan

Part A — unit, in-file `#[cfg(test)] mod tests`:

- `ui/editor/tests.rs`
  - `commit_follows_the_update_mode` (AC-8, AC-9): Write / Preview / pending per mode; `Update`
    writes the pending copy.
  - `undo_and_redo_restore_the_working_copy` (AC-10): three edits, two undos, one redo; redo cleared
    by a new edit; history capped at 100.
  - `update_mode_names_round_trip` (AC-20): `from_name`/`name` and serde; an unknown name errors.
  - `legacy_continuous_loads_as_a_mode` (AC-15): the three load cases above.
- `ui/widgets/value_display.rs`
  - `fast_result_replaces_without_an_empty_frame` (AC-3)
  - `slow_result_dims_and_shows_progress` (AC-4): after `Quiet`, `is_dimmed` and a status line with
    the status, progress and last log of the snapshot's metadata.
  - `error_keeps_the_previous_value` (AC-5)
  - `snapshot_of_another_query_is_ignored` (AC-6)
  - `slow_timer_offers_on_demand` (AC-21): the status line carries the switch after `Slow`, and
    the mode is unchanged.
  - `preview_is_framed_and_stale_is_dimmed` (AC-22)
- `ui/widgets/query_console_element.rs`
  - `panel_edit_evaluates_after_the_debounce` (AC-1): one request for a burst of two edits.
  - `on_demand_waits_for_the_check_mark` (AC-8, AC-13)
  - `panel_shows_no_controls_of_its_own` (AC-13): `host_controls` on the panel tree; egui and HTML
    have one mode control and one ✓.
  - `enter_applies_only_valid_text` (AC-14)
  - `pending_panel_edits_survive_a_frame` (AC-15): the probe of
    `QUERY-CONSOLE-EDITOR-LOSES-EDITS-WHEN-UPDATE-IMMEDIATELY-IS-OFF`, now passing.
  - `undo_in_the_panel_rewrites_the_text` (AC-10)
  - `browse_lists_the_store` (AC-16): `ListDirectory` sent; a `Directory` update reaches the panel.
  - `expiry_follows_the_mode` (AC-19): `Expired` → request (immediate) or `ChangedUpstream` (on demand).
  - `render_web_shows_mode_status_and_dimming` (AC-12, AC-22): `data-lq-event="qc:update"`,
    `qc:mode`, `qc:undo`, `.lq-status-bar`, `.lq-value-stale`, `.lq-value-preview`.
- `egui/editor.rs` — `toolbar_controls_follow_the_mode` (AC-13, AC-22): the three icons, ✓ states
  and no ✓ in Apply immediately, in a headless `egui::Context`.

Part B:

- `ui/editor/tests.rs` — `live_preview_clears_on_write_and_revert` (AC-17).
- `ui/commands.rs` — `mode_argument_sets_the_element_mode` (AC-20).
- `liquers-lib/tests/ui_editor_binding.rs`
  - `live_preview_of_a_recipe_leaves_recipes_yaml_unchanged` (AC-17, AC-18): the preview's value
    equals the recipe's value after apply, `recipes.yaml` is byte-identical before ✓.
  - `apply_expires_a_dependent_view` (AC-9, AC-19): a second console on a dependent query gets
    `Expired` and re-requests.
  - `on_demand_editor_writes_only_on_update` (AC-8): replaces the `with_continuous(false)` test.
- Existing: `cargo test -p liquers-lib --test registry_export` after regenerating the registry.

Commands: `cargo test -p liquers-lib --lib --tests`; for the cfg rows
`bash scripts/check-build-matrix.sh`.

## Learning Log

- A recipe's `cwd` set in code prepends the same `Step::SetCwd` as `DefaultRecipeProvider`
  (`RECIPES_AND_PLANS.md`), so a preview through `apply_queued` with that `cwd` should resolve
  relative keys as the recipe does; `live_preview_of_a_recipe_leaves_recipes_yaml_unchanged` proves
  it first in Part B.
- Timer events delivered through `update()` make the console's timing deterministic in tests: a
  test sends the `lq:timer` event itself instead of sleeping.
- The console's `EditorState` must never hold pending edits; that is the whole cause of the lost
  edit bug.
