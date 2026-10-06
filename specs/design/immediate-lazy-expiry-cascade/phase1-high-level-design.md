# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question — should lazy expiry cascade, and how far.**
  `dependency-audit-and-expiry-provenance` deliberately kept it non-cascading (its Phase 2,
  Example 6) and left the decision open.
- **Explanation:** Cascading on access is a two-line change through an existing primitive, and
  aligns the immediate manager with the queued one for the common case. It cannot cover a
  dependent that is read before its root is touched. That residual difference is documented
  rather than fixed.
- **Open questions:**
  1. **Proposed resolution — cascade on access:** the lazy path calls
     `expire_with_reason(Direct{Deadline})` instead of `expire_without_cascade`, so dependents get
     `Cascaded{Deadline, root, via}` as on the queued manager.
  2. **Open design question — dependent read first:** a `Ready` dependent whose root's deadline
     has passed but whose root was not accessed is served stale. Options: (a) accept and document
     (recommended for this S-sized issue), with `trigger_dependency_audit` / `OnLoad` as the
     remedy; (b) on every keyed lookup, walk recorded dependencies and check their deadlines.
     That is an O(dependencies) read per lookup, and a separate design.

## Problem

`ImmediateAssetManager` has no expiration monitor. It expires an asset whose deadline passed when
the asset is next looked up (`get_asset` and the keyed lookup in `liquers-core/src/assets.rs`).
That path calls `expire_without_cascade`. The queued manager's monitor calls `expire_with_reason`,
which also runs `cascade_expire_dependents`. So, with `b.txt` computed from `a.txt` and `a.txt`
declaring `expires: "in 1 sec"`, after a second plus a request for `a.txt` the queued manager has
both `Expired`, while the immediate manager leaves `b.txt` `Ready` and serves its old value.
`liquers-web` uses the immediate manager.

## Expected behaviour and acceptance

1. Immediate manager, after the deadline: a request for `a.txt` expires it, and `b.txt`'s stored
   and live status becomes `Expired` with `Cascaded{Deadline, root a.txt, via a.txt}`.
2. A subsequent request for `b.txt` recomputes it.
3. A shared scenario in `tests/common/manager_scenarios.rs` asserts 1–2 on both managers (on the
   queued manager the monitor does it without the access).
4. The residual case (dependent read first) is documented in `DEPENDENCIES_STATUS.md` with its
   remedy.

## Scope and non-goals

Non-keyed (query) assets have no key to cascade from. `expire_with_reason` already does nothing
extra for them, so they behave as today. A monitor for the immediate manager is out of scope.

## Design Dependencies

- `dependency-audit-and-expiry-provenance` — **overlaps** (complete). It owns the deadline cause
  and `cascade_expire_dependents`.

## Documentation assessment

- Reference: extend `specs/reference/DEPENDENCIES_STATUS.md` (immediate-manager expiry paragraph).
  Possibly `ASSETS.md` if it states the non-cascade.
- Updates: the code comments at both lazy sites, which name this issue as open.

## Consolidated Findings

- Both lazy sites (query lookup and keyed lookup) must change together. Only the keyed one has
  dependents in the graph, but keeping them identical avoids drift.
- Cascade before taking `key_mutation_lock` at the keyed site, as today's
  `expire_without_cascade` call does. The queued monitor also calls `expire_with_reason` without
  that lock, so the cascade path is known to be safe there. Re-check that
  `cascade_expire_dependents` never takes `key_mutation_lock` re-entrantly (it calls
  `expire_without_cascade` on dependents and writes their store entries).
- The decision on question 2 decides whether the readiness becomes `ready` after question 1 is
  accepted.
