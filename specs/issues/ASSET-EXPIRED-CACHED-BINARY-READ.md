---
id: ASSET-EXPIRED-CACHED-BINARY-READ
kind: issue
title: Expired asset can still return its cached binary on read
status: closed
priority: P0
complexity: M
area: [core/assets, core/store]
design: expiration-safety
created: 2026-08-08
github:
---
> **Verified live, then fixed.** The "needs verification against PR #11" caveat is resolved: the
> bug *was* still live at HEAD. PR #11 gated `poll_state` and added `poll_state_any_status` but
> never touched the binary path — `AssetData::poll_binary` had no status check at all. Fixed by
> `design/expired-binary-read-safety`.
>
> **The fix went wider than this issue describes.** Because `poll_binary` ignored status entirely,
> every non-`Value` status leaked cached bytes, not only `Expired`; the gate covers all of them.
> Two further consequences fell out: `AssetRef::get` gained the same pre-wait expiry check (it
> blocked forever on an already-expired asset), and `Step::GetAssetBinary` was reconciled with the
> dependency contract. Verification item 4 is closed by making `get_binary_any_status` serialize on
> demand rather than only returning cached bytes.

## Problem

Normal asset reads are intended not to expose an expired value. `AssetData::poll_state`
returns `None` for `Status::Expired`, and stale-value recovery is explicit through
the any-status APIs.

Cached binary reads do not apply the same rule:

- `AssetData::poll_binary` returns cached binary data without checking status.
- `AssetRef::poll_binary` delegates directly to it.
- `AssetRef::get_binary` calls `poll_binary` before the expiration-aware `get`
  path.

Consequently, an asset whose status is `Expired` can still return stale bytes
through the normal `poll_binary` or `get_binary` API when a binary representation
is cached. This bypasses the normal expiration contract and is a bug.

## Expected behavior

1. Normal binary reads do not return data for `Status::Expired`.
2. Binary and state reads follow the same expiration policy.
3. Access to retained expired data remains possible only through an explicit
   recovery API.
4. The behavior is consistent for both cached binary data and binary data produced
   by serializing an in-memory value.

## Verification

Add tests that create an asset with both value and cached binary data, expire it,
and verify:

1. `poll_binary` returns `None`.
2. `try_poll_binary` returns `None`.
3. `get_binary` does not return the expired cached bytes.
4. Explicit any-status recovery still exposes retained expired state.
5. `Ready`, `Source`, `Override`, and `Volatile` binary behavior remains unchanged.
