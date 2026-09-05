# Phase 2: Solution & Architecture - Stale-Dependency Status Finalization

## Overview

The stale-dependency rule moves out of the run harness and into the status authority, which is
renamed from `try_to_set_ready` to `finalize_status` because deciding `Ready` is only one of the
four outcomes it already produces. Deciding there makes the decision atomic with the status write
and puts it *before* persistence, which is what the store needs. One consequence is not free and is
designed for rather than accepted silently: `DependencyManager::track_asset` refuses an `Expired`
asset, so `evaluate`'s last step branches explicitly — a stale-dependency keyed asset invalidates
its dependents instead of registering itself as their current version.

No type is added, no signature changes but the rename, no crate but `liquers-core` is touched.

## Corrections owed before this phase is re-approved

**This document is the version approved on 2026-09-04. The Phase 4 review invalidated three of its
decisions, and it has deliberately NOT been edited in place** — the phase must go back through its
gate, and silently rewriting an approved architecture would hide that. The corrections below are
settled; applying them is the first task when work resumes.

| # | Section to change | Correction |
|---|---|---|
| C1 | §Store System, §"The decision inside `finalize_status`" | **Finalizing `Expired` before persistence prevents the write.** `evaluate` never sets `lock.binary`, so `save_to_store` falls through to `serialize_to_binary` (`assets.rs:2718`), which calls the *gated* `poll_state()` — `None` for `Expired`. The write fails and nothing is stored. Fix: `serialize_to_binary` uses `poll_state_any_status()`, and is renamed `serialize_to_binary_unchecked`. Verified: its only two callers are `save_to_store` (needs ungated) and `get_binary` (`:3140`, which already returned `Err` for `Expired` before reaching it), so no gated twin is needed and none should be created |
| C2 | §"The dependency-manager branch in `evaluate`" | **Drop the cascade.** `track_asset`'s early return for `Expired` is correct after all. `cascade_expire_dependents` would not expire keyed dependents anyway — `expire_internal` skips the walk when the source's version is unknown, which it always is for a computed asset — while still removing `keyed_dependents[K]` and `versions[K]`. The dependent invalidation this branch was meant to preserve **does not happen today**: `register_version(0 over 0)` reports no change. The branch becomes: volatile → nothing; otherwise → `track_asset` unchanged. |
| C3 | (dissolves with C2) | The `bound_owner_key()`-versus-`lock.key` correction and the delegated-asset gap only mattered because of the cascade. With C2 there is no new key derivation and no new branch, so both disappear. Do not carry them forward |
| C4 | §Error Handling | The `Ready` arm's discipline is to record a failed metadata write as a `LogEntry::warning`. Phase 4's sketch used `let _ =` on `set_status` and `set_expiration_time_from`, silently discarding both. Match the `Ready` arm |
| C5 | §"The `expired-binary-read-safety` regression is preserved" | Add the transitive consequence raised at review: a parent polling a stale-dependency child through `wait_for_dependency` now always sees `Expired` rather than a brief `Ready`, so whole dependent chains in one run finish `Expired`. Probably correct, but it is a behaviour change this design should name |

Two facts recorded during the review that this phase should state rather than leave implicit:

- `AssetData::reset` (`:1332`) clears data, binary, metadata, status and persistence status but **not**
  `stale_dependency`. `AssetRef::reset` has no callers today, so nothing is broken; with the rule
  moved into finalization, any future in-place re-evaluation would make the asset permanently born
  `Expired`.
- `set_value` (`:3330`) and `set_state` (`:3368`) also persist and never consult `stale_dependency`.
  They are unreachable with the flag set, so the "single status authority" claim should be scoped to
  the evaluate path rather than stated globally.

## Known-Issue Preflight

Searched: issues linked from `DESIGN.md` and Phase 1; every `draft`/`accepted`/`in_progress` row in
`specs/index.csv` whose `area` includes `core/assets`, `core/store` or `axum`; and the design
folders touching expiry (`expiration-mechanism`, `expiration-safety`, `expired-binary-read-safety`,
`dependency-scheduling`, `wp2-terminal-outcome`, `evaluate-path-consolidation`).

