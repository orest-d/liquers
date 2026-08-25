# Phase 2: Solution & Architecture - Keyed Asset Registration Semantics

## Overview

The mismatch is `ImmediateAssetManager`: its `HashMap::insert` replaces while the queued manager's
`scc::HashMap::insert_async` is already insert-if-absent. Replace the unused ambiguous
`insert_key_asset` trait method with atomic `try_insert_key_asset`, used only inside `liquers-core`.
Map registration never expires or notifies a displaced asset. A manager-level async lock serializes
keyed cache entry installation/eviction with external keyed mutations, so promotion, replacement,
removal, and durable writes cannot interleave.

## Known-Issue Preflight

Searched the linked issue, open `core/assets` records in `specs/index.csv`, and keyed map,
eviction, expiration, `set_binary`, `set_state`, `remove`, and `to_override` integration paths.

| Issue | Status | Priority | Impact | First? | Blocking? | Action | Priority |
|---|---|---|---|---|---|---|---|
| ASSET-MANAGER-INSERT-KEY-ASSET-NO-OVERWRITE | in_progress | P2 | Direct map-contract mismatch. | no | no | Resolve here. | Keep P2 |
| QUEUED-MANAGER-EVICTION-RACE | accepted | P2 | Same id-guard discipline; four unrelated queued eviction sites remain unsafe. | no | no | Do not widen this change; retain issue. | Keep P2 |
| ASSETS-IMPROVEMENTS | accepted | P2 | Broader persistence/eviction policy. | no | no | Independent. | Keep P2 |
| COMBINED-EXPIRES | accepted | P2 | Different expiration algebra. | no | no | Independent. | Keep P2 |

### Blocking and Priority Decision

No prerequisite blocker remains. The durability race found in review is resolved by serializing
manager mutations, rather than by making registration replace. `QUEUED-MANAGER-EVICTION-RACE` is
non-blocking because it concerns separate stale-terminal eviction paths and stays in its own issue.

## Data Structures

Add one field to each in-tree manager:

```rust
key_mutation_lock: tokio::sync::Mutex<()>
```

It is constructed unlocked, is runtime-only (no serde), and is not shared between managers. The
guard is held across map and store work by externally mutating keyed operations, and around a
keyed cache miss/stale eviction only until the new map entry is claimed or the old entry is removed.
Ordinary map hits, query assets, evaluation, and dependency traversal do not take it. The keyed
expiration monitor takes it while expiring and evicting a keyed asset. A global lock is intentional:
mutations and cache misses are rare, it avoids a leaky per-key lock table, works for wasm's
single-threaded immediate manager, and gives a simple total order for durable keyed changes. It may
be replaced by keyed locks only after measurement proves it hot.

No new value types, serialization, dependencies, or map type changes are required.

## Trait Implementations

Remove the existing `AssetManager` trait method. Add matching crate-private inherent operations to
the two bundled managers:

```rust
pub(crate) async fn try_insert_key_asset(
    &self,
    key: &Key,
    asset: AssetRef<E>,
) -> bool;
```

There are no repository callers outside `liquers-core`, so no compatibility wrapper is retained.
The operation is not part of `AssetManager` and is `pub(crate)`, preventing it from expanding the
public trait surface. Both manager implementations use one atomic map operation: queued uses
`scc::HashMap::insert_async(...).await.is_ok()` and immediate uses one `HashMap::entry` operation
while holding its existing mutex.

No arbitrary `replace_key_asset` is added. If a future caller needs one, it must return the displaced
`AssetRef` and explicitly own that ref's cancellation, expiration, notification, and cascade policy.

## Function Signatures

```rust
// Matching pub(crate) inherent method on DefaultAssetManager and ImmediateAssetManager.
pub(crate) async fn try_insert_key_asset(&self, key: &Key, asset: AssetRef<E>) -> bool;

// Private inherent helpers, one per manager; called while key_mutation_lock is held.
async fn set_binary_locked(&self, key: &Key, binary: &[u8], metadata: MetadataRecord)
    -> Result<(), Error>;
async fn set_state_locked(&self, key: &Key, state: State<E::Value>) -> Result<(), Error>;
async fn remove_locked(&self, key: &Key) -> Result<(), Error>;
async fn to_override_locked(&self, key: &Key) -> Result<(), Error>;
async fn get_resource_asset_locked(&self, key: &Key) -> Result<AssetRef<E>, Error>;
async fn remove_expired_key_locked(&self, key: &Key, asset_id: u64) -> bool;
async fn owned_key_asset(&self, key: &Key) -> Option<AssetRef<E>>; // overridden in-tree
```

Each public trait implementation acquires `key_mutation_lock`, then calls its matching helper. Both
bundled managers implement the formerly-default `set_state` and `to_override` flows locally, so
they can use their inherent insertion operation and lock. Their
`get_resource_asset` and keyed expiration paths also acquire the same guard when they must install
or evict a keyed map entry, then re-check the map while holding it.

## Lock Inventory

Every in-tree keyed map installation/removal path coordinates with `key_mutation_lock` and re-checks
its condition inside the guard:

