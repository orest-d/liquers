# Phase 2: Solution & Architecture — Self-contained recipe trees

## Overview

Four parts, all reading recipes as they are today:

1. **Node argument type** (`liquers-core/src/command_metadata.rs`): `ArgumentType::Node(NodeArgument)`.
   Parameter resolution (`ParameterValue::from_string`, called by `pop_value`) turns a bare name into
   a new `ParameterValue::NodeLink`, the way `ArgumentType::Enum` turns an alias into `EnumLink`; a
   link given to a Node stays a `ParameterLink`. `NodeLink` is evaluated exactly like a link.
2. **Tree analysis** (new `liquers-core/src/recipe_tree.rs`): the `tree:` declaration on
   `RecipeList`, a graph built from each recipe's plan, the per-recipe **strict rules** and the
   folder-wide **tree check**.
3. **Enforcement and reporting**: `DefaultRecipeProvider` applies the strict rules when it parses a
   strict folder's `recipes.yaml` (AC-24); `liquers-validate --recipes` adds a tree report (AC-9).
4. **`TreeEditor`** (same module): sync, in-memory edits of a `RecipeList` through
   `PipelineEditor` / `QueryEditor`, re-checked after each edit; an async `save` writes the result.
   The editors in `liquers-lib/src/ui/editor/` offer node choices for Node arguments (AC-18).

Rejected:
- *Rewriting references as text.* Fragile with escapes and links; parsing to pipelines gives
  exact positions and the existing representability rules.
- *A provider-set `Recipe::strict` flag read by `Recipe::to_plan`.* It adds a field to a struct
  serialized everywhere (`recipe-and-pipeline-structures.md` §1) for a fact the provider already
  has; checking at parse time also checks each recipe once, not once per plan.
- *Reusing `ParameterLink` for a resolved name.* Strict mode must tell a recipe-written link
  (refused) from a name (allowed), and rename and the editor need the name back.
- *Node choices through `computed-enum-alternatives`.* That design validates values against
  alternatives at planning; a node list depends on the folder, which planning does not know, and is
  needed only by the editor. Node choices use the editor's existing directory listing.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `TOP-LEVEL-RELATIVE-RESOURCE-QUERY-LOOKS-UP-A-RELATIVE-RECIPES-KEY` | draft | P3 | A bare Node name with no cwd: this design errors explicitly (AC-20) instead of relying on that path | No | No | Design around; AC-10 tests evaluate through the provider |
| `RECIPE-OVERRIDES-MISS-A-FINAL-PIPELINE-COMMAND` | draft | P3 | A Node name in a recipe's `arguments` override of a final pipeline command is not applied, so rename's rewrite of it has no effect there | No | No | Rename rewrites overrides anyway; noted in RECIPE_TREES.md |
| `RECIPE-WITHOUT-FILENAME-SILENTLY-OMITTED-FROM-LISTING` | draft | P3 | A recipe without a filename cannot be a node | No | No | The tree check reports it as an error |
| `RECIPE-CIRCULAR-DEPENDENCY-FIELDS-ARE-DEAD` | draft | P3 | Cycles are found by the tree check | No | No | Fields neither read nor set |
| `CONTEXT-APPLY-AD-HOC-SKIPS-RELATIVE-PIPELINE-KEYS` | draft | P3 | None: trees are provider recipes | No | No | — |
| `PIPELINES-NOT-IN-LANGUAGE-BINDINGS` | draft | P3 | Prerequisite of the bindings follow-up only | No | No | — |
| design `computed-enum-alternatives` | in_review | — | Both add `ArgumentType` handling; whichever lands second adds its arms to the other's matches | No | No | Rejected as the node-choice mechanism (above) |

Namespaces: none; no command is added. Existing commands keep their argument types.

## Interfaces

**Command metadata** (`command_metadata.rs`, sync: pure data):

```rust
pub enum ArgumentType { /* … */ Node(NodeArgument) }   // serde: {node: {extension, form}}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct NodeArgument {
    pub extension: String,                 // without the dot: "glb"
    #[serde(default)] pub form: NodeLinkForm,
}
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum NodeLinkForm { #[default] Resource, Key }

impl NodeArgument {
    /// `-R/./<name>.<ext>` or `-R-key/./<name>.<ext>`; refuses a name that is not one key segment.
    pub fn link(&self, name: &str) -> Result<Query, Error>;
    /// The name, when `query` is exactly the link this argument builds.
    pub fn name_of(&self, query: &Query) -> Option<String>;
}
```

