# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** Decided (Maintainer decision, 2026-10-06): "Laziness is the method of finding out that expiry needs to be
  done — but once it is known that an asset expired, all the consequences should follow, i.e.
  cascade expiry." The change routes the lazy path through the cascading primitive the queued
  monitor already uses.
- **Open questions:** None. A dependent read *before* its expired root is touched is not a case
  where expiry "is known", so it stays as today, documented with its remedy (audit or `OnLoad`).
  Checking upstream deadlines on every dependent read would be a different mechanism (a separate
  issue if wanted).

## Problem

`ImmediateAssetManager` has no expiration monitor. It expires an asset whose deadline has passed
when the asset is next looked up (the query-map loop in `get_asset` and the keyed lookup loop in
`liquers-core/src/assets.rs`), via `expire_without_cascade`. The queued manager's monitor calls
`expire_with_reason`, which also runs `cascade_expire_dependents`. So with `b.txt` computed from
`a.txt`, and `a.txt` declaring `expires: "in 1 sec"`: after the deadline and a request for `a.txt`,
the queued manager has both `Expired`, while the immediate manager leaves `b.txt` `Ready` and
serves its old value. `liquers-web` uses the immediate manager.

## Expected behaviour and acceptance

1. Immediate manager, after the deadline: a request for `a.txt` expires it, and `b.txt` becomes
   `Expired` (live and stored) with reason `Cascaded{Deadline, root a.txt, via a.txt}`.
2. A subsequent request for `b.txt` recomputes it.
3. A shared scenario in `tests/common/manager_scenarios.rs` asserts 1–2 on both managers.
4. `DEPENDENCIES_STATUS.md` states that lazy expiry cascades, and names the dependent-read-first
   case with its remedy.

## Scope and non-goals

Non-keyed (query) assets have no key in the dependency graph, so `expire_with_reason` cascades
nothing for them, the same as today. A monitor for the immediate manager is out of scope.

## Design Dependencies

- `dependency-audit-and-expiry-provenance` — **overlaps** (complete). It owns the `Deadline` cause
  and `cascade_expire_dependents`.

## Documentation assessment

- Reference: `specs/reference/DEPENDENCIES_STATUS.md`, the immediate-manager expiry paragraph.
- Code: the comments at both lazy sites name this issue as open.

## Consolidated Findings

- Both lazy sites change together, to keep them identical.
- At the keyed site, cascade before taking `key_mutation_lock`, where the current call is. The
  queued monitor already calls `expire_with_reason` without that lock. Confirm in Phase 4 that
  `cascade_expire_dependents` never takes `key_mutation_lock` (it calls `expire_without_cascade`
  on dependents and writes their store entries).
