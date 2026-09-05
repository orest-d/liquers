---
id: SERIALIZE-TO-BINARY-CONSULTS-THE-READ-GATE
kind: issue
title: The persistence path serializes through a gated read, so an asset at a hidden status cannot be stored
status: draft
priority: P2
complexity: S
area: [core/assets]
design: stale-dependency-status-finalization
created: 2026-09-05
github:
---

## Problem

`AssetRef::save_to_store` (`liquers-core/src/assets.rs:2604`) obtains bytes two ways, and applies
the "persisting is not a read" rule to only one of them.

The first is ungated, and says so:

```rust
// `binary_unchecked`, not `poll_binary`: persisting is not a read of the asset's exposed
// value, and this path runs at statuses the read gate hides. `AssetRef::set_state`
// persists with whatever status the caller supplied — reachable from `Context::set_state`
// — so consulting the gate here would turn a successful persist into an error.
let mut x = { self.data.read().await.binary_unchecked() };        // :2619
if x.is_none() {
    x = self.serialize_to_binary().await?;                        // :2624
}
```

The fallback is not:

```rust
async fn serialize_to_binary(&self) -> Result<Option<(Arc<Vec<u8>>, Arc<Metadata>)>, Error> {
    if let Some(data) = self.poll_state().await {                 // :2719 — GATED
        ...
    } else {
        Ok(None)
    }
}
```

`poll_state` returns `None` for `Status::Expired` (`:1199`, via `ReadExposure::Expired`,
`metadata.rs:368`). So when there is no cached binary — which is every freshly evaluated asset,
since `evaluate` installs `lock.data` and never `lock.binary` — an asset at a gated status
serializes to `Ok(None)`, `save_to_store` falls to its final `else`, and returns
`Err("Failed to obtain binary value for storing of the asset")`. The comment at `:2618` describes
exactly this outcome as the thing to avoid, six lines before it happens.

## Impact

Latent today, because nothing persists at a gated status. The evaluation path always reaches
persistence at `Ready` or `Volatile`, which the in-source comment at `:2552` maintains on purpose:
*"Must happen before persistence so poll_state() returns Some for serialization."* The ordering
satisfies the gate by accident of the statuses it produces, not by design.

The reachable-looking case is `set_state`, which clears `lock.binary` and then persists with
whatever status the caller supplied. Supplied an `Expired` state, it would fail to persist and
report the failure as an inability to obtain bytes. `Context::set_state` is `pub(crate)` with no
live caller found, so this is unreached rather than fixed.

P2 on that basis: no current path produces a wrong result, but the invariant that makes it safe is
undocumented and easy to break — as `stale-dependency-status-finalization` discovered by breaking
it.

## Expected behaviour

`serialize_to_binary` uses the ungated read, `poll_state_any_status()`, for the same reason
`save_to_store` already uses `binary_unchecked`. Verified: it has exactly two callers, and neither
wants the gate.

- `save_to_store` (`:2624`) needs it ungated.
- `AssetRef::get_binary` (`:3140`) checks `read_exposure` explicitly — twice, before and after its
  wait — and returns `expired_read_error` for `Expired` long before reaching it. The inner gate can
  never fire for this caller; if it somehow did, the caller would receive
  `Err("Failed to get binary")` instead of the purpose-built error, so removing it is also a small
  improvement.

`poll_state_any_status` differs from `poll_state` in exactly one arm — `Expired`
(`assets.rs:1261`) — so the change alters behaviour for one caller at one status and is provably a
no-op for the other. **No `_unchecked` twin should be created**: that would leave the gated original
with zero callers. Rename the function `serialize_to_binary_unchecked` instead, so the two byte
sources in `save_to_store` both announce that they bypass the gate — the asymmetry is what hid this
— with a doc comment noting there is deliberately no checked variant.

## Discovery

Found on 2026-09-04 by the cross-phase review of
`specs/design/stale-dependency-status-finalization/`, whose Phase 2 proposed finalizing an asset as
`Expired` before persisting it — the first caller to reach persistence at a gated status. That
design's Phase 3 had asserted the opposite ("`save_to_store` has no status gate"), having read
`save_to_store` and stopped one call short of `serialize_to_binary`.

This is the third instance of one pattern: a gate added to a read, with an internal caller that
needed the ungated version left behind. The other two — `ASSET-EXPIRED-CACHED-BINARY-READ` and
`DEPENDENCY-EXPIRED-STALE-VALUE-UNREACHABLE` — are closed. This one hid behind a fallback branch
that runs only when no binary is cached.

The fix is owned by `stale-dependency-status-finalization` (correction C1) since that design needs
it; filed separately because it predates that design and stands on its own if the design is
abandoned or delayed.
