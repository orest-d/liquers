# Phase 4: Implementation Plan — Query and pipeline editor

## Overview

Bottom up: the `liquers-core` edits first (each proven by its own tests, usable without any UI),
then the backend-free editor core in `liquers-lib`, then the element and the runner, then the two
renderers, then the commands. Each layer is tested before the next one uses it. Prerequisite
`recipe-dependency-versions` is merged (G8). Steps 9 and 10 are independent of each other.

## Progress

- [ ] Step 1: the query editor keeps excess parameters
- [ ] Step 2: argument edits on `ActionEditor`
- [ ] Step 3: safe namespace edits
- [ ] Step 4: presets and conversions
- [ ] Step 5: registry in `UIContext`, `Change` events, `type_name`
- [ ] Step 6: the editor core and its tree
- [ ] Step 7: the editor element and its messages
- [ ] Step 8: the runner's accessor and listing requests
- [ ] Step 9: egui rendering
- [ ] Step 10: browser rendering
- [ ] Step 11: `lui` commands, the console's Parameters panel, the registry export
- [ ] Step 12: integration tests and the build matrix

## Implementation Steps

### Step 1: the query editor keeps excess parameters
- Files / symbols: `liquers-core/src/pipeline/convert.rs` `encode_arguments`, `write_transform`;
  `pipeline/editor.rs` `ActionEditor::excess`.