DSL (`liquers-macro/src/registration.rs`), beside `enum:` in the argument's metadata parentheses:
`part: Value (node: "glb")` and `dir: Key (node(form: key): "glb")`. A key-form Node must have
Rust type `Key` (compile error otherwise), as a `-R-key` link yields a key.

**Planning** (`plan.rs`, sync):

```rust
pub enum ParameterValue { /* … */ NodeLink(String /*arg*/, String /*node*/, Query, Position) }
```

- `from_string`: `ArgumentType::Node(n)` → `NodeLink(arg, s, n.link(s)?, pos)`; an empty string
  falls back to the default as for `Any`.
- `ParameterValue::freeze_cwd` becomes `-> Result<(), Error>` (crate-private, as is its caller
  chain up to `Plan::freeze_cwd_with`, which already returns `Result`): a `NodeLink` with no CWD
  in the cursor fails (AC-20) instead of falling back to root; other links keep the fallback.
- Every exhaustive `match` on `ParameterValue` gains the arm; where links are evaluated or listed
  as dependencies (`interpreter.rs`, `commands.rs`, `find_dependencies`, `pipeline/plan.rs`,
  `store/realm.rs`, `liquers-axum/src/api_core/overrides.rs`) it joins `ParameterLink`.

**Tree** (new `recipe_tree.rs`, sync: CPU-only over in-memory recipes):

```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct TreeDeclaration {
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub roots: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub result: Option<String>,
    #[serde(default)] pub shape: TreeShape,               // Dag (default) | Tree
    #[serde(default)] pub dependencies: DependencyMode,   // Open (default) | Strict
}
// RecipeList gains: #[serde(default, skip_serializing_if = "Option::is_none")] pub tree: Option<TreeDeclaration>

pub struct TreeNode { pub filename: String, pub recipe: Option<usize> } // None: a stored file
pub enum RefKind { Resource, NodeArg { action: usize, arg: String }, Link { arg: String }, KeyValue }
pub struct TreeEdge { pub from: String, pub to: Key, pub kind: RefKind }
pub struct TreeGraph { pub nodes: BTreeMap<String, TreeNode>, pub edges: Vec<TreeEdge> }
pub enum TreeFinding { /* one variant per AC-3..AC-8, AC-16, AC-19, AC-23 rule */ }
pub struct TreeReport { pub roots: Vec<String>, pub result: Option<String>,
                        pub graph: TreeGraph, pub errors: Vec<TreeFinding>, pub warnings: Vec<TreeFinding> }

impl TreeGraph {
    /// Plans each recipe (`Recipe::to_plan`, unfrozen, so keys stay relative) and collects its
    /// references; `stored` lists the folder's stored filenames, `None` when unknown (validation).
    pub fn build(list: &RecipeList, stored: Option<&BTreeSet<String>>, cmr: &CommandMetadataRegistry) -> Result<Self, Error>;
}
/// The per-recipe strict rules (AC-7, AC-23) over one plan; walks nested `Step::Plan`,
/// `Step::Evaluate` queries and pipeline steps. `DefaultLink`, `DefaultPipeline`, `EnumLink` are trusted.
pub fn check_strict(plan: &Plan) -> Vec<TreeFinding>;
/// The whole check; never fails, all findings are in the report.
pub fn check_tree(list: &RecipeList, stored: Option<&BTreeSet<String>>, cmr: &CommandMetadataRegistry) -> TreeReport;
```

"Directly in the folder" is a relative key of exactly two segments, `.` and a filename. A node's
name is its filename without the extension. `check_strict` skips the leading `Step::SetCwd` that
`Recipe::to_plan` adds for the recipe's own `cwd`; any other `SetCwd` is a strict finding.

**Editor** (same module; edits sync, `save` async because it writes the store):

