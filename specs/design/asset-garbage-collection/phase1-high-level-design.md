# Phase 1: High-Level Design — Bounded asset cache with reachability-based collection

## Purpose

A long-running service (an internet-facing `liquers-axum` server, the agent-memory service) must
be able to bound what the asset manager keeps in memory. Today nothing removes an asset that is
merely unwanted. This design defines when an asset is *collectable*, bounds the key and query maps
by entry count and idle time, and leaves the store untouched.

## Problem Example

A server answers ad-hoc queries (`hello-1`, `hello-2`, … validate as `Transform` actions with
`liquers-validate --command hello`):

```rust
let m = envref.get_asset_manager();
for i in 0..100_000 {
    m.get_asset(&parse_query(&format!("hello-{i}"))?).await?.get().await?;  // asset dropped
}
// Today: query_assets holds 100 000 assets and their values for the life of the process.
```

`DefaultAssetManager.query_assets` / `.assets` (`liquers-core/src/assets.rs`) lose an entry only
by expiry (expiration monitor or lazy check), `remove`, replacement, or volatility eviction. A
value with no deadline stays forever. `query_cache_strategy: none` (`plan-policy`) stops
registration, but then nothing is reused at all.

Should be: with `assets: { max_cached_queries: 1000 }`, the map never holds more than 1000
collectable entries, keeping the most recently used ones. A query whose `AssetRef` a caller still
holds is never removed.

## Scope and Acceptance Criteria

- **AC-1** Count bound per map
  WHEN `max_cached_queries: 100` and 1000 distinct queries are evaluated and their `AssetRef`s dropped
  THEN after the last request the query map holds at most 100 entries, the 100 most recently used
- **AC-2** A held or running asset is never collected
  WHEN a caller holds an `AssetRef`, or the asset is being evaluated, and the map is over budget
  THEN the asset stays registered, and a new request returns the same asset id
- **AC-3** Idle bound
  WHEN `max_idle_seconds: N` and a registered asset is not requested for more than N seconds
  THEN the next collection removes it
- **AC-4** A collected keyed asset is reloaded, not recomputed or invalidated
  WHEN a stored keyed `Ready` asset is collected and its key is requested again
  THEN it is fast-tracked from the store (its command does not run again) and none of its
  dependents is expired
- **AC-5** Non-derivable assets stay
  WHEN an unreferenced `Source` or `Override` asset is over budget or idle
  THEN it is not collected
- **AC-6** No limits, no change
  WHEN no limit is configured
  THEN nothing is collected and `collect_garbage` returns an empty report
- **AC-7** Explicit collection on both managers
  WHEN `AssetManager::collect_garbage()` is called on the queued or the inline manager
  THEN collectable entries over a limit are removed and the report names each removed key and query
- **AC-8** Configuration
  WHEN an environment document sets `assets: { max_cached_keys: 10, max_cached_queries: 20,
  max_idle_seconds: 60 }`
  THEN the built manager enforces those limits, and serializing the options omits unset ones

Non-goals: a byte budget (needs a value size estimate, `VALUE-HAS-NO-MEMORY-SIZE-ESTIMATE`);
removing stored values (`STORED-DERIVED-VALUES-ARE-NEVER-PRUNED`); evicting the dependency graph's
versions and edges (they are per key and bounded by the key space); an event for a collection.

## Core Interactions

- **Assets:** the key and query maps of both built-in managers; a new `collect_garbage` primitive.
- **Store:** read only, through the fast track, when a collected key is requested again.
- **Configuration:** three new `AssetManagerOptions` fields (`ENVIRONMENT_CONFIG.md`).

## Crate Placement

`liquers-core` only: the maps, the options and both managers live there. No binding, command or
other crate changes.

## Documentation Intent

- Reference: extend `reference/ASSETS.md` (§When an asset is kept for reuse: how long) and
  `reference/ENVIRONMENT_CONFIG.md` (the three fields).
- Guide: extend `guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` (§Choosing caching strategies: bounding
  the cache) and `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` (collection is optional for an
  external manager).
