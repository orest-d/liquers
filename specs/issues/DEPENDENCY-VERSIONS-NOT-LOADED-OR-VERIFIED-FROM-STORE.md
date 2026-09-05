---
id: DEPENDENCY-VERSIONS-NOT-LOADED-OR-VERIFIED-FROM-STORE
kind: issue
title: The dependency manager treats an unloaded dependency as a mismatch instead of consulting its durable version
status: draft
priority: P1
complexity: M
area: [core/assets]
design:
created: 2026-09-05
github:
---

## Problem

`DependencyManager::versions` (`liquers-core/src/dependencies.rs:116`) holds only what this process
has registered since it started. It has no way to read a version that is already durable. The
manager is generic over `E: Environment` but holds no environment, envref or store handle — its
fields are five `scc` maps and a mutex — so a version it has not been handed does not exist as far
as it is concerned.

That absence is then read as evidence of change. `version_consistent` returns `false` for a key it
has never seen (`:215`), and `add_dependency` converts that answer into `expire(dependent)`
(`:242`):

```rust
if !version.is_unknown() {
    if !self.version_consistent(dependency, version).await {
        return Ok(self.expire(dependent).await);
    }
}
```

So on a cold start, with `K` persisted carrying version `v1` and dependent `D` persisted recording
`K@v1`, loading `D` before `K` — the ordinary order, since a dependent is what a caller asks for —
expires `D` for a dependency whose durable version is on disk and *equal*. Nothing consults it.

The same gap has a second face: nothing loads durable versions at startup. `AssetManager::start`
registers command metadata and implementation versions
(`load_command_versions_sync`, `assets.rs:3419`) and nothing else, so every keyed asset's version
map entry is absent until something evaluates or fast-tracks that asset.

Today this is nearly unreachable, because a computed keyed asset never records a version at all and
`add_dependency` skips the check for `Version::unknown()`
(`KEYED-EXPIRY-DOES-NOT-CASCADE-TO-KEYED-DEPENDENTS`). Fixing that issue — giving computed assets
real versions — makes every recorded dependency version concrete, and this path becomes the
ordinary one.

## Impact

The cross-process cache empties itself on first use. Every persisted dependent loaded before its
dependency is expired and recomputed, for a dependency the store could have confirmed — which is
the opposite of what persistence is for, and it scales with dependency depth.

P1: a correctness-adjacent cost rather than a wrong answer (the recomputed value is right, just
paid for), and it has a workaround in the interim rule that
`keyed-expiry-cascade-fix` must choose. But it is coupled to that P1 work, and it is the reason
that design cannot simply turn the version check on.

## Expected behaviour

The manager should be able to establish a dependency's durable version rather than concluding
staleness from absence. Two mechanisms, not exclusive:

- **Load on startup.** Read persisted versions for known keys when the manager starts, alongside
  the command versions it already loads. Bounded by how many keys the store can be asked about
  cheaply, so it may suit a directory-backed store better than a remote one.
- **Verify dynamically.** On an edge whose dependency key is unregistered, consult the store's
  sidecar for that key and register what it finds. Lazy, exact, and pays only for keys that are
  actually depended on — but it puts an `await` on a store read inside the dependency graph, which
  today does no I/O at all.

Either way the outcome should distinguish three cases that are currently one:

| Dependency's durable version | Correct outcome |
|---|---|
| present and equal to the record | keep the dependent — it is verified fresh |
| present and different | expire the dependent — verified stale |
| absent | expire the dependent — not durable, so it cannot be proved to reconstruct identically |

The third row is the rule already decided on `KEYED-EXPIRY-DOES-NOT-CASCADE-TO-KEYED-DEPENDENTS`;
the first is the one no mechanism can reach today.

**Relationship to `keyed-expiry-cascade-fix`: follow-up, not a prerequisite.** That design ships an
approximation — on an unregistered dependency key, register the recorded version *provisionally*
and let the later real registration decide. That is deferred verification rather than absent
verification: a persisted asset's version enters the manager the moment anything loads it, and
`register_version` then compares and cascades on a difference. It leaves one case for this issue to
close, which is the third row above: a dependent persisted against a dependency that was never
persisted is fast-tracked warm, nothing ever loads the dependency, and the correction never
arrives. Whichever mechanism this issue adopts must expire that dependent rather than confirm it.

Where the store access lives is the design question. The manager holding a store handle is the
direct route and the largest change to what `DependencyManager` is; performing the lookup in
`DefaultAssetManager` before calling `add_dependency`, and passing the result in, keeps the graph
free of I/O.

## Discovery

Raised by the project owner on 2026-09-05 during Phase 1 of `keyed-expiry-cascade-fix`: "the
durable versions should be considered correct — the dependency manager should probably dynamically
verify them or load them on startup". Found while tracing what happens when computed keyed assets
start carrying real versions and the `add_dependency` consistency check stops being skipped.