```rust
pub struct TreeEditor { list: RecipeList, stored: BTreeSet<String>, cmr: Arc<CommandMetadataRegistry> }
pub struct DataRename { pub from: String, pub to: String }   // stored-file renames save() performs

impl TreeEditor {
    pub fn new(list: RecipeList, stored: BTreeSet<String>, cmr: Arc<CommandMetadataRegistry>) -> Result<Self, Error>;
    pub async fn load<E: Environment>(folder: &Key, envref: EnvRef<E>) -> Result<Self, Error>;
    pub fn report(&self) -> TreeReport;
    pub fn recipes(&self) -> &RecipeList;
    pub fn rename(&mut self, from: &str, to: &str) -> Result<&mut Self, Error>;              // AC-1, AC-15
    pub fn add_child(&mut self, parent: &str, recipe: Recipe, action: usize, arg: &str) -> Result<&mut Self, Error>; // AC-11
    pub fn delete(&mut self, node: &str, cascade: bool) -> Result<&mut Self, Error>;         // AC-12
    pub fn move_reference(&mut self, node: &str, from: &str, to: &str, action: usize, arg: &str) -> Result<&mut Self, Error>; // AC-13
    pub fn result_key(&self, folder: &Key) -> Option<Key>;                                   // AC-19
    pub fn suggest_filename(&self, title: &str, extension: &str) -> String;                  // Phase 1 Q3 helper
    pub async fn save<E: Environment>(&self, folder: &Key, envref: EnvRef<E>) -> Result<(), Error>;
}
```

Each edit works on a clone and replaces `list` only when `check_tree` reports no new error, which
gives AC-14 without undo logic. A rename rewrites, per recipe: the query through `QueryEditor`
(so it keeps a query form), the `pipeline` through `PipelineEditor`, resource keys, Node names
(`Source::Value` strings of Node arguments), `Link` sources (parsed, rewritten, re-encoded),
`arguments` / `links` overrides, and the renamed recipe's own filename. A reference it cannot place
(AC-15) refuses the rename. `save` performs the `DataRename`s with `AsyncStore::rename_data`, then
writes `recipes.yaml` through the asset manager as the recipes accessor does, so the provider cache
and dependent assets follow.

**UI** (`liquers-lib/src/ui/editor/`, sync model): `effective_gui` maps `ArgumentType::Node` to a new
`ArgumentGUIInfo::NodeSelector`; `EditorState` sends the existing `EditorEffect::ListDirectory` for
the bound recipe's folder and builds `FieldValue` choices from the listing's entries with the Node's
extension. A field holding a `ParameterLink` equal to `NodeArgument::link(name)` shows `name`. Without
a folder (an unbound query) the field is a text box. Renderers: `egui/editor` and `ui/web/editor`
draw the choices as their enum selector.

**Validation** (`validate/`, sync): `ValidationReport` gains `#[serde(skip_serializing_if = "Option::is_none")] tree: Option<TreeReport>`;
`validate_recipes` fills it when the list has a `tree:` (with `stored: None`: a non-recipe key is a
warning "not checked: stored file?"); tree errors set the status to `Error` (exit 1).

**Value types:** none.

## Integration Points

