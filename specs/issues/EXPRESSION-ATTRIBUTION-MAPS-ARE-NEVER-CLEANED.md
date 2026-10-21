---
id: EXPRESSION-ATTRIBUTION-MAPS-ARE-NEVER-CLEANED
kind: issue
title: Schedule-time expression attribution entries are never removed outside tests
status: draft
priority: P3
complexity: M
area: [core/assets]
design: keyed-run-replaces-dependency-edges
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

## Problem Example

A keyed `k` evaluates the non-keyed `-R/data/x.txt/-/upper`. After that expression has expired and
`k` has re-run without it, the three maps still hold its entries, and `k` is still in its
attribution set, for the life of the process.

## Expected

An expression's attribution entries are dropped when the expression asset reaches a terminal
status, as the dependency-scheduling design says, or the maps are otherwise bounded.

## Triage (2026-10-20)

Reproduces at HEAD 18732dd (`remove_expression` is `#[cfg(test)]`; no library caller). The cleanup
point `dependency-scheduling` planned (the expression's terminal status) would break a cached `Ready`
expression's late joiners, because expiry does not cascade through an expression. The design applies
the cleanup only when the expression's value will not be served again. Designed with
`EVALUATION-KEEPS-DROPPED-DEPENDENCY-EDGES` in `design/keyed-run-replaces-dependency-edges/`.
Complexity re-sized from S to M (two files, lifecycle hooks).
