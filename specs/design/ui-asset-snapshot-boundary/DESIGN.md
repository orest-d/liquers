---
id: UI-ASSET-SNAPSHOT-BOUNDARY
kind: design
title: Widgets render asset snapshots; the runner owns asset subscriptions
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/egui, lib/ui]
issues: [EGUI-ASSET-MANAGER-INTEGRATION]
created: 2026-10-19
---
# Widgets render asset snapshots; the runner owns asset subscriptions

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

UI widgets must not reach the asset manager or spawn their own asset tasks. A widget either
receives its data when it is built, or receives `AssetSnapshot`s that `AppRunner` pushes.

### Problem Example

Verified at HEAD (`569c2f0`):

1. `liquers-lib/src/egui/widgets.rs` `TextEditor::new` (built by the `text_editor` command in
   `egui/commands.rs`) `tokio::spawn`s a task that calls
   `envref.get_asset_manager().get(&key).await?.get()`, delivers through a `oneshot`, and keeps a
   stale `// TODO: Use asset manager above when implemented correctly`. It panics outside a tokio
   runtime and records no dependency.
2. `AssetStatus::new` in the same file spawns a notification loop with `RwLock::write().unwrap()`.
   Nothing in the workspace constructs it.
3. `liquers-lib/src/ui/element.rs` `AssetViewElement::from_asset_ref` — called by
   `AppRunner::poll_evaluating_nodes` for every non-element result — subscribes and spawns a loop
   (`spawn_ui_task`) that lives as long as the asset's notification sender, i.e. as long as the
   asset is cached, even after the element is removed. Its `update` ignores
   `UpdateMessage::AssetUpdate`.

`QueryConsoleElement` already uses the centralized path: `AppMessage::RequestAssetUpdates` →
`AppRunner` subscribes and calls `update(AssetUpdate(snapshot))`, records `UIChange::Replaced` on
`NeedsRepaint`, and stops when the node goes (`ui_invalidation.rs` `snapshot_delivery_records_replaced`).

Concrete case: a pending node `-R-key/notes/readme.txt/-/text_editor` today shows a widget whose
text arrives from a detached task. Should be: `text_editor` reads `-R/notes/readme.txt` as a
recorded dependency and returns a `TextEditorElement` holding the text, mounted like any element;
a value node (`hello`) gets an `AssetViewElement` the runner keeps current, with no task.

### Scope and Acceptance Criteria

- **AC-1** Text editor through the runner
  WHEN a pending node with source `-R-key/notes/readme.txt/-/text_editor` is run, the store
  holding `notes/readme.txt` = `hello notes`
  THEN the node's element is a `TextEditorElement` whose text is `hello notes`
- **AC-2** Text editor nested in a console
  WHEN a `QueryConsoleElement` submits the same query
  THEN its rendered element is a `TextEditorElement` with that text, although nested elements are
  never `init`ed
- **AC-3** Missing text is an evaluation error
  WHEN the key does not exist
  THEN the node holds an `AssetViewElement` in Error mode naming the key; nothing panics
- **AC-4** The editor's asset depends on its key
  WHEN the editor's query has been evaluated
  THEN its metadata `dependencies` holds a `DependencyRecord` for `notes/readme.txt`, so the
  dependency manager can find the editor stale when the file changes
- **AC-5** Value views are updated by the runner
  WHEN a pending node `hello` has been mounted as an `AssetViewElement` and its asset is then expired
  THEN the next `AppRunner::run` delivers an `AssetUpdate` to it and records `UIChange::Replaced`
- **AC-6** No per-element tasks
  WHEN 20 pending value nodes are evaluated and mounted
  THEN the runtime's alive-task count after they settle equals the count before
- **AC-7** Removal stops monitoring
  WHEN a mounted value node is removed
  THEN after the next `run`, `has_monitoring()` is false
- **AC-8** Failure reaches the screen
  WHEN a console submits a query naming an unknown command
  THEN it receives an error snapshot and `UIChange::Replaced` is recorded for it
- **AC-9** Settled views do not force repainting
  WHEN every monitored asset was adopted from a pending node and has finished
  THEN `needs_repaint()` is false

Non-goals: saving the text (not saved today: `TEXT-EDITOR-EDITS-ARE-NEVER-SAVED`); removing
`ExtValue::Widget`; an event-driven wake-up instead of polling; input events (`ui-events`).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds the `pub` `TextEditorElement`, removes `pub` items
  `AssetStatus` and `AssetViewElement::from_asset_ref`, and changes the `text_editor` result type
  (rule 4).
- **Leading issue:** the boundary replaces the trait the accepted issue proposes (question 1).
- **Explanation:** every change is local to `liquers-lib` and reuses the message bus the console
  already proves; Phases 3-4 are written under the four recommended answers below.
