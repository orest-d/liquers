---
id: EXPRESSION-ATTRIBUTION-MAPS-ARE-NEVER-CLEANED
kind: issue
title: Schedule-time expression attribution entries are never removed outside tests
status: draft
priority: P3
complexity: S
area: [core/assets]
created: 2026-10-18
github:
---

## Problem

`DependencyManager` keeps three schedule-time maps for non-keyed (expression) dependents:
`expression_dependents`, `expression_keyed_deps` and `expression_expr_deps`
(`liquers-core/src/dependencies.rs`). `design/dependency-scheduling/` specifies a
`remove_expression` cleanup "at expression terminal status", but `remove_expression` is
`#[cfg(test)]` and nothing in library code calls it. The entries therefore live for the whole life
of the manager: memory grows with every distinct expression query evaluated, and a keyed asset
stays in an expression's attribution set long after the evaluation that put it there, so a later
evaluation of the same expression adds edges to it (the reason
`replacing-write-drops-stale-dependency-edges` had to remove a replaced key from these sets).

## Expected

An expression's attribution entries are dropped when the expression asset reaches a terminal
status, as the dependency-scheduling design says, or the maps are otherwise bounded.
