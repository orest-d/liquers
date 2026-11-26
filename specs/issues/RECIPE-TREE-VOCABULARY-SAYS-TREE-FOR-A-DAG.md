---
id: RECIPE-TREE-VOCABULARY-SAYS-TREE-FOR-A-DAG
kind: issue
title: The recipe-tree vocabulary says tree, roots and sub-nodes for what is a DAG
status: draft
priority: P3
complexity: S
area: [core/assets, lib/ui]
design: strict-tree-editing
created: 2026-11-26
github:
---
## Problem

`reference/RECIPE_TREES.md`, `TreeEditor`, `TreeDeclaration` and the recipes editor speak of a
*tree* with *roots*, but the default `shape` is `dag`: a node may have several parents and appear
in several places of the outline, and `roots` is only a display hint (design
`strict-tree-editing` AC-17). Raised by Orest on 2026-11-26 ("we need to clarify the tree
vocabulary - since it is now not a true tree, but a DAG"), to be done later.

## Expected behaviour

One glossary, in `RECIPE_TREES.md`, that the code, the editor's texts and the designs follow:
what a node, a root, the result and a dependency are in a DAG, and where "tree" still applies
(`shape: tree`, the outline view).

## Workaround

None needed; the terms are understood in context.
