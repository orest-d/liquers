---
id: STORED-DERIVED-VALUES-ARE-NEVER-PRUNED
kind: feature
title: Stored derived values that nothing wants any more are never removed from the store
status: draft
priority: P3
complexity: L
area: [core/store, core/assets]
design:
created: 2026-10-21
github:
---

## Problem

A keyed asset computed from a recipe is written to the store and stays there for good. Nothing
removes a stored *derived* value (one that a recipe can recompute) once it has gone unused for a
long time, or once its recipe has been removed from `recipes.yaml`. `asset-garbage-collection`
bounds the in-memory maps and, deliberately, never touches the store. The store grows with every
distinct recipe result ever computed.

## Impact

Disk growth in long-running deployments that use many short-lived recipes. Workaround: delete
derived folders by hand (supported: a missing value is recomputed from its recipe), or keep
metadata and delete data (`STORE_SEMANTICS.md` §8).

## Expected behaviour

A pruning operation over a store subtree that removes recipe-backed values meeting a policy (last
access older than N, or recipe gone). It must never remove a `Source` or `Override`, and must
leave dependents consistent: a pruned key's dependents are not expired, because the value can be
recomputed with the same version, unless the recipe itself is gone. Open questions: where last
access is recorded (store metadata writes on read are costly), and whether pruning keeps metadata
(cheap re-verification) or removes it.

## Discovery

Split off on 2026-10-21 from `CORE-ASSET-GC` ("persists in the cache and in the store") during bulk
design (`asset-garbage-collection` Phase 1, open question 4).
