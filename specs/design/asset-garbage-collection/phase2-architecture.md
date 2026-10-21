# Phase 2: Solution & Architecture — Bounded asset cache with reachability-based collection

## Overview

Each map value of both built-in managers becomes a private `Registration<E>`, which pairs the
`AssetRef` with an atomic last-use stamp. The stamp is updated on every map hit. A
`collect_garbage` pass snapshots the map, keeps the entries that are collectable (Phase 1 open
question 1), orders them by last use, and removes the idle ones and the oldest ones above the
limit. Each removal re-checks reachability inside one `remove_if` on the map. Collection runs on an
over-limit insert (both managers), on a periodic sweep (queued manager, a task started by `start`
when a limit is set) and on an explicit call.

Rejected alternatives:
- **Last use on `AssetData`.** A map hit would need the asset's async read lock, which waits
  behind evaluation writers on the hot path.
- **Weakly held map entries.** The cache would empty itself the moment a caller dropped its
  handle, so nothing would be reused.
- **Reachability through the dependency graph.** Recipes name keys, not assets. A key is always
  re-resolvable, so a graph walk adds cost and gives the same answer as R2.
- **Strict LRU list.** A linked list updated on every hit needs a lock on the hot path. Sorting a
  snapshot at collection time costs O(n log n) once per pass, and a low-water mark spreads that
  cost over many inserts.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `ASSETS-IMPROVEMENTS` §B | accepted, no design | P2 | decides which statuses are sticky | no | no | align on `Source`/`Override` (Phase 1 Q3) |
| `TEMPORARY-ASSET-NEVER-RUN-IS-KEPT-ALIVE-BY-ITS-SERVICE-LOOP` | draft | P3 | temporary assets are never registered | no | no | none |
| `SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY` | draft, designed | P2 | memory per entry, not entry count | no | no | none |
| `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` | draft | P3 | unregistered assets, which are never collected | no | no | none |

## Interfaces

```rust
// liquers-core/src/environment_builder.rs — AssetManagerOptions, three new fields (serde default,
// skipped when None) and builder methods `with_max_cached_keys`, `with_max_cached_queries`,
// `with_max_idle_seconds`.
pub max_cached_keys: Option<usize>,
pub max_cached_queries: Option<usize>,
pub max_idle_seconds: Option<u64>,

// liquers-core/src/assets.rs
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheLimits { pub keys: Option<usize>, pub queries: Option<usize>,
                         pub max_idle: Option<std::time::Duration> }
impl CacheLimits { pub fn is_unbounded(&self) -> bool; }

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct GcReport { pub keys: Vec<Key>, pub queries: Vec<Query> }

/// Private: the manager's registration record (the map value).
struct Registration<E: Environment> { asset: AssetRef<E>, last_used: AtomicI64 /* unix ms */ }

// AssetManager<E>, provided (an external manager keeps compiling and never collects)
fn cache_limits(&self) -> CacheLimits { CacheLimits::default() }
async fn collect_garbage(&self) -> GcReport { GcReport::default() }
```

`collect_garbage` is async because each candidate's status is read through the asset's lock. The
stamp and the reachability test are sync and lock-free. Both built-in managers override both
methods.

## Integration Points

`liquers-core/src/assets.rs`:
- `DefaultAssetManager.assets: scc::HashMap<Key, Registration<E>>`, `.query_assets` likewise;
  `ImmediateAssetManager.assets` / `.query_assets: Mutex<HashMap<_, Registration<E>>>`. Every read
  (`lookup_key_asset`, `lookup_query_asset`, `get_nonvolatile_resource_asset`,
  `get_nonvolatile_query_asset`, `get_query_asset_for`, the inline `get_resource_asset`) returns
  `registration.touch()` (stamp now, return a clone of the `AssetRef`). Removal sites
  (`remove_key_asset_if`, `remove_query_asset_if`, `remove_expired_from_maps`, `set_state`)
  compare `registration.asset.id()`.
