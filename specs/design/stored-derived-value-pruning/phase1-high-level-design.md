# Phase 1: High-Level Design — Prune stored derived values

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — new `pub` types (`PrunePolicy`, `PruneReport`) and a new
  provided `AssetManager` method in `liquers-core` (rules 3 and 4); `L`; open decisions (rule 5).
- **Leading issue:** **Open design - what "not used" means.** Recommended: the stored record's
  `updated` time (last computed or rewritten) plus "not registered in this manager right now";
  no write on read.
- **Explanation:** at HEAD, `AssetManager::remove` already drops a computed, recipe-backed value
  exactly as pruning needs (data removed, record rewritten as `Recipe` with its version kept,
  dependents not expired). `assets-api-admin-operations` adds its race-free guarded form
  `remove_computed`. Pruning is a walk that selects keys and calls it. Phases 3-4 assume the
  recommended answers.
- **Open questions:**
  1. **Open design - last use:** (a) `MetadataRecord::updated` of the stored record, skipping keys
     the manager currently has registered (recommended: free, and a registered key is by
     definition in use; a value read only through the fast track and never kept looks unused and
     is recomputed after pruning, which is the documented cost of pruning); (b) a `last_accessed`
     field written on fast-track load, throttled to once per day per key (accurate, but a store
     write on a read path and a serialized-format change); (c) an in-memory access log (lost on
     restart).
  2. **Open design - metadata:** keep the record (recommended, and what `remove` does: the version
     survives, so a recomputation with the same content leaves dependents valid, and the listing
     still shows the key as a recipe) or delete it too (less disk; dependents recorded against the
     version expire on their next audit).
  3. **Open design - orphans** (stored computed values whose recipe is gone): report them in
     `PruneReport::orphans` and remove them only when `policy.remove_orphans` is set (recommended:
     removing one expires its dependents and cannot be undone by recomputation, so it is opt-in),
     or always remove, or never touch.
  4. **Proposed resolution - trigger:** an explicit call only (`AssetManager::prune_store`); no
     periodic task, configuration field, command or HTTP route in this design. A scheduler,
     an admin route and a command are follow-ups once the policy is agreed.
  5. **Proposed resolution - never touched:** `Source`, `Override`, a stored object with no
     metadata (treated as user data, as `remove` does), directories, volatile records, in-flight
     statuses, and anything `remove_computed` refuses.

## Purpose

A long-running deployment with many short-lived recipes (per-request reports, agent memory
derivations) writes every distinct result to its store and never removes one, so disk grows
without bound. `asset-garbage-collection` bounds memory and deliberately never touches the store.
Give operators a safe way to drop stored values that a recipe can recompute and nobody has needed
for a while (VISION: server / data service; cache invalidation and freshness).

## Problem Example

A server with recipe folder `reports/` stores one `reports/<day>/summary.html` per day, each
computed from a recipe in `reports/recipes.yaml`. After a year the store holds 365 of them, 360
never requested again. Today the only remedy is deleting folders by hand, which also deletes any
`Override` a user placed there. Should be:

```rust
let report = envref.get_asset_manager()
    .prune_store(&parse_key("reports")?, &PrunePolicy::older_than(Duration::from_secs(30 * 86400)))
    .await?;
// report.pruned: the 360 old summaries; their records stay, status `Recipe`, version kept.
// report.kept: the overrides and the 5 recent summaries. Asking for an old one recomputes it.
```

## Scope and Acceptance Criteria

- **AC-1** Old computed values lose their data, keep their record
  WHEN `prune_store(dir, older_than(D))` runs and a recipe-backed key under `dir` is stored `Ready`
  (or `Expired`, `Error`) with `updated` older than D, and is not registered
  THEN its data object is removed, its record reads `status: Recipe` with the same `version`, and
  the key appears in `report.pruned`
- **AC-2** User values and recent values stay
  WHEN the subtree also holds a `Source`, an `Override`, a file with no metadata, and a computed
  value newer than D
  THEN none of them is changed and each is listed in `report.kept` with its reason
- **AC-3** In-use values stay
  WHEN an old computed key is registered in the manager (a caller holds it, or it is cached)
  THEN it is not pruned (`kept`, reason `Registered`)
- **AC-4** A pruned value is recomputed, dependents stay valid
  WHEN a pruned key is requested again and its recipe yields the same bytes
  THEN its command runs again, the version equals the kept one, and a stored dependent of the key
  is not expired
- **AC-5** Orphans are reported, removed only on request
  WHEN a stored computed key has no recipe any more
  THEN it is listed in `report.orphans` and unchanged, unless `remove_orphans` is set, in which
  case it is removed as `AssetManager::remove` removes a recipe-less key (dependents expired)
- **AC-6** Dry run
  WHEN `policy.dry_run` is set
  THEN the report is the same as a real run's and the store is unchanged
- **AC-7** Failures do not stop the pass
  WHEN removing one key fails (read-only mount, a concurrent `Override`)
  THEN that key is in `report.failed` with its error and the pass continues; `prune_store`
  returns `Err` only when the subtree cannot be listed
- **AC-8** No race with a user write
  WHEN a user writes an `Override` to a key while pruning selects it
  THEN the override survives (the guarded removal refuses)

Non-goals: periodic pruning, configuration, commands, HTTP routes; size-based or quota-based
policies; pruning query (non-keyed) results, which are never stored; pruning dependency-graph
state; recording last access on reads (question 1 b).

## Core Interactions

- **Assets:** a provided `AssetManager` method built on `listdir_keys_deep`, `lookup_key_asset`,
  the recipe provider's `has_recipes` and `remove_computed` / `remove`.
- **Store:** reads records (`get_metadata`); removal goes through the manager, never directly.

## Crate Placement

`liquers-core` (`assets.rs`): the decision must use the manager's key mutation lock, which only
the manager holds. No other crate changes.

## Documentation Intent

- Reference: extend `specs/reference/ASSETS.md` (removal section: pruning) and
  `specs/reference/STORE_SEMANTICS.md` §5 Removal (what a pruned key looks like in the store).
- Guide: extend `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` §Choosing caching strategies
  (bounding the store, beside bounding the cache).
- Other documents: none.
- Documents to update: the three above, History rows and `reviewed:` bumps.

## Design Dependencies

- `assets-api-admin-operations` (needs-decision, not implemented) — **requires**: its
  `AssetManager::remove_computed` is the race-free per-key removal (AC-8). If it has not landed,
  step 1 here adds the method as that design specifies (its Phase 2 §Interfaces), without its HTTP
  routes.
- `asset-garbage-collection` (needs-decision) — **overlaps**, weak: it bounds memory, this bounds
  disk; a collected key becomes prunable later (AC-3). Either order.
- `registered-asset-protection` (phase2-blocked) — **overlaps**, weak: a protected key is
  registered, so AC-3 already keeps it.
- `asset-registration-contract` — **overlaps**, weak: pruning ends no registration (it skips
  registered keys).

## Consolidated Findings

- The per-key effect already exists: `remove` on a computed key with a recipe rewrites the stored
  record as `Recipe`, keeps the version and does not expire dependents. Pruning adds only
  selection, reporting and the guard.
- Skipping registered keys makes pruning safe to run while serving: no live asset loses its value.
- `updated` is RFC 3339 with milliseconds (`MetadataRecord::set_updated_now`); a record whose
  `updated` does not parse is kept (reason `UnknownAge`), never pruned.
- A metadata-only record left by pruning is skipped on the next pass (status `Recipe`).
- Orphan removal is the only step that loses information; it is opt-in (question 3).
