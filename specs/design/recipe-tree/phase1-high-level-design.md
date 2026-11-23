# Phase 1: High-Level Design — Self-contained recipe trees

## Purpose

Let a folder of recipes be **one structured document**: a DAG of named nodes, each an asset built
from the nodes it references, with a checked dependency profile that makes the folder work
wherever it is placed, an optional single result that lets the whole folder stand for one asset,
and an editor that adds, renames, moves and deletes nodes while keeping every reference correct.
Applications that edit scene graphs, CAD node trees or multi-part reports can then keep Liquers
recipes as their source of truth, with caching, dependency tracking and recomputation
(`specs/reference/VISION.md`, "the transformation *is* the URL"), instead of generating recipes
from a private format. The guarantee is also the basis of the later portable archive
(`RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE`).

## Problem Example

A car model lives in `scenes/car/`. Each part is a recipe; `body.glb` and `tyre.glb` are stored
files (`Source` assets).

```yaml
# scenes/car/recipes.yaml, today
recipes:
  - title: Car
    query: "-R/./body.glb/-/attach-~X~-R/./wheel.glb~E/car.glb"
  - title: Wheel
    query: "-R/./tyre.glb/-/attach-~X~-R/materials/rubber.mat~E/wheel.glb"
```

Today:

- Renaming `wheel.glb` to `front_wheel.glb` means editing the text inside `car.glb`'s query by
  hand. Nothing finds the references, and a missed one fails only when the car is evaluated.
- `wheel.glb` depends on `/materials/rubber.mat`, outside the folder. Copy the folder to another
  store and it silently breaks; nothing tells the author.
- Nothing says `car.glb` is the result, so a tool cannot treat the folder as "a car".
- A cycle (`wheel.glb` referencing `car.glb`) is found only when evaluation fails.

With this design the folder declares itself a tree:

```yaml
tree:
  dependencies: strict  # only relative keys of this folder
  result: car.glb       # the folder stands for this asset; also its single root
recipes:
  - …as above…
```

`liquers-validate --recipes scenes/car/recipes.yaml --cwd scenes/car` reports the graph
(`car → body, wheel`; `wheel → tyre`) and refuses `wheel.glb`'s `/materials/rubber.mat` under
strict mode. Once `rubber.mat` is copied into the folder and referenced as `-R/./rubber.mat`, the
folder checks and works under any key. In the editor, `attach`'s `part` argument is a **Node**
argument: it offers `body`, `tyre`, `wheel`, … and writes `~X~-R/./wheel.glb~E`.
`TreeEditor::rename("wheel", "front_wheel")` renames the node and rewrites every reference to it.

## Scope and Acceptance Criteria

- **AC-1** Rename rewrites every reference
  WHEN a node is renamed through the `TreeEditor`
  THEN its filename and every reference to it in the folder (in a query, a link or a pipeline
  element) are rewritten, the tree still checks, and the result evaluates to the same value
- **AC-2** Declaration is optional and preserved
  WHEN a `recipes.yaml` with a `tree:` section is read and written back
  THEN the section is kept unchanged; a `recipes.yaml` without one reads, plans and evaluates
  exactly as before
- **AC-3** Roots: declared or default
  WHEN `roots` is declared
  THEN every root must be a node of the folder, or the check fails naming it; WHEN neither
  `roots` nor `result` is declared, the roots are the nodes nothing depends on, and the check
  reports them
- **AC-4** Cycles are refused
  WHEN node references form a cycle
  THEN the check fails naming the nodes of the cycle in order
- **AC-5** Shape is checked
  WHEN `shape: tree` and a node is referenced by two parents (or by two roots)
  THEN the check fails naming the node and both parents; under `shape: dag`, the default, it passes
- **AC-6** Orphans are reported
  WHEN a recipe or stored file of the folder is reachable from no root (from the result, when
  there is one)
  THEN the check reports it as a warning, not an error
- **AC-7** Strict dependencies (revised 2026-11-23)
  WHEN `dependencies: strict` and a node depends on a keyed asset by an absolute key, by a key
  outside the folder (`../`), or by a key in a subfolder, through its query, a link, a pipeline
  element or a command's default link
  THEN the check fails naming the node and the key; dependencies on assets directly in the folder
  through relative keys (`-R/./x`, `-R-key/./x`, `{resource: ./x}`) pass
- **AC-8** Dangling references
  WHEN a reference names a key in the folder that is neither a recipe nor a stored file
  THEN the check fails naming the node and the missing key
- **AC-9** Validation tool
  WHEN `liquers-validate --recipes` reads a `recipes.yaml` with a `tree:` section
  THEN its report adds the nodes, their edges, the roots and result, and the check's errors and
  warnings, with exit code 1 on an error
- **AC-10** Relocatable (revised 2026-11-23)
  WHEN a folder that passes the strict check is copied to another key, or to another store with
  the same commands
  THEN its result (each root) evaluates there to the same value
