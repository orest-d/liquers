---
id: ASSET-REGISTRATION-CONTRACT
kind: design
title: Ownership and registration of keyed assets are a stated contract
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets]
issues: [ASSET-REGISTRATION-OWNERSHIP-CONTRACT]
created: 2026-10-20
---
# Ownership and registration of keyed assets are a stated contract

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written, not
approved, nothing implemented. The source was filed `L`. Triage at HEAD found one of its four
points done and one better deferred, so it was re-sized to `M` before the design started, and the
compact form is used (see Scope Changes).

## Phase 1: High-Level Design

### Purpose

Make "who owns a key" and "what registration guarantees" a written contract that callers, external
managers and later features (`asset-garbage-collection`, `environment-orderly-shutdown`) can rely
on. Today the reference documents defer the contract to an issue, and the one property that makes
registration observable without a lock (monotonic registration) holds only by accident of the
built-in code.

### Problem Example

Verified at HEAD (`liquers-core/src/assets.rs`), on either built-in manager:

```rust
let m = envref.get_asset_manager();
let a = m.get(&key).await?;                       // registered: lookup_key_asset(&key) is `a`
m.remove(&key).await?;                            // unregistered
let b = m.get(&key).await?;                       // a fresh asset, new id
// Can any later operation make `lookup_key_asset(&key)` return `a` again?
```

Today the answer is "no", but only because every insertion site mints the asset it inserts
(`DefaultAssetManager::get_nonvolatile_resource_asset` and `get_query_asset_for` use
`entry_async(..).or_insert_with(new_from_recipe ..)`, `set_state` inserts a new `AssetData`,
`ImmediateAssetManager` inserts a freshly built `asset_ref`). Nothing states it, nothing tests it,
and `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` §7 does not ask an external manager for it. So a caller
cannot use the interval argument "registered at the start and at the end, so registered
throughout", and garbage collection cannot assume an evicted asset stays evicted.
`ASSET_LIFECYCLE.md` §Ownership versus registration and the guide both say the exact contract is
"`ASSET-REGISTRATION-OWNERSHIP-CONTRACT`", an issue rather than a document.

Should be: the reference states the contract below, the guide requires it of every manager, and a
shared scenario proves it on the queued, inline and external managers.

### Scope and Acceptance Criteria

The contract (to be written into `ASSETS.md` §Key ownership):

- **R1 Ownership is the asset's.** A keyed asset (key recorded at construction, `AssetData::key`,
  `MetadataRecord::key`) is the only kind that may write the store under that key. Which keyed asset
  is the key's dependency-graph node is `AssetRef::bound_owner_key`'s rule, unchanged: the
  registered owner when there is one; otherwise an unregistered, non-volatile `cached: false` asset
  built for the key; never a volatile asset.
- **R2 Registration is the manager's, and revocable.** A registration is a cache entry. The manager
  may end it at any time: expiry, `remove`, replacement by `set_state` / `set_binary`, volatility
  eviction, and later garbage collection. Correctness never depends on it; ending one costs a reload
  or a recomputation, never a wrong value.
- **R3 At most one registered asset per key, and per query.**
- **R4 Registration is monotonic.** An asset is registered at most once. After its registration
  ends, no operation registers it again. Every registration inserts a newly constructed asset.
- **R5 Interval observation.** By R4, an asset observed as the registered one at the start and at
  the end of an operation was registered throughout. No lock is needed.

- **AC-1** Registration is monotonic on every manager
  WHEN a registered asset leaves the map by `remove`, by replacement through `set_state`, by expiry
  (`AssetRef::expire` then a request for the key), and by volatility eviction (`owned_key_asset`
  on a volatile entry), and the key or query is then requested again, repeatedly
  THEN `lookup_key_asset` / `lookup_query_asset` never again returns the old asset's id, on the
  queued, the inline and the external test manager
- **AC-2** One registered asset per key
  WHEN several first requests for one key run concurrently
  THEN all of them get the same asset id (existing scenario, now cited as the R3 proof)
- **AC-3** The contract is stated where it is read
  WHEN a reader opens `ASSETS.md` §Key ownership, `ASSET_LIFECYCLE.md` §Ownership versus
  registration, `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` §7 or the `AssetRef::key` doc comment
  THEN R1-R5 are stated or cited there, and none defers to an issue. The non-registered-owner
  warning written by `save_to_store` names `ASSETS.md` §Key ownership instead of the issue

