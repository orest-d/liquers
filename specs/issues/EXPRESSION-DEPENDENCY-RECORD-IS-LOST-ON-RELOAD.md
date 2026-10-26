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

## Triage (2026-10-26, bulk design, proposed close)

Not reproducible by reading HEAD (`cd6d5da`), and already not at the commit the issue cites:
`Context::wait_for_dependency_recording` (`liquers-core/src/context.rs`) **inherits a non-keyed
dependency's own records** into the waiting asset ("A non-keyed dependency (a cut predecessor, a
link, a query a command evaluates) … records its run added … are inherited here"), added in
`91f1810` (2026-10-17, `search-read-dependencies`), an ancestor of `18732dd`. So in the Problem
Example, `k`'s stored records contain `-R/data/x.txt` (from `q`'s records) as well as `q`'s query
key, and a reload's `load_from_records` restores the edge `x → k`. Unit evidence:
`context::tests::submit_records_dependency_under_the_key_wait_uses` (inherited records present).

Residual, by contract rather than a defect: a command that waits through `AssetRef::get()` instead
of `Context::wait_for_dependency` bypasses both the stale-dependency policy and the inheritance;
`Context::evaluate`'s doc says to wait through `wait_for_dependency`. A regression test at close
(`expression_dependency_survives_reload`: evaluate `k`, fresh environment over the same store,
`set_binary(data/x.txt)`, `k` expired) would pin it. No design created; status left to the
maintainer.
