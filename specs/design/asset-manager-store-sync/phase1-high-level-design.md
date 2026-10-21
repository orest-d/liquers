# Phase 1: High-Level Design — One call reconciles a running asset manager with its store

## Purpose

A host that knows the store changed outside Liquers (a restore, a manual edit, another process)
needs one call that makes the running asset and dependency managers agree with the store again,
instead of restarting or hand-composing four operations in the right order.

## Problem Example

A running server holds `data/report.csv` (recipe-backed, registered, `Ready`) and its dependent
`data/summary.txt`. An operator restores `data/` from a backup and edits `data/recipes.manifest.yaml`.
Today the host must know to call, in order, `ManifestRecipeProvider::clear_cache`,
`AssetManager::verify_stored_versions(&root, true, AuditMode::Expire)`,
`trigger_dependency_audit_store(&root, AuditMode::Expire)` and
`trigger_dependency_audit_all_registered()`. Even then, a registered asset whose stored copy the
restore **deleted**, or whose stored metadata now records a different version without the bytes
changing, keeps being served from memory: no existing operation compares the live registrations
with the store.

Should be:

```rust
let report = envref.get_asset_manager()
    .synchronize_with_store(&parse_key("data")?, AuditMode::Expire).await?;
```

repairs interrupted writes, drops recipe-provider caches, applies outside edits, unmaps the
registrations the store no longer backs, runs both audits, and reports what each step found.
`AuditMode::ReportOnly` reports the same findings and changes nothing.

## Scope and Acceptance Criteria

- **AC-1** Outside byte edit
  WHEN a stored recipe-backed value under `root` is overwritten outside Liquers and
  `synchronize_with_store(root, Expire)` runs
  THEN the configured `ExternalChangePolicy` is applied, the live copy is unmapped, its dependents
  are expired, and the report lists the key under `versions.changed`
- **AC-2** Registration the store no longer backs
  WHEN a registered `Ready` keyed asset's stored entry is deleted, or its stored version differs
  from the live one
  THEN it is unmapped and listed in `unmapped`, and the next request reads the store or recomputes
- **AC-3** Stale stored dependents
  WHEN a stored value under `root` recorded a dependency version the store no longer holds
  THEN it is expired and listed in `stored_audit.expired`
- **AC-4** Recipe caches dropped
  WHEN a manifest under `root` was edited outside Liquers
  THEN after the call the new recipe is used: `ManifestRecipeProvider`'s cache is invalidated
  through the provider chain
- **AC-5** Report only
  WHEN the call runs with `AuditMode::ReportOnly`
  THEN the report has the same findings as `Expire` would, and no store write, unmapping,
  expiry or graph change happens
- **AC-6** Consistent store is a no-op
  WHEN nothing changed outside Liquers
  THEN every list in the report is empty and no asset changes status
- **AC-7** Both managers
  WHEN run on the queued and the inline manager
  THEN AC-1 to AC-6 hold on both

Non-goals: quiescing evaluations or locking the whole manager (decision 2); levels of checking
beyond `AuditMode` and the root (decision 1); detecting a change with no trace in the store
(only bytes, metadata and listings are compared); out-of-process triggering (an axum admin route
may call it later, `assets-api-admin-operations`).

## Core Interactions

- **Assets:** a provided `AssetManager` method; the live key map.
- **Store:** `AsyncStore::sync`, metadata and byte reads, the external-change writes that exist.
- **Recipes:** a provided `AsyncRecipeProvider::invalidate_caches`.

## Crate Placement

`liquers-core` (the operation and the provider hook), plus `liquers-records`, whose
`ManifestRecipeProvider` overrides the hook with its existing `clear_cache`.

## Documentation Intent

- Reference: extend `reference/ASSETS.md` §Content changed outside Liquers and
  `reference/DEPENDENCIES_STATUS.md` §Consistency policies (the operation and its guarantee).
- Guide: extend `guides/DEPENDENCY_CONSISTENCY_GUIDE.md` (a new "After an outside change" section
  beside §Running a startup audit).
- Other documents: none.
- Documents to update: the three above.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. `L`, crosses two crates, adds `pub` methods on two traits and
  a report type (rules 1, 3, 4, 6).
- **Leading issue:** Open design question, configurability (the issue's own question).
- **Explanation:** every step except the live-registration check is an existing tested operation,
  and the order is forced by what each step reads. Two contract choices remain, each with a
  recommended answer carried through Phases 3-4.
- **Open questions:**
  1. **Open design question, configurability:** recommended: one fixed, most conservative
     sequence, parameterised only by `root` and `AuditMode`. Levels (skip byte hashing, skip the
     stored-records walk) can be added as an options struct later without breaking the call.
     Alternative: a `SyncLevel` enum now, at the cost of a test matrix per level.
  2. **Proposed resolution, writes in flight:** no global lock and no waiting for evaluations.
     Each step takes the per-key `key_mutation_lock` where it already does. A write the manager
     itself makes during the call is consistent by construction, so the guarantee is: every
     inconsistency present when the call started, and not repaired concurrently by such a write,
     is resolved or reported when it returns. Holding a lock while evaluations need it would
     deadlock (`get_nonvolatile_resource_asset` takes `key_mutation_lock`).
  3. **Proposed resolution, deleted stored copy:** unmap only. Do not expire dependents. A
     recomputation that produces a different version cascades through `register_version` as usual,
     and one that produces the same version leaves the dependents valid.
  4. **Implementation detail (resolved):** `AsyncStore::sync` runs only in `Expire` mode, because it
     repairs (writes).

## Design Dependencies

- `external-index-sync` (asset expiration events): **overlaps**. Expirations performed by this call
  go through `record_expiry`, so they are announced on the event stream with no extra work.
- `asset-registration-contract`: **overlaps**. Unmapping is a revocable registration ending (R2).
- `dependency-chain-analysis-cost` (complete, E4): the origin. Its decision that a running manager
  trusts its own versions is why this operation is explicit.
- `environment-orderly-shutdown`: none beyond the area; independent.

## Consolidated Findings

- The order matters: recipe caches first (later steps resolve recipes), byte verification before
  the dependency audits (it registers the versions they compare against), the stored-records walk
  before the in-memory graph audit (the walk registers confirmed versions, which shrinks the gaps).
- The only new logic is the live-registration check (AC-2). It reads stored metadata once per
  registered key under `root`, without hashing.
- `ReportOnly` exists for every composed step except `AsyncStore::sync` and
  `refresh_command_versions_and_expire`, which are skipped in that mode and say so in the report.
- `ManifestRecipeProvider::clear_cache` already exists; the core hook only routes to it, and
  `RecipeProviderChain` forwards to its members.
