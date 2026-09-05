---
id: EXPIRY-RECORDS-NO-REASON
kind: issue
title: An asset that becomes Expired records no reason, and the one path that does names the dependency by asset id
status: draft
priority: P2
complexity: S
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

Every transition into `Expired` adds a log entry to the asset's metadata saying why, before the
status is persisted so the reason reaches the store with it. At minimum the routes should be
distinguishable:

| Route | Recorded reason |
|---|---|
| Deadline elapsed | the expiration time that fired |
| Cascade from a dependency | the dependency key that triggered it, and that this was a cascade |
| Explicit `expire()` | that it was requested, not derived |
| Stale dependency consumed mid-evaluation | the dependency **key or query**, and this asset's own, instead of two runtime ids |

Whether these are `LogEntry::warning` or `LogEntry::info` is worth deciding rather than defaulting:
a deadline elapsing is ordinary and reads as `info`; consuming a stale value is a departure from the
normal contract and reads as `warning`. Do not pick one on this issue's behalf.

Two constraints a fix has to respect:

- `mark_expired_status` already holds the `data` write lock when it changes the status, and computes
  its `persist_info` under that lock. The log entry must be added there, not after the lock is
  dropped, or the persisted metadata will again disagree with the in-memory record — the same
  ordering mistake as `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY`.
- Naming the dependency requires something better than `AssetRef::id()`. `asset_reference()` and
  the recorded `key` are both available and both mean something outside the process.

## Discovery

Raised by the project owner on 2026-09-04 during Phase 2 of
`specs/design/stale-dependency-status-finalization/`, while reviewing that design's decision to
invalidate dependents through `cascade_expire_dependents`: a cascade that expires assets silently
is hard to reason about. Confirmed against HEAD while filing — `mark_expired_status` has no
`add_log_entry` call, and `note_expired_dependency`'s message interpolates `dependency.id()`.
