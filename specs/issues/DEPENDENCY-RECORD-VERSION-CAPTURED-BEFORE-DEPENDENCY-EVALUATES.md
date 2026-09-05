---
id: DEPENDENCY-RECORD-VERSION-CAPTURED-BEFORE-DEPENDENCY-EVALUATES
kind: issue
title: A dependency record captures the dependency's version before the dependency has been evaluated, so it is always unknown
status: draft
priority: P1
complexity: M
area: [core/assets]
design: keyed-expiry-cascade-fix
created: 2026-09-05
github:
---

## Problem

`Context::schedule_dependency_asset` (`liquers-core/src/context.rs:553`) reads the dependency's
version out of the dependency manager and writes it into the dependent's `DependencyRecord`:

```rust
let version = manager
    .dependency_manager()
    .get_version(&query_dep_key)
    .await
    .unwrap_or_else(Version::unknown);
…
self.add_dependency(DependencyRecord::new(query_dep_key, version)).await;   // :591
```

The read happens at **schedule** time — before the dependency asset is obtained
(`get_dependency_asset`, `:582`) and therefore before it has evaluated. A dependency that the
dependent's own evaluation causes to be computed is not in the manager's `versions` map at that
moment, because the only thing that puts it there is `track_asset`, which runs at the *end* of that
dependency's evaluation (`assets.rs:2560`).

So for the ordinary recipe chain the captured version is `Version::unknown()`, and nothing ever
revisits it: `wait_for_dependency` returns the dependency's `State` (`assets.rs:3597`, `:4793`) and
no caller updates the record afterwards. `AssetRef::record_dependency_on_asset` (`assets.rs:1532`)
*is* the function that reads a dependency's live metadata version, but its only non-test caller is
the delegation branch at `assets.rs:2407`, where it returns immediately as a same-node hand-off.

Measured on a three-link chain (`a.txt` ← `hello`, `b.txt` ← `-R/a.txt/-/world/b.txt`,
`c.txt` ← `-R/b.txt/-/world/c.txt`), every record — in memory and in the store — carries zero:

```
--- b.txt own version = None
      dep -R/a.txt                          version = 00000000000000000000000000000000
      dep -R-recipe/a.txt                   version = 00000000000000000000000000000000
stored b.txt deps:
      stored dep -R/a.txt                   version = 00000000000000000000000000000000
```

The version is real only when the dependency happened to be warm in the manager when the
dependent's plan was finalized — an accident of evaluation order, not a property of the dependency.

## Impact

Every mechanism that compares a dependent's *recorded* version against a dependency's *actual* one
is inert, because `Version(0)` short-circuits every comparison (`Version::matches`,
`metadata.rs:65`):

- `AssetRef::try_fast_track`'s stale-dependency check (`assets.rs:1119`) never rejects anything;
- `DependencyManager::add_dependency`'s consistency check (`dependencies.rs:241`) is skipped;
- `load_from_records` reconstructs edges but can never detect that a stored dependent is stale.

The practical consequence is that **cross-process staleness detection does not exist**: a dependent
persisted against a dependency that has since changed is reloaded and served as fresh. Within a
process the dependency graph still works, because edges are registered by
`register_scheduled_dependency` regardless of version and invalidation flows through the graph
rather than through recorded versions.

P1 rather than P0: no wrong value is computed, the in-process path is unaffected, and a restart is
required to observe it. But it makes a documented mechanism (`MetadataRecord.dependencies` carrying
"versions of dependencies observed when this asset was last evaluated", `metadata.rs:943`) mean
something other than what it says.

## Expected behaviour

A `DependencyRecord` should carry the version the dependency actually had when the dependent
consumed it. The natural point is after the wait completes and the dependency's `State` is in hand
— `wait_for_dependency`'s callers, or a shared helper — using the same "prefer a concrete version
over unknown" upsert rule `Context::add_dependency` already implements
(`specs/reference/DEPENDENCIES_STATUS.md`, function glossary).

`AssetRef::record_dependency_on_asset` already does exactly this and is currently reachable only
from a path that discards it. Whether to reuse it or to add the capture inside
`schedule_dependency_asset`'s completion path is the design question.

Note that fixing this **activates** the three mechanisms above, which is the point, but also means
a dependent is expired whenever its recorded dependency version no longer matches — including on
the first run after this lands, for every asset persisted before it. That transition wants
deliberate handling.

## Discovery

Found on 2026-09-05 by the final cross-document review of `keyed-expiry-cascade-fix`, which had
assumed persisted records carry concrete versions and built its cold-start reasoning on that. A
probe printing `metadata.get_dependencies()` over that design's own fixture showed every version
zero. See `PLAN-DEPENDENCY-RECORDS-HARDCODE-VERSION-ZERO` for the second, independent cause of the
same symptom.
