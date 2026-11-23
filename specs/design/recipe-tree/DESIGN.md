---
id: RECIPE-TREE
kind: design
title: Self-contained recipe trees - a recipes folder as one checked, editable tree of assets
workflow: liquers-project
status: draft
phase: high-level
priority: P2
complexity: XL
area: [core/assets, core/validate, core/commands, macro, lib/ui]
issues: [RECIPES-CANNOT-DESCRIBE-A-SELF-CONTAINED-ASSET-TREE]
affects_docs: [specs/reference/RECIPE_TREES.md, specs/reference/RECIPES_AND_PLANS.md, specs/reference/PIPELINES.md, specs/reference/REGISTER_COMMAND_FSD.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md, specs/reference/UI_INTERFACE_FSD.md, docs/guides/rust/07-assets.md, docs/guides/rust/05-commands.md]
created: 2026-11-22
---
# Self-contained recipe trees

A `recipes.yaml` folder declared as a tree with a `tree:` section (roots or a single result, shape,
dependency mode). Its nodes are the folder's recipes and stored files, named by filename without
extension, and they reference each other by relative keys. A check refuses cycles, a wrong shape,
dangling references and, in `strict` mode, any dependency outside the folder, so a strict folder
works wherever it is placed. A Node argument type lets commands take a node by its bare name, which the plan turns into a
relative link (any link outside a strict folder), and the editors offer node names for it. A `TreeEditor` on `PipelineEditor` adds, renames, moves and deletes
nodes, rewriting every reference and re-checking after each edit. Evaluation is unchanged. The
portable archive is a separate design built on this one
(`RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE`).

- **2026-11-22** Phase 1 written; eight questions put to the maintainer.
- **2026-11-23** Maintainer answers: archive separate but based on a validated `strict` dependency
  profile; `tree:` section; node name = filename without extension; Node argument type; default
  roots; single result; stored files are nodes; flat folder. AC-3, AC-7, AC-10 revised, AC-16 to
  AC-19 added, questions 9-12 opened.
- **2026-11-23, later** Answers 9-12: a Node argument is written as the bare name and resolved to its
  link at planning; a Node is always local, in any query; one extension per Node, with an optional
  post-processing query; strict mode forbids non-Node links and is enforced at planning. AC-17,
  AC-18 revised, AC-20 to AC-24 added, questions 13-14 opened.
- **2026-11-23, evening** Revised: a Node's meaning depends on strictness (any link outside a strict
  folder, a bare name needing a working directory); strict mode allows no link arguments at all;
  post-processing out of scope (AC-22 retired). AC-20, AC-21, AC-23 revised. No open questions.
- **2026-11-23, later** Command default links and pipelines and enum-alias links are trusted command
  logic, allowed in strict recipes and not checked; only links the recipe writes are refused.
  AC-7, AC-20, AC-23 clarified.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
