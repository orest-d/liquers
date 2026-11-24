# Phase 2: Solution & Architecture — Update modes

## Overview

One enum, `UpdateMode` (`OnDemand`, `LivePreview`, `ApplyImmediately`), replaces the editor's
`continuous` flag. `EditorState` keeps an undo history and answers a committed edit by mode: a
`Write` (Apply immediately), a new `Preview` effect (Live preview) or nothing but `pending` (On
demand). A new `ValueDisplay` owns the value, its status and the "keep the old value until the new
one is there" logic, with the status bar, dimming and preview frame; the console and an editor
without a followed view draw through it. Snapshots carry the query they answer and whether they
are a preview, so a display ignores stale ones (AC-6). The runner evaluates a preview as an ad-hoc
recipe with the recipe's folder as cwd (`AssetManager::apply_queued`), so nothing upstream is
written or expires. Delays (debounce, quiet threshold, slow hint) are timer events the element
schedules for itself, never `Instant`.

Delivered in **two parts** (Phase 1 Open Question 8, decided 2026-11-24): **Part A** — the console:
`ValueDisplay`, snapshots, timers, the console's mode, ✓, Enter validation, undo, expiry by mode
and store browsing (AC-1 to AC-6, AC-10, AC-12 to AC-16, AC-19, AC-21, AC-22); **Part B** — the
editors: their three modes, the preview path and the `mode` arguments (AC-8, AC-9, AC-17, AC-18,
AC-20). Each part is one PR.

