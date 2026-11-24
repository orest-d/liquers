# Phase 2: Solution & Architecture — Recipe and recipes editors

## Overview

A new backend-free module `liquers-lib/src/ui/recipes/` holds the editors' state, in the pattern
of `ui::editor::EditorState` and the console's embedded Parameters panel
(`QueryConsoleElement::apply_parameter_commands`):

- `RecipesElement` (`lui/recipes_editor`) binds **one** accessor, the folder's `recipes.yaml`
  (text or bytes, Phase 1 Q1). It keeps the working copy as a `liquers_core::recipe_tree::TreeEditor`
  and draws the side panel.
- Its recipe windows are `RecipeForm`s it **owns**, keyed by filename (Q2). Each form edits its
  recipe in the working copy through the new `RecipeFileStep` (`get`/`put` are synchronous), and
  embeds an `EditorState` of kind `Query` for the query.
- Every write is one `AccessorOp::SetIf` of the whole document. When an edit queued stored-file
  renames, the write is `AppMessage::RenameData` instead (`TreeEditor::save`).
- `RecipeElement` (`lui/recipe_editor`) is the same `RecipeForm` bound to a recipe accessor
  (`acc/recipe_file-…` or `acc/recipe-…`), writing `Value::Recipe`.
- View, Metadata and a new value window are elements added under the panel with `lui/add-child`
  through `AppMessage::SubmitQuery`. The panel draws its children as windows, as
  `LayoutSpec::Windows` does.

Rejected:

- **Recipe windows as separate elements, each bound to its own accessor.** On-demand edits across
  windows would need cross-element coordination, and a rename would have to re-bind them.
- **A memory accessor (`MemoryRoot`) as the working copy, with windows adopting projections.**
  Every in-process edit would go through runner messages for no gain. The panel holds the copy, and
  `RecipeFileStep` is the same lens either way.
- **`Value::RecipeList`.** Phase 1 Q1.

