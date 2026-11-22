# Phase 1: High-Level Design — Self-contained recipe trees

## Purpose

Let a folder of recipes be **one structured document**: a tree (or DAG) of named nodes, each an
asset built from the nodes it references, with declared roots, a checked shape, references that
stay inside the folder, and an editor that renames, adds, moves and deletes nodes while keeping
every reference correct. Applications that edit scene graphs, CAD node trees or multi-part reports
can then keep Liquers recipes as their source of truth and keep caching, dependency tracking and
recomputation (`specs/reference/VISION.md`, "the transformation *is* the URL") instead of
generating recipes from a private format.

## Problem Example

A car model lives in `scenes/car/`. Each part is a recipe; `rubber.mat` is a shared material in
`/materials`, and `body.glb` and `tyre.glb` are stored files (`Source` assets).

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
- Nothing says `car.glb` is the result; a tool must guess.
- `-R/materials/rubber.mat` is an absolute key. Copy the folder to `scenes/truck/` and it still
  works, but a reference to `-R/scenes/car/body.glb` would silently keep pointing at the old car.
- A cycle (`wheel.glb` referencing `car.glb`) is found only when evaluation fails.

With this design the folder declares itself a tree:

```yaml
tree:
  roots: [car.glb]
  shape: dag            # tree | dag; several roots make a forest
  shared: [/materials]  # folders outside the tree that nodes may read
recipes:
  - …as above…
```

`liquers-validate --recipes scenes/car/recipes.yaml --cwd scenes/car` reports the graph
(`car.glb → body.glb, wheel.glb`; `wheel.glb → tyre.glb, /materials/rubber.mat`) and refuses
cycles, escapes and dangling references. `TreeEditor::rename("wheel.glb", "front_wheel.glb")`
rewrites the filename and the link in `car.glb` in one checked edit.

## Scope and Acceptance Criteria

- **AC-1** Rename rewrites every reference
  WHEN a node is renamed through the `TreeEditor`
  THEN its filename and every reference to it in the folder (in a query, a link or a pipeline
  element) are rewritten, the tree still checks, and the root evaluates to the same value
- **AC-2** Declaration is optional and preserved
  WHEN a `recipes.yaml` with a `tree:` section is read and written back
  THEN the section is kept unchanged; a `recipes.yaml` without one reads, plans and evaluates
  exactly as before
- **AC-3** Roots are explicit
  WHEN `roots` is missing, empty, or names something that is not a node of the folder
  THEN the tree check fails naming the root
- **AC-4** Cycles are refused
  WHEN node references form a cycle
  THEN the tree check fails naming the nodes of the cycle in order
- **AC-5** Shape is checked
  WHEN `shape: tree` and a node is referenced by two parents (or by two roots)
  THEN the check fails naming the node and both parents; under `shape: dag` it passes
- **AC-6** Orphans are reported
  WHEN a recipe or stored file of the folder is reachable from no root
  THEN the check reports it as a warning, not an error
- **AC-7** Containment
  WHEN a node references a key outside the folder that is not under a `shared` folder (an
  absolute key, or `../` leaving the folder)
  THEN the check fails naming the node and the key; a key under a `shared` folder passes and is
  not a node
- **AC-8** Dangling references
  WHEN a reference names a key in the folder that is neither a recipe nor a stored file
  THEN the check fails naming the node and the missing key
- **AC-9** Validation tool
  WHEN `liquers-validate --recipes` reads a `recipes.yaml` with a `tree:` section
  THEN its report adds the nodes, their edges and the tree check's errors and warnings, with
  exit code 1 on an error
- **AC-10** Relocatable
  WHEN a checked tree folder is copied to another key of the store
  THEN its root evaluates there to the same value, with every dependency inside the new folder or
  a `shared` one
- **AC-11** Add a child
  WHEN `TreeEditor::add_child(parent, recipe, argument)` is called
  THEN the recipe is added and referenced from `parent`; a filename already used in the folder is
  refused
- **AC-12** Delete
  WHEN a referenced node is deleted
  THEN the edit is refused naming its parents, unless cascading is asked for, which also removes
  the descendants no remaining node references
- **AC-13** Move (reparent)
  WHEN a reference to a node is moved from one parent to another
  THEN both recipes are rewritten, and the edit is refused if the result would break the shape
  (a cycle, or a second parent under `shape: tree`)
- **AC-14** A refused edit changes nothing
  WHEN any `TreeEditor` edit would leave the tree failing its check
  THEN the edit returns the error and the editor's recipes are unchanged
- **AC-15** Unrewritable references
  WHEN a rename meets a reference it cannot rewrite exactly (a query that does not parse, or a
  key built at run time)
  THEN the rename is refused naming the recipe, and nothing is changed

Non-goals: the archive export and import (split out as
[`RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE`](../../issues/RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE.md),
pending Q1); a GUI tree editor (it builds on `TreeEditor` later, as the query editors built on
`PipelineEditor`); enforcing the tree at evaluation (evaluation is unchanged; the check runs in
validation and in the editor); trees spanning subfolders (Q8); a new recipe kind or recipe trait
(`composite-command-definition/recipe-and-pipeline-structures.md` §4: nodes are plain recipes).

## Core Interactions

- **Recipes:** `RecipeList` gains an optional `tree` declaration; `Recipe` is unchanged.
- **Pipelines:** references are read and rewritten by converting queries to pipelines and back
  (`PipelineEditor`, `QueryEditor`); this needs the command metadata registry, as those editors do.
- **Validation:** `validate_recipes` and the `liquers-validate --recipes` report gain the tree check.
- **Store:** read only, to list the folder's stored files (leaf nodes) for AC-6 and AC-8.
- **Assets, Query parsing, Commands:** not changed.