| Path | Locked helper / re-check |
|---|---|
| Default and immediate `get_resource_asset` cache miss | Perform volatility/recipe I/O first; acquire guard, re-check map, then claim entry. |
| Default and immediate `get` stale-key eviction | Acquire guard, re-read current entry and status, then conditionally remove it. |
| Default `get_dependency_asset` keyed stale eviction | Delegate to `remove_expired_key_locked`; query-map eviction is out of scope. |
| Default and immediate expiration cleanup | Keyed branch acquires guard before expiry/eviction and re-checks id/status; query branch stays independent. |
| `owned_key_asset` volatile cleanup | Both in-tree managers override the default: acquire guard, re-check current id and volatility, then remove. |
| `set_binary`, `set_state`, `remove`, `to_override` | Acquire guard before lookup and retain it through map, store, and dependency work. |

The strengthened ordering contract belongs to the two built-in managers; no new external manager
extension contract is introduced or preserved.

## Caller and Lifecycle Contract

| Operation | Under `key_mutation_lock` | Map and durable behavior |
|---|---|---|
| `set_binary` | yes | Cancel/untrack and remove any current in-memory ref, then write the binary/metadata. |
| `set_state` | yes | Cancel and untrack the observed ref; `remove_key_asset_if(key, id)` prevents removing a replacement. Create a ref and require `try_insert_key_asset` before store persistence. A false result returns `Error::general_error(...).with_key(key)` without a write. |
| `remove` | yes | Cancel/untrack and remove the current ref, remove dependency state, then remove durable data. |
| `to_override` | yes | Re-read the current ref after acquiring the guard, then promote and persist that current ref. The previous defensive re-insertion is removed: the guard prevents eviction/replacement until persistence completes. |
| Cache miss/stale keyed eviction | briefly | Re-check and atomically install/remove the keyed entry while coordinated with external mutation; an ordinary cache hit takes no lock. |
| Direct primitive registration | no | `try_insert_key_asset` only changes reachability; it does not mutate either ref, persist, send a notification, untrack a timer, or cascade. Workflows that coordinate it with a cache miss or durable mutation acquire the global lock first. |

The old ref is **not** implicitly expired when a map entry changes. `set_state` requests its
existing cancellation path (which supplies its applicable cancellation/state notification); a
finished `Ready`/`Override` old ref is merely detached. The successful externally supplied
Ready/Source/Override state registers its new dependency version and the existing dependency
manager performs the one required dependent-expiration cascade. That cascade is never caused by
`try_insert_key_asset` itself.

## Sync vs Async Decisions

`try_insert_key_asset` stays async because `scc` is async. The global mutation lock is Tokio async
and intentionally spans store I/O, so neither a competing mutator nor a cache miss can make its
durable write stale. The immediate manager remains valid on wasm and on native inline tests; it has
no scheduler concurrency, but async re-entrancy around store awaits makes the same lock useful. Its
`std::sync::Mutex` map lock is never held across await. Switching its whole map to `scc` is rejected:
it aligns only the map's insert semantics, not store-write ordering; it would also replace the
deliberately wasm-friendly immediate map without solving the reviewed race.

## Integration Points

- `liquers-core/src/assets.rs`: AssetManager compatibility/default methods, both manager fields and
  constructors, mutator overrides/helpers, primitive implementations, and same-file tests.
- No query syntax, store trait, commands, value types, web/API, UI, Python bindings, or Cargo file changes.

## Error Handling and Rust-Practices Review

Race conflicts return existing `Error::general_error(...).with_key(key)`; no new error type,
`unwrap`/`expect`, wildcard match, or generic bound is added. The design retains object safety and
the crate dependency flow. The only lock across await is the deliberately async mutation lock;
short immediate-map mutex guards are dropped first.

## Documentation Architecture

### Reference Plan

Extend `specs/reference/ASSETS.md` (internal, `core/assets`) with insert-if-absent semantics, the
reachability/lifecycle boundary, and identity-safe `to_override`. Extend
`specs/reference/ASSET_SET_OPERATION.md` (internal, `core/assets`) with the global keyed-mutation
serialization and conflict behavior. Both receive `reviewed:` and History updates.

### Guide Plan and New Documents

None: this adds no repeatable user or contributor workflow.

### Affected Documents and Links

Authoritative `affects_docs`: `reference/ASSETS.md`, `reference/ASSET_SET_OPERATION.md`. Review but
do not change unless implementation requires it: `reference/ASSET_LIFECYCLE.md` and
`reference/DEPENDENCIES_STATUS.md`. Phase 5 updates `specs/README.md` to the highest-stage link and
closes the linked issue with evidence.

### Evidence to Collect

Test both managers' duplicate result; a gated-store race where old `to_override` is paused at
persistence, `set_state` is demonstrably
blocked on the global lock, and release leaves both map and store at the newer state; `set_state`
conflict does no durable write; normal `set_state` causes exactly one dependency cascade; and a
wasm build.

## Relevant Commands

None; no command namespace is relevant.