- **AC-11** Add a child
  WHEN `TreeEditor::add_child(parent, recipe, argument)` is called
  THEN the recipe is added and referenced from `parent`; a name already used in the folder is
  refused
- **AC-12** Delete
  WHEN a referenced node is deleted
  THEN the edit is refused naming its parents, unless cascading is asked for, which also removes
  the descendants no remaining node references
- **AC-13** Move (reparent)
  WHEN a reference to a node is moved from one parent to another
  THEN both recipes are rewritten, and the edit is refused if the result would fail the check
- **AC-14** A refused edit changes nothing
  WHEN any `TreeEditor` edit would leave the tree failing its check
  THEN the edit returns the error and the editor's recipes are unchanged
- **AC-15** Unrewritable references
  WHEN a rename meets a reference it cannot rewrite exactly (a query that does not parse, or a
  key built at run time)
  THEN the rename is refused naming the recipe, and nothing is changed
- **AC-16** Node names
  WHEN nodes are named
  THEN a node's name is its filename without the extension (`wheel` for `wheel.glb`); two nodes
  with one name and different extensions (`logo.png`, `logo.jpg`) are allowed while nothing
  depends on either, and the check fails naming both when something depends on one of them
- **AC-17** Node argument type
  WHEN a command declares an argument of type Node, with its extension(s) and its link form
  (`resource`: `-R/./<name>.<ext>`, or `key`: `-R-key/./<name>.<ext>`)
  THEN its metadata carries this through registration, the command registry export and
  `liquers-validate`, and the command receives the linked asset (resource form) or the key
  (key form)
- **AC-18** Node choices in the editor
  WHEN a Node argument is edited in a recipe of a tree folder
  THEN the editor offers the folder's node names with a matching extension, writes the link in
  the declared form, and shows an existing link as its node name
- **AC-19** Single result
  WHEN `result: car.glb` is declared
  THEN `car.glb` is the folder's only root, the check fails if it is not a node, and the tree's
  API returns the result's key, so the folder can be evaluated (and later serialized) as that one
  asset

Non-goals: the archive export and import (`RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE`; this
design supplies its strict-mode guarantee and its result); dependency modes other than `strict`,
such as declared shared folders; nodes in subfolders, whose name would be a relative key
(`parts/wheel`), and folder layouts like zip-based document formats (docx); enforcing the check at
evaluation (Q10); a new recipe kind or recipe trait
(`composite-command-definition/recipe-and-pipeline-structures.md` §4: nodes are plain recipes).

**Known limit.** Strict mode is a static guarantee over what a recipe declares: its query, links,
pipeline elements and its commands' default links, as plan dependency analysis sees them. A command
that builds a key at run time and reads it is not seen; Phase 2 decides whether commands can declare
that, and files it otherwise.

## Core Interactions

- **Recipes:** `RecipeList` gains an optional `tree` declaration; `Recipe` is unchanged.
- **Plans:** the strict check uses the plan's dependency analysis, so command default links count.
- **Pipelines:** references are read and rewritten by converting queries to pipelines and back
  (`PipelineEditor`, `QueryEditor`), which needs the command metadata registry.
- **Commands:** a Node argument type in `ArgumentType` / `ArgumentInfo`, the `register_command!`
  DSL and the registry export.
- **Validation:** `validate_recipes` and the `liquers-validate --recipes` report gain the check.
- **UI:** the query, action and recipe editors offer node choices for a Node argument.
- **Store:** read only, to list the folder's stored files (nodes without recipes).

## Crate Placement

- `liquers-core`: the declaration in `recipes.rs`; the graph, check and `TreeEditor` in a new
  `recipe_tree` module; the Node argument type in `command_metadata.rs`; the report in `validate/`.
  Core users (`liquers-web`, `liquers-py`) can expose it later without `liquers-lib`.
- `liquers-macro`: the Node argument in the DSL.
- `liquers-lib`: the node choices in `ui/editor/` and its egui and browser renderers; the registry
  exporter writes Node arguments.

## Language Integration

- Integration guide: no change now. A feature for recipe trees and the Node argument type (or an
  extension of `PIPELINE` and `COMMAND`) comes with the bindings.
- Python (`liquers-py`): later —
  [`RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS`](../../issues/RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS.md),
  which also covers declaring a Node argument on a Python command. Trees evaluate from Python
  unchanged.
- JavaScript (`liquers-web`): later — same issue, after `PIPELINES-NOT-IN-LANGUAGE-BINDINGS`.

## Documentation Intent

- Reference: new `specs/reference/RECIPE_TREES.md` (declaration, names, strict mode, the check,
  result, `TreeEditor`).
