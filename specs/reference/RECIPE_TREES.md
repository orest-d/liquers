---
title: Recipe Trees
kind: reference
audience: internal
area: [core/assets, core/plan, core/commands, core/validate]
reviewed: 2026-11-24
---
# Recipe Trees

A **recipe tree** is one folder's `recipes.yaml` read as a graph: its recipes and the files they
read are **nodes**, and the references between them are **edges**. A tree can be checked as a
whole, edited without breaking its references, and — in strict mode — copied anywhere and
evaluated to the same value. Module: `liquers_core::recipe_tree`.

## Sources

[`liquers-core/src/recipe_tree.rs`](../../liquers-core/src/recipe_tree.rs) (declaration, graph,
check, editor), [`recipes.rs`](../../liquers-core/src/recipes.rs) (`RecipeList::tree`,
`Recipe::is_source_declaration`, strict enforcement in `DefaultRecipeProvider`),
[`command_metadata.rs`](../../liquers-core/src/command_metadata.rs) (`ArgumentType::Node`),
[`plan.rs`](../../liquers-core/src/plan.rs) (`ParameterValue::NodeLink`),
[`assets.rs`](../../liquers-core/src/assets.rs) (source recipes),
[`validate/`](../../liquers-core/src/validate/) (the tree report), and the editors in
[`liquers-lib/src/ui/editor/`](../../liquers-lib/src/ui/editor/). Design:
[`design/recipe-tree/`](../design/recipe-tree/).

## The declaration

Any `recipes.yaml` becomes a tree by adding a `tree:` section; nothing else changes, and a file
without one reads, plans and evaluates exactly as before.

```yaml
# scenes/car/recipes.yaml
tree: {dependencies: strict, result: car.glb}
recipes:
  - {query: body.glb, title: Body}
  - {query: tyre.glb, title: Tyre}
  - {query: rubber.mat, title: Rubber}
  - {title: Wheel, query: "-R/./tyre.glb/-/material-rubber/wheel.glb"}
  - {title: Car,   query: "-R/./body.glb/-/attach-wheel/car.glb"}
```

| Field | Default | Meaning |
|---|---|---|
| `roots` | the recipes nothing depends on | filenames the tree is reached from; each must be a node |
| `result` | none | the single asset the tree stands for; then it is the only root (`roots` empty or `[result]`) |
| `shape` | `dag` | `tree` refuses a node with two parents |
| `dependencies` | `open` | `strict`: the folder is self-contained (below) |

Defaults are not written back, so a read-write round trip keeps the section as it was.

## Nodes and names

The nodes are the recipes that have a filename, the **source recipes**, and — when the folder
listing is known — the stored files. A node's **name** is its filename without the extension:
`wheel` for `wheel.glb`. Two nodes may share a name (`logo.png`, `logo.jpg`) only while nothing
depends on either; a reference to one of them is a `NameCollision` error. Nodes in subfolders
are not supported.

A **source recipe** is a recipe whose query is only a filename (`{query: body.glb, title: Body}`,
no pipeline, overrides or `volatile`; `Recipe::is_source_declaration`). It computes nothing: its
value is the data stored under its key. The asset manager's metadata for it has status `Source`
and the recipe's title and description; the store's own record is not changed. With no stored
data, or data that cannot be read, the status is `Error`, evaluation fails (`KeyNotFound` or the
read error) and nothing is stored. For the status of stored data such a key behaves as if it had
no recipe: written data is `Source`, never `Override`.

## Node arguments

A command argument of type `ArgumentType::Node(NodeArgument { kind, extension, form })` names a
node of the working folder:

```rust
register_command!(cr, fn attach(state, part: Value (node: local, ext: "glb")) -> result)?;
register_command!(cr, fn locate(dir: Key (node: local, ext: "glb", form: key)) -> result)?;
```

`node:` and `ext:` come together (a compile error otherwise); `form` is `resource` (the default,
the link `-R/./<name>.<ext>`, the command receives the asset's value) or `key` (`-R-key/./<name>.<ext>`,
the command receives the key; the argument must be a `Key` or a `Value`). `node:` names the
`NodeKind`; only `local` exists. The default widget is `ArgumentGUIInfo::NodeSelector`.