| Issue | Status | Current priority | Relevance and solution impact | Must be addressed first? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY` | draft | **P1** | The issue this design fixes | — | no | Fix here; correct its four stale citations | **Raised P2 → P1, applied 2026-09-04** |
| `EXPIRATION-RECOVERY-WEB-API` | accepted | P2 | This fix increases how often a store entry is `Expired`, so the recovery surface it asks for gets more valuable. It does not change what that surface must be | no | no | Monitor; link from the design | Keep P2 |
| `ASSET-FINISHED-PROGRESS-CONTRACT-UNDEFINED` | draft | P3 | Same region: `finalize_primary_progress` races the service loop at the end of a run. The architecture must not add a second decision whose outcome depends on that loop's timing — and it does not: the rule is decided under the `data` write lock, not by a service message | no | no | Independent; Phase 3 asserts the ordering rather than relying on it | Keep P3 |
| `QUEUED-MANAGER-EVICTION-RACE` | accepted | P2 | Touches `remove_expired_from_maps` and `get_asset`, which run *after* an asset is expired. Orthogonal: this design changes when a status is decided, not how an entry is evicted | no | no | Independent | Keep P2 |
| `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS` | draft | P2 | Touches `run_with_future_inline`, one of the two harnesses the rule is being removed from. Removing the relabel block does not touch claim or waiter handling | no | no | Independent | Keep P2 |
| `ASSET-REGISTRATION-OWNERSHIP-CONTRACT` | draft (feature) | P2 | Registration is what `track_asset` and `save_to_store` approximate ownership with. This design changes *whether* a stale-dependency asset registers, inside that same approximation | no | no | Monitor; record the new branch as another consumer of the unwritten contract | Keep P2 |
| `ASSETS-FIX1` | accepted (feature) | P2 | Catalogue of TODO/FIXME markers in the asset lifecycle. The relabel block being removed carries no marker | no | no | Independent | Keep P2 |
| `CORE-TOKIO-REMOVAL` | accepted | P3 | The rule currently lives in `finish_run_with_result`, which both harnesses share. Moving it into `finalize_status` removes one more thing the harnesses must agree on, which helps rather than hinders | no | no | Independent | Keep P3 |

**No blocker.** Nothing on the list must be resolved before this design is implementable, and no
architecture assumption here depends on an unresolved issue.

### Priority action: recommend P1 for the originating issue

The issue states "no wrong value is served in-process, which is why this is P2 rather than P1", and
that is true. It is also not the whole exposure, and Phase 2 verified the rest:

`AssetRef::try_fast_track` (`assets.rs:1048`) accepts a stored asset when its status is
`Ready | Source | Override`, then validates recorded dependency versions against the dependency
manager — but only where the DM *has* a version:

> `if let Some(dm_version) = dm.get_version(&dep_record.key).await { … }`

In a fresh process the DM is empty, so that guard is vacuous and every recorded dependency passes.
A stale-dependency asset stored as `Ready` is therefore loaded and served **without
recomputation** by the next process that asks for it. The in-process masking the issue describes is
real and is exactly what does not survive a restart — which is the case the persisted status exists
for.

That is a correctness risk with a workaround (recompute deliberately, or `expire()` the key), so it
reads as `P1` under `DOCS_STRUCTURE_GUIDE.md` §4.4, not `P0`: no computation is wrong, nothing is
lost, and it needs both a mid-flight dependency expiry and a process boundary. **Recommended, not
applied** — a priority change is confirmed with the owner (skill Phase 2 step 3).

## Data Structures

### New Structs

None.

### New Enums

None. No `match` over a Liquers-owned enum is added, so the no-default-arm rule has nothing new to
police; the two `match`es this design touches (`finish_run_with_result`'s status match,
`finalize_status`'s branch) keep their existing exhaustive form.

### Changed Structs

`AssetData<E>` is unchanged. `stale_dependency: bool` (`assets.rs:584`) keeps its type, its
initializer (`:955`) and its only writer, `note_expired_dependency` (`:1516`). Only its *reader*
moves.

**Why no new field.** The natural-looking alternative — record the finalized status separately, or
add a `terminal_status: Option<Status>` — reintroduces exactly the duplication that
`evaluate-path-consolidation` Phase 5 §2 rejected for `payload_required`: a second source for a
fact `status` already holds, which then has to be kept in step with it.

## Trait Implementations

None added or changed. `AssetManager<E>`'s trait surface is untouched, so no implementor —
`DefaultAssetManager`, `ImmediateAssetManager`, or anything in `liquers-py` — needs a change. The
DM branch in `evaluate` uses `cascade_expire_dependents`, an **existing shared default method**
(`assets.rs:3960`).

## Generic Parameters & Bounds

No bound is added or relaxed. Everything stays inside `impl<E: Environment> AssetRef<E>`, whose
existing bounds already cover the two calls being introduced.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|---|---|---|
| `finalize_status` (renamed `try_to_set_ready`) | Yes — unchanged | Holds `self.data.write().await`. No new I/O; the decision is pure computation over fields already under that lock |
| `cascade_expire_dependents` | Yes — existing | Already async; takes the DM's `expiration_lock` and may touch other assets |

No blocking I/O is introduced, and no lock is newly held across an `.await`: `finalize_status`
takes the write lock, decides, and releases it before `evaluate` continues, exactly as today.

**Atomicity, and why it holds.** `stale_dependency` is written by `note_expired_dependency` under
`data.write()` and read by `finalize_status` under the same lock, so no interleaving can lose the
flag. Ordering is stronger than that: every dependency wait happens inside `apply_recipe`, which
`evaluate_recipe_outcome` awaits to completion before `evaluate` reaches finalization, so the flag
cannot be set after the decision. Phase 3 asserts this rather than assuming it.

## Function Signatures

### `liquers-core/src/assets.rs` — `impl<E: Environment> AssetRef<E>`

```rust
/// Decide and install this asset's terminal status — the single status authority.
///
/// Was `try_to_set_ready`. Renamed because `Ready` is one of four outcomes it produces
/// (`Volatile`, `Expired`, `Ready`, `Error`), and because the old name is why the
/// stale-dependency rule was written somewhere else.
///
/// Runs **before** the `ValueProduced` notification and **before** persistence, so nothing
/// observes or stores a non-final status (`ASSET_LIFECYCLE.md` §"the one evaluation path", step 6).
async fn finalize_status(&self);
```

The signature is otherwise unchanged: no parameters, no return value. `evaluate` does not need the
outcome returned — it already performs one read of `save_in_background`, `cancelled` and
`is_volatile` after finalization, and that read gains `stale_dependency`.

**Why not `-> Status`.** Returning the installed status would force `evaluate` to `match` 15
variants to make a three-way decision, or to use the default arm the project forbids. Two booleans
read from the lock `evaluate` already takes express the same branch with no new match.

### The decision inside `finalize_status`

Structure only; the body lands in Phase 4:

```
if data is present:
    volatile (is_volatile || metadata.expires().is_volatile())  -> Status::Volatile   [unchanged]
    else if stale_dependency                                    -> Status::Expired    [moved here]
    else                                                        -> Status::Ready      [unchanged]