## Crate Placement

`liquers-core` only: the declaration beside `RecipeList` in `recipes.rs`, the reference graph,
check and `TreeEditor` in a new `recipe_tree` module, the report in `validate/`. Everything it needs
(recipes, pipelines, their editors, the registry snapshot, the store listing) is in core, and core
users such as `liquers-web` and `liquers-py` can later expose it without `liquers-lib`.

## Language Integration

- Integration guide: no change now. A feature for recipe trees (or an extension of `PIPELINE`)
  comes with the bindings.
- Python (`liquers-py`): later —
  [`RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS`](../../issues/RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS.md).
  Trees evaluate from Python unchanged; only checking and editing are missing.
- JavaScript (`liquers-web`): later — same issue, and after `PIPELINES-NOT-IN-LANGUAGE-BINDINGS`,
  since the editor builds on `PipelineEditor`.

## Documentation Intent

- Reference: new `specs/reference/RECIPE_TREES.md` (declaration, node and reference rules, the
  check, `TreeEditor`).
- Guide: none; the reference carries a worked example. A guide is added if the bindings follow.
- Other documents: none.
- Documents to update: `specs/reference/RECIPES_AND_PLANS.md` (the `tree:` section of
  `recipes.yaml`), `specs/reference/PIPELINES.md` §8 (`TreeEditor` beside the other editors),
  `specs/README.md`.
- API docs: `liquers-core/src/recipe_tree.rs` (new), `liquers-core/src/recipes.rs` and
  `liquers-core/src/validate/mod.rs` (the tree section and the check).
- User guides: `docs/guides/rust/07-assets.md` gains a short "recipe trees" section; Python and
  web guides none, since the feature is not available there yet (at most a note).

## Open Questions

Each has a recommended answer; Phase 2 assumes it unless you choose otherwise.

1. **Scope: archive.** Keep the portable archive (export with `Source` / `Override` data and
   metadata, import elsewhere) in this design, or split it out? *Recommended: split out* as
   `RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE`. It is store work (export and import of data with
   metadata) that other features would share, and with it this design would not fit its size limit.
2. **Where the declaration lives.** A top-level `tree:` section in `recipes.yaml`, or a separate
   `recipes.tree.yaml`? *Recommended: the section.* One file stays one unit (copied, edited and
   versioned together), and an older reader still evaluates every node correctly, because it only
   ignores the check. A separate file can drift from the recipes it describes.
3. **Node identity.** The node's name is its filename (`wheel.glb`), or a slug derived from its
   title, or a separate id? *Recommended: the filename*, as recipes are addressed today. Titles stay
   free text but must be unique within a tree (an error in the check). `TreeEditor` offers a
   helper that proposes a filename from a title (a slug plus the format's extension), never
   applied silently.
4. **How nodes reference each other.** Any relative resource key (`-R/./wheel.glb` in a query or
   link, `{resource: ./wheel.glb}` in a pipeline), rewritten by converting the query to a pipeline,
   editing it and converting back; or a new `{node: wheel}` element? *Recommended: relative keys*,
   with no new syntax. Every existing recipe is already a valid node, and AC-15 covers the
   references that cannot be rewritten.
5. **Roots.** Must `roots` be declared, or is the unique node that nothing references inferred?
   *Recommended: declared.* Inference changes silently when a node is added; AC-6 points at
   anything unreachable.
6. **Default shape.** *Recommended: `dag`*, since shared sub-nodes (instancing a wheel four times)
   are the common case in scene graphs; `tree` is opt-in.
7. **Stored files as nodes.** Is a stored file of the folder with no recipe (`body.glb`) a leaf
   node? *Recommended: yes*: it is checked to exist (AC-8), counted for orphans (AC-6), and renamed
   with its references (a store rename of the data).
8. **Subfolders.** Is a tree one flat folder, or may nodes live in subfolders (`./parts/wheel.glb`)?
   *Recommended: flat for now.* A subfolder may be listed in `shared`; nested trees (a subfolder
   with its own `tree:`) are a later extension, filed if wanted.

## Design Dependencies

- `composite-command-definition` (complete) — **requires**: recipe pipelines, `PipelineEditor`,
  `QueryEditor` and the query–pipeline conversions are what references are read and rewritten
  with; its Phase 1 lists this issue as required-by.
- `RECIPE-CIRCULAR-DEPENDENCY-FIELDS-ARE-DEAD` — **overlaps** (weak): cycles are reported by the tree
  check, not through those recipe fields; this design neither reads nor sets them.
- `TOP-LEVEL-RELATIVE-RESOURCE-QUERY-LOOKS-UP-A-RELATIVE-RECIPES-KEY` — **related**: node
  references are relative keys; inside a provider recipe the `cwd` is always set, so the issue
  does not affect trees, but AC-10's test must evaluate through the provider.
- `VALIDATE-CANNOT-SEE-NON-STANDARD-RECIPE-PROVIDERS` — **related**: the check covers
  `recipes.yaml` folders (`DefaultRecipeProvider`) only.
- `RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE`, `RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS` —
  **required-by**: follow-ups filed with this design.

## References

- Issue: [`RECIPES-CANNOT-DESCRIBE-A-SELF-CONTAINED-ASSET-TREE`](../../issues/RECIPES-CANNOT-DESCRIBE-A-SELF-CONTAINED-ASSET-TREE.md)
- [`composite-command-definition/recipe-and-pipeline-structures.md`](../composite-command-definition/recipe-and-pipeline-structures.md)
- `specs/reference/RECIPES_AND_PLANS.md`, `specs/reference/PIPELINES.md`