A **bare name** (`attach-wheel`) is planned as `ParameterValue::NodeLink`, the way an enum alias
becomes an `EnumLink`: it is evaluated and recorded as a dependency like a link, so
`attach-wheel` and `attach-~X~-R/./wheel.glb~E` give the same value. The link resolves against the
working directory; a bare name with none fails with `KeyNotAbsolute` naming the argument and the
node (`Error::node_needs_cwd`), so an ad-hoc query sets one: `-R-cwd/scenes/car/-R/./body.glb/-/attach-wheel`.
A link given to a Node argument needs no working directory, and outside a strict folder any link
is accepted.

## Strict mode

`dependencies: strict` makes the folder self-contained, the basis of relocation and of the
archive (`RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE`):

- a keyed dependency is a relative key directly in the folder (`./x`), through a resource
  segment, a pipeline element or a Node name; an absolute key, `../x` or `./sub/x` is a
  `StrictKey` error;
- a recipe passes no argument as a link or nested pipeline (`~X~…~E`, `{link: …}`,
  `{pipeline: …}`), whatever its type (`StrictLink`); only resource segments and Node names
  given by name reference anything;
- a recipe does not set the working directory (`-R-cwd/…`, `StrictCwd`);
- every referenced file is declared by a recipe — a source recipe for stored data
  (`UndeclaredSource`).

A command's own default links and pipelines and enum-alias links (`DefaultLink`,
`DefaultPipeline`, `EnumLink`) are command logic and trusted: a strict recipe can use such a
command but cannot override the default with a link. A pipeline command's body is likewise not
checked, apart from the Node names bound into it by the call.

**Enforcement.** `DefaultRecipeProvider` checks a strict `recipes.yaml` when it parses it, with
the store's listing of the folder. Any error refuses every recipe of the folder (each lookup
fails with the findings, the first one's error type) and the file is not cached, so a fix is
seen at once. A strict folder copied to another key or store with the same commands evaluates to
the same values.

## The check

`check_tree(list, listing, cmr) -> TreeReport` never fails: every problem is a `TreeFinding`.
It plans each recipe without a working directory, so keys stay as written. `listing` is the
folder's `listdir` from the store, the asset manager or the recipe provider (recipe names are
merged, so any works); `None` when unknown.

| Finding | Kind | When |
|---|---|---|
| `NoFilename`, `DuplicateFilename`, `PlanFailed` | error | a recipe cannot be a node |
| `UnknownRoot`, `ResultNotANode`, `RootsBesideResult` | error | roots and result |
| `Cycle` | error | nodes in order, the first repeated (`DependencyCycle`) |
| `SecondParent` | error | `shape: tree` |
| `Dangling` | error | with a listing: a reference to a file that is neither a recipe nor stored |
| `MissingSource` | error | with a listing: a source recipe whose file is not stored |
| `NameCollision` | error | see Nodes and names |
| `StrictKey`, `StrictLink`, `StrictCwd`, `UndeclaredSource` | error | strict mode only |
| `Orphan` | warning | a node no root reaches (from the result, when declared) |
| `Unchecked` | warning | without a listing, a reference to a file no recipe declares (an error under strict) |

`TreeReport::error()` gives the first error as an `Error` with all findings in its message.
`liquers-validate --recipes` adds the report as `ValidationReport::tree` for a list with a
`tree:` section (no listing); a tree error fails the run (exit 1) and is listed on stderr as
`ERROR    [tree] …`.

## Editing: `TreeEditor`

`TreeEditor::new(list, listing, cmr)` edits one folder's recipes in memory; a provider-set `cwd`
is cleared, since it is never written. Every edit works on a copy and is kept only when the check
reports no error the recipes did not already have; a refused edit changes nothing.

