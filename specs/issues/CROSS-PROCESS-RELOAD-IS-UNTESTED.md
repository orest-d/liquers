---
id: CROSS-PROCESS-RELOAD-IS-UNTESTED
kind: issue
title: No test exercises reloading a persisted dependent in a fresh environment
status: draft
priority: P2
complexity: M
area: [core/assets, build]
design:
created: 2026-09-06
github:
---

## Problem

Nothing in the repository builds two environments over one store, so no test exercises what happens
when a persisted dependent is loaded by a process that has not seen its dependency. That is the
whole cross-process half of dependency versioning: `try_fast_track` reading a stored record,
`AssetManager::version` resolving a key from the sidecar rather than from a live asset, and
`trigger_dependency_audit` deciding a dependent is stale on evidence that survived a restart.

The obstacle is mechanical. `AsyncMemoryStore` is neither `Clone` nor shareable, and
`Environment::with_async_store(Box::new(store))` takes ownership, so a second environment cannot be
pointed at the first one's store. A wrapper forwarding `AsyncStore` to a shared inner store is the
obvious fixture, and the trait's *two required methods* is a misleading measure of its size: the
other twenty are defaulted, but the defaults are not forwarding defaults — `set`'s default is
`Err(key_not_supported)` — so a wrapper overriding only the required pair compiles and then fails
every write.

## Impact

Two designs have now wanted this fixture and neither built it:
`stale-dependency-status-finalization` designed a `SharedMemoryStore` for it, and
`keyed-expiry-cascade-fix` scoped its I8/I9 reload tests out for the same reason. The behaviour
they would have covered is exercised in-process — `nothing_audits_by_default` and
`metadata_kept_data_deleted_still_verifies_clean` cover the audit mechanism itself — so what is
missing is specifically the *reload* dimension: a stored record confronted with a dependency this
process has never evaluated.

P2: no known defect goes undetected today, the in-process paths are covered, and the cost is
confidence rather than correctness. But it is the dimension the persisted `DependencyRecord.version`
exists for, and it is currently taken on faith.

## Expected behaviour

A shared-store fixture usable from integration tests, sized by compiling rather than by counting
required methods, and probably in a place both `liquers-core` designs can reach — a `tests/fixtures`
module, or `guides/UNITTEST_GUIDE.md` if it recurs a third time.

With it, the tests worth writing are: a dependent reloaded before its dependency is served (nothing
audits by default); an explicit audit expires it when the dependency's stored version has changed;
and a record written before versions existed, carrying `Version(0)`, still matches — the property
that makes the change safe to deploy against an existing store, and currently the least-tested
claim in the design.

## Discovery

Recorded on 2026-09-06 in Phase 5 of `keyed-expiry-cascade-fix`, as a deviation from its approved
Phase 3 rather than as something noticed in passing.
