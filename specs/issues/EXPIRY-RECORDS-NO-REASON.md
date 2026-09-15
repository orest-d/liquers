---
id: EXPIRY-RECORDS-NO-REASON
kind: issue
title: An asset that becomes Expired records no reason, and the one path that does names the dependency by asset id
status: draft
priority: P2
complexity: M
area: [core/assets]
design:
created: 2026-09-04
github:
---

## Problem

`Status::Expired` is reached by several distinct routes, and none of them leaves a usable record of
*which* route it was or *what* caused it.

**`mark_expired_status` records nothing at all** (`liquers-core/src/assets.rs:2920`). It flips
`Ready`/`Override` to `Expired`, sets the same status on the metadata record, sends
`AssetNotificationMessage::Expired`, and — for a keyed asset with a store entry — persists the new
status. It adds no `LogEntry`. Every caller inherits that silence:

- a finite expiration deadline firing on the queued manager's expiration monitor;
- lazy detection during manager access on the immediate manager;
- `AssetRef::expire`, called directly or through the web API;
- `expire_without_cascade`, called on **every** dependent reached by
  `cascade_expire_dependents` — so a cascade of any depth records nothing on any of the assets it
  expires.

So an operator looking at a stored sidecar sees `status: Expired` and a log that stops at the
successful evaluation. Nothing distinguishes "its TTL elapsed" from "an ancestor five edges away
was invalidated" from "somebody called `expire()` by hand".

**The one path that does record something names the wrong thing.**
`note_expired_dependency` (`:1515`) writes:

```rust
LogEntry::warning(format!(
    "Dependency asset {} expired during evaluation; using its stale value and \
     marking this asset expired for recomputation on next access",
    dependency.id()
))
```

`dependency.id()` is the runtime `u64` asset id. It is not the key, not the query, and it is not
stable across processes, so it identifies the dependency only to someone with a live debugger
attached to the same run. The message a reader needs is closer to *"expired because dependency
`data/input.csv` expired while evaluating `data/report.txt`"* — both ends named by what they are.

## Impact

Expiry is the mechanism the whole caching contract rests on, and it is the one state transition
that leaves no evidence. Debugging "why did this recompute?" or "why is this served as expired?"
currently means reasoning backwards from the dependency graph rather than reading the asset's own
log — which is what the log exists for. The cost falls hardest on cascades, where the asset that
actually expired may be several edges from the one being investigated.

Nothing is incorrect and no value is lost, so this is P2: the information is reconstructible, just
not recorded.

## Expected behaviour

Every transition into `Expired` records **why**, in the asset's metadata, written before the status
is persisted so the reason reaches the store with it. The four routes must be distinguishable:

| Route | Reason to record |
|---|---|
| Deadline elapsed | the expiration time that fired |
| Cascade from a dependency | the dependency key that triggered it, and that this was a cascade |
| Explicit `expire()` | that it was requested, not derived |
| Stale dependency consumed mid-evaluation | the dependency **key or query**, and this asset's own, instead of two runtime ids |

### Record it as a typed field, not as a new status

This was analysed at the request of the project owner on 2026-09-15, who asked whether a distinct
status (`Stale`) should mark the fourth row. **Recommendation: no new `Status` variant; a typed
reason in `MetadataRecord` instead** — something of the shape

```rust
enum ExpiryReason { Deadline, Cascade, Explicit, StaleDependency }
```

carried as an `Option<ExpiryReason>` alongside the existing status, and projected into `AssetInfo`.

**Why not a status.** A status drives behaviour, and no site would behave differently. Reads decline
either way; `get`/`get_binary` error either way; `*_any_status` and `to_override` recover either
way; `try_fast_track` refuses either way; the axum handlers answer either way; expiration
scheduling skips an already-expired asset either way.

