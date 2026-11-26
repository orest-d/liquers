---
title: Recipe Trees
kind: reference
audience: internal
area: [core/assets, core/plan, core/commands, core/validate]
reviewed: 2026-11-26
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
| `roots` | none | display hints: nodes to show at the top level besides the found roots (`RecipeList::roots`). Optional; an entry that is no node is a warning, and the editor drops it |
| `result` | none | the single asset the tree stands for. The editor keeps one whenever the tree has a recipe with a filename: the first found root, else the first such recipe |
| `shape` | `dag` | `tree` refuses a node with two parents |
| `dependencies` | `open` | `strict`: the folder is self-contained (below) |

Defaults are not written back, so a read-write round trip keeps the section as it was.

The graph is a DAG, not a tree in the strict sense: a node can have several parents and so appear
in several places (`RECIPE-TREE-VOCABULARY-SAYS-TREE-FOR-A-DAG`). `RecipeList::roots(cmr)` lists
what to show at the top level: the result, then the declared roots that are nodes, then every
recipe node nothing depends on, without duplicates. Any node may be shown as a root; a node not
listed can still be one.

## Nodes and names

The nodes are the recipes that have a filename, the **source recipes**, and — when the folder
listing is known — the stored files. A node's **name** is its filename without the extension:
`wheel` for `wheel.glb`. Two nodes may share a name (`logo.png`, `logo.jpg`) only while nothing
depends on either; a reference to one of them is a `NameCollision` error. Nodes in subfolders
are not supported.

A **source recipe** is a recipe whose query is only a filename (`{query: body.glb, title: Body}`,
no pipeline, overrides or `volatile`; `Recipe::is_source_declaration`), unless the extension is
`node`. It computes nothing: its
value is the data stored under its key. The asset manager's metadata for it has status `Source`
and the recipe's title and description; the store's own record is not changed. With no stored
data, or data that cannot be read, the status is `Error`, evaluation fails (`KeyNotFound` or the
read error) and nothing is stored. For the status of stored data such a key behaves as if it had
no recipe: written data is `Source`, never `Override`.

**`.node` files.** `metadata::NODE_EXTENSION` (`node`) is valid for any node, whatever its
type. `{query: 1.node}` is an **empty node**, not a source: it evaluates to the empty value (`None`)
and is what the editor's New creates. The extension seeds no data format (level 2), so a `.node`
value is written in its type's default format and read back by its metadata; a `.node` key whose
metadata names no type fails with "A .node file is read by its metadata…". A Node argument accepts
a `.node` file whatever its `ext:` (`attach-1.node` links `-R/./1.node`), and
`TreeEditor::node_choices` offers `.node` nodes by their filename.

## Node arguments

A command argument of type `ArgumentType::Node(NodeArgument { kind, extension, form })` names a
node of the working folder:

```rust
register_command!(cr, fn attach(state, part: Value (node: local, ext: "glb")) -> result)?;
register_command!(cr, fn locate(dir: Key (node: local, ext: "glb", form: key)) -> result)?;
register_command!(cr, fn describe(part: Value (node: local, ext: "glb", form: recipe)) -> result)?;
```

`node:` and `ext:` come together (a compile error otherwise); `form` is `resource` (the default,
the link `-R/./<name>.<ext>`, the command receives the asset's value), `key` (`-R-key/./<name>.<ext>`,
the command receives the key; the argument must be a `Key` or a `Value`) or `recipe`
(`-R-recipe/./<name>.<ext>`, the command receives the node's recipe as a `Value::Recipe`, or the
none value when the node has no recipe; the argument must be a `Value`). A Node name of every form
is an edge of the tree, so the dangling, undeclared-source, strict and cycle checks apply to all
three, and a recipe-form result depends on the named recipe (`-R-recipe/<key>`): editing it
expires the result. `node:` names the
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

