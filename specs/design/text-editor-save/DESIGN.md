---
id: TEXT-EDITOR-SAVE
kind: design
title: The text editor saves its text to the key it was opened on
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/ui, lib/egui]
issues: [TEXT-EDITOR-EDITS-ARE-NEVER-SAVED]
affects_docs: [UI_INTERFACE_FSD]
created: 2026-10-21
---
# The text editor saves its text to the key it was opened on

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written on 2026-10-21, not
approved, nothing implemented. It builds on `TextEditorElement`, which
`ui-asset-snapshot-boundary` introduces; it cannot start before that design's Step 3.

## Phase 1: High-Level Design

### Purpose

`text_editor` promises an editor ("for editing text identified by a key"). The user must be able
to save the edited text back to that key, see that it was saved or why it failed, and have every
asset that depends on the key follow.

### Problem Example

Verified at HEAD (`8ecfd7f`): `liquers-lib/src/egui/widgets.rs` `TextEditor` binds
`egui::TextEdit::multiline` to its own `String`; no code path writes it anywhere (no message, no
store or asset call; `AppMessage` in `ui/message.rs` has no write variant).

Concrete case: the store holds `notes/readme.txt` = `hello notes`; a node shows
`-R-key/notes/readme.txt/-/text_editor` (validated). The user types ` and more` and presses
Ctrl+S. Today nothing happens; re-evaluating shows `hello notes`. Should be:
`notes/readme.txt` becomes `hello notes and more` (asset status `Override`/`Source` per
`specs/reference/ASSET_SET_OPERATION.md`), the editor shows "Saved", and a view of
`-R/notes/readme.txt/-/ns-template/expand` or any other dependent is invalidated.

### Scope and Acceptance Criteria

- **AC-1** Save writes the key
  WHEN a mounted `TextEditorElement` for `notes/readme.txt` has text `hello notes and more` and
  the user saves (button or Ctrl+S)
  THEN after the next `AppRunner::run` the key's asset holds that text and the store has it
- **AC-2** Outcome reaches the editor
  WHEN the save completes
  THEN the editor receives the outcome, shows "Saved", and is no longer dirty
- **AC-3** Failure is shown
  WHEN the write fails (a read-only mount, an unregistered type)
  THEN the editor shows the error message, keeps the text and stays dirty
- **AC-4** Dependents follow
  WHEN a view depends on the key and the editor saves
  THEN that view's asset is invalidated as for any `set_state` (dependency manager), and a monitored
  view receives an update
- **AC-5** Save only when dirty
  WHEN the text equals the last loaded or saved text
  THEN the save control is disabled and Ctrl+S sends nothing
- **AC-6** Format kept
  WHEN the key's current metadata names a data format and media type (`readme.md`, `md`)
  THEN the saved state keeps them

Non-goals: conflict detection against a concurrent change (`store-conditional-write` may later
offer it); autosave; a web editing surface (`render_web` stays read-only); undo history.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a public `AppMessage` and `UpdateMessage` variant and
  the `AssetSetOutcome` type (rules 3, 4).