else:
                                                                -> Status::Error      [unchanged]
```

Three properties of the moved branch:

1. **It writes metadata, not just the field.** It goes through `AssetData::set_status` (`:1183`),
   which sets `self.status` *and* `self.metadata.set_status(status)`. The harness block it replaces
   already did this; stating it because the whole defect is metadata and memory disagreeing.
2. **The warning moves with it.** The existing "evaluated with an expired dependency value" log
   entry is written here, so it reaches the store with the value. Today it is added after
   persistence and is therefore absent from the stored sidecar — the quieter half of the same bug:
   the store keeps neither the status nor the reason.
3. **`expiration_time` follows the `Ready` arm.** `Expired` uses the same
   `set_expiration_time_from(&metadata_expires)` and `lock.expiration_time` update as `Ready`, so
   `finish_run_with_result`'s "schedule expiration if finite" step behaves as before. Its
   `!exp_time.is_expired()` guard already declines to schedule for an already-expired asset.

### The removed block

`finish_run_with_result` (`:2249-2261`) loses the relabel and its comment entirely. Its own
fallback `try_to_set_ready()` call (`:2224`, for a run that finished without `evaluate` finalizing)
becomes `finalize_status()` and therefore gains the rule — correct, and free: that path does not
persist.

### The dependency-manager branch in `evaluate`

Today, step 8:

```rust
if !lock_is_volatile {
    let expired = dm.track_asset(self).await;
    manager.expire_dependencies_result(expired).await;
}
```

`DependencyManager::track_asset` (`dependencies.rs:282`) processes only
`Ready | Source | Override` and returns early for `Expired`. So finalizing earlier would silently
stop this step from running — including the dependent invalidation it performs today as a side
effect of `register_version`. The branch is therefore made explicit:

```
if lock_is_volatile           -> nothing                       [unchanged: not a graph node]
else if stale_dependency:
        keyed                 -> cascade_expire_dependents(DependencyKey::from(key))
        non-keyed             -> nothing