**Not enforced at evaluation.** `DefaultRecipeProvider` no longer checks the tree: a folder whose
`recipes.yaml` fails the check is served as plain recipes, so it stays readable, writable and
evaluable, and the editors show why it is not a valid tree (design `strict-tree-editing`; before
it, one finding refused every recipe and locked the file). Each recipe then behaves as in any
folder: one whose query does not parse but ends in a filename (`…/name.ext`) is indexed under that
filename, so its key is an error asset with the parse error, and a node depending on an error node
is an error. Strict mode is what makes a folder relocatable: a strict folder that passes the check,
copied to another key or store with the same commands, evaluates to the same values, and only such
a folder can be archived (`RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE`).

## The check

`check_tree(list, listing, cmr) -> TreeReport` never fails: every problem is a `TreeFinding`.
It plans each recipe without a working directory, so keys stay as written. `listing` is the
folder's `listdir` from the store, the asset manager or the recipe provider (recipe names are
merged, so any works); `None` when unknown.

| Finding | Kind | When |
|---|---|---|
| `NoFilename`, `DuplicateFilename`, `PlanFailed` | error | a recipe cannot be a node |
| `ResultNotANode` | error | the result names no node |
| `UnknownRoot` | warning | a declared root names no node (roots are display hints) |
| `Cycle` | error | nodes in order, the first repeated (`DependencyCycle`) |
| `SecondParent` | error | `shape: tree` |
| `Dangling` | error | with a listing: a reference to a file that is neither a recipe nor stored |
| `MissingSource` | error | with a listing: a source recipe whose file is not stored |
| `NameCollision` | error | see Nodes and names |
| `StrictKey`, `StrictLink`, `StrictCwd`, `UndeclaredSource` | error | strict mode only |
| `Orphan` | warning | a node no root reaches (from the result, when declared) |

`RootsBesideResult` is still a variant but no longer reported: roots beside a result are hints.
| `Unchecked` | warning | without a listing, a reference to a file no recipe declares (an error under strict) |

`TreeReport::error()` gives the first error as an `Error` with all findings in its message.
`liquers-validate --recipes` adds the report as `ValidationReport::tree` for a list with a
`tree:` section (no listing); a tree error fails the run (exit 1) and is listed on stderr as
`ERROR    [tree] …`.

## Editing: `TreeEditor`

`TreeEditor::new(list, listing, cmr)` edits one folder's recipes in memory; a provider-set `cwd`
is cleared, since it is never written. Every edit works on a copy and is kept only when the check
reports no error the recipes did not already have; a refused edit changes nothing.
`TreeEditor::resolve(node)` gives the filename a node name or filename names, refusing an unknown
or ambiguous name; commands that take a node by name use it.

**Normalising.** `new` and every commit normalise the list: an empty list gets a `tree:` section
(a new editor holds an empty valid tree), and a list with one drops a `result` or `roots` entry
that names no node, drops `roots` equal to `[result]`, and sets a missing `result` (see the
declaration). A non-empty list without a section is a plain list and stays plain; it gets only
the uniqueness rule.

**Unique names.** No edit creates a second node with a used name, in a tree or a plain list.
`unique_filename(wanted, except)` gives `wanted` or its first free variant: `net.obj`, `net1.obj`,
`net2.obj`…; a stem that is only digits takes the first free integer from 1. Names compare by node
name (`net.obj` and `net.png` collide), against the recipes and the listing; `except` is the node
being renamed.

