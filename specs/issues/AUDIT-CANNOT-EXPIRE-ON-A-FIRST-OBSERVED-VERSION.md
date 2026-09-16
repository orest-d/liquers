---
id: AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION
kind: issue
title: An explicit dependency audit cannot expire a dependent whose dependency version merely moved
status: draft
priority: P2
complexity: S
area: [core/assets]
design:
created: 2026-09-15
github:
---

## Problem

`AssetManager::trigger_dependency_audit` documents three outcomes per gap: "a version that matches
leaves the dependent alone, one that differs expires it, and no durable version at all expires it".
The middle one does not happen when the dependency manager holds **no entry** for that dependency —
which is every dependency in a freshly started process, and therefore the entire case the audit
exists for.

The mechanism is in `DependencyManager::register_version`
(`liquers-core/src/dependencies.rs:158`):

```rust
match self.versions.entry_async(key.clone()).await {
    scc::hash_map::Entry::Occupied(mut entry) => {
        version_changed = *entry.get() != version;   // compares against what WE held
        *entry.get_mut() = version;
    }
    scc::hash_map::Entry::Vacant(entry) => {
        entry.insert_entry(version);                 // no comparison at all
    }
}
if version_changed { self.expire_stale_dependents(key, version).await } else { ... }
```

"Changed" means *changed relative to the version this manager previously held*. That is the right
question on the evaluation path — a first registration is not a recomputation. It is the wrong
question for an audit, whose input is precisely a key the manager holds **no** version for
(`missing_versions_for` selects exactly those), and whose comparison should be against the
**dependents' recorded expectations**, which `expire_stale_dependents` already walks.

`report_no_version` has no such gate — it expires every dependent carrying a concrete expectation —
so the "dependency vanished" outcome works and the "dependency moved" outcome does not. The two
differ only in whether the store still holds an entry.

## Reproduction

Written and then withdrawn while implementing
`specs/design/stale-dependency-status-finalization/`, where it was planned as test R2. It replays a
persisted `b.txt` into a fresh environment, rewrites the stored version of its dependency `a.txt`
without touching its status, reloads `b.txt` (which fast-tracks, correctly — nothing yet
contradicts the record), and audits:

```rust
let mut a_metadata = store2.get_metadata(&a_key).await?;
a_metadata.set_version(Some(Version::new(0xB0_0B)))?;
store2.set_metadata(&a_key, &a_metadata).await?;

let b = envref2.evaluate("-R/b.txt").await?;
let _ = b.get().await?;                       // Ready, fast-tracked
let report = manager.trigger_dependency_audit(&parse_query("-R/b.txt")?).await?;
```

Observed: `AuditReport { checked: [DependencyKey("-R/a.txt")], expired: [] }`, and `b.txt` stays
`Ready`. Expected: `b.txt` expired, because the version it recorded for `a.txt` is not the version
`a.txt` now has.

The test that shipped in its place,
`keyed_version_cascade::explicit_audit_expires_a_reloaded_dependent_whose_dependency_vanished`,
covers the `report_no_version` outcome and is the only test in the suite in which an audit expires
anything at all. Nothing exercised `audit_gaps`' expire path before it.

## Expected behaviour

An audit compares the dependency's **durable** version against each dependent's **recorded**
expectation, independently of whether this process has seen that dependency before. The natural
shape is an audit-specific entry point that records the version and then unconditionally runs
`expire_stale_dependents` — which already spares a dependent only on positive evidence, so it is
correct to call on a first observation.

Changing `register_version`'s `Vacant` arm instead would be the smaller diff and the wrong one: it
runs on the evaluation path, where a first registration genuinely is not a change, and would
broaden invalidation for every asset in the system.

## Discovery

Found on 2026-09-15 in Phase 5 of `specs/design/stale-dependency-status-finalization/`, while
writing the cross-process reload tests that `CROSS-PROCESS-RELOAD-IS-UNTESTED` asked for. It is the
same shape as the gap that design found in `try_fast_track`: a path whose *success* was asserted
nowhere, so nothing failed when it could not succeed.