- New private `fn collectable_snapshot(..) -> Vec<Candidate>`: per entry, `Arc::strong_count(
  &asset.data) == 1` and the stamp. A candidate's status is then read (the snapshot holds a clone,
  dropped before removal). Statuses `Ready`, `Expired`, `Error`, `Cancelled`, `Volatile`,
  `Directory`, `None`, `Recipe` are collectable. `Source` and `Override` are sticky. In-flight
  statuses are excluded, by an explicit `match` with no `_` arm.
- Removal: `remove_if_async(k, |r| r.asset.id() == id && Arc::strong_count(&r.asset.data) == 1 &&
  r.last_used == stamp)`. An entry touched since the snapshot is kept. After a removal from the key
  map the queued manager calls `untrack_expiration(id)`.
- Order: first every candidate idle for longer than `max_idle`, then the oldest candidates until the
  map holds at most `limit * 9 / 10` (low-water mark).
- Triggers: after each successful insert, `if len > limit` the queued manager sends
  `CollectorMessage::OverLimit` to its collector task (sync, unbounded). The inline manager awaits
  `collect_garbage()` before returning. `DefaultAssetManager::start` spawns
  `run_cache_collector(Weak<E>, rx, interval)` when the limits are not unbounded. The interval is
  `max_idle / 2`, capped at 60 s; with no idle limit it waits for messages only. The task upgrades
  the environment per pass and ends when the upgrade fails or `Shutdown` arrives. `Drop` and
  `shutdown()` send `Shutdown`.
- `with_policies` (both managers) copies the three options into `CacheLimits`.

`liquers-core/src/environment_builder.rs`: the three fields, their `with_*` methods, and
`AssetManagerKind::build` passes them through `with_policies` as it does today.

## Error Handling

Collection never fails: a status read cannot error, and a refused removal is simply skipped.
`collect_garbage` returns a report, not a `Result`. Configuration: `max_cached_keys: 0` or
`max_cached_queries: 0` is rejected at build time with `Error::from_error(ErrorType::
ConfigurationError, ..)`, as `job_capacity: 0` is. "Cache nothing" is spelled
`*_cache_strategy: none`.

## Relevant Commands

None. No command, namespace or `specs/command_registry.yaml` change. An admin route
(`assets-api-admin-operations`) may call `collect_garbage` later. That is not designed here.

## Documentation Architecture

- `specs/reference/ASSETS.md` §When an asset is kept for reuse: a "How long" paragraph covering
  collectability, the limits and the sticky statuses. Also: a collected keyed asset is reloaded
  without invalidating its dependents.
- `specs/reference/ENVIRONMENT_CONFIG.md` `assets` table: three rows; the "size limit belongs to
  `CORE-ASSET-GC`" sentence now says what exists and names `VALUE-HAS-NO-MEMORY-SIZE-ESTIMATE`.
- `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` §Choosing caching strategies: bounding the cache,
  with the internet-facing example.
- `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` §4: `cache_limits` / `collect_garbage` are
  optional.
- `affects_docs` as listed in `DESIGN.md`. No `specs/README.md` link change.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/assets.rs` (map value type at ~40 sites, collector), `environment_builder.rs`, the tests in `assets.rs` that insert into the maps directly |
| Crates and workflows affected | `liquers-core`; every evaluation path reads the maps, so behaviour is unchanged while no limit is set |
| Existing tests likely to change | `assets.rs` unit tests that call `insert_async(key, asset)` on the maps (about ten); a helper `register_for_test` replaces them |
| New validation | Phase 3 suite `asset_gc.rs`; `scripts/check-build-matrix.sh` (wasm32: no task spawned on the inline manager) |
| Compatibility / data / concurrency / performance / security | Default unbounded = today. Hot path cost: one atomic store per hit. Concurrency: removal re-checks id, count and stamp under the map's own lock. Security: bounding memory is the point for internet-facing servers |
| Recovery | One commit per step; the map wrapper alone changes no behaviour |
| Certainty and open questions | High for count and idle; the byte budget is out of scope (Q2); the sticky rule must agree with `ASSETS-IMPROVEMENTS` (Q3) |