- Guide: none; the reference carries a worked example. A guide is added if the bindings follow.
- Other documents: none.
- Documents to update: `specs/reference/RECIPES_AND_PLANS.md` (the `tree:` section),
  `specs/reference/PIPELINES.md` §8 (`TreeEditor`), `specs/reference/REGISTER_COMMAND_FSD.md` and
  `specs/guides/COMMAND_REGISTRATION_GUIDE.md` (Node arguments), `specs/reference/UI_INTERFACE_FSD.md`
  (node choices), `specs/README.md`.
- API docs: `liquers-core/src/recipe_tree.rs` (new), `liquers-core/src/recipes.rs`,
  `liquers-core/src/command_metadata.rs`, `liquers-core/src/validate/mod.rs`, and the
  `liquers-lib/src/ui/editor/` module that gains node choices.
- User guides: `docs/guides/rust/07-assets.md` gains a "recipe trees" section and
  `docs/guides/rust/05-commands.md` the Node argument; Python and web guides none, since the
  feature is not available there yet (at most a note).

## Decisions (2026-11-23)

The maintainer answered the first eight questions:

1. **Archive:** a separate design, but this one supplies its basis: a validated dependency profile.
   The first profile is `strict` (AC-7, AC-10).
2. **Declaration:** a `tree:` section in `recipes.yaml`, which holds further structures: roots,
   result, shape, dependency mode.
3. **Identity:** the filename. The main name is the filename without its extension (AC-16); the
   full filename also identifies a node, and a relative key will once subfolders are supported.
4. **References:** relative keys, plus a Node argument type that says which extension and which
   link form an argument takes, so the editor offers node names (AC-17, AC-18). A rename finds and
   replaces every reference without changing the recipes' meaning.
5. **Roots:** declared, with a default (AC-3).
6. **Shape:** DAG; a DAG with a single result can stand for one asset (AC-19).
7. **Stored files** are nodes.
8. **Flat folder** for now; richer layouts later.

## Open Questions

9. **How a Node argument is written in a query.** As the full link (`attach-~X~-R/./wheel.glb~E`),
   or as the bare name (`attach-wheel`) that the planner expands into the link using the argument's
   type? *Recommended: the full link.* The recipe then means the same with or without the metadata,
   every existing tool (planner, dependency analysis, `liquers-validate`) sees the dependency, and
   the editor still shows `wheel`.
10. **Where the guarantee is enforced.** *Recommended: in `liquers-validate` and in every
    `TreeEditor` edit*, with evaluation unchanged. A recipe provider that refuses to serve a folder
    failing its check can be added later if hand-edited folders prove a problem.
11. **"If in doubt, only png".** My reading: a Node argument may list several extensions in order
    of preference (`png`, `jpg`), the editor offers a name once, and when both `logo.png` and
    `logo.jpg` exist and nothing depends on them, a new link uses the first extension that exists.
    A dependency on either still makes the pair an error (AC-16). Is that right?
12. **Node choices in both renderers.** *Recommended: yes*, in the editor model with the egui and
    browser renderers together, since they share the editor elements; a node list is a choice list.

## Design Dependencies

- `composite-command-definition` (complete) — **requires**: recipe pipelines, `PipelineEditor`,
  `QueryEditor` and the query–pipeline conversions are what references are read and rewritten
  with; its Phase 1 lists this issue as required-by.
- `computed-enum-alternatives` (in review) — **overlaps**: a Node argument's choices are
  alternatives computed at run time from the folder. Phase 2 decides whether node choices use its
  provider mechanism or stay specific to the editor.
- `query-pipeline-editor`, `query-editor-ergonomics` (complete) — **requires**: the editors that
  gain node choices.
- `RECIPE-CIRCULAR-DEPENDENCY-FIELDS-ARE-DEAD` — **overlaps** (weak): cycles are reported by the
  check, not through those recipe fields; this design neither reads nor sets them.
- `TOP-LEVEL-RELATIVE-RESOURCE-QUERY-LOOKS-UP-A-RELATIVE-RECIPES-KEY` — **related**: inside a
  provider recipe the `cwd` is always set, so it does not affect trees, but AC-10's test must
  evaluate through the provider.
- `VALIDATE-CANNOT-SEE-NON-STANDARD-RECIPE-PROVIDERS` — **related**: the check covers
  `recipes.yaml` folders (`DefaultRecipeProvider`) only.
- `RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE`, `RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS` —
  **required-by**: follow-ups filed with this design.

## References

- Issue: [`RECIPES-CANNOT-DESCRIBE-A-SELF-CONTAINED-ASSET-TREE`](../../issues/RECIPES-CANNOT-DESCRIBE-A-SELF-CONTAINED-ASSET-TREE.md)
- [`composite-command-definition/recipe-and-pipeline-structures.md`](../composite-command-definition/recipe-and-pipeline-structures.md)
- `specs/reference/RECIPES_AND_PLANS.md`, `specs/reference/PIPELINES.md`