| Method | Does |
|---|---|
| `rename(from, to)` | renames a node (by name, or filename; `to` without an extension keeps it) and rewrites Node names, resource segments, links, `arguments`/`links` overrides, `roots`/`result` and its own filename. A stored file's rename is queued. A recipe whose query cannot be parsed and mentions the node refuses the rename |
| `add_child(parent, recipe, action, arg)` | adds `recipe` and names it in argument `arg` of the action at index `action` of `parent`; a used name is refused |
| `delete(node, cascade)` | removes the node's recipe; a node something references is refused naming its parents; `cascade` also removes what only it reaches. Stored data is never deleted |
| `move_reference(node, from, to, action, arg)` | moves a Node-argument reference; `from`'s argument takes its default |
| `declare_source(filename, title, description)` | adds a source recipe |
| `add(recipe)`, `duplicate(node, to)` | appends a recipe (a used filename is refused); appends a copy under a new name whose references are the original's. A new recipe that is a source declaration not yet stored is allowed by `add` and `add_child`, and reported by the check |
| `set_recipe(filename, recipe)`, `set_result(node)`, `replace(list)` | replaces one recipe (keeping its filename); declares or clears the tree's `result`; replaces the whole list (the YAML window) |
| `child_slots(parent)` | the Node arguments of the parent's actions a new child can be named in (`ChildSlot`: action index, command, argument, extension) |
| `node_choices(ext)`, `result_key(folder)`, `suggest_filename(title, ext)`, `report()`, `recipes()` | reading |
| `pending_data_renames()` | the stored-file renames, for a client without an environment |
| `save(folder, envref)` (async) | moves each renamed file with its stored record, then writes `recipes.yaml`, both through the asset manager |

## In the editors

`effective_gui` maps a Node argument to `NodeSelector`. An editor bound to a recipe of a
`recipes.yaml` (`-R-key/<folder>/recipes.yaml/-/ns-acc/accessor/…`, `recipe_folder`) asks the
runner for the folder's recipes and listing (`AppMessage::NodeContext` →
`UpdateMessage::NodeContext`, `EditorState::set_node_context`). Its Node fields then list
`EditorField::node_choices` (filename and title) and offer **Rename**:
`EditorCommand::RenameNode { from, to }` applies `TreeEditor::rename` to the whole document and
returns `EditorEffect::RenameData`, which the runner saves (`AppMessage::RenameData`). A link to a
node of the folder is shown as its name. Without a folder the field is a text box. The egui and
browser editors draw it as a selector with a rename box.

The recipes panel (`ui::recipes`, design `design/recipes-editor/`) is a `TreeEditor` over the
whole file: its outline shows node names when the file has a `tree:` section, marks the result
and sources, and attaches the check's findings to the rows they name. Its optional
**dependencies view** puts the roots on top (the declared result or roots, else the nodes nothing
depends on, then any node only a cycle leaves unreached) and each node's direct dependencies under
it: a node with several parents appears under each (marked repeated), and a node closing a cycle is
marked and not expanded. Rename, Delete, Duplicate, Add child and Set as result are the methods
above; see [`UI_INTERFACE_FSD.md`](UI_INTERFACE_FSD.md) §Recipe editors.

## Known limitations

- Nodes live directly in the folder; subfolders and other node kinds are future `NodeKind`s.
- Python and JavaScript bindings: `RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS`.
- A provider caches a checked strict file by its bytes, so a stored file removed outside the asset
  manager is noticed at the next change of the folder.
- A Node name in a recipe's `arguments` override of a final pipeline command is rewritten but not
  applied (`RECIPE-OVERRIDES-MISS-A-FINAL-PIPELINE-COMMAND`).

## Tests that pin this contract

`liquers-core/src/recipe_tree.rs` (`tests`), `liquers-core/tests/recipe_tree.rs`,
`liquers-core/tests/node_argument.rs`, `liquers-core/tests/validate_integration.rs`
(`recipes_tree_report_and_exit_code`), `liquers-lib/src/ui/editor/tests.rs` (`node_selector_*`),
`liquers-macro` (`test_parse_node_argument`, `test_node_argument_errors`).

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-23 | Created: declaration, nodes and names, source recipes, Node arguments, strict mode and its enforcement, the check, `TreeEditor`, the editors. | `design/recipe-tree/` |
| 2026-11-24 | `TreeEditor::add`, `duplicate`, `set_recipe`, `set_result`, `replace`, `child_slots`; a new unstored source is allowed by `add` / `add_child`; the recipes panel and its dependencies view. Reviewed against the implementation. | `design/recipes-editor/` |
