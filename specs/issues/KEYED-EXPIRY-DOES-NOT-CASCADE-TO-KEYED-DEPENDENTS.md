---
id: KEYED-EXPIRY-DOES-NOT-CASCADE-TO-KEYED-DEPENDENTS
kind: issue
title: Expiring a computed keyed asset never invalidates the keyed assets that depend on it
status: in_progress
priority: P1
complexity: L
area: [core/assets]
design: keyed-expiry-cascade-fix
created: 2026-09-04
github:
---

## Problem

`DependencyManager::expire_internal` (`liquers-core/src/dependencies.rs:555`) walks the dependency
graph breadth-first. Before walking on from a node it consults that node's registered version, and
declines to continue when the version is unknown:

```rust
let mut skip_cascade = false;
if include_root || current != *key {
    if let Some(entry) = self.versions.get_async(&current).await {
        if (*entry.get()).is_unknown() { skip_cascade = true; }
    }
}
self.versions.remove_async(&current).await;
expired_keys.push(current.clone());
if !skip_cascade {
    /* collect keyed dependents into the BFS frontier */
}
self.keyed_dependents.remove_async(&current).await;
/* collect dependent_assets — OUTSIDE the guard */
```

The rule is deliberate and tested (`expire_skips_version_zero_cascade`, `:943`): without a real
version you cannot conclude a dependent is stale.

**But a computed asset never has a real version.** `MetadataRecord::version` is set in exactly four
places — `set_binary` and `set_state` on each of the two managers (`assets.rs:5203`, `:5313`,
`:6379`, `:6429`), i.e. the paths where a value is *handed in*. The evaluation path never sets it,
so `track_asset` registers `mr.version.unwrap_or(Version::new(0))` — unknown — for every keyed
asset the system computes (`dependencies.rs:312`).

Consequence: for any keyed asset produced by evaluation, `skip_cascade` is true on the very first
iteration, and **no keyed dependent is ever reached**. Expiring `c.txt` does not invalidate
`b.txt` or `a.txt` in a keyed chain.

Two things hide this:

1. **Non-keyed dependents still expire.** `dependent_assets` (weak refs from *query* assets) are
   collected outside the `skip_cascade` guard, so they are invalidated normally. The existing
   end-to-end test `test_dependent_expiration`
   (`liquers-core/tests/expiration_integration.rs:283`) exercises exactly this shape — its
   dependent is `envref.evaluate("-R/hello.txt/-/world")`, a query asset — so it passes while the
   keyed→keyed path is untested.
2. **The unit tests supply versions by hand.** Every cascade test in `dependencies.rs` calls
   `register_version(&k, Version::new(1))` and friends, which no production evaluate path does.

A second, possibly related discrepancy sits in the same condition. The comment above it reads
"…we don't cascade to its dependents **(except for the root key)**", but `include_root || current
!= *key` *enables* the version check for the root when `include_root` is true, rather than
exempting it — and when `include_root` is false the root is not in the queue at all, so the guard
is always true. Either the comment or the condition states the intent; they do not agree.

## Impact

Dependency-driven invalidation is one of the system's central promises, and for keyed-to-keyed
relationships between computed assets it does not happen. A stale `a.txt` keeps serving after the
`b.txt` it was derived from has expired, until something else expires it — a TTL, an explicit
call, or eviction.

P1 rather than P0: the value served is not *wrong* in the sense of being miscomputed, it is stale;
there are workarounds (expire the dependent explicitly, use TTLs); and the query-asset path, which
is the common shape in the tests and probably in use, does work.

## Expected behaviour

Decide, and then make the code and the comment agree, which of these is intended:

1. **The root of an explicit expiry always cascades**, regardless of its version — the caller has
   just declared the asset invalid, so "we don't know the version" is not a reason to doubt that
   its dependents are affected. This is what the comment's "(except for the root key)" appears to
   describe, and it would make keyed chains propagate.
2. **Computed assets get real versions**, so the existing version logic has something to work
   with. Larger, and it interacts with what a version is supposed to mean for a value produced by
   a recipe rather than supplied.
3. **The current behaviour is correct** and keyed→keyed propagation is intentionally left to
   TTL/eviction — in which case the comment should say so, and a test should pin it.

Whichever is chosen, a test covering a keyed→keyed chain of *computed* assets is missing and should
exist; today no test distinguishes the three.

### Decision (project owner, 2026-09-05): option 2

**Computed assets should have real versions. This is an old omission, not intended behaviour.**
The rule:

- an asset that **serializes to a binary** — which every stored asset must — takes its version from
  the hash of those bytes;
- an asset that does not takes a fallback version, e.g. a timestamp.

Both primitives already exist and are used elsewhere: `Version::from_bytes` (BLAKE3 over the bytes,
`metadata.rs:33`), already used by `set_binary` (`assets.rs:6379`); and `Version::new_unique`
(`metadata.rs:69`), a nanosecond timestamp shifted over an atomic counter. What is missing is
*calling* them on the evaluation path.

