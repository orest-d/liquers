---
id: ASSET-REMOVE-FORGETS-DEPENDENTS
kind: issue
title: AssetManager::remove drops a key's dependency edges without expiring its dependents
status: draft
priority: P2
complexity: M
area: [core/assets]
design: 
created: 2026-09-27
github:
---
## Problem

`DefaultAssetManager::remove` (`liquers-core/src/assets.rs`) cancels and unmaps the live asset,
deletes the stored value, and calls `DependencyManager::remove(&dep_key)`
(`liquers-core/src/dependencies.rs`), which drops the key's `versions`, `keyed_dependents` and
`dependent_assets` entries. Nothing is expired. The same applies to the queued manager.

So after `remove(key)`:

- assets computed from `key` stay `Ready`, although the value they were built from is gone — for a
  `Source` key it cannot come back, and for an `Override` key the recipe will produce a different
  value;
- the edges to those dependents are forgotten, so when `key` is later recomputed or set again,
  `register_version` has no dependents to cascade to. Whether a dependent is ever caught depends on
  it being reloaded from the store and its stored `DependencyRecord`s being re-checked.

`reference/ASSETS.md` ("Remove Semantics (RESOLVED)") specifies `remove` as "always delete, same
for all statuses", and says nothing about dependents.

## Impact

Silent staleness: a dependent keeps serving a value derived from data that was deleted or
replaced. There is no error to notice. Anyone deleting a `Source` or `Override` asset through
code (and, once `specs/design/axum-assets-endpoints/` ships, through HTTP) is exposed.

## Expected behaviour

Removal that changes a value's identity cascades like any other version change. The design in
`specs/design/axum-assets-endpoints/` splits the operation: **delete** (a user-supplied `Source` or
`Override` value; cascades) and **evict** (a recomputable cached value; keeps the version, so it
does not cascade). Either `remove` adopts one of those semantics, or it is deprecated in favour of
the two. Whichever is chosen, `ASSETS.md` should state what happens to dependents.

## Discovery

Reading `remove` and `DependencyManager::remove` while resolving Q9 of
`specs/design/axum-assets-endpoints/phase1-high-level-design.md`, 2026-09-27.