Non-goals: **protected registration** (a flag that forbids ending a registration) and its expiry
question, which have no consumer yet and are filed as
`REGISTERED-ASSET-CANNOT-BE-PROTECTED-FROM-UNREGISTRATION` (decision 1); reducing or removing
pure-key delegation (`AssetRef::evaluate_recipe`), which stays as it is because a non-owner keyed
asset still exists by R1-R2; recording the key in metadata, which is done (`MetadataRecord::key`,
`ASSET_LIFECYCLE.md` §6).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. Documentation, one log message and a new test scenario, so
  rules 1-4 and 6 hold, but readiness is not `ready` (rule 5). It becomes eligible once decision 1
  is made, unless protected registration is pulled back into scope.
- **Leading issue:** Proposed resolution, scope: defer protected registration.
- **Explanation:** R1-R5 describe what HEAD already does, verified at every insertion site, so the
  work is to state and test it. The one open choice is whether to design protected registration
  now, without a consumer.
- **Open questions:**
  1. **Proposed resolution, scope:** protected registration is deferred to
     `REGISTERED-ASSET-CANNOT-BE-PROTECTED-FROM-UNREGISTRATION`, together with its expiry question
     (downgrade, refuse, or recompute in place). Recommended, because no feature needs it yet,
     and recompute-in-place needs a new `Expired` → `Recipe` transition (today `try_claim_for_run`
     refuses `Expired`, so an expired asset is always replaced, never re-run). Consequence of the
     alternative: this design grows back to `L` and blocks on that transition.
  2. **Implementation detail (resolved):** R4 is proved by a scenario, not enforced by a runtime
     check. A check would need a per-asset "was registered" flag on `AssetData`, a new field for an
     invariant that fresh construction already guarantees.

### Design Dependencies

- `asset-garbage-collection` (CORE-ASSET-GC): **required-by**. Garbage collection is a new way
  to end a registration and relies on R2 and R4.
- `environment-orderly-shutdown`: **overlaps**. Shutdown ends work, not registrations. No ordering.
- `environment-manager-reference-cycle` (ready): **overlaps**. It changes what an asset holds, not
  what the maps hold. No ordering.
- `evaluate-path-consolidation` (complete, E4): the origin of the issue. Its "ownership is
  approximated by keyedness" is R1.

### Scope Changes

- 2026-10-20, intake: issue point 4 (key in metadata) is done at HEAD. Point 1 is mostly answered:
  `mark_expired_status` and `save_to_store` now use the recorded key (`AssetRef::key`), and
  `bound_owner_key` answers for an uncached keyed asset. That leaves volatile keyed assets as the
  only keyed assets with no graph node, which is deliberate (`ASSETS.md` §Volatile assets are never
  owned). Point 3 (protected registration) is split off. Complexity re-sized from `L` to `M`: one
  crate, no API change.

### Consolidated Findings

- The contract describes HEAD. No production code changes except one log message.
- R4 holds because insertion always constructs. `scc::HashMap::entry_async(..).or_insert_with`
  and `ImmediateAssetManager`'s check-then-insert under the map mutex never take an existing
  `AssetRef` from outside. The guide's example (§7) is already monotonic, so external managers
  written from it comply.
- Temporary assets (`create_temporary_asset`, id 0, `TEMPORARY-ASSETS-SHARE-ID-ZERO`) are never
  registered, so the id-based test is sound for registered assets.
- `asset-garbage-collection` is the first consumer of R2 and R4. It is ordered after this design,
  though it does not strictly need the code (there is none), only the stated contract.

## Phase 2: Architecture

### Solution

Write the contract down where readers and implementors look, and prove R4 on all three manager
kinds with a shared scenario. Change the warning text so it no longer names an issue that is about
to close.

Rejected alternatives:
- **A runtime check of R4** (debug assertion on insert). Needs a new `AssetData` flag, and only
  re-checks what fresh construction guarantees.
- **Protected registration now.** No consumer, and an unresolved expiry semantics (decision 1).
- **Registration as ownership** (making `owned_key_asset` the definition of who may write). Rejected
  by `evaluate-path-consolidation`. A volatile keyed asset and a `cached: false` keyed asset must
  write without being registered.

Known-issue preflight: `QUEUED-MANAGER-EVICTION-RACE` is closed (`remove_key_asset_if`).
`INLINE-RUN-CLAIM-REPAIR-SKIPPED-UNDER-LOCK-CONTENTION` concerns run claims, not registration.
`TEMPORARY-ASSETS-SHARE-ID-ZERO` concerns unregistered assets only. None blocks.

### Changes

