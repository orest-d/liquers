---
id: CROSS-PROCESS-RELOAD-IS-UNTESTED
kind: issue
title: No test exercises reloading a persisted dependent in a fresh environment
status: closed
priority: P2
complexity: M
area: [core/assets, build]
design: stale-dependency-status-finalization
created: 2026-09-06
github:
---

## Resolution (2026-09-15)

**Closed — resolved by `design/stale-dependency-status-finalization/`**
(PR [#71](https://github.com/orest-d/liquers/pull/71)). All three named tests exist, in
`liquers-core/tests/keyed_version_cascade.rs`:

| | Test | What it establishes |
|---|---|---|
| R1 | `reloaded_dependent_is_served_without_audit` | A process that has never evaluated the dependency still serves the stored dependent. Nothing audits by default, across a restart. |
| R2 | `explicit_audit_expires_a_reloaded_dependent_whose_dependency_vanished` | An explicit audit *does* expire it when the dependency can no longer be shown to reconstruct. The only test in the suite in which an audit expires anything. |
| R3 | `a_pre_versions_record_with_version_zero_still_matches` | A record written before computed assets carried versions still matches, so upgrading does not invalidate an existing store on first contact. |

They are joined by F0–F4, which cover `try_fast_track` across the same boundary, including the
baseline nobody had written: **no test asserted that the fast track can succeed at all**, only that
it can refuse.

### Why the shared-store fixture was not built

§Problem proposes a wrapper forwarding `AsyncStore` to a shared inner store, so two environments can
be pointed at one store. That fixture was **deliberately not built**, and the reason is
architectural rather than one of effort — it is recorded here so the wrapper is not reintroduced
later for the wrong reason.

> The asset manager has been designed as the main way to assure synchronization. The store is not
> equipped for that.

Two environments over one live store is not a configuration the system supports, so a fixture for it
would exercise a coordination point that does not exist, and tests written on it would be asserting
behaviour nobody has designed. This is now stated in `reference/ASSETS.md` §Who decides status.

**Re-hydration is also the more faithful mechanism.** A second process does not share a live store
object — it reads persisted bytes. `fixtures::StoreSnapshot` captures selected entries out of one
store and replays them into a fresh one behind a second environment, after the first is dropped, so
the new manager and dependency manager start genuinely empty. It is the first Rust module in
`liquers-core/tests/fixtures/`, and it carries `absorb` (entries captured at different moments) and
`downgrade_dependency_versions_to_unknown` (the R3 deployment case).

### The wrapper warning stands

A small wrapper *was* still needed, for F4's assertion that a live dependency is answered from the
manager without a store read. `fixtures::CountingStore` confirms §Problem's warning: `AsyncStore`'s
defaults are **error stubs, not forwarding defaults**, so a wrapper must be sized by compiling. That
rule is now in `guides/STORE_IMPLEMENTATION_GUIDE.md` §1 and
`guides/UNITTEST_GUIDE.md` §Testing Assets.

### Promotion

This issue's own condition — "`guides/UNITTEST_GUIDE.md` if it recurs a third time" — was met, and
the promotion happened: `guides/UNITTEST_GUIDE.md` §Testing Assets now carries the re-hydration
pattern along with the other asset-testing rules that cost a debugging session each.

### Found on the way

R2 did not ship in the form planned. A dependency whose stored version has *moved* is not expired by
an audit, because `register_version` compares against a version the manager previously held and a
fresh process holds none. Filed as `AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION`, with the
withdrawn test as its reproduction. R2 shipped covering the `report_no_version` outcome, which has
no such gate.

Note on terminology used below: §Problem writes "sidecar" where it means the metadata a store holds
for a key. `reference/STORE_SEMANTICS.md` §8 reserves that word for a specific companion-key layout,
which a memory store does not have.

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
