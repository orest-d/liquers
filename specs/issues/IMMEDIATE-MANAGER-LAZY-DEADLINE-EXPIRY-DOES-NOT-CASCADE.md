---
id: IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-DOES-NOT-CASCADE
kind: issue
title: On the immediate manager, an elapsed deadline expires the asset but not its dependents
status: closed
priority: P3
complexity: S
area: [core/assets]
design: immediate-lazy-expiry-cascade
created: 2026-10-02
github:
---

## Problem

`ImmediateAssetManager` has no expiration monitor. It expires an asset whose deadline has passed
lazily, when the asset is next looked up (`get_asset` and the keyed lookup in
`liquers-core/src/assets.rs`). That path calls `expire_without_cascade`, with the reason
`Direct{Deadline}`. The queued manager's monitor calls `expire()`, which also cascades to the dependents.

Example: `b.txt` is computed from `a.txt`, and `a.txt`'s command declares `expires: "in 1 sec"`.

- On the queued manager, after one second, both are `Expired`. `b.txt`'s reason is
  `Cascaded{Deadline, root a.txt, via a.txt}`.
- On the immediate manager, `a.txt` becomes `Expired` only when it is next requested, and `b.txt`
  stays `Ready`. Requesting `b.txt` serves its old value, unless something else causes a
  fast-track dependency check.

## Impact

The two managers give different freshness for the same recipes. This matters most in the browser
(`liquers-web` uses the immediate manager). The design `dependency-audit-and-expiry-provenance`
kept the non-cascading behaviour deliberately (Phase 2, Example 6). It fixed only the condition
that made lazy expiry unreachable (`IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES`) and left
this decision open.

Workaround: run `trigger_dependency_audit`, or use the `OnLoad` audit policy. That catches the
case only if the root was expired and re-evaluated with a new version first.

## Expected behaviour

Decide whether the lazy path cascades. If it does, it calls `expire()` (with the `Deadline` cause),
so dependents get `Cascaded{Deadline, …}` as on the queued manager. A shared scenario in
`tests/common/manager_scenarios.rs` would then pin the behaviour for every manager. A dependent that
is itself looked up first still needs its upstream deadline checked, so a cascade on access alone
may not be enough.

## Discovery

Found 2026-10-02 in Phase 5 of `design/dependency-audit-and-expiry-provenance/`, while writing the
resolution of `IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES`.

## Resolution (2026-10-07)

Fixed by design `immediate-lazy-expiry-cascade`, applying the maintainer decision of 2026-10-06:
"Laziness is the method of finding out that expiry needs to be done — but once it is known that an
asset expired, all the consequences should follow, i.e. cascade expiry." Both lazy-expiry sites of
`ImmediateAssetManager` now expire through `expire_with_reason`, which cascades to dependents with
`Cascaded { Deadline, root, via }` as the queued monitor does. A dependent inherits its root's
deadline, so reading it first also recomputes it.

Evidence: `lazy_deadline_expiry_cascades_{default,immediate}` and
`lazy_dependent_read_first_immediate` (`liquers-core/tests/manager_parametric.rs`),
`external_manager_lazy_deadline_expiry_cascades` (`liquers-core/tests/external_asset_manager.rs`).