- Other documents: none.
- Documents to update: the four above, with History rows and `reviewed:` bumps.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. `L`; adds `pub` items (`CacheLimits`, `GcReport`, trait
  methods) and configuration fields (rules 1 and 4).
- **Leading issue:** Proposed resolution, reachability: what makes an asset collectable.
- **Explanation:** a working design exists for every part except the byte budget the maintainer
  asked for on 2026-10-10. That part needs a value size estimate, which no value type has. Phases
  3-4 cover the count and idle bounds under the assumptions below.
- **Open questions:**
  1. **Proposed resolution, reachability:** an asset is collectable when the manager's map entry
     is its only strong reference (`Arc::strong_count(&asset.data) == 1`), its status is finished,
     and it is not `Source` or `Override`. A recipe naming it by key does **not** keep it alive:
     a key is re-resolvable from the store or the recipe, so collecting costs a reload or a
     recomputation, never a wrong value (`asset-registration-contract` R2). This is the answer to
     the issue's "what makes an asset unreachable once recipes can name it by key".
  2. **Proposed resolution, scope:** this deliverable bounds by count and idle time. The byte
     budget is filed as `VALUE-HAS-NO-MEMORY-SIZE-ESTIMATE` (a `ValueInterface` method, implemented
     per `ExtValue` variant in `liquers-lib`), after which `max_cached_bytes` slots in beside these
     limits. Consequence: an internet-facing service bounds entries, not bytes, until then.
  3. **Proposed resolution, sticky statuses:** `Source` and `Override` are never collected, because
     they are inputs that may exist only in memory. This is the rule `ASSETS-IMPROVEMENTS` §B
     proposes, so the two designs must agree.
  4. **Proposed resolution, the store:** collection never deletes stored data. Stored derived
     values that nothing wants any more are a separate problem, filed as
     `STORED-DERIVED-VALUES-ARE-NEVER-PRUNED`.
  5. **Implementation details (resolved in Phase 2):** last use is stamped on the manager's map
     entry; collection runs on an over-limit insert, on a periodic sweep in the queued manager, and
     on an explicit call.

## Design Dependencies

- `asset-registration-contract`: **requires**. Collection is a new way to end a registration, and it
  relies on R2 (revocable) and R4 (monotonic: a collected asset is never re-registered).
- `ASSETS-IMPROVEMENTS` (issue, no design yet; another cluster): **overlaps**, shared contract on
  which statuses are never evicted (open question 3). The first design to land fixes the rule and
  the other cites it.
- `serialized-binary-disposal-policy` (needs-decision): **overlaps**. It releases cached bytes, and
  this design releases whole assets. Independent; both reduce memory.
- `environment-manager-reference-cycle` (ready): **overlaps**. It frees everything when the
  environment drops; this design frees entries while the environment lives. No order.
- `environment-orderly-shutdown`: **overlaps**. Its `stop_background_tasks` stops the collector task.
- `plan-policy` (complete): decides *whether* an asset is registered; this design decides for how
  long.

## Consolidated Findings

- Strong-count reachability is exact and cheap. The dependency graph (`dependent_assets`), the
  expiration monitor (`TimedAsset`) and temporary assets hold only `WeakAssetRef`s. Running work
  holds strong clones (job queue, run claims, delegating waiters), so in-flight assets are excluded
  without a status read.
- The check and the removal happen in one map operation (`scc::HashMap::remove_if_async`, or the
  inline manager's mutex). A concurrent request that clones the entry first makes the count 2, so
  the entry is kept. Nobody but a holder can change an unheld asset's status.
- A collected keyed asset is not "removed": no store write, no cascade, no `Removed` notification.
  Its key keeps its version in the dependency manager, so a reload by fast track passes the version
  checks, and dependents stay valid (AC-4).
- The last-use stamp belongs to the registration, which is the manager's (R2), not to the asset.
  It lives in a private wrapper around each map value. The two maps' types change, but no public
  type does.
- wasm32: the inline manager has no timer, so idle collection happens only on an over-limit insert
  or an explicit call. This is stated in the configuration reference.
