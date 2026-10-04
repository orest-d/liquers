---
id: IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES
kind: issue
title: ImmediateAssetManager's lazy deadline expiry tests a condition that can never be true
status: closed
priority: P2
complexity: S
area: [core/assets]
design: dependency-audit-and-expiry-provenance
created: 2026-09-28
github:
---

## Problem

`ImmediateAssetManager` has no expiration monitor. `track_expiration` does nothing, and the
comments call the replacement "lazy expiration-on-access". The replacement is in
`get_asset` (`liquers-core/src/assets.rs:6823`) and in the keyed lookup (`:6911`):

```rust
if status == Status::Ready && assetref.is_expired().await {
    let _ = assetref.expire_without_cascade().await;
```

`AssetRef::is_expired` (`assets.rs:3365`) is `self.data.read().await.status == Status::Expired`, not
a deadline check. `status` was read just before as `Ready`, so the condition is true only if another
task changes the status between the two reads. Except for that race the branch is dead, and an asset
whose `expiration_time` has passed is served as `Ready` for as long as it stays in the map.

The deadline check the code seems to mean is `assetref.expiration_time().await.is_expired()`
(`expiration.rs:822`).

## Impact

On an immediate (inline) environment, finite `expires:` declarations never take effect for an asset
that is already in memory. `liquers-web` uses the immediate manager, so this reaches the browser. The
branch also calls `expire_without_cascade`, so if it were reached, dependents would still not be
expired. That differs from the queued manager's monitor, which calls `expire()` and cascades.

Workaround: none short of evicting the asset by hand.

## Expected behaviour

The check compares the deadline, not the status. Whether lazy expiry cascades should be decided
deliberately and made consistent with the queued manager. A test on `ImmediateEnvironment` with a
short `expires:` shows the asset becoming `Expired` on access.

## Discovery

Found 2026-09-28 in Phase 2 of `design/dependency-audit-and-expiry-provenance/`, while listing every
route into `Expired` to give each one an expiry reason. The immediate manager's deadline route
turned out to be unreachable.

## Resolution

Fixed 2026-10-02 by `design/dependency-audit-and-expiry-provenance/` (Step 5, orest-d/liquers#75).
Both lazy checks now test `expiration_time().await.is_expired()` and expire the asset with reason
`Direct{Deadline}`. As the approved design specified (Phase 2, Example 6), they still use
`expire_without_cascade`, unlike the queued monitor. Whether lazy expiry should cascade remains open;
it is filed as `IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-DOES-NOT-CASCADE`. Evidence:
`immediate_manager_deadline_fires` (`tests/expiration_integration.rs`), which fails without the fix.