else                          -> track_asset + expire_dependencies_result   [unchanged]
```

**Rationale.** `track_asset` does two things for a keyed asset: it registers this value as the
key's current version, and — as a side effect of that registration changing the version — it
expires dependents that recorded an older one. The first is wrong here: advertising an
uncacheable value as the key's current version is the same category of lie as storing it `Ready`.
The second is right and must be kept. `cascade_expire_dependents` is exactly the second without
the first, is an existing shared default method, and is what `AssetRef::expire` already does for an
ordinary expiry of the same key — so the stale-dependency completion and a normal expiry converge
on one mechanism instead of two.

It is *broader* than today in one respect: `expire(key)` invalidates every dependent, where
`register_version` invalidated only those whose recorded version differed. That is deliberate and
conservative — the key's newest value is expired, so every dependent recorded against that key is
built on a superseded input — and it is the cost `AssetRef::expire` already pays on every keyed
expiry.

The non-keyed arm does nothing because `track_asset`'s query branch registers the asset as a
*dependent* of its own dependencies, so that a later expiry reaches it. An asset that is already
`Expired` gains nothing from being reachable that way.

**Rejected alternative — do nothing (let `track_asset` early-return).** Simpler by three lines, and
defensible on the argument that an expired asset should not be a graph node. Rejected because it
silently drops the dependent invalidation that happens today, trading a persistence bug for a
smaller invalidation bug. **Confirmed by the project owner at the Phase 2 gate (2026-09-04):
`cascade_expire_dependents` is the approach.**

**The cascade must not be silent.** The owner's confirmation came with a requirement attached: an
asset that becomes `Expired` should record why — "expired due to dependency X expiring while
evaluating Y" — and that gap is general, not specific to this path. `mark_expired_status` adds no
log entry at all, so every asset a cascade reaches records nothing, and the one path that does
record something (`note_expired_dependency`) names the dependency by its runtime `u64` id. Filed as
`EXPIRY-RECORDS-NO-REASON` (P2, S) rather than absorbed here: it spans the deadline, cascade,
explicit-`expire()` and stale-dependency routes, only one of which this design touches, and it
carries a choice — `info` versus `warning` per route — that is not this design's to make. What this
design does owe it is the ordering precedent: §"The decision inside `finalize_status`" already moves
the stale-dependency warning ahead of persistence so the reason reaches the store with the status,
which is the shape `EXPIRY-RECORDS-NO-REASON` has to follow for the other routes.

**Rejected alternative — route the relabel through `expire()`/`mark_expired_status` (`:2920`).**
Phase 1 open question 3. That helper already persists `Expired` for a keyed asset (the WP-3 rule),
notifies, and cascades — so it looks like the fix already exists. It does not fit: it writes
metadata only `if store.contains(&key)`, and at finalization time the entry has not been written
yet, so the write would be skipped; used after persistence instead, it costs a second store
round-trip and leaves the invariant violated in between. Its `Ready | Override`-only guard would
also have to grow a `Volatile` answer. What survives from it is the *cascade*, which the branch
above adopts.

**No `Expired` notification.** `mark_expired_status` sends `AssetNotificationMessage::Expired`;
this path deliberately does not. That message announces a transition away from a value that was
being served, and subscribers use it to stop relying on one. Here nothing was ever served: the
asset is born expired, `ValueProduced` and `JobFinished` are the truthful messages, and adding
`Expired` would make a waiter believe a value it never received had just been withdrawn.

### The `expired-binary-read-safety` regression is preserved, and stops being racy

Phase 1 open question 5. That design's owner-decided position (its cross-phase finding B1, resolved
as "Option 2 — accept the regression") is that a stale-dependency completion is uniformly `Expired`:
normal reads of either family decline it, and the caller opts in explicitly through `to_override()`
or a `*_any_status` read. This design must not weaken that, and it does not — it makes it hold
sooner and without a race.

| Read | Today | After |
|---|---|---|
| `poll_state` / `poll_binary` between `ValueProduced` and `finish_run_with_result` | status is still `Ready`, so the value **is** served | status is already `Expired`, so it is declined |
| the same reads after `finish_run_with_result` | declined | declined — unchanged |
| `poll_state_any_status` / `get_binary_any_status` | retained value returned | unchanged |
| `to_override()` | promotes `Expired` → `Override` | unchanged; and now also works after an eviction-and-reload, because the store agrees |

The middle window is exactly what B1 called out as racy — "the 10 ms poll may observe either side of
the relabel". Deciding the status before the notification closes it: there is no instant at which a
stale-dependency asset is observable as `Ready`. So the accepted regression becomes deterministic
rather than scheduling-dependent, which is what a test can pin.

The one genuinely new consequence is on the *store* side, and it runs the same way: a
stale-dependency asset that is evicted and re-requested previously came back from the store as
`Ready` (the bug), and now comes back refused by `try_fast_track` and recomputed. Recovery of that
value is then the recovery API's job, which `test_get_any_status_and_to_override_from_store_only`
(`expiration_integration.rs:1336`) already covers for an ordinary expired keyed asset.

**Confirmed here, proved in Phase 3:** Phase 3 owns re-running the `I5` scenario against the new
ordering, and asserting the middle row above rather than assuming it.

## Integration Points

### Crate: liquers-core

**File:** `liquers-core/src/assets.rs` — the only file changed.

| Site | Line (HEAD) | Change |
|---|---|---|
| `try_to_set_ready` | `:1818` | Rename to `finalize_status`; add the `stale_dependency` branch and move the warning into it |
| `evaluate` — finalize | `:2553` | Call site rename |
| `evaluate` — post-finalize read | `:2555-2566` | Read `stale_dependency` alongside the three facts already read |
| `evaluate` — DM step | `:2575-2582` | Explicit three-way branch |
| `finish_run_with_result` — fallback | `:2224` | Call site rename |
| `finish_run_with_result` — relabel | `:2249-2261` | Removed |
| module rustdoc | `:~200` | The read-exposure table's `Expired` row and the flow summary name `finish_run_with_result` as where the label is applied |

**File:** `liquers-core/src/dependencies.rs` — **read only.** `track_asset`'s status gate is the
reason for the branch, and is left exactly as it is: it is right to refuse an expired asset.

### Crates not touched

`liquers-store`, `liquers-lib`, `liquers-axum`, `liquers-web`, `liquers-py`, `liquers-macro`. No
public item changes: `finalize_status` is private (`async fn`, no `pub`), and the two behaviours
that change — the status in a stored sidecar, and which DM call a stale-dependency asset makes —
are internal. The dependency flow is respected; nothing new is imported in either direction.

### Dependencies

None added or changed. No `Cargo.toml` in the workspace is touched, and no feature gate is
involved: `assets.rs` is unconditional core code, so the `check-build-matrix.sh` configurations
compile the same source in every one.

## Documentation Architecture

### Reference Plan

**Extend three existing references. Create none.** The behaviour has no surface a reader reaches
directly, so it belongs in the documents that already describe evaluation and expiry.

| Path | Audience | Area | Change |
|---|---|---|---|
| `specs/reference/ASSET_LIFECYCLE.md` | internal | `core/assets` | Step 6 of "the one evaluation path" already says status is finalized before the notification and before persistence. Name the four outcomes it decides between, including the stale-dependency one, so the step is a specification rather than an ordering note. Add the DM branch to step 8 |
| `specs/reference/ASSETS.md` | internal | `core/assets` | §Expiry (`:241-244`) attributes the `Ready`→`Expired` relabel to `finish_run_with_result`. Retarget to `finalize_status`, and say the asset is *born* expired rather than relabelled — which is what makes the `*_any_status`/`to_override` recovery sentence beside it still correct |
| `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` | both | `core/assets` | `:246-248` says the parent "records the stale dependency and finishes as `Expired`". True, and now also true of the store — add that, since the paragraph's next sentence is about what manager access does next |

Each gets a `## History` row and a `reviewed:` bump in the same commit
(`DOCS_STRUCTURE_GUIDE.md` §9.2).