This is why the issue is `complexity: L` and needs a design folder rather than a patch — the work
is small but the consequences are not:

1. **It switches on a cascade that has never run.** Keyed→keyed invalidation currently never fires
   for computed assets. Giving them versions turns it on for every such chain at once. The blast
   radius is "what gets recomputed, and when" — correctness *and* cost.
2. **The two version sources behave oppositely.** A content hash is stable: recomputing an asset
   that produces identical bytes yields the same version, so dependents are *not* invalidated —
   which is the desirable property and the reason to prefer hashing. A timestamp is the reverse:
   every evaluation yields a new version, so every evaluation invalidates all dependents. The
   fallback therefore turns non-serializable keyed assets into permanent cascade sources. Whether
   that is acceptable, or whether such assets should keep `Version::unknown()`, is the central
   design question.
3. **It makes `try_fast_track`'s dependency check non-vacuous for the first time.** That guard
   (`assets.rs:1119`) is currently skipped or trivially satisfied because versions are unknown.
   Real versions make it bite, changing cache-hit behaviour across the board — including across a
   process boundary, where it is what `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY` relies on.
4. **Where the version is set is itself a decision.** `serialize_to_binary` is the obvious place —
   it is where the bytes exist — but it runs only during persistence, so a keyed asset whose
   persist is skipped or fails would have no version. Setting it as part of *finalizing the value*
   instead is more consistent but touches a different function.
5. **Stored metadata changes shape.** `version` is persisted in the sidecar; entries written before
   this have none. The field is `Option`, so this should be compatible, but it wants confirming
   rather than assuming.

Once real versions exist, the `stale-dependency-status-finalization` design's decision to leave the
dependency-manager step alone (`track_asset` early-returns for `Expired`) is worth revisiting:
`register_version` would then see a genuine version change and cascade, which is the behaviour that
design originally wanted and could not get.

## Discovery

Found on 2026-09-04 while designing `stale-dependency-status-finalization`. That design proposed
routing a stale-dependency asset through `cascade_expire_dependents` on the belief that it would
invalidate the key's dependents; tracing `expire_internal` to check showed that for a computed
asset it invalidates no keyed dependent at all. The design dropped the cascade as a result; this
issue records the underlying gap, which is independent of it.

## Correction (measured at HEAD, 2026-09-05)

**The claim above — "no keyed dependent is ever reached" — is wrong, and the correct statement is
narrower.** A probe of a three-link chain of computed keyed assets (`a.txt` ← `hello`,
`b.txt` ← `-R/a.txt/-/world/b.txt`, `c.txt` ← `-R/b.txt/-/world/c.txt`) gives:

```
statuses AFTER expire(a) : a=Expired  b=Expired  c=Ready
after recompute, expire(b): b=Expired  c=Expired
```

Invalidation reaches **direct** dependents and never propagates beyond them. One level per explicit
expiry, always exactly one.

The reason is that a keyed asset is recorded twice. `Context::evaluate` calls `add_dependent_asset`
whenever the current asset is keyed, so a keyed dependent appears in `dependent_assets` as a weak
reference *as well as* in `keyed_dependents` as a graph edge. `expire_internal` collects
`dependent_assets` outside the `skip_cascade` guard and traverses `keyed_dependents` inside it —
so the weak-reference route always fires once, and the graph route, which is the only one that
enqueues a node and therefore the only one that can reach a second level, never runs.

This does not change the fix, the priority, or the owner's decision. It changes what a regression
test must look like: **a two-asset test passes at HEAD.** Three links are the minimum that fails,
which is why the existing 34 expiration tests are green and why the defect survived several
designs. The paragraph above ("Consequence: … no keyed dependent is ever reached") should be read
as "no *transitive* keyed dependent is ever reached".

## Status

`in_progress` since 2026-09-05: designed in
[`specs/design/keyed-expiry-cascade-fix/`](../design/keyed-expiry-cascade-fix/), under the owner's
option-2 decision recorded above. The design's Phase 1 records two consequences this file does not:
`DependencyManager::add_dependency` treats an *unregistered* dependency key as a version mismatch
and expires the dependent, which real versions would make reachable on every cold start; and the
point at which the version is assigned is observable by a concurrent parent, because
`record_dependency_on_asset` reads it from the child's live metadata.

## Future work (owner, 2026-09-05)

**Version persistence for non-durable assets** is explicitly out of this issue's scope, to be
introduced "if needed". A keyed asset whose value does not serialize leaves no durable trace at all
— the evaluate path's `save_to_store` propagates the `SerializationError` and writes nothing, not
even metadata-only — so its version exists only in memory, and its dependents are expired after a
restart. That is the intended behaviour: an asset that is not durable and cannot be provably
reconstructed with the same value should be effectively expired on restart. A mechanism that
persisted such a version would be what changes it, and nothing in the current design depends on
having one.
