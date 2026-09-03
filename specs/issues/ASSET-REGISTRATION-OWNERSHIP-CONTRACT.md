---
id: ASSET-REGISTRATION-OWNERSHIP-CONTRACT
kind: feature
title: Registration of a keyed asset is a manager convenience, not a contract anything can rely on
status: draft
priority: P2
complexity: L
area: [core/assets]
design:
created: 2026-09-03
github:
---

## Problem

Whether a keyed asset is *registered* in the asset manager's key map is currently the manager's
private decision, and nothing else may depend on it. That is correct today and deliberately so:

- Registering a non-volatile keyed asset is a **caching** decision. Declining to register one still
  produces correct results — the asset is simply recomputed on the next request.
- A volatile keyed asset is deliberately never registered
  (`DefaultAssetManager::get_volatile_resource_asset`, `liquers-core/src/assets.rs:4292`).
- At most one asset is registered under a key at any time.

Two things follow, and neither is written down anywhere:

**1. Ownership is currently approximated by registration, and the approximation leaks.**
`AssetRef::bound_owner_key` (`:1557`) answers "do I own this key?" by looking the key up in the
manager's map and comparing asset ids. So a volatile keyed asset — which *is* associated with a key
and does write to the store — reports that it owns nothing. Consumers of `bound_owner_key`
(`mark_expired_status` `:2699`, `Context::owner_key` `:925`, `DependencyManager::track_asset`
`dependencies.rs:303`) therefore treat a legitimately keyed asset as unkeyed.

**2. Nothing can rely on registration, so a future feature cannot be built on it.** Using assets as
a **communication channel between asset users** — several holders of the same `AssetRef` observing
one evaluation through the notification channel — requires that they converge on the *same* asset,
which requires that registration be guaranteed rather than opportunistic. Under today's rules a
manager may legitimately decline to register, and the users would silently get separate assets.
That feature is not designed and not scheduled; this issue is the record that it needs a contract
first.

## Impact

No incorrect behaviour today. The cost is that ownership questions have no authoritative answer, so
each caller re-derives one (see `CORE-EVALUATE-PATH-CONSOLIDATION`), and that a whole class of
future feature is blocked on a guarantee the model does not currently offer.

## Expected behaviour

A stated contract covering four things.

### 1. Ownership is a property of the asset, registration is a property of the manager

A keyed asset — one created *for* a key, with that key recorded on it — is the owner of that key
for the purposes of writing to the store. Registration is a separate, manager-owned question about
sharing and caching. `specs/design/evaluate-path-consolidation/` records the key on the asset and
approximates ownership by "keyed", which is the interim answer; this issue is where the exact
contract belongs.

Interim mitigation adopted by that design: when a **non-registered keyed asset writes**, record a
**warning in its metadata**. The gray zone stays legal but stops being silent.

### 2. Registration should be monotonic, so it can be observed without a lock

Asking "is this asset registered?" from outside means looking the key up and comparing ids, which
is not atomic with respect to whatever the caller does next.

One property makes that safe without a lock: **once an asset is unregistered it is never registered
again.** Registration then has a single downward transition per asset, so

> registered at the start of an operation ∧ registered at the end ⟹ registered throughout.

An interval check replaces a held lock. This should be verified against the current eviction and
replacement paths and then stated as an invariant — `QUEUED-MANAGER-EVICTION-RACE` is adjacent and
should be read together with it.

### 3. Some assets may need registration guaranteed

Proposal: a `protected_registration` flag on the asset. When set, the manager is *required* to keep
the asset registered, turning registration from convenience into contract for that asset. This is
what a communication-channel feature would need, and what makes an operation such as
`AssetManager::set_state(key, state)` safe against an unregistration racing it mid-call.

**Open question, deliberately unresolved here:** what happens when a protected-registration asset
**expires**? Expiry normally evicts, which is precisely what protection forbids. Candidates: expiry
downgrades protection; expiry is refused while protected; the asset stays registered but reports
`Expired` and is recomputed in place. Each has consequences for the expiration cascade, so this
needs the design folder that `complexity: L` already requires.

### 4. The key belongs in metadata

A keyed asset and a non-keyed query asset built from the same query are **not** the same thing: the
keyed one knows its key. Today their states can be indistinguishable, which is part of why
delegation exists at all (see below). Recording the key in `MetadataRecord` makes the difference
observable to clients, to the store sidecar, and to tests.

## Relationship to delegation

Delegation in `AssetRef::evaluate_recipe` originates here. A keyed asset resolves to a recipe that
typically carries a query; that query could itself be requested from the manager and cached as a
**non-keyed** asset, so two assets end up representing the same computation with different identity.
Delegation reconciles them at evaluation time. With the key recorded on the asset and reflected in
metadata, the two are distinguishable by construction, and the reconciliation may be reducible or
removable — which is a question for this issue's design, not for the consolidation.

## Discovery

Raised on 2026-09-03 by the project owner while reviewing
`specs/design/evaluate-path-consolidation/` Phase 4. That design needs an answer to "which key does
this asset own?" and adopts "the key recorded at creation" as the working answer; the questions of
what registration guarantees, how it can be observed safely, and whether it can be made contractual
are larger than that design and are recorded here rather than settled inside it.
