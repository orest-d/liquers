---
id: CONSOLE-MOUNTS-NESTED-ELEMENT
kind: design
title: A query console mounts a UIElement result as its child node
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/ui]
issues: [UI-ELEMENT-NESTED-IN-CONSOLE-HAS-NO-HANDLE]
affects_docs: [UI_INTERFACE_FSD]
created: 2026-10-21
---
# A query console mounts a UIElement result as its child node

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written on 2026-10-21, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

An element shown through a query console should behave as it does anywhere else: be placed in
`AppState`, get `init` with a handle, receive updates and be able to send messages. Today only
passive elements work inside a console, which constrains every element design.

### Problem Example

Verified at HEAD (`8ecfd7f`), `liquers-lib/src/ui/widgets/query_console_element.rs`: on
`UpdateMessage::AssetUpdate`, `update` stores `as_ui_element()?.clone_boxed()` in the private
`ui_element` field; `show_content` calls its `show_in_egui`; `render_web` renders the value with
`value_to_html`, which calls the element's `render_web`. Nothing calls `init`, so `handle()` is
`None`. `submit_query` and `request_asset_updates` send only `if let Some(handle) = self.handle`.

Concrete case: console A evaluates `hello/q/ns-lui/query_console` (validated: `Ok`), whose value
is a `QueryConsoleElement` B. The user types `hello` into B and presses Enter. Today nothing
happens: B has no handle, sends no `RequestAssetUpdates`, and the runner could not deliver to it
anyway. Should be: B is a child node of A with its own handle; Enter evaluates `hello` and B
shows the result.

### Scope and Acceptance Criteria

- **AC-1** Nested element is mounted and initialised
  WHEN console A receives a snapshot whose value is a `UIElement`
  THEN after the next `AppRunner::run` A has exactly one child node holding that element, and the
  element's `handle()` is that child's handle
- **AC-2** Nested console works
  WHEN the nested console B submits `hello`
  THEN the runner sends B the snapshot of `hello`, and B shows it
- **AC-3** Replacement
  WHEN A later receives a snapshot with a different `UIElement`, or a non-element value
  THEN the previous child (and its subtree) is removed; the new element is mounted, or no child remains
- **AC-4** Refresh does not remount
  WHEN A receives a snapshot carrying the same value `Arc` again (volatile refresh, expiry resubmit)
  THEN the child is kept, with its state
- **AC-5** Removal cascades
  WHEN A is removed
  THEN its child is removed, and the runner stops monitoring both
- **AC-6** Rendering
  WHEN A is in data view with a mounted child
  THEN egui renders the child through `AppState` (extract-render-replace) and `render_web` renders
  the child node's markup inside A's content

Non-goals: mounting results of elements other than the console (each container decides);
persisting the child separately from A's query (it is rebuilt from A's query on load).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a public `AppMessage` variant (rule 4).
- **Leading issue:** **Proposed resolution - mount, not document:** the issue offers mounting or
  documenting the limit. Mounting is recommended: it removes the constraint that shaped
  `ui-asset-snapshot-boundary`; documenting would leave nested consoles broken.
- **Explanation:** `AppState` already has child nodes, subtree removal and `UIChange` records; the
  runner already initialises elements it places. The change is one message and its handler.
- **Open questions:**
  1. **Proposed resolution - mount, not document:** as the leading issue.
  2. **Proposed resolution - who mounts:** the runner, on `AppMessage::ReplaceChildren { parent,
     element: Option<Box<dyn UIElement>> }` sent by the console from `update`. Rejected: the console
     locking `AppState` inside `update` (the lock-free calling convention is not part of the
     `UIElement` contract) and a new trait method (every element would carry it).
  3. **Proposed resolution - identity:** remount only when the value `Arc` differs from the one last
     mounted (`Arc::ptr_eq`), so refreshes keep the child's state (AC-4).

### Design Dependencies

- `ui-asset-snapshot-boundary` (in review): **overlaps** — its decision 2 loads the text editor at
  command time partly because nested elements lacked a handle; that choice stays valid. Both edit
  `QueryConsoleElement::update`; textual rebase only.
- `text-editor-save`: **required-by** (soft) — a nested editor gets a handle and so its save outcome.
- `ui-events` (in review): **overlaps** — both add an `AppMessage` variant and edit the console.
- `query-editor-live-validation`: **overlaps** — edits the console's toolbar, not its content.

### Consolidated Findings

- `DirectAppState::remove` already removes a subtree, so AC-5 needs no new code; the runner's
  `poll_monitored_assets` drops entries of nodes that no longer exist.
- A serialized app state keeps A's child; on load A re-submits its query and the first snapshot
  replaces the child, so stale children do not survive.

