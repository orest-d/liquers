# Phase 2: Solution & Architecture — Query and pipeline editor

## Overview

The editor is split the way the `ui-split-elements` discussion proposes (its option C, a
model/view pair exchanging typed messages; branch `claude/project-thread-v0dh04`,
`element-protocol-discussion.md`, not yet code), so split mode is a wrapping rather than a rewrite:

- **Model** (`EditorModel`, server half): owns the binding, a stack of `QueryEditor` /
  `PipelineEditor`s along the edited path, and the registry. It turns an `EditorCommand` into a
  validated edit and keeps an `EditorTree`. Both steps are synchronous and pure; anything
  asynchronous (reading or writing the accessor, listing a directory) is returned as an
  `EditorEffect` that the runner carries out.
- **View** (`EditorView`, client half): renders an `EditorTree` and turns input into
  `EditorCommand`s. It never sees the registry: the tree carries labels, docs, widgets, presets and
  the command catalogue.
- **`EditorElement`** (fused `UIElement`, monolithic): holds both halves and passes the messages in
  process. One element type; its `EditorKind` (`Query`, `Action`, `Resource`, `Key`) picks what it
  shows. Four `lui` commands create it.

`EditorTree` (down) and `EditorCommand` (up) are serde types. When `ui-split-elements` defines its
model and view traits, `EditorModel` and `EditorView` implement them and nothing else changes.

