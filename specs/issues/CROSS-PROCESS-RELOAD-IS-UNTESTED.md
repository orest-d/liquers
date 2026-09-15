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

## Decision 2026-09-15 — re-hydration, not a shared store

Taken at the Phase 4 gate of `stale-dependency-status-finalization`, which adopts this issue's three
tests. Recorded here because it corrects the *mechanism* this issue proposed, and without the note
the wrapper would be reintroduced later for the reason this issue gives.

**The shared-store fixture is out of scope, and not only on cost.** The project owner:

> The asset manager has been designed as the main way to assure synchronization. The store is not
> equipped for that.

Two environments over one live store is therefore not a scenario the system supports, and a fixture
built to exercise it would be testing a coordination point that does not exist. A shared store may
have interesting uses later; covering *reload* is not one of them.

**Re-hydration is the more faithful mechanism, as well as the cheaper one.** A second process does
not share a live store object — it reads persisted bytes. Reading the bytes and metadata out of the
first store, dropping the first environment entirely, and `set`-ing them into a fresh
`AsyncMemoryStore` behind a second environment is what a restart actually looks like. The technique
is already proven inline in `test_get_any_status_and_to_override_from_store_only`
(`liquers-core/tests/expiration_integration.rs:1336`); what was missing was only that nobody had
lifted it out of that one test.

This issue's own condition for promotion — "`guides/UNITTEST_GUIDE.md` if it recurs a third time" —
is met, so the helper goes to `liquers-core/tests/fixtures/`, which today holds only data files and
gains its first Rust module.

**All three named tests are in scope**, including the `Version(0)` back-compat case this issue calls
the least-tested claim in the versions design — it is the only one whose failure would mean an
existing deployment invalidates everything on upgrade.

The wrapper-sizing warning in §Problem stands and has been carried into that design's Phase 3 and 4:
`AsyncStore`'s defaults are error stubs, not forwarding defaults, so any wrapper — including the
small counting store still needed for one test — must be sized by compiling.

## Discovery

Recorded on 2026-09-06 in Phase 5 of `keyed-expiry-cascade-fix`, as a deviation from its approved
Phase 3 rather than as something noticed in passing.