Rejected: a second preview pane (a value shown twice; Orest, 2026-11-24); a debounce in the runner
(it would still evaluate superseded requests and needs a clock in the loop); `std::time::Instant`
in elements (panics on wasm32); a console `EditorState` in On demand (its working copy then differs
from the text, which is today's lost-edit bug) — the console's editor always writes, and the
console itself decides when to evaluate; switching modes automatically on a slow result (AC-21).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `QUERY-CONSOLE-EDITOR-LOSES-EDITS-WHEN-UPDATE-IMMEDIATELY-IS-OFF` | draft | P2 | The defect AC-15 fixes | — | No | Fixed by Part A |
| `QUERY-CONSOLE-EDITOR-CANNOT-BROWSE-THE-STORE` | draft | P3 | Same dropped effects | — | No | Fixed by Part A (AC-16) |
| `QUERY-EDITOR-HAS-NO-LIVE-PREVIEW` | draft | P2 | The feature AC-17 adds | — | No | Fixed by Part B |
| `UI-ELEMENT-NESTED-IN-CONSOLE-HAS-NO-HANDLE` (`console-mounts-nested-element`, in review) | draft | P3 | Changes how the console holds a UI-element value, which `ValueDisplay` takes over | No | No | Whichever lands second moves the child into `ValueDisplay` |
| `CORE-QUERY-EDITOR-ACCEPTS-AN-UNKNOWN-COMMAND` | draft | P3 | AC-14 must refuse an unknown command | No | No | Designed around: Enter validates with `PlanBuilder`, which refuses it |
| `WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS` | draft | P2 | In the browser the status bar cannot tick while the runner awaits | No | No | Previews use `apply_queued`, which returns at once; AC-12 is tested on what the runner delivers |
| `query-editor-live-validation` (design, in review) | — | P2 | Also changes the console's query field | No | No | Independent: it reacts to typing, this to Enter |

Namespaces involved: `lui` only (to be confirmed at the gate).

## Interfaces

**`liquers-lib/src/ui/editor/` (both parts; sync — pure state, called from render and `update`).**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UpdateMode {
    #[serde(rename = "on-demand")] OnDemand,
    #[default] #[serde(rename = "preview")] LivePreview,
    #[serde(rename = "immediate")] ApplyImmediately,
}
impl UpdateMode {
    pub fn from_name(name: &str) -> Result<UpdateMode, Error>; // "on-demand" | "preview" | "immediate"
    pub fn name(self) -> &'static str;
}
// EditorCommand: SetContinuous(bool) is replaced by
    SetUpdateMode(UpdateMode),
    Undo,
    Redo,
// EditorEffect gains
    /// Show `value` as a preview (Live preview); nothing is written.
    Preview(Value),
    /// Drop the preview: show the applied value again (Revert, or a write in Live preview).
    ClearPreview,
// EditorState: `continuous` becomes `update_mode`
pub fn with_update_mode(self, mode: UpdateMode) -> Self;
pub fn update_mode(&self) -> UpdateMode;
pub fn can_undo(&self) -> bool;
pub fn can_redo(&self) -> bool;
// EditorTree: `continuous: bool` becomes
pub update_mode: UpdateMode,
pub can_undo: bool,
pub can_redo: bool,
/// The host (the console) draws the mode, ✓ and undo controls; the editor draws none (AC-13).
pub host_controls: bool,
```

The history is two `Vec<Edited>` (undo, redo) of whole working copies, capped at 100: an applied
mutating command pushes the copy it replaced and clears redo; `Undo`/`Redo` swap and then
`commit()` as the mode says. `commit()` by mode: `ApplyImmediately` → `Write`; `LivePreview` →
`pending = true`, `Preview(value)`; `OnDemand` → `pending = true`, `None`. `Update` writes the
pending copy (and, in Live preview, the following `on_written` yields `ClearPreview`); `Revert`
also yields `ClearPreview` in Live preview. `EditorKey` gains `Undo` (Ctrl+Z), `Redo`
(Ctrl+Shift+Z, Ctrl+Y) and `Discard` (Esc, = `Revert` in Live preview).

**`liquers-lib/src/ui/widgets/value_display.rs` (new, Part A; sync).**

```rust
#[derive(Debug, Clone, Default)]
pub struct ValueDisplay { /* value, metadata, error, status, ui_element: Option<Box<dyn UIElement>>,
    requested: Option<String>, shown: Option<String>, preview: bool, phase: DisplayPhase, generation: u64 */ }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayPhase { #[default] Settled, Waiting, Slow, ChangedUpstream }
pub enum SnapshotOutcome { Ignored, Progress, Settled }
impl ValueDisplay {
    /// A new query is awaited; the shown value stays. Returns the generation its timers carry.
    pub fn request(&mut self, query: &str, preview: bool) -> u64;
    pub fn on_snapshot(&mut self, snapshot: &AssetSnapshot) -> SnapshotOutcome; // Ignored: not `requested`
    pub fn on_timer(&mut self, generation: u64, timer: DisplayTimer);             // Quiet (250 ms), Slow (3 s)
    pub fn mark_changed_upstream(&mut self);
    pub fn is_dimmed(&self) -> bool;    // Slow, or ChangedUpstream, or an error over an old value
    pub fn status_line(&self) -> Option<StatusLine>; // status, progress, latest log, error, preview note
    pub fn value(&self) -> Option<&Arc<Value>>;
    #[cfg(feature = "egui")] pub fn show(&mut self, ui: &mut egui::Ui, ctx: &UIContext, app_state: &mut dyn AppState) -> egui::Response;
    #[cfg(feature = "webui")] pub fn render_html(&self, app_state: &dyn AppState) -> String;
}
pub enum DisplayTimer { Quiet, Slow }
```

**`liquers-lib/src/ui/message.rs` and `ui/mod.rs`.**

```rust
// AssetSnapshot gains (Part A)
pub query: Option<String>,   // the query the runner was asked for
pub preview: bool,           // Part B; false until then
// AppMessage gains (Part B)
/// Evaluate `query` as a preview — an ad-hoc asset with `cwd`, never stored — and push its
/// snapshots to `handle` as `UpdateMessage::AssetUpdate` with `preview: true`.
Preview { handle: UIHandle, query: String, cwd: Option<String> },
// ui/mod.rs (Part A): delivers `event` to `handle` after `after` (tokio::time::sleep natively,
// gloo_timers under `webui` on wasm32, at once otherwise).
pub fn schedule_event(ctx: &UIContext, handle: UIHandle, after: std::time::Duration, event: UIEvent);
```

Timer events are `UIEventKind::Named("lq:timer")` with fields `generation` and `timer`
(`debounce`, `quiet`, `slow`); an element acts on one only if its generation is current.

**`QueryConsoleElement` (Part A).** New fields: `update_mode: UpdateMode` (serde, default
`ApplyImmediately`; `LivePreview` is normalised to `ApplyImmediately`), `display: ValueDisplay`
(skip), `applied: String` (the last evaluated query, skip), `typed_error: Option<Error>` (skip).
`value`, `metadata`, `error`, `status` and `ui_element` move into `display`. Its `EditorState`
persists while the panel lives, always `ApplyImmediately`, with `host_controls` set by
`panel_tree`; it is rebuilt only when the text is *applied* from outside (history, presets,
Enter), not while typing. `submit_query` stops clearing the value.

**`EditorElement` (Part B).** `continuous: bool` becomes `update_mode: Option<UpdateMode>` with a
legacy `#[serde(default, skip_serializing)] continuous: Option<bool>` resolved once in
`ensure_state` (true → `ApplyImmediately`, false → `OnDemand`, absent → `LivePreview`).
`with_continuous` becomes `with_update_mode`. An editor with no `follow` gets its own
`display: ValueDisplay` (skip) for previews.

## Integration Points

- **Part A:** `ui/editor/{command,state,tree,view,keys}.rs` (mode, undo, `host_controls`);
  `ui/widgets/value_display.rs` (new) and `ui/widgets/mod.rs`; `ui/widgets/query_console_element.rs`
  (`apply_parameter_commands` handles every `EditorEffect` explicitly — `ListDirectory` sent as
  `AppMessage::ListDirectory`, `UpdateMessage::Directory` passed to the state; `RenameData` stays
  dropped, there is no node context in the console — toolbar, ✓, Enter, the `Expired` branch,
  `render_web`); `ui/message.rs` (`AssetSnapshot`), `ui/runner.rs` (`handle_request_asset_updates`,
  `poll_monitored_assets`, `build_snapshot` take the query; `MonitoredAsset` keeps it);
  `ui/mod.rs` (`schedule_event`; `schedule_volatile_refresh` moves onto it);
  `egui/editor.rs` (`toolbar`, `query_text`, `EditorKey::{Undo,Redo,Discard}`), `ui/web/editor.rs`;
  `ui/theme/{model,builtin,css}.rs` and `builtin.yaml` (a `preview` state colour, CSS
  `--lq-preview`); `ui/editor/element.rs` only for `SetUpdateMode` and the renamed tree field.
- **Part B:** `ui/editor/element.rs` (mode field, `Preview`/`ClearPreview` effects → `AppMessage::Preview`
  to `follow`, or to itself, with `recipe_folder(address)` as cwd; `AssetUpdate` into its own
  display); `ui/runner.rs` (`AppMessage::Preview`: `parse_query`, `Recipe::from(query)` with `cwd`,
  `get_asset_manager().apply_queued(recipe, State::new())`, then the same monitoring as
  `RequestAssetUpdates` with `preview: true`); `ui/widgets/query_console_element.rs` (a preview
  snapshot is framed; the next applied request clears it); `ui/commands.rs` (the `mode` arguments);
  `specs/command_registry.yaml` (regenerated, changelog line).
- **Tests:** `ui/editor/tests.rs` and `liquers-lib/tests/ui_editor_binding.rs` replace
  `with_continuous`/`SetContinuous`; the 14 `AssetSnapshot` literals gain the new fields.

## Error Handling

- Enter on text that does not parse or plan (AC-14): the `parse_query` / `PlanBuilder::build`
  error is kept in `typed_error`, its `position` highlighted; nothing is applied.
- `UpdateMode::from_name` with another name: `Error::general_error(format!("unknown update mode '{name}'; use on-demand, preview or immediate"))`.
  The command's `enum:` metadata refuses it earlier, in planning.
- Undo or Redo with an empty history: `Ok(EditorEffect::None)`, not an error (the buttons are
  disabled, a key may still arrive).
- A preview whose query fails to parse or evaluate: the runner delivers a snapshot with the error
  and `preview: true` (as `RequestAssetUpdates` does); `AppMessage::Preview` for a missing handle
  starts no monitoring (`DeliveryOutcome::Missing`).
- No new error types; no `Error::new`.

## Relevant Commands

Namespace `lui` (Part B; `register_command!` per `COMMAND_REGISTRATION_GUIDE.md`, enum parameters):

```rust
fn query_console(state, mode: String = "immediate" (enum: ["immediate", "on-demand"])) -> result
fn query_editor(state, mode: String = "preview" (enum: ["preview", "immediate", "on-demand"])) -> result
// action_editor, resource_editor and key_editor take the same `mode` argument as query_editor
```

Each keeps `version: auto`; the functions gain `mode: String` and pass `UpdateMode::from_name`.

## Documentation Architecture

| Document | Kind | Change |
|---|---|---|
| `specs/reference/UI_INTERFACE_FSD.md` | reference | Update modes, ✓ and Enter, undo, in-place preview and its cues, the console's panel, `ValueDisplay`, `AppMessage::Preview`, snapshot fields; History row, `reviewed:` |
| `specs/reference/THEME.md` | reference | The `preview` state colour; History row, `reviewed:` |
| `specs/command_registry.yaml` | generated | Regenerated (Part B) |
| `docs/guides/{rust,python,web}/11-ui.md` | user guide | "Update immediately" becomes the three modes; the console's ✓ and status bar |

API docs (`API_DOCUMENTATION_GUIDE.md` §3): `ui::editor` (main components: `UpdateMode`, undo;
example: a Live preview edit yields `Preview`), `ui::widgets::value_display` (new: all five parts),
`ui::widgets::query_console_element` (main components and example: an edit, then ✓), `ui::commands`
(where it is used: the `mode` arguments). `affects_docs` adds `specs/reference/THEME.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | About 20 in `liquers-lib/src/ui/`, `src/egui/`, two test files, the registry |
| Crates and workflows affected | `liquers-lib` (`egui`, `webui`); `check-build-matrix.sh` rows for both features and wasm32 |
| Existing tests likely to change | `ui/editor/tests.rs` (continuous), `ui_editor_binding.rs`, console tests asserting a cleared value on submit or "nothing is submitted" (now: submitted after the debounce), `registry_export` |
| New validation | Unit tests per AC on `EditorState`, `ValueDisplay` and the console with a test context; a runner test for `AppMessage::Preview` with a cwd; egui `Harness` tests for the toolbar; `render_web` HTML assertions |
| Compatibility / data / concurrency / performance / security | Saved elements: `continuous` still loads. Serialized `UpdateMode` is new. Timers are spawned tasks that only send a message; a stale one is ignored by generation. Previews are ad-hoc assets, never stored; Apply immediately in an editor can still write often, grouped by the 300 ms debounce |
| Recovery | Each part reverts on its own; Part B without Part A is not possible (it uses `ValueDisplay`) |
| Certainty and open questions | High for Part A. Part B: whether `apply_queued` with a recipe `cwd` resolves relative keys as the recipe's own evaluation does (AC-18) is verified first in Phase 3 by a test; if not, the preview uses the recipe's key with an override instead |