- `liquers-core`: `command_metadata.rs` (`NodeArgument`, `ArgumentGUIInfo::NodeSelector`),
  `plan.rs` (`NodeLink`, fallible `freeze_cwd`), `interpreter.rs`, `commands.rs`, `pipeline/`
  (`plan.rs`: a `Source::Value` string for a Node argument also becomes a `NodeLink`, and a
  `NodeLink` converts back to `Source::Value(name)`; `convert.rs` reads a Node parameter as text;
  `editor.rs`), `store/realm.rs`,
  `command_declaration.rs` (declared Node arguments), `recipes.rs` (`RecipeList::tree`;
  `DefaultRecipeProvider::cached_recipes` runs `check_strict` per recipe of a strict folder and keeps
  the findings in `CachedRecipes`; `recipe()` returns the node's error), new `recipe_tree.rs`,
  `validate/`, `bin/liquers_validate.rs`, `error.rs`, `lib.rs`.
- `liquers-macro`: `registration.rs` parses `node:` and emits `ArgumentType::Node`.
- `liquers-lib`: `ui/editor/{tree,state}.rs`, `egui/editor`, `ui/web/editor`, `egui/widgets.rs`,
  `utils.rs`, `commands.rs` (match arms); the registry exporter writes Node arguments unchanged
  (serde).
- `liquers-mcp/src/{schema,server}.rs`: a Node argument is a JSON-schema string with a description
  naming the extension.
- `liquers-py/src/command.rs`, `liquers-axum/src/api_core/overrides.rs`: match arms only.
- `specs/command_registry.yaml`: unchanged unless a command adopts Node (none does here).

## Error Handling

| Failure | Constructor |
|---|---|
| A strict rule (AC-7, AC-23), at parse time or in the check | new `Error::strict_dependency(node, key_or_arg, detail)` (`ParameterError`) |
| Cycle (AC-4) | new `Error::tree_cycle(&[String])` (`DependencyCycle`) |
| Shape, root, result, dangling, name collision (AC-3, 5, 8, 16, 19) | new `Error::tree_check(detail)` (`ParameterError`), one per finding, naming nodes |
| Bare Node name with no CWD (AC-20) | new `Error::node_needs_cwd(arg, name)` (`KeyNotAbsolute`) |
| Node name that is not one key segment | existing `Error::invalid_token(value, class)` |
| Edit refused (AC-11–15) | the finding's error, `.with_key(folder)` |
| `save` store failures | propagated from the store / asset manager |

## Relevant Commands

No command is added or changed, so `specs/command_registry.yaml` is not regenerated. Any existing
command can later adopt a Node argument (`part: Value (node: "glb")`); that is a signature change
for that command, with its own registry regeneration. Namespaces involved: none.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/RECIPE_TREES.md` | reference, internal, `core/assets` | new: declaration, names, Node, strict rules, check, result, `TreeEditor`, worked example |
| `specs/reference/RECIPES_AND_PLANS.md` | reference | `tree:` section; strict enforcement in the provider contract; History row |
| `specs/reference/PIPELINES.md` §8 | reference | `TreeEditor` beside the editors; History row |
| `specs/reference/REGISTER_COMMAND_FSD.md`, `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | reference, guide | `node:` argument metadata; History rows |
| `specs/reference/UI_INTERFACE_FSD.md` | reference | `NodeSelector`; History row |
| `docs/guides/rust/07-assets.md`, `05-commands.md` | user guide | recipe trees section; Node arguments |
| `specs/README.md` | map | link RECIPE_TREES.md |

Module docs: `recipe_tree.rs` (new, all five parts); `recipes.rs` (purpose and components gain the
tree section and strict enforcement); `command_metadata.rs` (components: `NodeArgument`);
`validate/mod.rs` (components: the tree report); `liquers-lib/src/ui/editor/mod.rs` (components:
node choices). `affects_docs` stays as in `DESIGN.md`; `specs/reference/PROJECT_OVERVIEW.md` does
not change, since query and key encoding do not.

## Questions for the gate

1. **DSL spelling.** `part: Value (node: "glb")` and `dir: Key (node(form: key): "glb")`, in the
   argument's metadata parentheses beside `enum:`. *Recommended as written.*
2. **Scope of a strict failure.** Only the offending node fails to plan; the other nodes of the
   folder are still served. The `cwd` precedent instead refuses the whole file. *Recommended: per
   node*, since one bad recipe should not take down a document being edited.
3. **Stored files in `liquers-validate`.** The tool opens no store, so a reference to a key that
   has no recipe is a warning ("not checked: stored file?") there, and an error only in the
   `TreeEditor`, which lists the folder. *Recommended as written.*

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | ~25, most of them one match arm for `NodeLink` / `ArgumentType::Node` |
| Crates and workflows affected | core, macro, lib, mcp, py, axum (arms); `check-build-matrix.sh` (arms in feature-gated UI code) |
| Existing tests likely to change | none by behaviour; tests constructing `ParameterValue` exhaustively, and `freeze_cwd` call sites |
| New validation | `recipe_tree` unit tests; provider strict test; `liquers-validate` tree report; editor node choices |
| Compatibility / data | `tree:` is optional and skipped when absent; older readers ignore it. `ArgumentType` serde gains a variant: an old reader of a registry export with a Node argument fails on it, and none is exported until a command adopts Node |
| Performance | the strict rules run once per `recipes.yaml` parse (cached); the tree check only in validation and the editor |
| Security | strict mode is a containment aid, not a sandbox: commands are trusted (Phase 1 trust boundary) |
| Recovery | each part lands as its own step; Node can ship without the tree, the tree without the UI |
| Certainty and open questions | High for analysis and Node; medium for rename coverage of `Link` sources inside nested pipelines, proven by Phase 3 tests |