### Guide Plan

**None.** Phase 1's rationale stands and Phase 2 did not disturb it: there is no repeatable task a
developer performs here, and the recovery workflow a caller *does* perform is already documented
with the `*_any_status` family in `ASSETS.md`. The condition for reconsidering is unchanged — an
architecture that changed what a caller must do — and the chosen architecture does not.

### Other Documents to Create

**None.** The two adjacent findings this phase produced are recorded where they belong rather than
written up here: the cross-process fast-track exposure goes into the originating issue (with the
priority recommendation), and if Phase 3 shows the missing `Expired` notification or the DM branch
is wrong in a way this design should not absorb, that is a new issue under §4.8.

### New Reference or Guide Documents

None.

### Existing Documents to Review or Update

Candidates were generated by `area` (`core/assets`) and each was decided, not skipped:

| Document | In `affects_docs`? | Why |
|---|---|---|
| `ASSET_LIFECYCLE` | **yes** | Owns the ordering invariant this restores |
| `ASSETS` | **yes** | §Expiry names the old location |
| `DOC_03_ASSETS_EXECUTION_LIFECYCLE` | **yes** | Describes the execution-time expiry outcome |
| `ASSET_SET_OPERATION` | no | `set`/`set_state` do not enter `evaluate` and have no dependency wait |
| `DEPENDENCIES_STATUS` | no | Specifies `Status::Dependencies` — a *scheduling* state left before evaluation finishes; untouched |
| `PROJECT_OVERVIEW` | no | Core-concept level; no concept changes |
| `DOC_01_ARCHITECTURE_REFERENCE` | no | Architecture level; the evaluation path's shape is unchanged |
| `DOC_08_RECIPES_PLANS` | no | Recipes and plans; no plan or recipe behaviour changes |
| `ENVIRONMENT_CONFIG`, `ENVIRONMENT_CONSTRUCTION_GUIDE` | no | Construction and configuration; nothing configurable changes |
| `LANGUAGE-INTEGRATION_GUIDE` | no | No public item changes, so no binding changes |

