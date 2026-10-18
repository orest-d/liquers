---
id: EXTERNAL-MANAGER-CANNOT-SHARE-AN-EVALUATION-SESSION
kind: issue
title: An external asset manager cannot hand a dependency its parent's evaluation session
status: draft
priority: P3
complexity: S
area: [core/assets]
design: external-manager-evaluation-session-sharing
created: 2026-10-12
github:
---
## Problem

Within one evaluation, `Context::schedule_dependency_asset_with_key` pins each dependency it
resolves in the evaluation session (`liquers-core/src/evaluation_session.rs`). For the
dependency's *own* dependencies to be pinned in the same session, the manager must give the
dependency its parent's session before the dependency starts. `DefaultAssetManager` and
`ImmediateAssetManager` do this with `AssetRef::adopt_evaluation_session`, which is `pub(crate)`.
An `AssetManager` implemented outside `liquers-core` (`liquers-core/tests/common/minimal_manager.rs`
`MinimalInlineAssetManager`, or one in another crate) cannot call it. Each dependency it runs
therefore begins a session of its own.

## Problem Example

With recipes `v = tick` (volatile), `a = -R/data/v.txt/-/upper`, `b` the same, and
`c = -R/data/a.txt/-/join-~X~-R/data/b.txt~E`, evaluate `-R/data/c.txt` on
`MinimalInlineAssetManager`. Today `a` and `b` each begin their own session, so `tick` can run
twice and the two halves of `c` can differ. It should run once, as on the built-in managers.

## Impact

- **Affected:** external asset managers only.
- **Severity:** each dependency is still requested once per dependent evaluation, so the
  exponential re-run of `FAILING-UPSTREAM-RERUNS-EXPONENTIALLY-AT-QUEUE-CAPACITY` does not come
  back. But an upstream shared by two dependencies of one evaluation (a diamond) may run once per
  dependency, and a volatile upstream can have two values in one evaluation.
- **Workaround:** none for the external manager. The built-in managers are not affected.

## Expected behaviour

An external manager can keep one evaluation in one session. For example, make
`adopt_evaluation_session` public, or pass the session to `AssetManager::get_dependency_asset`,
or let `Context` adopt it when the manager hands back an asset that has not started.

## Discovery

Found on 2026-10-12 while documenting `dependency-resolved-once-per-evaluation` (Phase 4, step 12):
the guide asked managers to adopt the parent's session, which only crate-internal code can do.

Triage: it overlaps `dependency-resolved-once-per-evaluation` (T2, the same contract), but its fix
widens the public `AssetManager` surface, which that design kept unchanged, and it touches
`external-manager-replacement-surface`. It is filed separately so that the API choice gets its
own decision.