The one site that looks like an exception is `DependencyManager::track_asset`, which refuses an
`Expired` asset and so declines to register a stale-dependency asset's new content version. That
difference is **not** provenance: it is that one asset has new content and the other does not. A
deadline-expired asset's version is already in the map, so `register_version` reports no change and
does nothing. Both are correct through one code path; what blocks them is the gate, not a missing
distinction. (See `specs/design/stale-dependency-status-finalization/` Phase 2.)

**What a status would cost**, measured against HEAD:

- roughly **65 explicit match arms across 9 files in 5 crates** — the no-default-arm rule makes
  every one mandatory;
- it **crosses the public boundary**: `Status` is in the `liquers-py` and `liquers-web` bindings,
  so a new variant is breaking for downstream code that matches on it;
- it **breaks store forward-compatibility**: `Status` derives `Deserialize` with no
  `#[serde(other)]` fallback, so an older binary reading a newer sidecar fails to deserialize the
  whole `MetadataRecord`, not merely the status field. A store shared between versions degrades
  from "wrong status" to "unreadable metadata";
- the compiler catches a **missing** arm, not a **wrong** one. Sixty-five sites each get a chance
  to sort the new variant into the wrong bucket, and the uniform treatment of `Expired` that
  `expired-binary-read-safety` deliberately established would have to be re-created by hand at
  every one.

**And it would contradict the settled semantics.** `Expired` has one meaning — the data is stale —
with routes differing only in provenance (project owner, 2026-09-15). Putting provenance into the
status encodes history where the enum records state, and provenance has more than two values: once
one route earns a status, the argument for the others is identical.

**Why a metadata field is the right shape.** "Why is this expired?" is asked once, during
diagnosis; a status is consulted on every read by every consumer, in every binding. A typed field
is persisted with the asset so the question can be answered from the store after a restart, surfaces
in `AssetInfo` for UI and HTTP without obliging any consumer to handle it, extends to all four
routes, costs one optional field and zero match arms, and is additive for serde.

**What would justify revisiting it:** a consumer that must *act* differently — for example an HTTP
layer that serves a stale-dependency result with a warning header while refusing a deadline-expired
one. That is a legitimate product decision, and even then the metadata field carries it; the status
would need to fork only if the distinction had to be enforced at every read site by the type
system, which would assert a difference in meaning that does not exist.

### The human-readable half

The typed reason does not replace the log entry, it explains it. Alongside the field, each route
should add a message naming the participants — *"expired because dependency `data/sales.csv`
expired while evaluating `data/report.txt`"* — which is what the owner asked for originally.

Whether a given route logs at `warning` or `info` is worth deciding rather than defaulting: a
deadline elapsing is ordinary and reads as `info`; consuming a stale value is a departure from the
normal contract and reads as `warning`. Do not pick one on this issue's behalf.

### Two constraints a fix has to respect

- `mark_expired_status` already holds the `data` write lock when it changes the status, and computes
  its `persist_info` under that lock. The reason and the log entry must be written there, not after
  the lock is dropped, or the persisted metadata will again disagree with the in-memory record — the
  same ordering mistake as `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY`.
- Naming the dependency requires something better than `AssetRef::id()`. `asset_reference()` and
  the recorded `key` are both available and both mean something outside the process.

Complexity raised from S to M: a typed field that is persisted, projected into `AssetInfo`, and set
on four routes is more than the log line the issue originally described.

## Discovery

The `Stale`-status question was raised by the project owner on 2026-09-15 at the Phase 2 gate of
`stale-dependency-status-finalization`, and the analysis above is the answer.

Originally raised by the project owner on 2026-09-04 during Phase 2 of
`specs/design/stale-dependency-status-finalization/`, while reviewing that design's decision to
invalidate dependents through `cascade_expire_dependents`: a cascade that expires assets silently
is hard to reason about. Confirmed against HEAD while filing — `mark_expired_status` has no
`add_log_entry` call, and `note_expired_dependency`'s message interpolates `dependency.id()`.
