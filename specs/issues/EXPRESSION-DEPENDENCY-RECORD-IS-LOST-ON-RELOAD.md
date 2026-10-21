---
id: EXPRESSION-DEPENDENCY-RECORD-IS-LOST-ON-RELOAD
kind: issue
title: A keyed asset that read a key only through a non-keyed expression is not expired by that key after a reload
status: draft
priority: P2
complexity: M
area: [core/assets]
design:
created: 2026-10-20
github:
---

## Problem

When a keyed asset `k` evaluates a non-keyed query `q` (for instance through `context.evaluate`),
`k`'s metadata records `q`'s `DependencyKey`, not the keys `q` read. In the running process the
link from a key `x` that `q` reads to `k` is the schedule-time attribution edge `x → k`
(`DependencyManager::propagate_attribution`, `liquers-core/src/dependencies.rs`). That edge is not
in any record. After a restart, or whenever `k` is loaded on the fast track,
`load_from_records` adds only the edge `q → k`. Expiry does not cascade through an expression key:
`expire_dependencies_result` expires a query asset without cascading, and nothing registers a
version for `q`. So a later write of `x` does not expire the reloaded `k`.

Inferred from reading the code at 18732dd. Not reproduced.

## Problem Example

`data/k.txt` = `uses_q`, a command that returns `context.evaluate("-R/data/x.txt/-/upper")`.
Evaluate `k.txt`, restart the process (a fresh environment over the same store), then
`set_binary(data/x.txt, ..)`. `get(data/k.txt)` serves the stored value built on the old `x`.

## Impact

Stale values after a restart, for keyed assets whose inputs are reached only through non-keyed
expressions. A dependency audit does not help, because it also works from the records.
Workaround: remove the stored copy of `k`.

## Expected behaviour

A reload rebuilds edges equivalent to the ones the run had. Two possible ways: record `q`'s keyed
dependencies (transitively) on `k` at completion, or make an expression record expand through
`q`'s stored records when loaded. Choosing between them is the design's task.

## Discovery

Found while designing `keyed-run-replaces-dependency-edges` (bulk design, 2026-10-20), when
checking which edges a fast-track reload restores after a run's edges are cleared.
