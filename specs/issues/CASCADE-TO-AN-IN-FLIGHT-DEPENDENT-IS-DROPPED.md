---
id: CASCADE-TO-AN-IN-FLIGHT-DEPENDENT-IS-DROPPED
kind: issue
title: An expiry cascade that reaches a registered dependent still evaluating is dropped, so it finishes Ready on a superseded input
status: draft
priority: P2
complexity: M
area: [core/assets]
design: cascade-marks-in-flight-dependent-stale
created: 2026-10-21
github:
---

## Problem

`AssetManager::expire_dependencies_result` expires a registered dependent through
`AssetRef::expire_without_cascade` → `mark_expired_status`, which returns `Err` (StatusConflict) for
`Processing`, `Dependencies`, `Partial` and the other in-flight statuses; the caller ignores it
(`let _ =`). The edge to the dependent exists from schedule time, so the cascade does find it — and
then does nothing. When the dependent finishes, `track_asset` → `load_from_records` →
`add_dependency` records the superseded version without verifying it (by contract), and the
dependent stays `Ready` in memory and in the store, computed from the old input, until that
dependency changes again.

The stale-dependency route (`AssetRef::note_expired_dependency`, finishing `Expired` with
`Direct { StaleDependency }`) already covers a dependency that was expired when consumed and an edge
recorded against a superseded version (`design/dependency-edge-superseded-version/`); it is not
applied when the change arrives after the edge exists and before the dependent finishes.

Inferred from reading the code at `18732dd`; not reproduced by a test.

## Problem Example

Keyed `data/p.txt = -R/data/c.txt/-/gated_upper/p.txt` (registered, `cached` default). While
`gated_upper` waits after reading `c = "a"`, `set_state(data/c.txt, "b")` cascades to `data/p.txt`,
which is `Processing`: the expiry is refused and dropped. `p` finishes `Ready` with `A`; a later
`get(data/p.txt)` in the same process is expected to serve `A`. It should recompute `B` (the
dependent takes the stale-dependency route and finishes `Expired`).

Triage at HEAD (`cd6d5da`, 2026-10-26): unchanged. The example below changes the dependency's
*version*, so once `runtime-dependency-record-superseded-check` (ready, not implemented) lands, its
completion check (records against the version map) catches it. What that check cannot catch is a
cascade that changes no version: an explicit `expire`, a deadline, `remove`, or a **transitive**
cascade — `c → q → p` with `p` running after reading `q`: setting `c` expires `q` (version
unchanged) and reaches `p`, refused; `p`'s record `q @ v(q)` still matches, and `p` finishes
`Ready` on the value built from the old `c`. That is the remaining scope (design
`cascade-marks-in-flight-dependent-stale`).

## Impact

Stale results served from memory and the store after an input changes during a long evaluation.
`expiration_integration.rs` `test_dependency_expiring_during_parent_evaluation_is_allowed` tolerates
the race for a *non-keyed* parent (it only requires the parent to complete with the value it read),
so the expected outcome for a keyed parent needs a decision: finish `Expired` (callers waiting on
`get()` then see an expired error) or finish `Ready` and expire immediately after.

## Expected behaviour

A cascade reaching an in-flight dependent labels it for the stale-dependency route instead of
being refused, so it finishes `Expired` with a `StaleDependency` reason. Alternatively the
dependent compares its records with the dependency manager at finalization (as
`uncached-write-back-rechecks-dependency-versions` does after the store write for uncached keys).

## Discovery

2026-10-21, bulk design of `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` (cluster C1b),
which is the uncached special case; its design names this as a non-goal. Related:
`RUNTIME-DEPENDENCY-RECORD-AGAINST-SUPERSEDED-VERSION-IS-NOT-CHECKED` (records a command adds
itself; different path).
