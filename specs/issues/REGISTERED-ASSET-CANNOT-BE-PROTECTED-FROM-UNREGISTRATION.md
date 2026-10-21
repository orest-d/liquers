---
id: REGISTERED-ASSET-CANNOT-BE-PROTECTED-FROM-UNREGISTRATION
kind: feature
title: A registered asset cannot be protected from unregistration, so asset users cannot rely on converging on one asset
status: draft
priority: P3
complexity: L
area: [core/assets]
design:
created: 2026-10-20
github:
---

## Problem

Registration of an asset in the asset manager's key or query map is a revocable cache entry
(contract R2 in `specs/design/asset-registration-contract/`): expiry, `remove`, replacement by
`set_state` / `set_binary`, volatility eviction and, once designed, garbage collection may each end
it at any time. Nothing lets a caller say "this asset must stay registered".

That is correct for caching, and it blocks one class of feature: using an asset as a
**communication channel between asset users**, where several holders of the same key observe one
evaluation through its notification channel. They converge on the same asset only while its
registration lasts. Operations such as `AssetManager::set_state(key, state)` racing an
unregistration have the same need.

## Impact

Nothing is incorrect today, and no feature needs this yet. It records the contract a future
communication-channel feature would need, so that feature is not built on registration as it
is today.

## Expected behaviour

A per-asset protection flag (working name `protected_registration`). While it is set, the manager
must keep the asset registered. Open question, the reason this needs a design: **what happens
when a protected asset expires?** Expiry normally replaces the asset (an `Expired` asset is not
claimable, `AssetRef::try_claim_for_run` refuses it, so the next request builds a new one). The
candidates are:

1. expiry downgrades the protection;
2. expiry is refused while protected;
3. the asset stays registered, reports `Expired` and is recomputed in place. This needs a new
   `Expired` → `Recipe` transition and a claim that accepts it.

Each changes the expiration cascade differently. Garbage collection would treat a protected asset
as not collectable (`specs/design/asset-garbage-collection/`).

## Discovery

Split off on 2026-10-20 from `ASSET-REGISTRATION-OWNERSHIP-CONTRACT` (its expected behaviour, point
3) during bulk design. The rest of that issue is designed in `asset-registration-contract`, which
recommends deferring this part until a feature needs it.
