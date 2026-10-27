# Phase 1: High-Level Design — A registration asset users can rely on

## Design Readiness

- **Readiness:** phase2-blocked
- **Automatic fixing:** not-eligible — a new per-asset contract and `pub` API on `AssetManager` /
  `AssetRef` in `liquers-core` (rules 3 and 4), `L`, and not `ready` (rule 5).
- **Leading issue:** **Blocking - what a protected registration must survive.** The feature has no
  consumer, and the three answers to "what happens when a protected asset expires" change the
  asset lifecycle in incompatible ways (example below). Phase 2 shows that every other way a
  registration ends is either already harmless to a holder or is an explicit operation whose
  refusal is easy to specify, so expiry is the whole question.
- **Explanation:** Phases 1-2 inventory every way a registration ends at HEAD and in
  `asset-garbage-collection`, and specify the two parts that do not depend on the answer
  (the handle and the refusal of explicit replacement). Phases 3-4 are not written, because a test
  of "the users still converge after expiry" has a different expected result under each answer.
- **Open questions:**
  1. **Blocking - expiry of a protected asset.** Example: an asset `chat/session-1` used as a
     channel. User A holds it and listens on `subscribe_to_notifications()`; at 12:00 its recipe's
     `expires: in 1 hour` fires. User B asks for the key at 12:01.
     - *Downgrade (issue option 1):* the asset becomes `Expired`, leaves the map as today; B gets a
       new asset, A keeps the old one. A and B no longer converge, so the protection gave nothing
       at the one moment it was needed.
     - *Refuse (option 2):* the asset stays `Ready` and registered past its deadline; B gets the
       stale value. This breaks the expiration contract (`ASSETS.md` §Expiration), and every
       dependent of the key keeps a value its recipe declared invalid.
     - *Recompute in place (option 3):* the asset stays registered, goes `Expired` → `Recipe` and
       is re-run; A and B converge. This needs a new transition and a claim that accepts `Expired`
       (`AssetRef::try_claim_for_run` refuses it at HEAD), and breaks the invariant that an
       `Expired` asset never changes value again, which `wait_for_dependency`'s stale-dependency
       rule, the expiration cascade and `AssetRef::get` callers holding an `Expired` asset rely on.
     The answer is the consumer's requirement. **Recommended: build nothing until a consumer
     exists; then prefer following the key (Phase 2, alternative C), which needs none of the three.**
  2. **Open design - explicit replacement while protected:** `remove`, `set_state` / `set_binary`
     and `to_override` on a protected key: refuse with `StatusConflict`, or proceed. Recommended:
     refuse, with the protection's owner able to release it (Phase 2).
  3. **Proposed resolution - garbage collection:** nothing to add. `asset-garbage-collection`
     never collects an asset whose `AssetRef` a caller holds (`Arc::strong_count == 1` rule), so a
     holder already prevents collection; a protection handle is such a holder.

## Purpose

Several users of one key (a UI, an agent, a background producer) can observe one evaluation
through its notification channel only while the manager keeps that asset registered. Registration
is a revocable cache entry (R2 in `asset-registration-contract`), so no feature can use an asset as
a communication channel between its users. This design states what a "protected registration"
would have to guarantee, and why its expiry semantics must come from a real consumer.

## Problem Example

```rust
let m = envref.get_asset_manager();
let a = m.get(&key).await?;                  // user A: registered asset, id 17
let mut rx = a.subscribe_to_notifications().await;
// ... the asset's deadline passes; the expiration monitor expires it ...
let b = m.get(&key).await?;                  // user B: a new asset, id 18
// A listens on 17, B on 18: they no longer share a channel.
```

Correct caching today; nothing lets A say "keep 17 registered", and a third party's `remove` or
`set_state` diverges A and B the same way. Should be (once a consumer exists): while A holds a
protection, B converges on A's asset and a third party's replacement is refused.

## Scope and Acceptance Criteria

These scenarios hold under every answer to question 1; the expiry scenario is listed with its
expected result left open, which is why Phase 3 is not written.

- **AC-1** A held protection keeps users converged
  WHEN user A holds a protection for a key and user B requests the key, with no expiry, removal or
  replacement in between
  THEN B receives A's asset (same id) — true today as long as nothing ends the registration
- **AC-2** Garbage collection never ends a protected registration
  WHEN the key map is over its `max_cached_keys` limit and a protection for the key is held
  THEN the asset stays registered (follows from `asset-garbage-collection` AC-2)
- **AC-3** Explicit replacement is refused while protected (question 2)
  WHEN `remove`, `set_state`, `set_binary` or `to_override` targets a protected key
  THEN it fails with `StatusConflict` naming the protection, and the asset is unchanged
- **AC-4** Releasing the protection restores today's behaviour
  WHEN every protection for the key is dropped
  THEN the registration is an ordinary revocable cache entry again
- **AC-5** Expiry of a protected asset — **expected result open (question 1)**
  WHEN the protected asset's deadline passes and B then requests the key
  THEN *undecided*: B gets a new asset (downgrade), the stale asset (refuse), or A's asset
  re-evaluated (recompute in place)

Non-goals: protecting a query (non-keyed) asset; protection across processes or restarts; a
persisted flag in `MetadataRecord`; changing what `Override` / `Source` mean.

## Core Interactions

- **Assets:** both managers' registration maps; the expiration path; `remove`, `set_state`,
  `set_binary`, `to_override`. Holders learn of an ending through `Expired` / `Removed`
  notifications.

## Crate Placement

`liquers-core` only (`assets.rs`): the maps, the lifecycle and the manager trait live there.

## Documentation Intent

- Reference: extend `specs/reference/ASSETS.md` §Key ownership (written by
  `asset-registration-contract`) with protected registration, once decided.
- Guide: extend `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` §7 (an external manager must
  honour or refuse protection).
- Other documents: `specs/reference/ASSET_LIFECYCLE.md` if option 3 adds a transition.
- Documents to update: none until question 1 is answered.

## Design Dependencies

- `asset-registration-contract` (needs-decision, not implemented) — **requires**. It states R2
  (registration is revocable) and R4 (monotonic); a protection is an exception to R2 and must not
  break R4. It recommends deferring this feature (its decision 1).
- `asset-garbage-collection` (needs-decision, not implemented) — **overlaps**. Its strong-count
  reachability already protects a held asset (question 3); nothing to order.
- `external-index-sync` (`ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET`) and
  `ASSET-EVENT-STREAM-CARRIES-ONLY-EXPIRATIONS` — **overlaps**. A key-scoped event stream is the
  building block of alternative C (follow the key instead of pinning the asset).
- `assets-api-admin-operations` — **overlaps**, weak: its guarded `remove_computed` would also
  have to respect a protection (AC-3).