- **Leading issue:** **Open design - outcome delivery:** a typed `UpdateMessage::AssetSet(AssetSetOutcome)`
  variant versus `UpdateMessage::Custom(Box<AssetSetOutcome>)`. The typed variant breaks seven
  exhaustive matches (as `ui-events`' `User` does); `Custom` needs a downcast and is invisible to
  the compiler. Recommended: typed variant.
- **Explanation:** the write path exists (`AssetManager::set_state`, with its metadata checks and
  dependent invalidation); the work is a message, a runner handler and the element's save state.
- **Open questions:**
  1. **Open design - outcome delivery:** as the leading issue.
  2. **Proposed resolution - write path:** the runner calls `set_state(key, state)` (memory and
     store, dependents invalidated), not a raw store write that would bypass the asset layer.
  3. **Proposed resolution - unmounted editor:** an editor without a handle (shown nested in a
     console before `console-mounts-nested-element` lands) still saves; the outcome is not
     delivered and the editor shows "Sent".
  4. **Proposed resolution - metadata:** `type_identifier`/`type_name` `text`; `data_format` and
     `media_type` copied from the key's current metadata when it has a record, else the value's
     default (AC-6).

### Design Dependencies

- `ui-asset-snapshot-boundary` (in review): **requires** — creates `TextEditorElement`. Overlap
  test T3 (same type) holds, but that design declares saving a non-goal and this design only adds
  to its finished element, so it is an order (`requires`), not a merge.
- `console-mounts-nested-element`: **overlaps** — gives a nested editor a handle (question 3).
- `template-editor-ui`: **required-by** — the template editor saves through this path.
- `ui-events` (in review): **overlaps** — also adds an `UpdateMessage` variant; whichever lands
  second adds the other's arm.
- `store-conditional-write` (in review): **overlaps** — a later conflict check (non-goal).

### Consolidated Findings

- `set_state` already enforces type/format consistency and invalidates dependents; the editor
  needs no store code.
- A save needs the editor's handle only for the outcome; the write itself does not.
- Seven files match `UpdateMessage` exhaustively (`ui/element.rs`, console, markdown element,
  `examples/ui_button_app.rs`, `examples-web/ui_button_app`, `tests/ui_invalidation.rs`,
  `tests/ui_runner.rs`).

## Phase 2: Architecture

### Solution

`TextEditorElement` (from `ui-asset-snapshot-boundary`) gains `#[serde(skip)] saved_text: String`
(set on construction) and `#[serde(skip)] save_state: SaveState`. Saving sends
`AppMessage::SetAssetValue { handle, key, value }`; `AppRunner::process_messages` calls
`set_state` and delivers `UpdateMessage::AssetSet(AssetSetOutcome)` to `handle` through the
existing take-update-put path (`deliver_snapshot` generalised to an `UpdateMessage`, as
`ui-events` also plans).

Rejected: writing the store from the element (no runtime in elements); a `lui/save` command
(needs the text as a query parameter); `value-accessor` (abandoned).

### Changes

```rust
// ui/message.rs
pub enum AppMessage { /* … */ SetAssetValue { handle: Option<UIHandle>, key: Key, value: Value } }
#[derive(Debug, Clone)]
pub struct AssetSetOutcome { pub key: Key, pub error: Option<Error> }
// ui/element.rs
pub enum UpdateMessage { /* … */ AssetSet(super::message::AssetSetOutcome) }
// ui/widgets/text_editor_element.rs
#[derive(Debug, Clone, Default, PartialEq)]
pub enum SaveState { #[default] Clean, Dirty, Saving, Sent, Saved, Failed(String) }
impl TextEditorElement {
    pub fn is_dirty(&self) -> bool;
    /// Sends `SetAssetValue` when dirty; returns whether a message was sent.
    pub fn save(&mut self, ctx: &UIContext) -> bool;
    pub fn save_state(&self) -> &SaveState;
}
```

Runner: `async fn handle_set_asset_value(&mut self, handle, key, value, app_state)` — metadata
from `get_asset_info`/existing record where present (question 4), `State::from_parts`, then
`self.envref.get_asset_manager().set_state(&key, state).await`; the `Result` becomes the outcome.
Element: `update(AssetSet(o))` for its key sets `Saved` + `saved_text = text` or `Failed(msg)`;
`show_in_egui` adds a Save button (enabled when dirty), Ctrl+S while the editor has focus, and a
status label. Other elements answer `Unchanged` to `AssetSet`. Async: the runner awaits; elements
stay sync. Errors: `set_state` errors pass through unchanged. Commands: none
(`specs/command_registry.yaml` unchanged). Documents: `UI_INTERFACE_FSD.md` "Saving from an
element" (History, `reviewed:`).

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Lost update if the key changed since loading | non-goal; documented; `store-conditional-write` later | high |
| `.md` key saved with the `text` default format | copy format from current metadata (AC-6) | medium |
| Exhaustive matches break | intended; listed above; same step | high |
| Read-only mount | `set_state` error shown (AC-3) | high |

Recovery: revert; nothing persisted beyond the user's own save.

## Phase 3: Examples and Tests

### Examples

`-R-key/notes/readme.txt/-/text_editor` (validated) mounted in a pending node over an
`AsyncMemoryStore` holding `notes/readme.txt`; `-R/notes/readme.txt` (validated) read back.

### Tests

New `liquers-lib/tests/ui_text_editor_save.rs` (`#![cfg(feature = "egui")]`, harness of
`tests/ui_invalidation.rs`, `register_egui_commands!`), each
`#[tokio::test(flavor = "multi_thread", worker_threads = 2)]`:

- `save01_writes_key_and_store` — mount, edit text, `save`, `run`; `-R/notes/readme.txt` evaluates to the new text and the store holds it: AC-1
- `save02_outcome_marks_saved_and_clean` — AC-2
- `save03_read_only_mount_reports_failure` — environment with a `read_only` mount: AC-3
- `save04_dependent_view_is_invalidated` — a monitored `-R/notes/readme.txt` view receives an update after the save: AC-4
- `save06_keeps_markdown_format` — key `notes/readme.md`; saved metadata `data_format` `md`: AC-6

Unit, `ui/widgets/text_editor_element.rs`: `save_when_clean_sends_nothing` (AC-5),
`save_without_handle_is_sent` (AC-1, question 3), `asset_set_outcome_for_other_key_ignored` (AC-2).

Run: `cargo test -p liquers-lib --test ui_text_editor_save && cargo test -p liquers-lib --lib text_editor_element`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `ui/message.rs` `AppMessage::SetAssetValue`, `AssetSetOutcome`; `ui/element.rs` `UpdateMessage::AssetSet` and the arm in every exhaustive match (seven files) — `cargo test -p liquers-lib --lib --tests` builds and passes. Rollback: revert.
- [ ] 2. `ui/runner.rs` `handle_set_asset_value`, `process_messages` arm, delivery through the generalised `deliver_update` (introduce it here if `ui-events` has not) — `cargo test -p liquers-lib --test ui_text_editor_save save01 save03 save06`.
- [ ] 3. `ui/widgets/text_editor_element.rs` `saved_text`, `SaveState`, `is_dirty`, `save`, `update(AssetSet)`, egui Save button, Ctrl+S and status — remaining tests. Rollback: revert the file.
- [ ] 4. `specs/reference/UI_INTERFACE_FSD.md` "Saving from an element" (History, `reviewed:`); close `TEXT-EDITOR-EDITS-ARE-NEVER-SAVED` — `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh` (egui-off and
wasm32 rows build the message types without `egui`); a diff review for edits outside
`liquers-lib/src/ui`, the tests, the examples' match arms and the reference document.
