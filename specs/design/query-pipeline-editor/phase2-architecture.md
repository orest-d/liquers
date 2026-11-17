# Phase 2: Solution & Architecture — Query and pipeline editor

## Overview

The editor is a **view and control** connected to its **model** by a value accessor. One
`UIElement`, `EditorElement`, holds a working copy of the query or pipeline in a `QueryEditor` or
`PipelineEditor` (`liquers-core/src/pipeline/editor.rs`), which also give it everything the
widgets need from the command metadata registry (`command_metadata`, `argument_info`,
`argument_source`, `unset_arguments`, `resolved_namespaces`). Every user action becomes an
`EditorCommand`, applied as a validated edit of that copy; the result is set into the accessor at
once or when the user presses Update (AC-26). Changes made elsewhere arrive as accessor events and
replace the copy. The element is monolithic (egui, and the browser in the wasm build); queries and
pipelines travel easily, so a later split design can run the same element in a client with a copy
of the registry and an accessor channel.

Inside the element, a backend-free core (`EditorState`, in `liquers-lib/src/ui/editor/`) applies
commands and builds an `EditorTree` that both renderers draw, so egui and the browser share every
rule and the rules are tested without a UI. Its `EditorKind` (`Query`, `Action`, `Resource`, `Key`)
picks what it shows; four `lui` commands create it.