`DESIGN.md`'s `affects_docs` is therefore `[ASSET_LIFECYCLE, ASSETS, DOC_03_ASSETS_EXECUTION_LIFECYCLE]`
— already set, and confirmed rather than assumed.

Also updated, as documents rather than as `affects_docs` entries:
`specs/issues/ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY.md` (four stale citations, the cross-process
exposure, and the status/priority outcome at Phase 5), `specs/README.md` and `specs/index.csv`.

### Design and Capability Links

`specs/README.md` carries the design-folder line added in Phase 1. At Phase 5 the capability is
anchored in `ASSET_LIFECYCLE.md`, and no reader should need this folder to learn when a status is
final — the design is linked from the issue, not from the reference.

### Evidence to Collect During Implementation

- Whether the DM branch changes any existing test's expectations — that is the measurable form of
  "is the broader cascade acceptable?"
- Whether `test_wait_for_retained_expired_dependency_labels_asset_expired_on_completion` still
  passes unchanged; it uses a non-keyed asset, so it should, and if it does not the branch is wrong.
- Confirmation, in a test, of the middle row of §"The `expired-binary-read-safety` regression is
  preserved": between `ValueProduced` and the end of the run, a stale-dependency asset is never
  observable as `Ready`. Phase 2 argues it; only a test settles it.
- The cross-process scenario as a runnable test — it is the fix's real payoff and nothing exercises
  it today.
- Any place the rename makes a comment or doc-link inaccurate.

## Relevant Commands

### New Commands

