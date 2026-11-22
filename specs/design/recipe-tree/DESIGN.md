---
id: RECIPE-TREE
kind: design
title: Self-contained recipe trees - a recipes folder as one checked, editable tree of assets
workflow: liquers-project
status: draft
phase: high-level
priority: P2
complexity: XL
area: [core/assets, core/validate]
issues: [RECIPES-CANNOT-DESCRIBE-A-SELF-CONTAINED-ASSET-TREE]
affects_docs: [specs/reference/RECIPE_TREES.md, specs/reference/RECIPES_AND_PLANS.md, specs/reference/PIPELINES.md, docs/guides/rust/07-assets.md]
created: 2026-11-22
---
# Self-contained recipe trees

A `recipes.yaml` folder declared as a tree with a `tree:` section (roots, shape, shared folders):
its nodes are the folder's recipes and stored files, they reference each other by relative keys,
and a check refuses cycles, a wrong shape, dangling references and keys outside the folder. A
`TreeEditor` on `PipelineEditor` adds, renames, moves and deletes nodes, rewriting every reference
and re-checking after each edit. Evaluation is unchanged. The portable archive is split out
(`RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE`, pending Q1).

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