**Update modes.** `query-update-modes` (PR #285) is not built yet. The panel and the form use the
editor's existing `continuous` flag: *Apply immediately* is on, and *On demand* is off, with ✓
(`Update`) and Revert. When that design lands, they adopt its mode type. This is AC-13 within
today's vocabulary.

## Known-Issue Preflight

| Issue | Status | Impact | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|
| `YAML-DOCUMENT-EDIT-DROPS-COMMENTS` | draft P3 | structured edits re-serialize the file | no | no | non-goal; the YAML window writes the text as typed |
| `RECIPE-WITHOUT-FILENAME-SILENTLY-OMITTED-FROM-LISTING` | draft P3 | such a recipe has no node | no | no | the outline lists it as `#<index>` with the default icon (AC-2) |
| `WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS` | draft P2 | Evaluate must not block the runner | no | no | a new `AppMessage::Evaluate` starts the asset and does not await it |
| `UI-ELEMENT-NESTED-IN-CONSOLE-HAS-NO-HANDLE` | draft P3 | n/a | no | no | View windows are real child elements with handles |
| `RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS` | draft P3 | unchanged | no | no | — |
| `QUERY-EDITOR-HAS-NO-LIVE-PREVIEW` (PR #285) | draft | AC-13 maps onto `continuous` | no | no | adopt the modes when built |

## Interfaces

**liquers-core**, synchronous (pure document edits):

```rust
// accessor/steps.rs — a recipe of a recipes.yaml by its filename (AC-6)
#[derive(Debug, Clone)]
pub struct RecipeFileStep(pub String);
impl<V: ValueInterface> ProjectionStep<V> for RecipeFileStep { /* name "recipe_file-<f>" */ }
// put refuses a recipe whose filename differs ("rename it through the recipes editor").

// recipe_tree.rs — TreeEditor
pub fn add(&mut self, recipe: Recipe) -> Result<&mut Self, Error>;                 // AC-7
pub fn duplicate(&mut self, node: &str, to: &str) -> Result<&mut Self, Error>;     // AC-9
pub fn set_recipe(&mut self, filename: &str, recipe: Recipe) -> Result<&mut Self, Error>; // AC-5
pub fn set_result(&mut self, node: Option<&str>) -> Result<&mut Self, Error>;      // menu
pub fn replace(&mut self, list: RecipeList) -> Result<&mut Self, Error>;          // AC-12
```

Each edit goes through the existing `commit`, so it is refused when it adds a check error (AC-14).
`replace` is the YAML window's apply. It is **not** refused on findings: a typed file may be
mid-repair. It is refused only when it fails to parse, and that happens before the call.

**liquers-lib** `ui/recipes/` (backend-free, synchronous):

```rust
pub enum RecipeLabel { Name, Title, Both }                    // serde lowercase; AC-16
pub enum RecipesView { Outline, Dependencies }                // AC-11
pub struct RecipeRow {                                         // what renderers draw, AC-1..3, 17, 18
    pub filename: Option<String>, pub name: String, pub title: String, pub description: String,
    pub icon: String, pub status: RowStatus, pub findings: Vec<String>, pub is_result: bool,
    pub is_source: bool, pub depth: usize, pub expandable: Option<bool>, pub repeated: bool,
    pub cycle: bool, pub formats: Vec<String>,
}
pub enum RowStatus { None, Status { status: Status, label: String }, Progress { percent: u8, message: String },
                     Error(String) }
pub enum RecipesCommand {                                      // from renderers
    SetLabel(RecipeLabel), SetView(RecipesView), Toggle(String /*row path*/),
    Open(String), Close(String), Rename { from: String, to: String }, New { title: String, extension: String },
    AddChild { parent: String, title: String, extension: String, action: usize, arg: String },
    Duplicate(String), Delete(String), SetResult(Option<String>),
    View(String), Metadata(String), Evaluate(String), SaveAs { filename: String, format: Option<String>, path: String },
    Form { filename: String, command: FormCommand }, OpenYaml, CloseYaml, SetYaml(String), ApplyYaml,
    Update, Revert, SetContinuous(bool),
}
pub enum FormCommand { SetTitle(String), SetDescription(String), SetFilename(String),
                       SetOption { field: String, text: String }, SetOverride { name: String, text: String },
                       RemoveOverride(String), Editor(EditorCommand) }
pub enum RecipesEffect { None, Write { value: Value, expected: Option<Fingerprint> },
                         SaveTree { folder: String, editor: Box<TreeEditor> }, Reload,
                         Submit(String /*query*/), Evaluate(String), SaveAs { query: String, format: Option<String>, path: String },
                         RefreshStatus }
pub struct RecipesState { /* working copy, fingerprint, pending, rows, forms, yaml buffer, message */ }
impl RecipesState {
    pub fn new(registry: Arc<CommandMetadataRegistry>) -> Self;
    pub fn on_snapshot(&mut self, value: &Value, fingerprint: Option<Fingerprint>) -> Result<(), Error>;
    pub fn on_context(&mut self, context: NodeContext);       // asset infos, formats, listing
    pub fn on_written(&mut self, fingerprint: Fingerprint) -> RecipesEffect;
    pub fn on_conflict(&mut self, live: &Value, fingerprint: Option<Fingerprint>);   // AC-15
    pub fn apply(&mut self, command: RecipesCommand) -> Result<RecipesEffect, Error>;
    pub fn rows(&self) -> &[RecipeRow];  pub fn forms(&self) -> impl Iterator<Item = &RecipeForm>;
}
pub struct RecipeForm { /* filename, recipe, query EditorState, typed fields */ }
pub fn yaml_spans(text: &str) -> Vec<(Range<usize>, YamlRole)>;   // AC-12; query values use spans.rs
pub struct RecipesElement; pub struct RecipeElement;                // UIElement, typetag
```

`RowStatus::Progress` replaces the circle while the asset's first `ProgressEntry` has a total and
is not done (AC-17). Tooltips are derived text and need no fields of their own (AC-18).

`NodeContext` (`ui/editor/tree.rs`) gains `assets: BTreeMap<String, AssetInfo>` (filename to info),
`formats: BTreeMap<String, Vec<String>>` (filename to the `supported_data_formats` of its type) and
`notice: Option<String>` (the outcome of the last Save as). The runner fills them, so an existing Node-argument editor ignores them.

**Runner messages** (`ui/message.rs`), asynchronous, handled in `AppRunner::process_messages`:

```rust
AppMessage::Evaluate { query: String },                         // start; never awaited (AC-19)
AppMessage::SaveAs { handle: UIHandle, query: String, format: Option<String>, path: String },
```

`SaveAs` evaluates the query, serializes the value (`as_bytes` in `format`, else the asset's own
`data_format`, else the extension), and writes `path` with `tokio::fs`. It is native-only
(`#[cfg(not(target_arch = "wasm32"))]`). Its outcome comes back as the next
`UpdateMessage::NodeContext`, in `notice`. The egui *Save as…* dialog is `rfd::FileDialog` (Q8), an optional native dependency
behind the `egui` feature.

## Integration Points

- `liquers-core`: `accessor/steps.rs` (`RecipeFileStep`, tests), `recipe_tree.rs` (new edits,
  tests).
- `liquers-lib`:
  - `accessor/mod.rs`: `recipe_file` command.
  - `ui/recipes/{mod,state,form,rows,yaml,element,tests}.rs` (new).
  - `ui/mod.rs`: `pub mod recipes`.
  - `ui/commands.rs`: two `lui` commands.
  - `ui/editor/tree.rs`: `NodeContext` fields.
  - `ui/message.rs`, `ui/runner.rs`: `Evaluate`, `SaveAs`, `node_context` fills the assets and
    formats.
  - `egui/recipes.rs` (new): panel, menu, windows, YAML layouter.
  - `ui/web/recipes.rs` (new): panel, per-row menu, recipe form, YAML textarea.
  - `Cargo.toml`: `rfd`.
- `specs/command_registry.yaml`: regenerated.

## Error Handling

| Failure | Constructor |
|---|---|
| no recipe with that filename (step `get` is `Ok(None)`; `put`) | `Error::general_error("there is no recipe '<f>'")` |
| a step `put` changing the filename | `Error::general_error(...)` |
| a name already used, a refused tree edit | `Error::tree_check(..)` (existing) |
| YAML that does not parse | `Error::conversion_error_with_message("YAML", "RecipeList", msg)` |
| Save as: no value, serialization, write | the evaluation error; `Error::from_error(ErrorType::General, io)` |

A refused command changes nothing and is shown as the panel's or the form's message.

## Relevant Commands

```rust
register_command!($cr, async fn recipe_file(state, filename: String, context) -> result
    namespace: "acc" label: "Recipe by filename"
    doc: "The accessor of the recipe with this filename in the recipe list the state accessor reads")?;
register_command!(cr, fn recipes_editor(state) -> result namespace: "lui" label: "Recipes editor"
    doc: "A side panel editing the recipes.yaml the state accessor reads: outline, dependencies, recipe and YAML windows")?;
register_command!(cr, fn recipe_editor(state) -> result namespace: "lui" label: "Recipe editor"
    doc: "A form editing the recipe the state accessor reads, with the query editor inside")?;
```

Namespaces: `acc`, `lui`.

## Documentation Architecture

| Path | Change |
|---|---|
| `specs/reference/UI_INTERFACE_FSD.md` | §Editors: recipe and recipes editors, messages `Evaluate` / `SaveAs`, `NodeContext` fields; History row |
| `specs/reference/RECIPE_TREES.md` | §In the editors: the panel's outline and dependencies view, the new `TreeEditor` edits; History row |
| `specs/reference/VALUE_ACCESSORS.md` | `recipe_file` step and command; History row |
| module docs | `ui/recipes/mod.rs` (new, all five parts); `accessor/steps.rs`, `recipe_tree.rs`, `accessor/mod.rs`, `ui/editor/mod.rs` (main components) |

`affects_docs` as in `DESIGN.md`. `specs/README.md` already links the design.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | ~15, listed above |
| Crates and workflows affected | core, lib; build matrix (`egui`, `webui`, wasm32 for `rfd` gating) |
| Existing tests likely to change | constructions of `NodeContext` in `ui/editor/tests.rs` |
| New validation | `registry_export` (new commands) |
| Compatibility | additive; `recipes.yaml` written as `serde_yaml` of `RecipeList`, as `TreeEditor::save` does |
| Concurrency | one write in flight; `set_if` conflicts reload (AC-15) |
| Performance | rows rebuilt on each change (`check_tree` over a small folder) |
| Recovery | revert the PR; no data format changes |
| Certainty | medium: egui window and menu ergonomics are verified by the headless example check, not by eye |
