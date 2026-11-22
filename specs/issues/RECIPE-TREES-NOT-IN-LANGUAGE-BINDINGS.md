---
id: RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS
kind: feature
title: Python and JavaScript cannot validate or edit a recipe tree
status: draft
priority: P3
complexity: M
area: [py, web]
design:
created: 2026-11-22
github:
---
## Problem

The `recipe-tree` design gives Rust a tree declaration in `recipes.yaml`, a shape and containment
check, and a `TreeEditor` in `liquers-core`. Neither `liquers-py` nor `liquers-web` exposes them,
and `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` has no feature for them (the nearest are `RECIPE`
and `PIPELINE`).

## Impact

A Python or JavaScript application that edits a structured document as a recipe tree cannot check
it or rename a node with its references rewritten; it has to edit `recipes.yaml` text. A tree
written in Rust still evaluates from either language, because evaluation does not change.

## Expected behaviour

The language integration guide gains a feature (or extends `PIPELINE`) for recipe trees: checking a
folder's tree and its report, and the `TreeEditor` operations. Each binding implements it with the
conformance tests that feature prescribes.

## Discovery

Recorded as **later** in `recipe-tree` Phase 1 `## Language Integration` (2026-11-22). Depends on
`PIPELINES-NOT-IN-LANGUAGE-BINDINGS`, since the editor builds on `PipelineEditor`.