Rejected:
- *Edit the query text by positions* (the proof of concept's `ExtendedQuery`): wrong once defaults,
  variadics, links or named arguments appear.
- *A model/view split of the editor itself* (the previous draft): Orest, 2026-11-17. The accessor
  is the model boundary; splitting the editor too adds a protocol and gains nothing while the UI is
  monolithic.
- *One element type per editor*: the four share the core, the tree and every widget.
- *The element calls the accessor itself*: `show_in_egui` and `update` are synchronous and elements
  spawn nothing (`ui-asset-snapshot-boundary`); the runner owns asynchronous work.
- *Directory listing through a `-R-dir/…` query*: the runner monitors one asset per handle; a typed
  request is simpler.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED` (design `recipe-dependency-versions`) | draft | P2 | AC-26 with a recipe binding: editing `recipes.yaml` does not expire what the recipe computed, so a result view does not recalculate | for that case | no | dependency recorded in Phase 1; G8 |
| `EDITOR-UNSET-OF-A-MIDDLE-POSITIONAL-ARGUMENT-SAYS-NOT-SET` | draft | P3 | AC-18 | no | no | fixed by `ActionEditor::reset` (Step 1) |
| `CORE-QUERY-EDITOR-ACCEPTS-AN-UNKNOWN-COMMAND` | draft | P3 | AC-17, AC-23 | no | no | designed around; stays open |
| `UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS` | draft | — | browser change events and drag and drop | no | no | `Change` added here (G2); drag stays there, the browser gets buttons |
| `UI-VARIADIC-ARGUMENT-LIST-EDITOR` (design `variadic-argument-slots`) | draft | P3 | list editing (AC-2) | no | no | item edits on `ActionEditor` here; noted on that design |
| `WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS` | draft | P2 | a slow `set_if` blocks the runner loop | no | no | accepted risk R1 |
| `VALUE-ACCESSOR-CANNOT-LOCK` | draft | P3 | two editors | no | no | conflicts as AC-9 |

Found by this phase (triage case 1, belongs here): the query editor's conversion back to query
text (`Pipeline::to_query_lenient`, `pipeline/convert.rs`, used by `QueryEditor::from_query` and by
every edit in query mode) refuses an action with more parameters than its command takes, so such a
query cannot be opened (AC-23). Step 1 makes that conversion keep the extra parameters (G6).

**Accessor alignment (AC-26), verified in `liquers-core/src/accessor/hub.rs` and `root.rs`.** A
`set_if` writes the root (`AccessorRoot::write`), then classifies every live projection of it: a
holder whose part changed gets `AccessorEvent::Changed`, one whose slot moved gets `Invalidated`,
and the writer gets the new snapshot and remembers its fingerprint. A keyed-asset root writes with
`AssetManager::set_state`, a store root with `set_binary`, so dependents of that key expire and
their views re-evaluate. So both modes work through the accessor alone: the editor decides *when* to
set, and the change event and expiry follow from the set. Two limits: a memory binding has no
dependents (the element's `follow` handle re-points a view instead), and the recipe case above.

Command namespaces involved: `lui` (four new commands), `acc` (used, unchanged).

## Interfaces

### liquers-core (sync, like every edit)

`ActionEditor<'a>` is already the control of one action inside a pipeline, with no GUI: it borrows
the `PipelineEditor` and an element index (`pipeline/editor.rs`). The new edits go beside it:

```rust
impl ActionEditor<'_> {
    pub fn set_source(&mut self, arg: impl Into<ArgRef>, source: Source) -> Result<&mut Self, Error>;
    /// AC-18: removes the argument when it is the last one written, otherwise writes its default in
    /// its place; `missing_argument` when an earlier omitted argument has no default.
    pub fn reset(&mut self, arg: impl Into<ArgRef>) -> Result<&mut Self, Error>;
    /// Variadic items (AC-2); `at` counts within the argument.
    pub fn insert_item(&mut self, arg: impl Into<ArgRef>, at: usize, source: Source) -> Result<&mut Self, Error>;
    pub fn remove_item(&mut self, arg: impl Into<ArgRef>, at: usize) -> Result<&mut Self, Error>;
    pub fn move_item(&mut self, arg: impl Into<ArgRef>, from: usize, to: usize) -> Result<&mut Self, Error>;
    /// AC-23: parameters beyond what the command's arguments take.
    pub fn excess(&self) -> Result<Vec<Source>, Error>;
}

/// AC-25: what an `ns` edit would do to the actions it covers.
#[derive(Debug, Clone, PartialEq)]
pub enum NamespaceImpact {
    Same,
    Changed(Vec<CommandChange>),     // still valid, but these actions resolve elsewhere
    Invalid(Error),                  // an action no longer resolves, or its arguments no longer fit
}
#[derive(Debug, Clone, PartialEq)]
pub struct CommandChange { pub at: usize, pub before: CommandKey, pub after: CommandKey }

impl PipelineEditor {
    /// The actions an `ns` at action `at` covers: `at` and the following actions with the same
    /// `ns` (in a query, up to the next `ns`, a `q` or the end of the segment).
    pub fn namespace_run(&self, at: usize) -> Result<std::ops::Range<usize>, Error>;
    /// Every namespace with commands in the pipeline's realm, and those not in the run's list.
    pub fn available_namespaces(&self, at: usize) -> Result<Vec<String>, Error>;
    pub fn unused_namespaces(&self, at: usize) -> Result<Vec<String>, Error>;
    pub fn namespace_impact(&self, at: usize, namespaces: &[String]) -> Result<NamespaceImpact, Error>;
    /// Applies an `ns` edit to the run. `Same` applies; `Changed` applies only with
    /// `accept_changes`, otherwise it returns the impact and changes nothing; `Invalid` is refused.
    pub fn set_namespaces(&mut self, at: usize, namespaces: Vec<String>, accept_changes: bool)
        -> Result<NamespaceImpact, Error>;
}
```

`QueryEditor` forwards the same five namespace methods (it already forwards the other edits).
`CommandMetadataRegistry::namespaces(&self, realm: &str) -> Vec<String>` lists the namespaces that
have commands. `ActionElement::from_action_request(&ActionRequest, Option<Namespaces>)` is factored
out of the `Pipeline::try_from(&Query)` loop (for `CommandPreset::action`), and
`Source::from_parameter_value(&CommandParameterValue) -> Option<Source>` converts a default or a
preset value. `ParameterPreset` gains `new(name, value, description)`, `name()`, `value()`,
`description()`, and `ArgumentInfo::with_preset(self, ParameterPreset) -> Self` (AC-24); its
serialized form is unchanged. `to_query_lenient` keeps excess parameters (G6); the strict
`to_query_with` still refuses them.

### liquers-lib: the element and its core (`src/ui/editor/`, no feature gate)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorElement {
    handle: Option<UIHandle>, title: String,
    kind: EditorKind,
    binding: EditorBinding,
    /// AC-26: set into the accessor after each committed edit; otherwise on Update.
    #[serde(default = "yes")] continuous: bool,
    /// AC-13: element re-pointed at the edited query after each update (for a memory binding).
    follow: Option<UIHandle>,
    #[serde(skip)] state: Option<EditorState>,  // built in `init` from the context's registry
    #[serde(skip)] view: EditorView,            // open nodes, text being typed, drag state
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EditorBinding {
    Accessor(String),    // the accessor's query, e.g. `…/ns-acc/accessor/recipe-0/query`
    Memory(String),      // the current query text or pipeline JSON; no accessor outside the session
}

/// The backend-free core. Debug, Clone; not serde (rebuilt from the binding).
pub struct EditorState { /* kind, root: Edited (QueryEditor | PipelineEditor), registry:
    Arc<CommandMetadataRegistry>, fingerprint, pending: bool, write_in_flight: bool,
    tree: EditorTree (rebuilt by every method below), picker filter, open folder,
    awaiting_confirmation: Option<(EditorCommand, Vec<CommandChange>)> */ }
impl EditorState {
    pub fn new(kind: EditorKind, registry: Arc<CommandMetadataRegistry>) -> Self;
    pub fn on_snapshot(&mut self, value: &Value, fingerprint: Fingerprint) -> Result<(), Error>;
    pub fn on_conflict(&mut self, live: &Value, fingerprint: Fingerprint); // AC-9: reload + message
    pub fn on_directory(&mut self, listing: DirectoryListing);
    /// A refused edit returns the error and changes nothing (AC-5).
    pub fn apply(&mut self, command: EditorCommand, continuous: bool) -> Result<EditorEffect, Error>;
    pub fn tree(&self) -> &EditorTree;
    pub fn value(&self) -> Value;                 // the working copy, for a write or a Memory binding
}

#[derive(Debug, Clone)]
pub enum EditorEffect { None, Write { value: Value, expected: Option<Fingerprint> }, ListDirectory(Key) }
```

`EditorTree` (Debug, Clone, PartialEq) is what the renderers draw: the query `text` and its
highlighting tokens (from `StyledQueryToken`), a `text_note` (parse error, or why a pipeline's
text is read-only) and `stale` flag (AC-4, AC-10); `pending` (AC-26); the `nodes`; the filename
and `filename_default` (`None` hides Default, AC-22); the command `catalog` while the picker is
open (AC-17); the `directory` while a file selector is open (AC-14); a `message` (refusal,
conflict, or a namespace change awaiting confirmation, AC-25). A node is one of:

| Node | Shows | From |
|---|---|---|
| `Resource { header, key }` | header dropdown before the key, file selector (AC-7, AC-14) | `ElementKind::Resource`, `KeyValue`, `Cwd` |
| `Namespaces { list, available, unused }` | ordered list: drag to reorder, delete, append from `unused` (AC-16, AC-25) | the first action of each `namespace_run` |
| `Action { command, label, doc, presets, next, filename, fields, excess }` | header with hover doc, presets and next menus, filename button, field grid (AC-2, AC-3, AC-20 to AC-23) | `ElementKind::Action` |
| `Quote`, `Volatile` | removable directive (AC-16) | `Quote`, `Volatile` |
| `Value { json }` | read-only value | `ElementKind::Value` |
| `Nested { label, nodes }` | a subtree | `Evaluate`, `QueryValue`, a nested realm |
| `Opaque { text, reason }` | read-only, with the reason | query `Text`, placeholders, unknown commands |

A field carries its name, label, description, type and default text (hover, AC-3), the
**effective widget** (`gui_info`, unless it is `DEFAULT_GUI` or `None` and the type is bool,
integer, float or enum), enum alternatives with their aliases, its value (`Unset`, a literal, a
`Link` or `Pipeline` with its own nodes, or `Items` for a variadic argument), its presets
(AC-24), its index among the numeric arguments (AC-13), and whether its link is `-R-key/…` (AC-15).
Every `ElementKind` and `Source` variant maps to exactly one row above, matched exhaustively.

`EditorCommand` (Debug, Clone, PartialEq) names one intention and the node path it acts on:
`SetText`, `SetField`, `SetLink`, `ToLink` / `ToValue` (AC-19), `Reset` (AC-18), `InsertItem`,
`RemoveItem`, `MoveItem` (AC-2), `Insert`, `Append`, `Remove`, `Move` (AC-6), `SetNamespaces`
and `ConfirmNamespaces` / `CancelNamespaces` (AC-25), `ApplyPreset`, `ApplyNext` (AC-20, AC-21,
AC-24), `SetHeader`, `SetKey`, `Browse`, `CloseBrowser` (AC-7, AC-14), `SearchCommands`,
`ClosePicker` (AC-17), `SetFilename`, `FilenameFrom`, `DefaultFilename` (AC-22),
`SetNumeric { index, fraction }` (AC-13), `Update`, `Revert` and `SetContinuous` (AC-26). A path
walks elements and arguments; the core opens a nested `QueryEditor` on a link's text or a
`PipelineEditor` on a nested pipeline, applies the command there, and writes the level back with
`set_source` or `replace`.

**Writes (AC-26).** With `continuous` on, a committed edit returns `Write` with the working copy
and the last fingerprint. At most one write is in flight: an edit made meanwhile marks the copy
pending and is written when the first one returns, so a dragged slider produces a few writes, not
one per frame. With `continuous` off, edits only mark the copy pending; `Update` returns `Write`,
and `Revert` reloads from the accessor. A change event that arrives while the copy is pending is a
conflict for the next write (AC-9), never a silent overwrite.

**What each kind reads.** `Query`: a `Query` or `Pipeline` value. `Action`: the one-element
pipeline an `ActionStep` gives. `Resource`: a query or pipeline, of which it shows the first
resource. `Key`: a text value (a recipe field through `acc/field`, or memory). Any other type makes
the element read-only with the conversion error.

Helpers: `command_catalog(registry, realm, filter)` groups commands by namespace and filters by
name, label and doc (AC-17), reusing `type_name` (moved to `utils.rs` from
`search/command_catalog.rs`, which is behind `records`); `scale_numeric(argument, fraction) ->
Result<serde_json::Value, Error>` (AC-13); `effective_gui(argument) -> ArgumentGUIInfo`.

## Integration Points

- `ui/message.rs`: `AppMessage::Accessor { handle, source: EditorBinding, op: AccessorOp }` with
  `AccessorOp::{Watch, SetIf { value: Value, expected: Option<Fingerprint> }}`, and
  `AppMessage::ListDirectory { handle, key: Key }`.
- `ui/element.rs`: `UpdateMessage::Accessor(AccessorUpdate)` (`Snapshot`, `Conflict` with the live
  value, or `Failed(Error)`) and `UpdateMessage::Directory(DirectoryListing)`.
- `ui/runner.rs` (async; the only place that awaits): evaluates an `Accessor` binding and takes
  `try_into_accessor`, or makes a `memory_accessor` for `Memory`; keeps
  `accessors: HashMap<UIHandle, Watched>` beside `monitoring`, polls each `watch::Receiver` in the
  same loop (calling `refresh` where no watcher runs, on wasm) and delivers each `Changed` as a
  fresh `get`; `SetIf` calls `set_if` and delivers the snapshot, or on `Error::accessor_conflict` a
  fresh `get` as `Conflict`; `ListDirectory` uses the asset manager's `listdir_asset_info` (names and
  `is_dir`). Entries go when the element goes. An accessor with an empty `address()` (made in code)
  works for the session; the element stores it as `Memory` and says it will not re-bind after a
  reload.
- `ui/ui_context.rs`: `with_command_registry(Arc<CommandMetadataRegistry>)` and
  `command_registry() -> Option<&Arc<CommandMetadataRegistry>>`. `AppRunner::new`, `NativeDriver`
  and the web mount take one `command_metadata_snapshot()` (it clones the registry) and give the
  same `Arc` to every `UIContext` they build (`query-editor-live-validation` Step 3, with an `Arc`
  getter; whichever design lands first adds it).
- `ui/event.rs`, `ui/web/app.rs` (G2): `UIEventKind::Change(String)`, naming the changed field.
  `data-lq-event="change"` on a field makes the driver send it on `change` with that field's
  `data-lq-field`; `read_field_value` reads `checked` for a checkbox and the value of a `<select>`.
- `ui/widgets/query_console_element.rs`: a toggled Parameters panel (salvaged) holding an
  `EditorState` of kind `Action` over the console's text; an edit writes `query_text` and does not
  submit; `#[serde(default)] show_parameters: bool` (AC-12).
- `ui/commands.rs` (the four commands), `ui/mod.rs` (the module), `utils.rs` (`type_name`).
- egui (`src/egui/editor.rs`, feature `egui`): `pub fn show_editor(ui: &mut egui::Ui, tree:
  &EditorTree, view: &mut EditorView) -> Vec<EditorCommand>`. The text uses `edit_query`
  (`styled_query_to_layout_job` when read-only); nodes are `egui::CollapsingHeader`s with ids from
  their paths; hover is `on_hover_text`; fields are a two-column `egui::Grid` (the proof of
  concept's layout), widgets matched exhaustively over `ArgumentGUIInfo`; variadic items and the
  `ns` list reorder with `Ui::dnd_drag_source` / `Ui::dnd_drop_zone`; presets and next are
  `menu_button`s; the header is a `ComboBox` before the key; the file selector, the command picker
  and the AC-25 confirmation are popups; the "Update immediately" checkbox, and Update and Revert
  when it is off, sit in the editor's toolbar. Text fields commit on Enter or focus loss; sliders,
  checkboxes and selectors on change.
- Browser (`src/ui/web/editor.rs`, feature `webui`): `pub fn editor_html(tree: &EditorTree, dom_id:
  &str) -> String`. Nodes are `<details>`; hover is `title=`; fields use `field_attr("f/<path>")`;
  buttons use `event_attr(Named("<command>/<path>"))`, selectors and checkboxes `Change`; widgets
  follow the salvaged mapping (`<input list>` with `<datalist>`, `type=number|range|color|date`);
  lists get up, down and delete buttons instead of drag and drop; the text is an `<input>` with the
  highlighted query beside it. `EditorView::from_ui_event(&UIEvent) -> Vec<EditorCommand>` decodes
  events in `update` (`render_web` only draws).

## Error Handling

All errors are `liquers_core::error::Error`, shown in the tree's `message` at the node's path; none
is fatal to the element. A refused edit is the editor's own error (`invalid_argument`,
`invalid_token`, `missing_argument`), passed through; a path with no node is
`Error::invalid_pipeline("no node at …")`, as the editor's own out-of-range errors are; an invalid
`ns` edit is the `NamespaceImpact::Invalid` error; a conflict is `Error::accessor_conflict` and
reloads (AC-9); a binding that fails to evaluate, or whose value is not an accessor, makes the
editor read-only with that error; a failed listing keeps the selector open with the error and still
accepts a typed key (AC-14); no registry in the context is `Error::general_error("no command
registry")` and only the text is shown; `scale_numeric` on a non-numeric argument is
`invalid_argument`. No `unwrap`, no new `ErrorType`.

## Relevant Commands

New in `liquers-lib/src/ui/commands.rs`, namespace `lui` (after
`specs/guides/COMMAND_REGISTRATION_GUIDE.md`). The state is what to edit: an `Accessor` value (its
`address()` becomes the binding) or a query, pipeline or text (`Memory`). Each returns
`ExtValue::UIElement`, like `query_console`.

```rust
register_command!(cr, fn query_editor(state) -> result
    namespace: "lui" label: "Query editor"
    doc: "An editor of a query or pipeline: a tree of its steps beside the highlighted text"
    version: auto)?;
register_command!(cr, fn action_editor(state) -> result
    namespace: "lui" label: "Action editor"
    doc: "An editor of one action's arguments, from the command's argument metadata"
    version: auto)?;
register_command!(cr, fn resource_editor(state) -> result
    namespace: "lui" label: "Resource editor"
    doc: "An editor of a resource query: its header and its key, with a store file selector"
    version: auto)?;
register_command!(cr, fn key_editor(state) -> result
    namespace: "lui" label: "Key editor"
    doc: "An editor of a key, with a store file selector"
    version: auto)?;
```

`specs/command_registry.yaml` is regenerated (four commands) with a CHANGELOG line.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/UI_INTERFACE_FSD.md` | reference | new section "Query, action, resource and key editors": the element, kinds, binding, continuous and on-demand update, the new `AppMessage` / `UpdateMessage` variants, `UIEventKind::Change`, `UIContext::command_registry` |
| `specs/reference/PIPELINES.md` | reference | `ActionEditor::set_source`, `reset`, item edits, `excess`; safe namespace edits; the query editor keeps excess parameters |
| `specs/guides/VALUE_ACCESSOR_GUIDE.md` | guide | "Binding an editor": `…/ns-acc/accessor/recipe-0/query/ns-lui/query_editor`, and when a set recalculates a view |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | guide | one line: `label`, `doc`, `description`, `gui_info`, presets, `next` and `filename` are what the editors show |

Module docs (`API_DOCUMENTATION_GUIDE.md` §3, all five parts): new `liquers-lib/src/ui/editor/mod.rs`,
`src/egui/editor.rs`, `src/ui/web/editor.rs`; "Main components" and "Where it is used" change in
`liquers-core/src/pipeline/editor.rs`, `liquers-core/src/pipeline/mod.rs` and
`liquers-lib/src/ui/mod.rs`. Proposed `affects_docs`: the four documents above.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | core `pipeline/{editor,mod,convert}.rs`, `command_metadata.rs`; lib `ui/editor/` (new), `egui/editor.rs`, `ui/web/editor.rs`, `ui/{element,message,runner,ui_context,event,commands,mod,native}.rs`, `ui/web/app.rs`, `ui/widgets/query_console_element.rs`, `utils.rs`, `search/command_catalog.rs`; `specs/command_registry.yaml` |
| Crates and workflows affected | `liquers-core`, `liquers-lib` (features `egui`, `webui`, none); the build matrix; the browser test loop |
| Existing tests likely to change | `registry_export`; tests that build a `UIContext`; exhaustive matches over `AppMessage`, `UpdateMessage`, `UIEventKind`; pipeline tests expecting the query editor to refuse excess parameters |
| New validation | `EditorState` unit tests per command; core tests of `namespace_impact` (same, changed, invalid); egui rendering tests; web markup and event decoding; a runner test with a stored `recipes.yaml` and two editors (AC-9) |
| Compatibility / data / concurrency / performance / security | old app states load (AC-12); R1: `set_if` awaits inline in the runner like every query; R2: the tree is rebuilt on edits and snapshots only; R3: continuous mode keeps one write in flight; R4: a recipe edit does not recalculate its result until `recipe-dependency-versions`; a binding query runs with the UI's own rights |
| Recovery | each step is additive; the element can stay unregistered while the core edits remain |
| Certainty and open questions | high for the core edits and egui; medium for the browser change event and the namespace run rule in pipelines |

## Decisions for the gate

- **G1** `ns` is a node where the query has `ns-…`, edited as an ordered list over the run of
  actions it covers, with the safe-edit rule of AC-25 in `PipelineEditor` / `QueryEditor`.
- **G2** `UIEventKind::Change(String)` and its driver support are added here; with "Update
  immediately" (AC-26) a change is written at once, otherwise on Update.
- **G3** One element type with four kinds, created by four `lui` commands. *(Agreed.)*
- **G4** Two new runner requests (`Accessor`, `ListDirectory`). *(Agreed.)*
- **G5** The new edits go in `pipeline/editor.rs`: item, reset, source and excess edits on
  `ActionEditor`, namespace edits on `PipelineEditor` and `QueryEditor`.
- **G6** The query editor's conversion back to text keeps excess parameters instead of refusing.
- **G7** Namespaces `lui` and `acc`. *(Agreed.)*
- **G8** Make `recipe-dependency-versions` a prerequisite of this design's implementation, or ship
  without it and say a recipe edit refreshes the editors but not the recipe's result view.