| Method | Does |
|---|---|
| `add_empty(title, ext)` | adds an empty node `<n>.<ext>` (`ext` empty: `node`), `<n>` the first free integer, and returns its filename; New uses it |
| `rename(from, to)` | renames a node (by name, or filename; `to` without an extension keeps it; a used name is made unique) and rewrites Node names, resource segments, links, `arguments`/`links` overrides, `roots`/`result` and its own filename. A stored file's rename is queued. A recipe whose query cannot be parsed and mentions the node refuses the rename |
| `add_child(parent, recipe, action, arg)` | adds `recipe` and names it in argument `arg` of the action at index `action` of `parent`; a used name is refused |
| `delete(node, cascade)` | removes the node's recipe; a node something references is refused naming its parents; `cascade` also removes what only it reaches. Stored data is never deleted |
| `move_reference(node, from, to, action, arg)` | moves a Node-argument reference; `from`'s argument takes its default |
| `declare_source(filename, title, description)` | adds a source recipe |
| `add(recipe)`, `duplicate(node, to)` | appends a recipe (a used filename is refused); appends a copy under a new name, made unique, whose references are the original's. A new recipe that is a source declaration not yet stored is allowed by `add` and `add_child`, and reported by the check |
| `set_recipe(filename, recipe)`, `set_result(node)`, `replace(list)` | replaces one recipe (keeping its filename); declares or clears the tree's `result`; replaces the whole list (the YAML window) |
| `child_slots(parent)` | the Node arguments of the parent's actions a new child can be named in (`ChildSlot`: action index, command, argument, extension) |
| `node_choices(ext)`, `result_key(folder)`, `suggest_filename(title, ext)`, `report()`, `recipes()` | reading |
| `pending_data_renames()` | the stored-file renames, for a client without an environment |
| `save(folder, envref)` (async) | moves each renamed file with its stored record (skipping one never stored), then writes `recipes.yaml`, both through the asset manager |

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
above. New adds an empty `.node` node; a file with a `tree:` section that fails the check shows the
notice "Not a valid recipe tree: plain recipes" with the first findings and stays editable; see
[`UI_INTERFACE_FSD.md`](UI_INTERFACE_FSD.md) §Recipe editors.

## Known limitations

- Nodes live directly in the folder; subfolders and other node kinds are future `NodeKind`s.
- Python and JavaScript bindings: `RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS`.
- Strict mode is checked by the editors and `liquers-validate`, not at evaluation: an invalid
  strict folder evaluates as plain recipes. A stricter rule may return once the editors keep trees
  valid (design `strict-tree-editing`).
- A Node name in a recipe's `arguments` override of a final pipeline command is rewritten but not
  applied (`RECIPE-OVERRIDES-MISS-A-FINAL-PIPELINE-COMMAND`).

## Tests that pin this contract

`liquers-core/src/recipe_tree.rs` (`tests`), `liquers-core/tests/recipe_tree.rs` (among them the
`strict-tree-editing` cases: `invalid_strict_folder_downgrades_to_plain_recipes`,
`node_file_round_trips_by_metadata`, `rename_makes_the_name_unique`, `roots_are_display_hints`),
`liquers-core/tests/node_argument.rs`, `liquers-core/tests/validate_integration.rs`
(`recipes_tree_report_and_exit_code`), `liquers-lib/src/ui/editor/tests.rs` (`node_selector_*`),
`liquers-macro` (`test_parse_node_argument`, `test_node_argument_errors`).

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-23 | Created: declaration, nodes and names, source recipes, Node arguments, strict mode and its enforcement, the check, `TreeEditor`, the editors. | `design/recipe-tree/` |
| 2026-11-24 | `TreeEditor::add`, `duplicate`, `set_recipe`, `set_result`, `replace`, `child_slots`; a new unstored source is allowed by `add` / `add_child`; the recipes panel and its dependencies view. Reviewed against the implementation. | `design/recipes-editor/` |
| 2026-11-25 | §Node arguments: the `recipe` form; a Node name of every form is an edge (key-form names had none). | `design/node-argument-recipe-form/` |
| 2026-11-26 | An invalid strict folder is downgraded to plain recipes, not refused; unplannable recipes with a filename are error assets; `.node` files; roots are display hints (`RecipeList::roots`, `UnknownRoot` a warning, `RootsBesideResult` not reported); the editor normalises lists, keeps a result, makes names unique (`unique_filename`, `add_empty`); `save` skips unstored files. | `design/strict-tree-editing/` |
| 2026-11-26 | `TreeEditor::resolve` is public: a command given a node name finds its filename. | `design/recipes-editor-open-action/` |
