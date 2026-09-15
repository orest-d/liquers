# Phase 2: Solution & Architecture - Stale-Dependency Status Finalization

> **Revision 2 (2026-09-12).** Rewritten against HEAD after `keyed-expiry-cascade-fix` (PR #69)
> landed. Revision 1 was approved on 2026-09-04 and then invalidated in three places by the Phase 4
> review; the corrections are folded in here rather than appended, and §"What changed since
> Revision 1" records what moved and why, because one decision has now pointed three different ways
> and the reasoning is the valuable part.

## Overview

**The asset manager is the authority on status; the store is a mirror of it.** The rule that
follows — stated by the project owner at the Revision 2 gate and adopted here as this design's
governing principle — is that *whenever an asset expires, the store metadata is brought up to date*.
Best-effort: it does not close every window, but the store should never be knowingly left
disagreeing with the manager.

Verified against HEAD, the codebase already honours that rule **everywhere but one place**. There
are exactly two production writers of `Status::Expired`: `mark_expired_status` (`assets.rs:3112`),
which persists the new status for any keyed asset the store already holds, and the stale-dependency
relabel in `finish_run_with_result` (`:2415`), which writes nothing. This design closes that one
exception.

It closes it in the strongest available form. Rather than writing the value as `Ready` and issuing a
second, corrective metadata write, the status is *decided before the single write already happening*
— in `finalize_status_with_version`, the authority that already chooses between `Ready`, `Volatile`
and `Error`. One write, and no interval in which the store is knowingly wrong.

A second consequence is designed for rather than absorbed: `DependencyManager::track_asset` refuses
an `Expired` asset, and since computed keyed assets now carry real content versions, that refusal
would silently drop the dependent invalidation such an asset owes. `evaluate`'s last step therefore
registers the version directly for this one case.

No type is added, no public item changes, and only `liquers-core` is touched.

## What changed since Revision 1

Revision 1 was written against code in which computed keyed assets had no version. Three of its
decisions do not survive that assumption being fixed.

**Revision 2 was also corrected at its own gate**, by the project owner, on two points that are
folded in above rather than appended: the governing principle is that the manager is authoritative
and the store is kept up to date on every expiry (§Overview), and `Expired` has a single meaning —
stale data — with routes differing only in provenance (§"The dependency-manager step"). Revision 2
argued for two meanings; that argument is withdrawn, and the decision it supported survives on a
better one.

| Revision 1 said | Now |
|---|---|
| Rename `try_to_set_ready` → `finalize_status`, since `Ready` is one of four outcomes | **Dropped.** The authority is already `finalize_status_with_version` (`assets.rs:1957`), with `try_to_set_ready` (`:1947`) a thin wrapper passing `None`. The rename this design wanted has happened, under a better name |
| `serialize_to_binary` consults the gated `poll_state`, so finalizing `Expired` before persistence would prevent the write entirely | **Resolved upstream.** It reads `poll_state_any_status()` (`:2911`); `SERIALIZE-TO-BINARY-CONSULTS-THE-READ-GATE` is closed. `prepare_version` also installs the serialized bytes into `lock.binary` (`:1930`), so `save_to_store`'s ungated `binary_unchecked()` usually answers first and the fallback is not reached |
| The DM step should call `cascade_expire_dependents` | **Wrong then and now**, for a reason Revision 1 did not have: the cascade removes `versions[K]`, discarding the content version the asset has just earned |
| (Phase 4 review) The DM step should do nothing and let `track_asset` early-return | **Correct then, wrong now.** `register_version` used to see `Version(0) → Version(0)` and expire nothing, so skipping it cost nothing. It now sees a real change and calls `expire_stale_dependents` |

**The DM step has been "cascade", "nothing", and now "register directly" — each correct for the code
as it stood.** Only the last is available in a system where computed assets have versions, because
before there was no version to register. Recording this is the point of the table: the next reader
should not re-derive an earlier answer from a stale premise.

## Known-Issue Preflight

Searched: issues linked from `DESIGN.md`; every `draft`/`accepted`/`in_progress` row in
`specs/index.csv` whose `area` includes `core/assets`, `core/store` or `axum`; and the expiry design
folders, now including `keyed-expiry-cascade-fix`.

| Issue | Status | Priority | Relevance and solution impact | Blocking? | Action |
|---|---|---|---|---|---|
| `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY` | draft | P1 | The issue this design fixes. Still live at HEAD: the relabel sits in `finish_run_with_result` (`assets.rs:2409-2422`), after persistence | no | Fix here |
| `EXPIRY-RECORDS-NO-REASON` | draft | P2 | Every route into `Expired` is silent about why. This design writes its reason *before* persistence, which is the ordering that issue must follow for the other routes. It now also carries the answer to "should a stale-dependency completion get its own `Stale` status?" — **no**, a typed `ExpiryReason` in metadata instead, analysed there rather than here | no | Monitor; this design sets the ordering precedent and does not block on it |
| `SAVE-TO-STORE-REPORTS-CANCELLED-WRITE-AS-PERSISTED` | draft | P2 | A cancelled write is recorded as `Persisted`. Orthogonal — this design changes what status is written, not what a skipped write reports | no | Independent |
| `CROSS-PROCESS-RELOAD-IS-UNTESTED` | draft | — | Filed by `keyed-expiry-cascade-fix` for the fixture Phase 3 also needs (`AsyncMemoryStore` is not shareable). **Phase 3's `SharedMemoryStore` is the same missing fixture** | no | Phase 3 builds it; consider closing that issue with this work |
| `ASSET-REGISTRATION-OWNERSHIP-CONTRACT` | draft | P2 | Ownership is approximated by keyedness. This design's DM branch relies on `bound_owner_key()`, which is the ownership-aware side of that approximation | no | Monitor |
| `ASSET-FINISHED-PROGRESS-CONTRACT-UNDEFINED` | draft | P3 | Same region of `finish_run_with_result`. This design's decision is taken under the `data` write lock, not via the service loop, so it does not join that race | no | Independent |
| `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS` | draft | P2 | Touches `run_with_future_inline`, one of the two harnesses losing the relabel. Claim and waiter handling are untouched | no | Independent |
| `BUILD-SYSINFO-REQUIRES-NEWER-RUSTC` | draft | P2 | Blocks `cargo test -p liquers-lib --lib --tests` in this environment; `keyed-expiry-cascade-fix` hit it and recorded the `--no-default-features` workaround | no | Phase 4's validation uses the workaround and says so |

**No blocker.** Nothing must be resolved first, and no assumption here rests on an unresolved issue.

## Data Structures

**None added or changed.** `AssetData.stale_dependency: bool` (`assets.rs:584`) keeps its type, its
initializer (`:955`), and its only writer, `note_expired_dependency` (`:1547`). Only its reader
moves.

No new field records the outcome: `status` already holds it, and a second source would reintroduce
the duplication `evaluate-path-consolidation` Phase 5 §2 rejected for `payload_required`.

## Trait Implementations

None added or changed on `AssetManager`, so no implementor — `DefaultAssetManager`,
`ImmediateAssetManager`, or anything in `liquers-py` — needs a change.

**One extraction inside `DependencyManager`** (see §"The dependency-manager step"): the keyed branch
of `track_asset` becomes a `pub(crate)` method that `track_asset` and `evaluate` both call. This is
a refactor, not new behaviour — no signature that exists today changes.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|---|---|---|
| `finalize_status_with_version` | Yes — unchanged | Holds `self.data.write().await`. The added branch is pure computation over fields already under that lock |
| `DependencyManager::track_keyed_asset` (extracted) | Yes — as the code it is extracted from | Takes DM maps and, through `version_for_tracking`, an asset write lock |

No blocking I/O, no new lock held across an `.await`. The added branch runs inside the write
transaction `finalize_status_with_version` already opens, which is what makes it atomic with the
status.

## Function Signatures

### `liquers-core/src/assets.rs`

`finalize_status_with_version` keeps its signature. Its decision gains a third input, read from the
lock it already holds:

```
if data is present:
    volatile (is_volatile || metadata.expires().is_volatile())  -> Status::Volatile   [unchanged]
    else if stale_dependency                                    -> Status::Expired    [moved here]
    else                                                        -> Status::Ready      [unchanged]
else:
                                                                -> Status::Error      [unchanged]
```

Four properties of the moved branch, each the correction of a way to get it wrong:

1. **It goes through `AssetData::set_status`**, which writes `self.status` *and*
   `self.metadata.set_status(...)`. Setting only the field reproduces the original defect one layer
   down — the store would still receive `Ready`.
2. **The warning moves with it**, written under the same lock, so the reason reaches the store with
   the value. Today it is added after persistence and the stored sidecar carries neither the status
   nor the explanation. This is the ordering `EXPIRY-RECORDS-NO-REASON` will need for the other
   expiry routes.
3. **`expiration_time` mirrors the `Ready` arm** — the same `set_expiration_time_from(&metadata_expires)`
   and `lock.expiration_time` update — so `finish_run_with_result`'s scheduling step is unaffected.
4. **Errors are logged, not discarded.** The surrounding function reports a failed metadata write as
   a `LogEntry::warning`; the new branch matches that rather than using `let _ =`.

**The version is unaffected and must stay so.** `prepare_version` (`:1904`) runs before finalization
and derives the version from content. Staleness is a statement about *freshness*, not about content:
two evaluations producing identical bytes must produce identical versions whether or not a
dependency expired mid-run. The branch therefore touches status and metadata, never
`prepared.version`.

### The removed block

`finish_run_with_result` loses `:2409-2422` — the relabel and its comment. Its own fallback
`try_to_set_ready()` (`:2385`), for a run that finished without `evaluate` finalizing, thereby gains
the rule. That is correct and free: the fallback does not persist, so there is no ordering cost.

### `liquers-core/src/dependencies.rs`

Extract the keyed branch of `track_asset` (`:348`) so both callers share one definition of what
registering a keyed graph node means:

```rust
/// Register a keyed asset as a graph node: its current version, and its incoming edges.
///
/// Split out of [`Self::track_asset`] so the evaluation path can reach it for an asset whose
/// status that method's gate refuses — see `stale-dependency-status-finalization`.
pub(crate) async fn track_keyed_asset(
    &self,
    asset: &crate::assets::AssetRef<E>,
    key: &Key,
    records: &[DependencyRecord],
) -> ExpiredDependents<E>;
```

`track_asset` calls it after its status gate and `bound_owner_key()` lookup; nothing about its
behaviour changes.

## The dependency-manager step

This is the design's one substantive decision and the one that has moved most.

### What `track_asset` does, and why its gate excludes us

`track_asset` (`dependencies.rs:348`) gates on status, admitting only `Ready | Source | Override`,
then for a keyed asset does two things:

- `register_version(dep_key, version_for_tracking())` — records this content as the key's current
  version, and, when the version *changed*, calls `expire_stale_dependents` (`:196`) to invalidate
  dependents still holding the previous one;
- `load_from_records(dep_key, &deps)` — registers this asset's incoming edges, so a later expiry of
  its dependencies reaches it.

**`Expired` has one meaning: the data is stale.** What differs between routes into it is
provenance — data that was valid and has since gone stale, versus an execution that never produced
valid data, only expired data. The exceptional flow that produces the second (use the stale input
rather than restart) exists to avoid an unbounded recompute loop; it does not give the status a
second meaning. This is the project owner's correction to Revision 2, which argued the opposite and
was wrong.

The gate is therefore not conflating two meanings of `Expired`. It is conflating **status with
version** — freshness with content identity. Those are independent facts:

- the **version** answers *"what content does this key hold?"*;
- the **status** answers *"is that content fresh?"*.

An asset expired by a TTL holds the same content it registered, so the gate costs nothing there:
re-registering would be a no-op. A stale-dependency asset holds **new content** — the evaluation ran
and produced a different value, with a different hash — and is stale from birth. Refusing to record
that content leaves the graph asserting that the key still holds the *previous* content, which is
simply untrue, and leaves every dependent built on that previous content uninvalidated.

So the registration is not an assertion that the asset is fresh. It is an assertion about what the
key contains, which the status then independently qualifies.

### The branch

```
if volatile          -> nothing                      [unchanged: not a graph node]
else if stale_dependency:
        bound_owner_key() == Some(key)  -> track_keyed_asset(self, &key, &deps)
        bound_owner_key() == None       -> nothing
else                 -> track_asset(self)            [unchanged]
```

**`bound_owner_key()`, not `lock.key`.** This is the ownership-aware derivation `track_asset` itself
uses (`dependencies.rs:370`), and using it is what makes the delegation case safe without a special
branch: a delegating asset can carry `stale_dependency` — delegation runs through
`wait_for_dependency`, the same call that sets the flag — and `bound_owner_key()` returns `None` for
a keyed non-owner, so such an asset registers nothing. `lock.key` would have had it overwrite the
real owner's version.

**Rejected — `cascade_expire_dependents`.** It removes `versions[K]` and `keyed_dependents[K]`
(`expire_internal`), discarding the version the asset just earned and the edges it just recorded.
It was proposed in Revision 1 on the belief that it was `track_asset`'s invalidation without the
registration; it is neither.

**Rejected — do nothing.** Correct only while computed assets had no version. With real versions
this silently drops the dependent invalidation, which is the whole reason the step exists.

**Not changed — `track_asset`'s gate.** Widening it to admit `Expired` would be wrong for every
other caller and every other route into that status. The exception is knowledge the *call site* has
— that this particular `Expired` means "fresh but uncacheable" — so the call site is where it
belongs.

### No `Expired` notification

`mark_expired_status` sends `AssetNotificationMessage::Expired`; this path deliberately does not.
That message announces a value being withdrawn from service. Here nothing was ever served — the
asset is born expired, and `ValueProduced` followed by `JobFinished` are the truthful messages.
Adding `Expired` would tell a waiter that something it never received had been taken away.

Scope note for the claim: `mark_expired_status` remains the only sender of `Expired` *for this
asset*. Dependents invalidated by `expire_stale_dependents` are expired through that helper and do
notify, which is correct — for them, a served value is being withdrawn.

## Integration Points

**`liquers-core/src/assets.rs`** — the only file with behaviour changes.

| Site | Line (HEAD 2026-09-12) | Change |
|---|---|---|
| `finalize_status_with_version` | `:1957` | Add the `stale_dependency` branch; move the warning into it |
| `evaluate` — post-finalize read | `:2736-2746` | Read `stale_dependency` alongside the three facts already read |
| `evaluate` — DM step | `:2762-2769` | The three-way branch above |
| `finish_run_with_result` — relabel | `:2409-2422` | Removed |
| module rustdoc | `:~200` | The read-exposure table's `Expired` row names `finish_run_with_result` as where the label is applied |

**`liquers-core/src/dependencies.rs`** — one extraction, no behaviour change (`:348`).

**Crates not touched:** `liquers-store`, `liquers-lib`, `liquers-axum`, `liquers-web`, `liquers-py`,
`liquers-macro`. No public item changes: `finalize_status_with_version` is private and
`track_keyed_asset` is `pub(crate)`. No `Cargo.toml`, no feature gate — `assets.rs` is
unconditional core code, so every `check-build-matrix.sh` configuration compiles the same source.

## Documentation Architecture

### Reference Plan

**Extend three; create none.**

| Path | Change |
|---|---|
| `specs/reference/ASSET_LIFECYCLE.md` | Step 6 says status is finalized before the notification and before persistence. Name the four outcomes it decides between, including the stale-dependency one. Add the DM branch to step 8 |
| `specs/reference/ASSETS.md` | §Expiry (`:241-244`) attributes the relabel to `finish_run_with_result`. Retarget to `finalize_status_with_version`, and say such an asset is *born* expired rather than relabelled — which is what keeps the `*_any_status`/`to_override` sentence beside it correct |
| `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` | `:246-248` says the parent "records the stale dependency and finishes as `Expired`". Add that the store agrees, and that its dependents are invalidated |

Each gets a `## History` row and a `reviewed:` bump in the same commit (§9.2). Whether
`keyed-expiry-cascade-fix` created a versions reference that should also carry the interaction is
checked when Phase 5 reviews `affects_docs`.

### Guide Plan

**None.** No repeatable task a developer performs; the recovery workflow a caller does perform is
already documented with the `*_any_status` family in `ASSETS.md`.

### Other Documents to Create

**None.** Findings that outlive this design are filed as issues, as five already have been.

### Existing Documents to Review or Update

`affects_docs` is `[ASSET_LIFECYCLE, ASSETS, DOC_03_ASSETS_EXECUTION_LIFECYCLE]`. Candidates
generated by `area: core/assets` and rejected with reasons: `ASSET_SET_OPERATION` (`set`/`set_state`
do not enter `evaluate`), `DEPENDENCIES_STATUS` (a scheduling state left before evaluation ends),
`PROJECT_OVERVIEW` and `DOC_01` (no concept or architecture change), `DOC_08` (no recipe or plan
change), `ENVIRONMENT_CONFIG` / `ENVIRONMENT_CONSTRUCTION_GUIDE` (nothing configurable changes),
`LANGUAGE-INTEGRATION_GUIDE` (no public item changes).

Also updated as documents rather than `affects_docs` entries:
`specs/issues/ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY.md`, `specs/README.md`, `specs/index.csv`.

### Evidence to Collect During Implementation

- Whether the DM branch changes any existing expiration test's expectations — the measurable form of
  "is registering an `Expired` asset's version acceptable?"
- Whether `test_wait_for_retained_expired_dependency_labels_asset_expired_on_completion` passes
  unchanged. It uses a non-keyed asset, so it should.
- Whether the three-link regression fixture from `keyed-expiry-cascade-fix` still behaves when the
  middle asset is stale-dependency rather than TTL-expired.
- Confirmation, in a test, that between `ValueProduced` and the end of the run the asset is never
  observable as `Ready`.

## Relevant Commands

**None.** No `register_command!` invocation is added, changed or removed, so
`specs/command_registry.yaml` is not regenerated and `registry_export` is unaffected. This design
has no query-reachable surface, so no namespace is relevant — answered rather than asked, as
`expired-binary-read-safety` did for the same reason.

## Web Endpoints

**None.** No route, handler or response shape changes. The visible difference is that `AssetInfo`
for a stored stale-dependency asset reports `Expired` instead of `Ready` — the correction — and the
axum handlers already have explicit `Status::Expired` arms (`query/handlers.rs:113`, `:261`).

## Error Handling

No new error type, no new `ErrorType`, no `Error::new`, no `unwrap`/`expect`.

| Scenario | Handling |
|---|---|
| `metadata.set_status(Expired)` fails | `LogEntry::warning`, as every other arm of `finalize_status_with_version` does |
| Persisting the `Expired` value fails | Unchanged: `record_persistence_result` records `PersistenceStatus` and keeps the value |
| `bound_owner_key()` returns `Err` | Treated as `None` — no registration — matching `track_asset`'s own `.ok().flatten()` |

## Serialization Strategy

Unchanged. `Status` already round-trips in `MetadataRecord`, and `try_fast_track` reads `Expired`
back and refuses it (`:1066`) — the mechanism the whole fix relies on. No serde annotation changes.

## Concurrency Considerations

- **The decision is atomic.** `stale_dependency` is written by `note_expired_dependency` under
  `data.write()` and read by `finalize_status_with_version` under the same lock. Ordering is
  stronger still: every dependency wait completes inside `apply_recipe`, which
  `evaluate_recipe_outcome` awaits fully before `evaluate` reaches finalization, so the flag cannot
  arrive after the decision. Phase 3 asserts this rather than assuming it.
- **One transaction with the version.** The branch runs inside the write transaction that installs
  the version and the status together — the invariant `keyed-expiry-cascade-fix` established so no
  observer can see a readable asset without its version. Adding the status decision to that same
  transaction preserves it.
- **No new lock, no new ordering.** The DM call happens after the `data` guard is released, the
  shape `evaluate` already uses for `track_asset`.
- **Both harnesses, one rule.** `run_with_future` and `run_with_future_inline` share
  `finish_run_with_result`, so the defect is on both today; moving the rule into `evaluate`'s
  finalization reaches both without depending on the service-message loop, whose termination is what
  made the current placement unfixable in place.
- **wasm.** No `tokio::` primitive is introduced, so the inline path stays spawn-free.

## Open Questions

1. **Is registering an `Expired` asset's version acceptable?** The document argues yes — the version
   describes content, the status describes freshness, and they are independent facts. The
   alternative is to accept losing dependent invalidation for this case. This is the decision to
   confirm.
2. **Should the extraction be a `DependencyManager` method, or should `evaluate` call
   `register_version` and `load_from_records` itself?** Both are `pub`, so the call site could do it
   with no DM change at all — at the cost of a second definition of "register a keyed node" that can
   drift. Recommended: the extraction.

## References

- Phase 1: `./phase1-high-level-design.md`
- `specs/design/keyed-expiry-cascade-fix/` — versions for computed keyed assets; the work that
  unblocked this design and reversed its DM decision
- `specs/design/expired-binary-read-safety/` §"Expiry is an error" — the owner-decided read
  semantics this design preserves. Its uniform treatment of `Expired` on every read is consistent
  with the one-meaning reading above; its finding B1 described the two *provenances*, not two
  meanings
- `specs/design/evaluate-path-consolidation/` — the one evaluation path and the C8/C10 corner cases
