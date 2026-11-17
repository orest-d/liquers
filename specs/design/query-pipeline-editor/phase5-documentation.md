# Phase 5: Documentation - query-pipeline-editor

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical

## Implementation Summary

Built as approved in Phases 2 to 4, all 12 steps ticked.

- **Core** (`liquers-core/src/pipeline/`): `ActionEditor::{set_source, reset, insert_item,
  remove_item, move_item, excess}`; lenient query opening that keeps excess parameters; safe
  namespace edits (`namespace_impact`, `set_namespaces`, `NamespaceImpact`, `CommandChange`);
  `ParameterPreset` accessors and `ActionElement::from_action_request`.
- **Editor core** (`liquers-lib/src/ui/editor/`): `EditorState` applies every `EditorCommand` as a
  validated edit and builds the `EditorTree`. `EditorElement` is the `UIElement`, with kinds
  `Query`, `Action`, `Resource` and `Key`, bound by `EditorBinding::{Accessor, Memory}`.
- **Runner** (`ui/runner.rs`, `ui/message.rs`): `AppMessage::Accessor` / `ListDirectory`, then
  `UpdateMessage::Accessor` / `Directory`. It watches one accessor per handle, writes with
  `set_if`, turns a conflict into a reload, and keeps at most one write in flight.
- **Renderers**: `egui::editor::show_editor` (drag and drop for `ns` entries and list items) and
  `ui::web::editor::editor_html`, with the browser `change` event (`UIEventKind::Change`).
- **Commands**: `lui/query_editor`, `action_editor`, `resource_editor` and `key_editor`. The query
  console gained a Parameters panel.
- **Example** (added at Orest's request): `liquers-lib/examples/ui_spec_editor.rs`, a UISpec app whose init queries open a query editor and an action editor of a recipe and a console of its result; `--check` runs it without a window and shows an edit reaching the console.

Deviations from the approved design, none of which changes a contract:

- **The `scale_numeric` signature.** It is `scale_numeric(argument, value, max)`; Phase 2 had it
  take a fraction. The controller's range now travels with its value, which is what AC-13
  describes.
- **The new `AccessorOp::Adopt`.** An accessor made in code has no address. The `lui` command
  hands it to the runner as it is, for the session, and the element saves it as `Memory`. This is
  how Phase 2 says such an accessor works "for the session".
- **The Parameters panel.** It runs an `EditorState` of kind `Query` over the console text and
  shows only the last action node. Phase 2 had kind `Action`. Showing only the last node gives the
  same panel, with no separate one-action copy to keep in step with the text.
- **`ActionEditor::unset`.** Unsetting a middle argument by name now says why it is refused, and
  points to `reset`.

## Documentation Delivered

### New Reference Documents
None. The editors are a section of `UI_INTERFACE_FSD.md`.

### New Guide Documents
None.

### Existing Documents Reviewed or Updated
Each document got a History row and a `reviewed: 2026-11-17` bump.

- `specs/reference/UI_INTERFACE_FSD.md`:
  - a new section, §Editors: the kinds, the binding, the protocol, the edits and the renderers;
  - UI events gained `Change(field)` and `data-lq-event="change:<field>"`;
  - element-owned values now link to §Editors.
- `specs/reference/PIPELINES.md` §8: the form edits, lenient opening and namespace edits.
- `specs/guides/VALUE_ACCESSOR_GUIDE.md` §4: a new subsection, "Binding an editor".
- `specs/guides/COMMAND_REGISTRATION_GUIDE.md` §5: what the editors show from a command's metadata.

### Module Docs
`check_api_docs.py` reports 0 errors and 0 warnings for each of these:

- New: `ui/editor/mod.rs`, `egui/editor.rs` and `ui/web/editor.rs`.
- Updated: `liquers-core/src/pipeline/editor.rs`, and `ui/runner.rs`.
- Brought up to the guide (none had a module doc before): `ui/message.rs`, `ui/commands.rs` and
  `ui/widgets/query_console_element.rs`.

### User-Guide Chapters
`docs_index.py chapters-for` lists the chapters numbered 03, 04, 05, 07 and 11, plus appendices a
and c, in each of the three guides. I reviewed them: they are still true, because every change is
additive and `unset` behaves as the chapters show. No chapter describes the editors yet; that gap
is filed as `USER-GUIDES-DO-NOT-DESCRIBE-THE-EDITORS`.

### Links and Capability Map
`specs/README.md`: the entry reads "complete".

## Issues Filed

- `EDITOR-CANNOT-EDIT-INSIDE-A-NESTED-PIPELINE`: nested pipelines are shown but cannot be edited.
  This is a known limit of Phase 2.
- `USER-GUIDES-DO-NOT-DESCRIBE-THE-EDITORS`: the gap in the user-guide chapters above.
- `QUERY-CONSOLE-HAS-NO-QUERY-EDITOR-ON-DEMAND`: the console shows only the last action's form, not the whole query editor (Orest's request).

Closed:

- `NO-COMMAND-PARAMETER-EDITOR`.
- `UI-VARIADIC-ARGUMENT-LIST-EDITOR`.
- `EDITOR-UNSET-OF-A-MIDDLE-POSITIONAL-ARGUMENT-SAYS-NOT-SET`, now resolved by `reset`.

Progress noted: `UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS` has its Change kind; the other
kinds remain.

## Important Learning

- **The `ns` directive is not an element.** The conversion merges it into the actions of its run.
  Node paths therefore count the resource as element 0 and then the actions, and an `ns` node
  shares the path of the first action in its run.
- **Read names change on encoding.** The header dropdown offers the short names (`dir`), but the
  query encodes the canonical ones (`-R-directory/…`). `Read::from_instruction` accepts both.
- **Clone before writing back.** An element is edited outside the app state by taking it, cloning
  the downcast editor and putting the clone back. `UIElement` has no `as_any_mut`, and a clone
  avoids an unsafe cast.
- **Headless egui runs need cleanup.** A headless egui 0.36 run must use `Context::run_ui` and
  clear `textures_delta`, or the test panics when the output is dropped.

## Conformance and Remaining Work

AC-1 to AC-26 are implemented and tested:

- 9 core tests;
- 26 editor tests;
- 1 egui test and 2 browser tests;
- 4 integration tests;
- 1 query console test.

Out of scope, as Phase 1 recorded:

- browser drag and drop, which waits on `UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS`;
- definition mode, which is `WEB-PIPELINE-EDITOR-HAS-NO-DEFINITION-MODE`;
- MIDI input.

Remaining: editing inside nested pipelines (filed above).

## Validation

These all pass:

- `cargo test -p liquers-core --lib`;
- `cargo test -p liquers-lib --lib --tests`, which includes `registry_export` and
  `ui_editor_binding`;
- `cargo test -p liquers-lib --lib --features webui -- ui::web`;
- `cargo test -p liquers-lib --no-default-features --features webui --lib -- ui::web::editor`.

`bash scripts/check-build-matrix.sh`: every native row passes; the 14 wasm32 rows pass after
`rustup target add wasm32-unknown-unknown`. `cargo test -p liquers-lib --doc -- ui:: egui::editor`
passes (the module examples). `check_api_docs.py` reports 0 errors on the touched modules;
`validate_phase.py query-pipeline-editor 5` and `docs_index.py --check` pass.