- **Open questions:**
  1. **Proposed resolution - boundary:** no `UiAssetAdapter` trait. The stable boundary is the
     existing `AppMessage::RequestAssetUpdates` → `UpdateMessage::AssetUpdate(AssetSnapshot)` →
     `UpdateResponse` path, documented in `UI_INTERFACE_FSD.md`. A trait would duplicate it and
     make non-generic, typetag-serialized elements carry an `Arc<dyn …>` they cannot serialize.
     Recommended: accept.
  2. **Proposed resolution - text editor data:** `text_editor` becomes an async command that loads
     the text with `context.get_dependency_state` and returns `ExtValue::UIElement`
     (`TextEditorElement`); its result identifier changes from `egui.Widget` to `UIElement`, and a
     legacy caller rendering the value with `Value::show` sees a label instead of the editor.
     Runner-pushed snapshots were rejected because nested elements have no handle (AC-2).
     Recommended: accept.
  3. **Proposed resolution - removals:** delete `AssetStatus` (no callers; `AssetViewElement`
     Progress/Metadata modes and `show_asset_info` show the same) and `from_asset_ref` (only caller
     is the runner). Recommended: delete, no deprecation period.
  4. **Proposed resolution - repaint:** entries the runner adopts from pending nodes count toward
     `needs_repaint()` only until their last status is finished; console entries keep today's
     always-repaint rule. Otherwise every UISpec app repaints continuously. Trade-off: a later
     expiry of a settled view shows at the next input-driven frame, as today. Recommended: accept.

### Design Dependencies

- `ui-events` — **overlaps** (`in_review`): input delivery; shares `UIElement::update`, no
  decision. No order.
- `value-accessor` — **overlaps** (draft): could later back a saving editor; not required.
- `core-tracing-migration` — **overlaps** (`in_review`): converts the `eprintln!`s in
  `text_editor`, `TextEditor` and `AssetStatus`, which this design deletes. T3 (same change site)
  was judged not to hold: the only shared edit is log lines; whichever lands second drops them.

### Consolidated Findings

- The centralized path already exists and is tested for the console; the work is migrating three
  stragglers, one (`from_asset_ref`) inside the runner itself.
- `AssetStatus` is dead `pub` code; `TextEditor` is the last library producer of
  `ExtValue::Widget`, which then serves only external callers (a follow-up once true).
- Loading at command time records the key as a dependency (AC-4), works nested, and needs no
  runtime in the widget; runner-pushed data would fail nested (`UI-ELEMENT-NESTED-IN-CONSOLE-HAS-NO-HANDLE`).
- Runner-owned view monitoring would make `needs_repaint()` always true; decision 4 bounds it.
- `text_editor` is `version: auto`: `specs/command_registry.yaml` is regenerated (impl version only;
  arguments unchanged).

## Phase 2: Architecture

### Solution

1. **`TextEditorElement`** — new `liquers-lib/src/ui/widgets/text_editor_element.rs`, re-exported
   from `ui/widgets/mod.rs`: `#[derive(Clone, Debug, Serialize, Deserialize)]` with `handle`,
   `title_text`, `key: Key`, `text: String`; `#[typetag::serde] impl UIElement`; `show_in_egui`
   (`cfg(feature = "egui")`) draws what the old widget drew; `render_web` escapes the text into a
   read-only `<textarea>`. No task, channel or `EnvRef`.
2. **`text_editor`** (`egui/commands.rs`) becomes
   `pub async fn text_editor<E: Environment<Value = Value>>(state: &State<Value>, context: Context<E>) -> Result<Value, Error>`:
   `state.value()?.try_into_key()?`, `context.get_dependency_state(&Query::from(&key)).await?`,
   `.value()?.try_into_string()?`, then `Value::from(ExtValue::UIElement { value: Arc::new(..) })`.
   Registration: `async fn text_editor(state, context) -> result`, same label, doc, `version: auto`.
3. **`AssetViewElement`** (`ui/element.rs`): remove `from_asset_ref`, `notification_rx` and
   `sync_from_notifications`; `update` handles `AssetUpdate(snapshot)` — value (kept when an
   `Expired`/`Volatile` snapshot carries none, as the console does), error, `progress_info` from
   `snapshot.metadata`, view mode — returning `NeedsRepaint` when anything changed.
4. **`AppRunner`** (`ui/runner.rs`): `poll_evaluating_nodes` mounts
   `AssetViewElement::new_value(title, value)` and inserts a `MonitoredAsset` (subscribing first);
   `MonitoredAsset` gains `adopted: bool` and `last_status: Status`; `needs_repaint()` becomes
   `has_evaluating() || monitoring.values().any(|m| !m.adopted || !m.last_status.is_finished())`
   (decision 4); `poll_monitored_assets` updates `last_status` from each snapshot.
5. **Delete** `TextEditor`, `AssetStatus` and their `Debug`/`WidgetValue` impls from
   `egui/widgets.rs`, with the now-unused `oneshot`, `EnvRef`, `AssetManager` imports.

Rejected: the `UiAssetAdapter` trait (decision 1); a separate asset service (the runner is one);
a full reducer (rewrites `AppState`); requesting data in `init` (fails nested, AC-2); keeping
`from_asset_ref` beside runner monitoring (two subscriptions per view).

Known-issue preflight: no open issue names these symbols; `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT`
loses two sites in `AssetStatus`. Namespaces: root (`text_editor`), `lui` (unchanged).

### Changes

New/changed signatures only:

