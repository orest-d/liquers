---
id: ASSET-EXPIRED-CACHED-BINARY-READ
kind: issue
title: Expired asset can still return its cached binary on read
status: draft
priority: P0
complexity: M
area: [core/assets, core/store]
design: expiration-safety
created: 2026-08-08
github:
---
> **Needs verification.** `expiration-safety` is `complete` and PR #11 merged; whether that work
> resolved this could not be determined from the code during the 2026-08-08 migration triage. It is
> carried forward unchanged as a live P0 because the safe reading of an unverifiable P0 is that it
> is still live. Confirm or close it against PR #11 before scheduling anything else here.

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