Rejected:
- *Edit the query text by positions* (the proof of concept's `ExtendedQuery`): positions map
  parameters to arguments wrongly once defaults, variadics, links or named arguments appear.
- *One element type per editor*: the four share the model, the tree and every widget.
- *The element calls the accessor itself*: `show_in_egui` and `update` are synchronous and elements
  spawn nothing (`ui-asset-snapshot-boundary`); the runner owns asynchronous work.
- *Directory listing through a `-R-dir/…` query*: the runner monitors one asset per handle, and the
  editor already uses that slot's equivalent for its accessor; a typed request is simpler.
- *`ns` only as an action property*: a query's `QueryEditor` re-parses after each edit and gives
  every later action of the segment the `ns` in force (`pipeline/convert.rs` around lines 281-335),
  so a per-action edit would spill onto the following actions. AC-1 and AC-16 show `ns` as a node.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `EDITOR-UNSET-OF-A-MIDDLE-POSITIONAL-ARGUMENT-SAYS-NOT-SET` | draft | P3 | AC-18 resets middle arguments | no | no | fixed here by `ActionEditor::reset` (Step 1); closed in Phase 5 |
| `CORE-QUERY-EDITOR-ACCEPTS-AN-UNKNOWN-COMMAND` | draft | P3 | AC-17 inserts only catalogue commands; AC-23 shows unknown ones | no | no | designed around; stays open |
| `UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS` | draft | — | a browser checkbox, select or slider must commit without a button | no | no | `UIEventKind::Change` added here (G2); the other kinds stay in that issue |
| `UI-VARIADIC-ARGUMENT-LIST-EDITOR` (design `variadic-argument-slots`) | draft | P3 | list editing of AC-2 | no | no | item edits on `ActionEditor` here (G5); a note is on that design |
| `WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS` | draft | P2 | an accessor set to a slow store blocks the runner loop | no | no | accepted risk R1: no await that a query does not already have |
| `ACCESSOR-CANNOT-BE-SENT-TO-A-CLIENT` | draft | P3 | split mode | no | no | designed around: the accessor stays in the model |
| `VALUE-ACCESSOR-CANNOT-LOCK` | draft | P3 | two editors | no | no | conflicts as AC-9 (first writer wins) |

Found by this phase: lenient query encoding refuses excess parameters (`too_many_parameters` in
`pipeline/convert.rs` `encode_arguments`, reached from `QueryEditor::from_query` and every query
edit), so a query with one excess parameter could not be opened or edited (AC-23). It belongs to
this design (triage case 1): Step 1 changes lenient mode to write excess parameters through.

Command namespaces involved: `lui` (four new commands), `acc` (used, unchanged).

## Interfaces

### liquers-core (sync, like every edit)

`pipeline/editor.rs`, `impl ActionEditor<'_>`:

```rust
/// Sets an argument from any source (literal, link, nested pipeline), checked as `set`.
pub fn set_source(&mut self, arg: impl Into<ArgRef>, source: Source) -> Result<&mut Self, Error>;
/// AC-18: removes the argument when it is the last one written; otherwise writes its default in
/// its place. Refused (`missing_argument`) when an earlier omitted argument has no default.
pub fn reset(&mut self, arg: impl Into<ArgRef>) -> Result<&mut Self, Error>;
/// Variadic items (AC-2): `at` counts within the argument.
pub fn insert_item(&mut self, arg: impl Into<ArgRef>, at: usize, source: Source) -> Result<&mut Self, Error>;
pub fn remove_item(&mut self, arg: impl Into<ArgRef>, at: usize) -> Result<&mut Self, Error>;
pub fn move_item(&mut self, arg: impl Into<ArgRef>, from: usize, to: usize) -> Result<&mut Self, Error>;
/// AC-23: the parameters beyond what the command's arguments take.
pub fn excess(&self) -> Result<Vec<Source>, Error>;
```

`pipeline/mod.rs` and `convert.rs`:

```rust
impl ActionElement {
    /// The element a query action becomes: factored out of the `Pipeline::try_from(&Query)` loop,
    /// used for `CommandPreset::action` (AC-20, AC-21).
    pub fn from_action_request(action: &ActionRequest, ns: Option<Namespaces>) -> ActionElement;
}
impl Source {
    /// An argument default or a preset value as a source; `None` for `CommandParameterValue::None`.
    pub fn from_parameter_value(value: &CommandParameterValue) -> Option<Source>;
}
```

`to_query_lenient` writes excess positional parameters through as they stand instead of failing;
`to_query` (strict) still refuses them.

`command_metadata.rs`: `ParameterPreset::new(name: &str, value: CommandParameterValue,
description: &str) -> Self`, the getters `name()`, `value()`, `description()`, and
`ArgumentInfo::with_preset(self, preset: ParameterPreset) -> Self` (AC-24). Serialized form
unchanged.

### liquers-lib: model and messages (`src/ui/editor/`, no feature gate)

Every type here derives `Debug, Clone, PartialEq, Serialize, Deserialize` unless said otherwise.

```rust
pub struct NodePath(pub Vec<PathStep>);            // also Eq, Hash
pub enum PathStep { Element(usize), Argument(String), Item(usize) }
pub enum EditorKind { Query, Action, Resource, Key }               // also Copy, Eq
/// The resource header as the dropdown shows it, before the key; `Asset` is the empty entry (AC-7).
pub enum ResourceHeader { Asset, Key, Dir, Recipe, Cwd, Binary, Metadata, StoreDir, Stored, StoredMetadata }
pub struct StyledToken { pub class: TokenClass, pub text: String }  // from `StyledQueryToken`
pub enum TokenClass { Parameter, Entity, Separator, ResourceName, ActionName, Header, Highlight }

/// Down: everything a view draws.
pub struct EditorTree {
    pub kind: EditorKind,
    pub text: String,                     // the query; "" for an unrepresentable pipeline
    pub tokens: Vec<StyledToken>,         // highlighting
    pub text_note: Option<String>,        // parse error, or why the text is read-only (AC-10)
    pub stale: bool,                      // the text does not parse; nodes are the last valid ones
    pub nodes: Vec<EditorNode>,
    pub filename: Option<String>,
    pub filename_default: Option<String>, // the last command's `filename`; `None` hides Default
    pub catalog: Option<CommandCatalog>,  // while the command picker is open (AC-17)
    pub directory: Option<DirectoryListing>, // while a file selector is open (AC-14)
    pub message: Option<EditorMessage>,   // refusal or conflict
}
pub enum EditorNode {
    Resource { path: NodePath, header: ResourceHeader, key: String, note: Option<String> },
    Namespaces { path: NodePath, namespaces: Vec<String> },        // AC-16
    Action(ActionNode),
    Quote { path: NodePath }, Volatile { path: NodePath },         // AC-16
    Value { path: NodePath, json: serde_json::Value },
    /// `Evaluate`, `QueryValue`, and a nested realm (`ElementKind::Pipeline`).
    Nested { path: NodePath, label: String, nodes: Vec<EditorNode> },
    /// Shown read-only with a reason: query text not yet resolved, a placeholder (definition
    /// mode is a non-goal), an unknown command (AC-23).
    Opaque { path: NodePath, text: String, reason: String },
}
pub struct ActionNode {
    pub path: NodePath, pub command: CommandKey, pub label: String, pub doc: String,
    pub presets: Vec<PresetEntry>, pub next: Vec<PresetEntry>,     // AC-20, AC-21
    pub filename: Option<String>,                                  // AC-22
    pub fields: Vec<FieldNode>, pub excess: Vec<String>,           // AC-23
}
pub struct FieldNode {
    pub path: NodePath, pub name: String, pub label: String, pub description: String,
    pub type_name: String, pub default_text: String,              // hover (AC-3)
    pub gui: ArgumentGUIInfo,                                     // effective widget (AC-2)
    pub alternatives: Vec<EnumChoice>, pub others_allowed: bool,
    pub value: FieldValue, pub presets: Vec<PresetEntry>,          // AC-24
    pub numeric: Option<usize>,                                   // AC-13
    pub is_key: bool,                                             // a `-R-key/…` link (AC-15)
}
pub enum FieldValue {
    Unset,                                                        // the default applies
    Literal(serde_json::Value),
    Link { text: String, nodes: Vec<EditorNode> },                // AC-1, AC-19
    Pipeline { nodes: Vec<EditorNode> },                          // a `Source::Pipeline`
    Items(Vec<FieldValue>),                                       // variadic
    Opaque(String),                                               // placeholder, state
}
pub struct EnumChoice { pub name: String, pub alias: String }
pub struct PresetEntry { pub label: String, pub description: String }
pub struct CommandCatalog { pub filter: String, pub groups: Vec<NamespaceGroup> }
pub struct NamespaceGroup { pub namespace: String, pub commands: Vec<CatalogEntry> }
pub struct CatalogEntry { pub key: CommandKey, pub label: String, pub doc: String }
pub struct DirectoryListing { pub folder: String, pub entries: Vec<(String, bool)>, pub error: Option<String> }
pub struct EditorMessage { pub path: Option<NodePath>, pub text: String, pub conflict: bool }

/// Up: one user intention, naming the node it acts on.
pub enum EditorCommand {
    SetText(String),
    SetField { path: NodePath, value: serde_json::Value },
    SetLink { path: NodePath, query: String },
    ToLink { path: NodePath }, ToValue { path: NodePath },          // AC-19
    Reset { path: NodePath },                                       // AC-18
    InsertItem { path: NodePath, at: usize }, RemoveItem { path: NodePath },
    MoveItem { path: NodePath, to: usize },
    Insert { before: NodePath, node: NewNode }, Append(NewNode),     // AC-6
    Remove { path: NodePath }, Move { path: NodePath, up: bool },
    SetNamespaces { path: NodePath, namespaces: Vec<String> },      // AC-16
    ApplyPreset { path: NodePath, index: usize },                   // AC-20, AC-24
    ApplyNext { path: NodePath, index: usize },                     // AC-21
    SetHeader { path: NodePath, header: ResourceHeader },          // AC-7
    SetKey { path: NodePath, key: String },
    Browse { path: NodePath, folder: String }, CloseBrowser,        // AC-14
    SearchCommands(String), ClosePicker,                            // AC-17
    SetFilename(Option<String>), FilenameFrom { path: NodePath }, DefaultFilename, // AC-22
    SetNumeric { index: usize, fraction: f64 },                     // AC-13, 0.0..=1.0
}
pub enum NewNode { Action(CommandKey), Resource, Namespaces(Vec<String>), Quote, Volatile }

/// What the element is bound to; serialized with the element (AC-12).
pub enum EditorBinding {
    /// An accessor named by its query (`…/ns-acc/accessor/recipe-0/query`).
    Accessor(String),
    /// A value that lives only in the editor: the current query text or pipeline JSON.
    Memory(String),
}

/// What the runner must do after an edit. Not serde (it carries a `Value`).
#[derive(Debug, Clone)]
pub enum EditorEffect { None, Write { value: Value, expected: Option<Fingerprint> }, ListDirectory(Key) }

/// Not serde: rebuilt from the binding. Also Debug, Clone.
pub struct EditorModel { /* kind, binding, root: Edited (QueryEditor | PipelineEditor), registry:
    Arc<CommandMetadataRegistry>, fingerprint: Option<Fingerprint>, tree: EditorTree (cached),
    picker filter, open folder */ }
impl EditorModel {
    pub fn new(kind: EditorKind, binding: EditorBinding, registry: Arc<CommandMetadataRegistry>) -> Self;
    pub fn on_snapshot(&mut self, value: &Value, fingerprint: Fingerprint) -> Result<(), Error>;
    pub fn on_conflict(&mut self, live: &Value, fingerprint: Fingerprint); // AC-9: reload + message
    pub fn on_directory(&mut self, listing: DirectoryListing);
    /// A refused edit returns the error and leaves the model unchanged (AC-5).
    pub fn apply(&mut self, command: EditorCommand) -> Result<EditorEffect, Error>;
    pub fn tree(&self) -> &EditorTree;   // rebuilt inside `apply` and `on_*`, never in `tree`
    /// The binding to serialize: a `Memory` binding carries the current text (AC-12).
    pub fn binding(&self) -> EditorBinding;
}

/// Transient view state: open nodes and text being typed, per path. Default, Debug, Clone; not serde.
pub struct EditorView { /* open: HashSet<NodePath>, buffers: HashMap<NodePath, String> */ }

pub fn command_catalog(registry: &CommandMetadataRegistry, realm: &str, filter: &str) -> CommandCatalog;
/// AC-2: `gui_info`, unless it is `DEFAULT_GUI` or `None` and the type is bool, integer, float
/// or enum; an explicit `TextField(n)` is kept.
pub fn effective_gui(argument: &ArgumentInfo) -> ArgumentGUIInfo;
/// AC-13: a controller's 0..1 scaled to the argument (integer range or slider, float slider, or
/// 0..1); `invalid_argument` for a non-numeric argument.
pub fn scale_numeric(argument: &ArgumentInfo, fraction: f64) -> Result<serde_json::Value, Error>;
```

**Paths and nesting.** A path walks elements and arguments. The model opens an editor per level:
a link's text in a `QueryEditor`, a `Source::Pipeline` or a nested realm in a `PipelineEditor`, and
writes each level back to its parent (`set_source(…, Source::Link | Source::Pipeline)`, or
`replace(at, ElementKind::Pipeline(…))`). Every `ElementKind` and `Source` variant maps to a node
(the table above); placeholders and `State` are `Opaque`.

**Namespaces.** In query mode the tree shows an `ns` node where the query has `ns-…`. Editing it
sets `ns` on every action of the run it covers (up to the next `ns`, a `q`, or the segment's end);
inserting one before an action sets it on that action's run; removing it clears that run.
Re-encoding then puts the directive back at the same place. In a pipeline that is not
representable, `ns` is the action's own field, shown as a `Namespaces` node before the action.
Inserting a command outside the active namespaces inserts a `Namespaces` node with its namespace
(AC-17); `ApplyNext` does the same with the preset's `ns` hint, as `utils::find_next_presets` does.

**What each kind reads.** `Query`: a `Query` or `Pipeline` value (as `accessor/steps.rs` `Edited`).
`Action`: the one-element pipeline an `ActionStep` gives. `Resource`: a query or pipeline, of which
it shows the first resource element. `Key`: a text value (a recipe field through `acc/field`, or
memory). A value of another type puts the element in read-only mode with the conversion error.

### liquers-lib: element

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorElement {
    handle: Option<UIHandle>, title: String, kind: EditorKind, binding: EditorBinding,
    /// Element re-pointed at the edited query after each change (`RequestAssetUpdates`): the
    /// proof of concept's re-evaluation, for a query that lives only in the editor (AC-13).
    follow: Option<UIHandle>,
    #[serde(skip)] model: Option<EditorModel>,   // built in `init` from the context's registry
    #[serde(skip)] view: EditorView,
}
```

`init` builds the model and sends `AppMessage::Accessor { handle, source, op: Watch }`; `update`
handles `UpdateMessage::Accessor`, `Directory` and `UI` (the browser's commands, decoded with
`EditorView::from_ui_event`); `show_in_egui` draws `model.tree()` and applies the commands it
returns; `render_web` only draws. After each snapshot the element copies `model.binding()` into
`binding`, so saving the app state keeps a memory editor's text.

## Integration Points

- `ui/message.rs`: `AppMessage::Accessor { handle: UIHandle, source: EditorBinding, op: AccessorOp }`,
  `AccessorOp::{Watch, SetIf { value: Value, expected: Option<Fingerprint> }}`, and
  `AppMessage::ListDirectory { handle: UIHandle, key: Key }`.
- `ui/element.rs`: `UpdateMessage::Accessor(AccessorUpdate)` with
  `AccessorUpdate::{Snapshot { value: Arc<Value>, fingerprint }, Conflict { value, fingerprint },
  Failed(Error)}`, and `UpdateMessage::Directory(DirectoryListing)`.
- `ui/runner.rs` (async; the only place that awaits): evaluates an `Accessor` binding and takes
  `try_into_accessor`, or makes a `memory_accessor` from a `Memory` binding; keeps
  `accessors: HashMap<UIHandle, Watched>` (`Watched { accessor, events: watch::Receiver }`) beside
  `monitoring`, polled in the same loop and calling `refresh` where no watcher runs (wasm); `SetIf`
  calls `set_if` and delivers the snapshot, or on `Error::accessor_conflict` a fresh `get` as
  `Conflict`; `ListDirectory` uses the asset manager's `listdir_asset_info` (names and `is_dir`).
  Entries are dropped when the element goes, as monitoring is. An accessor whose `address()` is empty
  (made in code) is kept for the session but the element's binding becomes `Memory` with its value,
  and a note says it will not re-bind after a reload.
- `ui/ui_context.rs`: `with_command_registry(Arc<CommandMetadataRegistry>)` and
  `command_registry() -> Option<&Arc<CommandMetadataRegistry>>`. `AppRunner::new`, `NativeDriver`
  and the web mount take one `command_metadata_snapshot()` (it clones the registry) and pass the
  same `Arc` to every `UIContext` they build. This is `query-editor-live-validation` Step 3 with an
  `Arc` getter; whichever design lands first adds it.
- `ui/event.rs` and `ui/web/app.rs` (G2): `UIEventKind::Change(String)`, the changed field's name.
  Markup `data-lq-event="change"` on a field makes the driver listen to `change` and send it with
  that field's `data-lq-field`; `read_field_value` reads `checked` for a checkbox and the value of a
  `<select>`. `from_name("change")` is not used for it.
- `ui/widgets/query_console_element.rs`: a toggled Parameters panel (salvaged) holding its own
  `EditorModel` (`Action` kind, `Memory` binding on the console's text) and `EditorView`; an edit
  writes `query_text` and does not submit. New `#[serde(default)] show_parameters: bool` (AC-12).
- `ui/commands.rs`: the four commands; `ui/mod.rs`: the `editor` module.
- `utils.rs`: `type_name(&ArgumentType)` moves here from `search/command_catalog.rs` (which is behind
  `records`) and both use it.

Rendering:
- egui (`src/egui/editor.rs`, feature `egui`): `pub fn show_editor(ui: &mut egui::Ui, tree:
  &EditorTree, view: &mut EditorView) -> Vec<EditorCommand>` (sync). The text uses `edit_query`
  (and `styled_query_to_layout_job` for read-only text); each node is an `egui::CollapsingHeader`
  with an id from its path; hover is `on_hover_text`; fields are a two-column `egui::Grid` (the proof
  of concept's layout) with widgets matched exhaustively over `ArgumentGUIInfo`; presets and next are
  `ui.menu_button`; the header is an `egui::ComboBox` before the key; the file selector and the
  command picker are popups. Text fields commit on Enter or focus loss; sliders, checkboxes and
  selectors commit on change.
- Browser (`src/ui/web/editor.rs`, feature `webui`): `pub fn editor_html(tree: &EditorTree, dom_id:
  &str) -> String` (pure). Nodes are `<details>`; hover is `title=`; fields use
  `field_attr("f/<path>")`; buttons use `event_attr(Named("<command>/<path>"))`, selectors and
  checkboxes `Change`; widgets follow the salvaged mapping (`<input list>` with `<datalist>` for
  enums, `type=number|range|color|date`). The text is an `<input>` with the highlighted query beside
  it as classed spans from `tokens`. `EditorView::from_ui_event(&UIEvent) -> Vec<EditorCommand>`.

## Error Handling

All errors are `liquers_core::error::Error`, shown in `EditorTree::message` at the node's path; none
is fatal to the element. A refused edit is the editor's own error (`invalid_argument`,
`invalid_token`, `missing_argument`), passed through; a path with no node is
`Error::invalid_pipeline("no node at …")`, as the editor's own out-of-range errors are; a conflict
is `Error::accessor_conflict` and triggers a reload (AC-9); a binding that fails to evaluate, or
whose value is not an accessor, shows the error and the editor read-only; a failed listing keeps
the selector open with the error and still accepts a typed key (AC-14); no registry in the context
is `Error::general_error("no command registry")` and the tree shows text only; `scale_numeric` on a
non-numeric argument is `invalid_argument`. No `unwrap`, no new `ErrorType`.

## Relevant Commands

New in `liquers-lib/src/ui/commands.rs`, namespace `lui`, written after
`specs/guides/COMMAND_REGISTRATION_GUIDE.md`. Each takes what it edits as its state: an `Accessor`
value (its `address()` becomes the binding) or a query, pipeline or text (`Memory`). Each returns
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
| `specs/reference/UI_INTERFACE_FSD.md` | reference | new section "Query, action, resource and key editors": the element and kinds, binding, tree and command messages, the new `AppMessage` / `UpdateMessage` variants, `UIEventKind::Change`, `UIContext::command_registry` |
| `specs/reference/PIPELINES.md` | reference | `ActionEditor::set_source`, `reset`, item edits, `excess`; lenient encoding keeps excess parameters |
| `specs/guides/VALUE_ACCESSOR_GUIDE.md` | guide | "Binding an editor": `…/ns-acc/accessor/recipe-0/query/ns-lui/query_editor` |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | guide | one line: `label`, `doc`, `description`, `gui_info`, presets, `next` and `filename` are what the editors show |

Module docs (`API_DOCUMENTATION_GUIDE.md` §3, all five parts): new `liquers-lib/src/ui/editor/mod.rs`,
`src/egui/editor.rs`, `src/ui/web/editor.rs`; "Main components" and "Where it is used" change in
`liquers-core/src/pipeline/editor.rs`, `liquers-core/src/pipeline/mod.rs` and
`liquers-lib/src/ui/mod.rs`.

Proposed `affects_docs`: the four documents above. `specs/README.md` keeps its UI line.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | core `pipeline/{editor,mod,convert}.rs`, `command_metadata.rs`; lib `ui/editor/` (new), `egui/editor.rs`, `ui/web/editor.rs`, `ui/{element,message,runner,ui_context,event,commands,mod,native}.rs`, `ui/web/app.rs`, `ui/widgets/query_console_element.rs`, `utils.rs`, `search/command_catalog.rs`; `specs/command_registry.yaml` |
| Crates and workflows affected | `liquers-core`, `liquers-lib` (features `egui`, `webui`, and none); the build matrix (`cfg` on renderers); the browser test loop for the web form |
| Existing tests likely to change | `registry_export`; tests that build a `UIContext`; exhaustive matches over `AppMessage`, `UpdateMessage`, `UIEventKind`; pipeline tests that expect lenient encoding to refuse excess parameters |
| New validation | model unit tests per command (no UI); egui rendering tests; web markup and event-decoding tests; a runner test with a stored `recipes.yaml` and two editors (AC-9) |
| Compatibility / data / concurrency / performance / security | old app states load (new element and a defaulted console field, AC-12); R1: `set_if` awaits inline in the runner loop like every query today; R2: the tree is rebuilt only on edits and snapshots, not per frame; R3: the catalogue (about 190 commands) is in the tree only while the picker is open; a binding query runs with the UI's own rights, no new route |
| Recovery | each step is additive; the element can stay unregistered while the core edits remain |
| Certainty and open questions | high for the core edits, the model and egui; medium for the browser change event; split mode waits for `ui-split-elements` Phase 2 |

## Decisions for the gate

- **G1** `ns` is a node where the query has `ns-…`; editing it edits the run of actions it covers.
  In an unrepresentable pipeline it is shown before each action that has its own `ns`.
- **G2** Add `UIEventKind::Change(String)` and the driver support for it here; the other event
  kinds stay in `UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS`.
- **G3** One element type with four kinds, created by four `lui` commands.
- **G4** Two new runner requests (`Accessor`, `ListDirectory`) now, moved into the split design's
  runner when it exists.
- **G5** The list, reset and excess edits go into `ActionEditor` (pipelines), not into `plan.rs` as
  `variadic-argument-slots` planned for `ActionRequest`.
- **G6** Lenient encoding writes excess parameters through (a core behaviour change, strict mode
  unchanged).
- **G7** Namespaces: `lui` (new commands) and `acc` (used).
