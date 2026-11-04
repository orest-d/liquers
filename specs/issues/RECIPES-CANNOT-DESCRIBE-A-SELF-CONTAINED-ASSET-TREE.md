---
id: RECIPES-CANNOT-DESCRIBE-A-SELF-CONTAINED-ASSET-TREE
kind: feature
title: A recipes folder cannot be used as a self-contained, editable tree of assets
status: draft
priority: P2
complexity: XL
area: [core/assets, core/store, core/validate]
design:
created: 2026-11-02
github:
---

## Problem

Recipes could serve as a **data format**: a folder of named nodes, each one an asset defined by a
recipe and built from the nodes it references, such as a scene graph, a CAD model or a report
assembled from parts. Today a `recipes.yaml` (`liquers-core/src/recipes.rs`, `RecipeList`) is a flat
list of independent recipes. Nothing in it lets a tool treat the folder as one structure:

- **References are query text.** A node refers to another through a link or resource inside its
  query string (`-R/./wheel.glb`). Renaming a node means finding and rewriting text inside other
  queries, which no API does.
- **No identity beyond the filename.** A recipe is addressed by its query's filename. Titles are
  free text and may repeat. Nothing derives a filename from a title or keeps the two in step.
- **No root.** When the folder describes one result, nothing says which asset it is.
- **No shape check.** Nothing validates that the references form a tree (or a forest, or a DAG),
  that there are no cycles, or that every node is reachable from a root.
- **No containment.** A recipe can depend on any key in the store, so a folder cannot be moved,
  copied or shared and still mean the same thing.
- **No portable form.** There is no way to export the folder, with the `Source` and `Override`
  assets it needs (and, optionally, its generated assets), as one archive, or to import it
  elsewhere.

## Impact

Applications that edit structured documents (scene graphs, CAD node trees, multi-part reports)
must keep the structure in their own format and generate recipes from it. They lose Liquers'
caching, dependency tracking and recomputation as the source of truth, or they reimplement them.

## Expected behaviour

What it would take, from the maintainer's outline (2026-11-02). Options are listed; this issue does
not choose.

1. **Structured references.** Nodes reference each other by name through recipe pipelines
   (`design/composite-command-definition/`): `resource`, `link` and nested `pipeline` sources are
   structured, so a rename walks the pipelines instead of editing text. This is the main
   dependency on that design.
2. **Naming.** All nodes in one folder, referenced by name. Options: enforce unique titles; derive
   the filename from the title (a slug, with the extension from the recipe's format); refuse or
   suffix on collision. A rename changes title, filename and every reference in one edit.
3. **Roots.** The single output must be unambiguous. Options: an explicit `root` (or `roots`) in
   the folder's declaration; the first recipe; the unique node nothing depends on. A forest
   declares several roots, and each tree can be edited on its own.
4. **Shape validation.** Acyclic; every node reachable from a root; tree (each node has one
   parent), DAG (shared sub-nodes, as scene-graph instancing needs), or forest, as declared. The
   plan dependency analysis already walks recipe dependencies and could report the graph. This
   belongs in `liquers-validate`.
5. **Containment.** Dependencies only on `./` (the folder) or on declared shared folders (for
   example a materials library); absolute keys elsewhere are refused. All references are relative,
   so the folder is relocatable.
6. **`TreeEditor`.** It builds on `PipelineEditor` and supports adding a child, renaming (with
   references rewritten), moving or reparenting, deleting (refused while referenced, or cascading),
   and checking the shape after each edit.
7. **Portable archive.** A zip holding `recipes.yaml`, the declaration file, the data and metadata
   of every `Source` and `Override` asset, and optionally the generated assets. Import into any
   folder, which works because references are relative. This needs a store-level export and import
   of data with its metadata.
8. **Where the declaration lives.** It is a conventional `recipes.yaml` plus tree metadata (roots,
   shape, shared folders, naming policy). Options: a top-level section in `recipes.yaml` (`RecipeList`
   ignores unknown keys, so older readers would silently drop it), or a separate file. **It must not
   be named `*.manifest.yaml`**: `ManifestRecipeProvider` (`liquers-records/src/provider.rs`) claims
   every file with that suffix as a records manifest. Candidates are `recipes.tree.yaml` and
   `bundle.yaml`.

A design is needed (`XL`). It should start right after `composite-command-definition`, as the
maintainer asked on 2026-11-02.

## Discovery

The maintainer's review of `composite-command-definition` on 2026-11-02 ("Recipes as a data format …
investigate what would be required").