## Phase 2: Architecture

### Solution

`QueryConsoleElement` drops the `ui_element` field and gains `#[serde(skip)] mounted: Option<Arc<Value>>`.
In `update(AssetUpdate)`: if the (kept) value is a `UIElement` and not `Arc::ptr_eq` with
`mounted`, it sends `ReplaceChildren { parent: handle, element: Some(elem.clone_boxed()) }` and
records `mounted`; if it is not an element and `mounted` is set, it sends `element: None` and
clears `mounted`. Without a handle it sends nothing (it is itself unmounted). The runner's handler
locks `AppState`, calls `remove_children(parent)`, then for `Some(e)` `add_node(Some(parent), 0,
ElementSource::None)` and `set_element_and_init(child, e, ctx.with_handle(Some(child)))`, and
records `UIChange::Replaced { parent }`. `show_content` renders `app_state.children(handle)`'s first
child with take-show-put; `render_web` renders `app_state.get_element(child)?.render_web(app_state)`
in place of `value_to_html` for an element value.

### Changes

```rust
// ui/message.rs
pub enum AppMessage { /* … */ ReplaceChildren { parent: UIHandle, element: Option<Box<dyn UIElement>> } }
```

`AppMessage` derives `Debug, Clone`; `Box<dyn UIElement>` is both (`impl Clone for Box<dyn UIElement>`,
`UIElement: Debug`). Runner: private `async fn handle_replace_children(&mut self, parent, element,
app_state)`; a missing parent is ignored (`node_exists` false). Errors from `add_node` are logged
with `eprintln!` like other runner failures. Sync/async: message handling is async in the runner;
the console stays sync. Commands: none. Documents: `UI_INTERFACE_FSD.md` "Elements inside a
console" (History, `reviewed:`).

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Remount loop on every refresh | `Arc::ptr_eq` identity (AC-4) | high |
| An element value shared by two consoles | `clone_boxed` per mount; each gets its own handle | high |
| Existing tests reading `ui_element` | field is private; console unit tests updated | high |
| Web markup of nested ids | child renders its own `ui-element-{id}` wrapper, as children of `UISpecElement` do | medium |

Recovery: revert the three files.

## Phase 3: Examples and Tests

### Examples

Console A with query `hello/q/ns-lui/query_console` (validated), nested console B submitting
`hello`; the runner harness with `register_lui_commands!` that the console integration suite
already builds.

### Tests

New cases in `liquers-lib/tests/query_console_integration.rs`, `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]`:

- `nested01_element_result_is_mounted_and_initialised` — AC-1
- `nested02_nested_console_receives_its_snapshot` — B's `get_value()` is `hello`'s value after it submits: AC-2
- `nested03_new_value_replaces_child` — element, then text value: child removed: AC-3
- `nested04_same_value_keeps_child` — deliver the same snapshot twice: one `Inserted`, child handle unchanged: AC-4
- `nested05_removing_console_removes_child` — `node_exists(child)` false, `has_monitoring()` false: AC-5

Unit, `query_console_element.rs`: `update_with_element_sends_replace_children` (AC-1),
`update_with_same_arc_sends_nothing` (AC-4). SSR, `tests/webui_ssr.rs`:
`console_renders_mounted_child_markup` (AC-6). egui rendering of the child is covered by the
take-show-put helper already used for `UISpecElement`; `nested01` asserts the child is
extractable (AC-6).

Run: `cargo test -p liquers-lib --test query_console_integration && cargo test -p liquers-lib --lib query_console_element`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `ui/message.rs` `AppMessage::ReplaceChildren`; `ui/runner.rs` `process_messages` arm and `handle_replace_children` — `cargo test -p liquers-lib --test query_console_integration nested01`. Rollback: revert both.
- [ ] 2. `ui/widgets/query_console_element.rs` — `mounted`, `update`, `show_content`, `render_web`; remove `ui_element`; unit tests — `cargo test -p liquers-lib --lib query_console_element` and `nested02`-`nested05`.
- [ ] 3. `tests/webui_ssr.rs` `console_renders_mounted_child_markup` — `cargo test -p liquers-lib --no-default-features --features webui --test webui_ssr`.
- [ ] 4. `specs/reference/UI_INTERFACE_FSD.md` (History, `reviewed:`); close `UI-ELEMENT-NESTED-IN-CONSOLE-HAS-NO-HANDLE` — `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh`; after
`cargo clean`, the `ui_query_console_app` Playwright suite (a nested console case is added to
`examples-web/tests/ui_query_console_app.spec.ts` in Step 3 as a designed, browser-only check).