- Change: `encode_arguments` takes `lenient: bool`; in lenient mode positional parameters beyond
  the slots are appended as written instead of `too_many_parameters`. Strict callers pass `false`.
  `excess(&self) -> Result<Vec<Source>, Error>` returns them.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib pipeline` — `excess_parameters_are_kept_and_listed`.
- Rollback: revert the two files; nothing else calls the new flag.

### Step 2: argument edits on `ActionEditor`
- Files / symbols: `pipeline/editor.rs` `ActionEditor::{set_source, reset, insert_item,
  remove_item, move_item}`.
- Change: Phase 2 signatures. `reset` pops the argument when it is the last positional or named
  one, otherwise writes `Source::from_parameter_value(&default)`; an earlier argument with no
  default is `missing_argument`. Item edits rewrite the variadic run as one `Source` list.
- Depends on: Step 4's `Source::from_parameter_value` (do Step 4's conversion first).
- Proof: `reset_removes_last_and_writes_default_in_middle`, `variadic_items_insert_remove_move`,
  `set_source_switches_between_value_and_link`.
- Rollback: additive methods.

### Step 3: safe namespace edits
- Files / symbols: `command_metadata.rs` `CommandMetadataRegistry::namespaces`;
  `pipeline/editor.rs` `NamespaceImpact`, `CommandChange`, `PipelineEditor::{namespace_run,
  available_namespaces, unused_namespaces, namespace_impact, set_namespaces}` and their
  `QueryEditor` forwards; re-export from `pipeline/mod.rs`.
- Change: `namespace_impact` applies the list to a copy of the run, resolves each action
  (`lookup`) and checks its arguments (`check_argument_names`, `check_action`), and compares keys.
- Depends on: none.
- Proof: `namespace_impact_same_changed_invalid`, `set_namespaces_needs_acceptance_for_changed`,
  `available_and_unused_namespaces`.
- Rollback: additive.

### Step 4: presets and conversions
- Files / symbols: `command_metadata.rs` `ParameterPreset::{new, name, value, description}`,
  `ArgumentInfo::with_preset`; `pipeline/mod.rs` `Source::from_parameter_value`;
  `pipeline/convert.rs` `ActionElement::from_action_request` (factored from `Pipeline::try_from`).
- Depends on: none.
- Proof: `parameter_preset_accessors`, `action_from_preset_request`; the existing pipeline tests.
- Rollback: additive; `try_from` keeps its behaviour.

### Step 5: registry in `UIContext`, `Change` events, `type_name`
- Files / symbols: `liquers-lib/src/ui/ui_context.rs` `with_command_registry`,
  `command_registry`; `ui/runner.rs` `AppRunner` keeps one `Arc` snapshot and passes it to every
  `UIContext` it builds; `ui/event.rs` `UIEventKind::Change(String)` (`from_name("change:<f>")`);
  `ui/web/app.rs` driver reads `checked` and `<select>`; `utils.rs` `type_name` (moved).
- Depends on: none.
- Proof: `cargo test -p liquers-lib --lib ui::event ui::ui_context`; `cargo test -p liquers-lib
  --test search_commands`.
- Rollback: additive; the old `type_name` path re-exports.

### Step 6: the editor core and its tree
- Files / symbols: new `liquers-lib/src/ui/editor/{mod,tree,command,state,catalog}.rs`:
  `EditorKind`, `EditorTree`, `EditorNode`, `EditorField`, `FieldValue`, `NodePath`,
  `EditorCommand`, `EditorEffect`, `EditorState`, `command_catalog`, `scale_numeric`,
  `effective_gui`.
- Change: Phase 2 interfaces; matches over `ElementKind`, `Source`, `ArgumentGUIInfo` explicit.
- Depends on: Steps 1 to 4.
- Proof: `cargo test -p liquers-lib --lib ui::editor` — the 26 lib core tests of Phase 3.
- Rollback: the module is new; unlink it in `ui/mod.rs`.

### Step 7: the editor element and its messages
- Files / symbols: `ui/editor/element.rs` `EditorElement`, `EditorBinding`, `EditorView`;
  `ui/message.rs` `AppMessage::{Accessor, ListDirectory}`, `AccessorOp`; `ui/element.rs`
  `UpdateMessage::{Accessor, Directory}`, `AccessorUpdate`, `DirectoryListing`; every exhaustive
  match over them.
- Depends on: Step 6.
- Proof: `element_round_trips_and_old_console_state_loads`, `second_editor_conflict_reloads`.
- Rollback: remove the variants and the element.

### Step 8: the runner's accessor and listing requests
- Files / symbols: `ui/runner.rs` `accessors: HashMap<UIHandle, WatchedAccessor>`,
  `handle_accessor`, `handle_list_directory`, `poll_accessors` (Phase 5 of `run`); new awaits boxed
  (`Box::pin`) for the wasm stack.
- Depends on: Step 7.
- Proof: `cargo test -p liquers-lib --test ui_editor_binding`.
- Rollback: the requests are ignored when the map is absent.

### Step 9: egui rendering (parallel with Step 10)
- Files / symbols: new `liquers-lib/src/egui/editor.rs` `show_editor`; `EditorElement::show_in_egui`.
- Depends on: Step 7.
- Proof: `cargo test -p liquers-lib --lib egui::editor`.

### Step 10: browser rendering (parallel with Step 9)
- Files / symbols: new `liquers-lib/src/ui/web/editor.rs` `editor_html`;
  `EditorView::from_ui_event`; `EditorElement::render_web`.
- Depends on: Steps 5, 7.
- Proof: `cargo test -p liquers-lib --no-default-features --features webui --lib ui::web::editor`.

### Step 11: `lui` commands, the console's Parameters panel, the registry export
- Files / symbols: `ui/commands.rs` `query_editor`, `action_editor`, `resource_editor`,
  `key_editor`; `ui/widgets/query_console_element.rs` `show_parameters`;
  `specs/command_registry.yaml`.
- Proof: `cargo test -p liquers-lib --test registry_export --test query_console_integration`.
- Rollback: unregister the commands; regenerate the export.

### Step 12: integration tests and the build matrix
- Files / symbols: `liquers-lib/tests/ui_editor_binding.rs`.
- Proof: the default loop (`cargo test -p liquers-lib --lib --tests`, `cargo test -p liquers-core`)
  and `bash scripts/check-build-matrix.sh`.

## Testing Plan

Each step runs only its own tests while it is written. After Step 8, `cargo test -p liquers-core`
and `cargo test -p liquers-lib --lib --tests` once; after Step 12 the build matrix once (new
`#[cfg(feature = "egui")]` and `"webui"` modules, new `UpdateMessage` variants).

## Rollback Plan

Every step is additive. If only the core steps land, they stand on their own (documented in
`PIPELINES.md`); the element stays unregistered. A partly landed UI is unlinked from `ui/mod.rs`
and `register_lui_commands!`, and the remainder is filed as an issue (§5.6).

## Documentation Updates

`UI_INTERFACE_FSD.md`, `PIPELINES.md`, `VALUE_ACCESSOR_GUIDE.md`, `COMMAND_REGISTRATION_GUIDE.md`
(History rows, `reviewed:` bumped); module docs of `ui/editor/mod.rs`, `egui/editor.rs`,
`ui/web/editor.rs`, `pipeline/editor.rs`, `pipeline/mod.rs`, `ui/mod.rs`, checked with
`python3 scripts/check_api_docs.py`; `specs/command_registry.yaml` regenerated with a CHANGELOG
line; `specs/index.csv` regenerated; `NO-COMMAND-PARAMETER-EDITOR` and
`EDITOR-UNSET-OF-A-MIDDLE-POSITIONAL-ARGUMENT-SAYS-NOT-SET` closed.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