```rust
pub struct TextEditorElement { handle: Option<UIHandle>, title_text: String, pub key: Key, pub text: String }
impl TextEditorElement { pub fn new(key: Key, text: String) -> Self; }
pub async fn text_editor<E: Environment<Value = Value>>(state: &State<Value>, context: Context<E>) -> Result<Value, Error>;
```

Errors: `get_dependency_state` / `try_into_string` errors propagate. Async: the command awaits;
elements stay synchronous. Documents:
`specs/reference/UI_INTERFACE_FSD.md` gains "Asset updates" (the boundary, who subscribes, repaint
rule) with a History row and `reviewed:` bump; `specs/command_registry.yaml` regenerated with a
CHANGELOG line.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A caller renders a `text_editor` value with `Value::show` | runner and console mount elements; no in-tree caller; decision 2 | medium |
| View loses live updates during migration | AC-5 test; runner path already proven by the console | high |
| `needs_repaint` change hides a late expiry | only adopted, finished entries; AC-9; console unchanged | medium |
| wasm: element compiled without `egui` | `show_in_egui` gated; `scripts/check-build-matrix.sh` | high |

Recovery: one commit per step.

## Phase 3: Examples and Tests

### Examples

`-R-key/notes/readme.txt/-/text_editor` (validated: `UseKeyValue[notes, readme.txt]`, then
`Action{text_editor}`, root namespace) and its dependency `-R/notes/readme.txt`; `hello` (a test
command) for value views.

### Tests

Harness: `tests/ui_invalidation.rs`'s `harness()`, with an `AsyncMemoryStore` holding
`notes/readme.txt` and `register_egui_commands!` added; queued `DefaultEnvironment`.

New file `liquers-lib/tests/ui_asset_snapshot_boundary.rs` (`#![cfg(feature = "egui")]`), each
`#[tokio::test(flavor = "multi_thread", worker_threads = 2)]`:

- `snapshot01_text_editor_node_holds_text` — pending node, run until mounted: AC-1
- `snapshot02_text_editor_in_console` — console `RequestAssetUpdates`; downcast via `get_value()` → `as_ui_element()`: AC-2
- `snapshot03_text_editor_missing_key_is_error_view` — AC-3
- `snapshot04_text_editor_records_key_dependency` — `get_metadata()` of the editor's asset lists the key: AC-4
- `snapshot05_value_view_receives_runner_updates` — `hello` mounted, `envref.evaluate("hello")` then `expire()`, run, `UIChange::Replaced` for the handle: AC-5
- `snapshot06_value_views_spawn_no_tasks` — `Handle::current().metrics().num_alive_tasks()` before/after 20 nodes: AC-6
- `snapshot07_removed_view_stops_monitoring` — AC-7
- `snapshot08_unknown_command_error_snapshot_repaints` — AC-8
- `snapshot09_settled_views_do_not_need_repaint` — AC-9

Unit tests: `ui/element.rs` `asset_view_update_sets_value_and_repaints`,
`asset_view_expired_snapshot_keeps_value` (AC-5); `text_editor_element_serde_round_trip` (AC-1).

Run: `cargo test -p liquers-lib --test ui_asset_snapshot_boundary`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-lib/tests/ui_asset_snapshot_boundary.rs` — add the nine tests; 01-06 and 09 fail at HEAD — `cargo test -p liquers-lib --test ui_asset_snapshot_boundary`. Rollback: delete the file.
- [ ] 2. `ui/widgets/text_editor_element.rs`, `ui/widgets/mod.rs` — `TextEditorElement` + serde unit test — `cargo test -p liquers-lib --lib text_editor_element`.
- [ ] 3. `egui/commands.rs` `text_editor` + `register_egui_commands!` — async, dependency load, `UIElement` result — snapshot01-04.
- [ ] 4. `ui/element.rs` `AssetViewElement` (`update`, remove `from_asset_ref`, `notification_rx`, `sync_from_notifications`) and `ui/runner.rs` (`poll_evaluating_nodes`, `MonitoredAsset`, `poll_monitored_assets`, `needs_repaint`) — snapshot05-09 and the element unit tests; `--test ui_invalidation --test ui_runner --test query_console_integration`. Rollback: revert steps 4 together.
- [ ] 5. `egui/widgets.rs` — delete `TextEditor`, `AssetStatus`, unused imports — `cargo test -p liquers-lib --lib --tests`.
- [ ] 6. `specs/command_registry.yaml` — regenerate (`cargo run -p liquers-lib --features cli --bin export-command-registry -- --format yaml -o specs/command_registry.yaml`), CHANGELOG line — `cargo test -p liquers-lib --test registry_export`.
- [ ] 7. Docs — `UI_INTERFACE_FSD.md` "Asset updates" (History row, `reviewed:`); close `EGUI-ASSET-MANAGER-INTEGRATION` with a resolution note — `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh` (egui-off and wasm
rows: the element is built without `egui`); after `cargo clean`, the `ui_spec_demo` Playwright run
(`CLAUDE.md`); `python3 scripts/docs_index.py --check`; a diff review for leftover debug output and
edits outside `liquers-lib`, the registry and the two documents.