- `liquers-core/src/assets.rs`
  - `AssetRef::save_to_store`, the non-registered-owner warning: replace
    `(ASSET-REGISTRATION-OWNERSHIP-CONTRACT)` with `(see ASSETS.md §Key ownership)`. Nothing
    matches on the text (`grep` finds no test that asserts it).
  - Doc comments that cite the issue (`AssetData::key`, `AssetRef::key`, the module doc around
    line 103, the comment in `save_to_store`): cite R1-R4 and `ASSETS.md` §Key ownership.
  - `AssetManager::lookup_key_asset`, `lookup_query_asset`, `owned_key_asset` doc comments: one
    sentence each on R4 and R5.
- `liquers-core/tests/common/manager_scenarios.rs`: new `scenario_registration_is_monotonic(envref)`.
- `liquers-core/tests/manager_parametric.rs`: `registration_is_monotonic_default`, `…_immediate`.
- `liquers-core/tests/external_asset_manager.rs`: `external_manager_registration_is_monotonic`.
- No `pub` signature, command, serialized format or other crate changes. Nothing is async that was
  not.
- Documents: `ASSETS.md` §Key ownership (R1-R5), `ASSET_LIFECYCLE.md` §Ownership versus registration
  (cites `ASSETS.md`, drops the issue reference), `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` §7 (adds
  "register a fresh asset, never re-register" and drops "until … settles"). Each gets a History row
  and a `reviewed:` bump.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| The scenario finds an R4 violation | it would be a real defect; fix the insertion site in the same PR or file it | high (every site read) |
| Expiry eviction differs per manager (monitor vs lazy check) | the scenario expires, then requests, then compares ids, which works for both | high |
| The external test manager evicts differently | `MinimalInlineAssetManager` implements `get`, `set_state` and `remove_key_asset_if` (`tests/common/minimal_manager.rs`); the scenario only compares ids | high |
| Log-text change breaks a consumer | no test or code matches on it | high |

Recovery: documentation and tests only. Revert the commit.

## Phase 3: Examples and Tests

### Examples

The Problem Example, extended to every route, is `scenario_registration_is_monotonic`. Keys are
plain store keys in an `AsyncMemoryStore` (`data/a.txt`, `data/v.txt` with a volatile recipe), so
no query needs validation beyond the existing scenario stores (`recipe_store`,
`volatile_recipe_store`).

### Tests

- `scenario_registration_is_monotonic(envref)` in `liquers-core/tests/common/manager_scenarios.rs`
  (AC-1). For each route — `remove`, `set_state` replacement, `AssetRef::expire` then `get`, and a
  volatile key read twice — record the registered id, perform the route, then call `get` (or
  `get_asset` for a query) three times. Assert after each call that `lookup_key_asset` /
  `lookup_query_asset` is `None` or has an id different from every id recorded earlier.
- `registration_is_monotonic_default`, `registration_is_monotonic_immediate` in
  `liquers-core/tests/manager_parametric.rs`, and `external_manager_registration_is_monotonic` in
  `liquers-core/tests/external_asset_manager.rs`, each running the scenario (AC-1).
- `concurrent_first_evaluations_*` (existing, `scenario_concurrent_first_evaluations`),
  `external_manager_registers_one_asset_per_key` and
  `default_try_insert_key_asset_preserves_original_on_duplicate` (existing): unchanged, cited as
  the R3 proof (AC-2).
- Documentation review (AC-3): `grep -rn 'ASSET-REGISTRATION-OWNERSHIP-CONTRACT' liquers-core
  specs/reference specs/guides` finds nothing except History rows; `python3 scripts/docs_index.py
  --check` passes.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test manager_parametric --test
external_asset_manager registration`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/tests/common/manager_scenarios.rs` `scenario_registration_is_monotonic`
  and its three callers (AC-1). They should pass at HEAD. If one fails, stop: that is an R4 defect
  to fix or file before the contract is written. Proof: `cargo test -p liquers-core --test
  manager_parametric --test external_asset_manager registration`. Rollback: delete the tests.
- [ ] 2. `liquers-core/src/assets.rs` — warning text in `save_to_store` and the doc comments listed
  in Phase 2 (AC-3). Proof: `cargo test -p liquers-core --lib` and `cargo doc -p liquers-core
  --no-deps`. Rollback: revert.
- [ ] 3. `specs/reference/ASSETS.md` §Key ownership (R1-R5), `specs/reference/ASSET_LIFECYCLE.md`
  §Ownership versus registration, `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` §7, each
  with a History row and a `reviewed:` bump (AC-3). Close `ASSET-REGISTRATION-OWNERSHIP-CONTRACT`
  with a resolution note naming the follow-up issue. Proof: the AC-3 `grep`, `python3
  scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `python3 scripts/docs_index.py --check`; a diff review
that only tests, comments, one log string and the three documents changed.