**None.** No `register_command!` invocation is added, changed or removed, so
`specs/command_registry.yaml` is not regenerated and `cargo test -p liquers-lib --test
registry_export` is unaffected.

### Relevant Existing Namespaces

**None.** This design has no query-reachable surface at all: it changes what an evaluation writes
about itself, which no command names and no query selects. There is nothing here for a namespace to
be relevant to, so the Phase 2 command question is answered rather than asked — flagged at the gate
for confirmation, as `expired-binary-read-safety` did for the same reason.

## Web Endpoints

**None.** No route, handler or response shape changes. `liquers-axum` is not edited. The visible
difference is that `AssetInfo` for a stored stale-dependency asset reports `Expired` instead of
`Ready`, which is the correction, and which the existing handlers already have arms for.

## Error Handling

No new error type, no new `ErrorType` variant, and no `Error::new`. The moved branch inherits the
existing failure discipline of `finalize_status`: a metadata write that fails becomes a warning log
entry on the asset rather than an error return, because the status decision itself cannot fail and
losing the record of it must not lose the value.

| Scenario | Handling |
|---|---|
| `metadata.set_status(Expired)` fails | `LogEntry::warning` on the asset, as the `Ready` arm already does for its own metadata writes |
| Persisting the `Expired` value fails | Unchanged: `persist_with_status_tracking` → `record_persistence_result`, which records `PersistenceStatus` and keeps the value |
| `cascade_expire_dependents` finds nothing | Not an error; it returns an empty set |

No `unwrap()` or `expect()` is introduced. The design adds no `?` in a path that previously could
not fail.

## Serialization Strategy

Unchanged. `Status` already serializes as part of `MetadataRecord`, and `Expired` already round-trips
— `try_fast_track` reads it back and refuses it (`:1063`), which is the mechanism the whole fix
relies on. No serde annotation is added.

## Concurrency Considerations

- **The decision is atomic.** Flag write and flag read take the same `data.write()` lock; and the
  only writer runs strictly before the reader (dependency waits complete inside `apply_recipe`).
- **No new lock, no new lock ordering.** `finalize_status` takes the lock it already takes.
  `cascade_expire_dependents` takes the DM's `expiration_lock` — as `AssetRef::expire` already does
  from a comparable position, and with no asset `data` lock held across it.
- **Both harnesses, one rule.** `run_with_future` (`:2287`) and `run_with_future_inline` (`:2326`)
  share `finish_run_with_result`, so the bug is present on both today and the fix reaches both.
  Moving the rule into `evaluate`'s finalization keeps that property without depending on the
  service-message loop, whose termination point is what made the current placement unfixable in
  place.
- **wasm.** No `tokio::` primitive is introduced. The branch uses `futures`-free, executor-agnostic
  calls, so the inline path stays spawn-free and `liquers-web` is unaffected.

## Open Questions

1. ~~**Confirm the DM branch**~~ — **resolved 2026-09-04 (owner): `cascade_expire_dependents`.**
   The accompanying diagnostics requirement is filed as `EXPIRY-RECORDS-NO-REASON`.
2. ~~**Confirm the priority recommendation**~~ — **resolved 2026-09-04 (owner): raised to P1**, and
   applied to the issue.
3. ~~**Confirm the rename**~~ — **resolved 2026-09-04 (owner): rename.** `try_to_set_ready` becomes
   `finalize_status`, so `ASSET_LIFECYCLE.md` step 6 describes a function whose name matches what it
   decides.
4. **Confirm "no commands in scope"** — answered above rather than asked, per the template.

Phase 1's open question 5 is **closed** by §"The `expired-binary-read-safety` regression is
preserved, and stops being racy": the position is preserved and becomes deterministic. Phase 3
carries the proof obligation, not an open decision.

## References

- Phase 1: `./phase1-high-level-design.md`
- `specs/reference/ASSET_LIFECYCLE.md` §"the one evaluation path" — the invariant restored
- `specs/design/expired-binary-read-safety/` §"Expiry is an error" and the B1 resolution — the
  owner-decided semantics this design must not disturb
- `specs/design/dependency-scheduling/` — the execution-time expiry policy the rule implements
- `specs/design/evaluate-path-consolidation/phase3-examples.md` C8/C10 — the corner cases
