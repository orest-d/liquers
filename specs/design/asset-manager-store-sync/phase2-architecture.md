# Phase 2: Solution & Architecture — One call reconciles a running asset manager with its store

## Overview

A provided `AssetManager::synchronize_with_store(root, mode)` runs seven steps in a fixed order
and returns a `StoreSyncReport` with one field per step. Five steps are existing operations. One is
new: comparing every registered keyed asset under `root` with its stored metadata. The last is a
new provided hook, `AsyncRecipeProvider::invalidate_caches`, which `ManifestRecipeProvider`
overrides with its existing `clear_cache` and which `RecipeProviderChain` forwards to its members.

Rejected alternatives:
- **Reload all live metadata from the store.** The manager is the authority on status
  (`ASSETS.md` §Who decides status). Overwriting live metadata with the store's would invert that.
  Unmapping lets the next request read the store through the normal, verified fast track.
- **A global quiesce (lock the manager).** It deadlocks with evaluations that create key assets
  under `key_mutation_lock` (Phase 1 Q2).
- **A host-side recipe in the guide only.** It is what the issue's workaround already is. The
  order is subtle (Phase 1 Consolidated Findings), and the live-registration step cannot be
  written outside core without enumerating the map.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` | draft | P3 | an audit expiring an uncached stored copy can race an evaluation; the same race exists for the audits today | no | no | cite in the guide |
| `EVALUATION-KEEPS-DROPPED-DEPENDENCY-EDGES` | draft | P3 | the graph audit may expire on an edge that no longer exists (conservative) | no | no | none |
| `RECIPE-RESULT-UNDER-STORAGE-NONE-READS-AS-SOURCE` | draft | P3 | `storage: none` mounts have no stored copy; AC-2 must not unmap every registration there | no | no | step 4 skips `stored: false` and unwritten values; under a mount that keeps no data, a registration may be unmapped on every call (cost only, a reload or recompute), stated in the guide |

## Interfaces

```rust
// liquers-core/src/assets.rs
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct StoreSyncReport {
    /// `AsyncStore::sync` (Expire mode only; `None` when skipped).
    pub repaired: Option<crate::store::SyncReport>,
    pub versions: VersionVerificationReport,   // step 3
    /// Registered keyed assets the store no longer backs (unmapped in Expire mode).
    pub unmapped: Vec<Key>,                    // step 4
    pub stored_audit: AuditReport,             // step 5
    pub graph_audit: AuditReport,              // step 6
    /// Command dependency keys whose version changed (Expire mode only).
    pub commands: Vec<crate::metadata::DependencyKey>,   // step 7
}

// AssetManager<E>, provided
fn registered_keys(&self) -> Vec<Key> { Vec::new() }   // both built-in managers override
async fn synchronize_with_store(&self, root: &Key, mode: AuditMode)
    -> Result<StoreSyncReport, Error>;

// liquers-core/src/recipes.rs — AsyncRecipeProvider<E>, provided
async fn invalidate_caches(&self) {}
```

All async. `registered_keys` is a sync snapshot (`scc::HashMap::scan` / the inline manager's mutex).
`StoreSyncReport` derives `Debug, Clone` and has no serde: it is an in-process result, like
`AuditReport`.

## Integration Points

The steps of `synchronize_with_store`, in order:

1. `Expire` only: `self.get_envref().get_async_store().sync().await?` → `repaired`.
2. `self.get_recipe_provider().invalidate_caches().await`.
3. `self.verify_stored_versions(root, true, mode).await?` → `versions`.
4. New private `reconcile_registrations(root, mode)`: for each `k` in `registered_keys()` with
   `k.has_key_prefix(root)`, read the live asset's status. Only `Ready`, `Override` and `Source`
   assets whose metadata says `stored()` and whose persistence status is not `NonSerializable` or
   `NotPersisted` are checked. A loaded asset (fast track) is checked; a value that was never
   written is not. Then `store.contains(k)`: when it is false, or `store.get_metadata(k)?.version()`
   differs from the live metadata's version, push `k`. In `Expire` mode, take `key_mutation_lock`,
   `untrack_expiration(id)` and `remove_key_asset_if(k, id)`, as `verify_stored_versions` does. A
   write still in flight in the background can make a fresh asset look unbacked. Unmapping it only
   costs a reload, so this is accepted rather than locked against.
5. `self.trigger_dependency_audit_store(root, mode).await?` → `stored_audit`.
6. `self.trigger_dependency_audit_all_registered_with(mode).await?` → `graph_audit`.
7. `Expire` only: `self.refresh_command_versions()?` → `commands`, then the same cascade as
   `refresh_command_versions_and_expire`, factored into a private helper that takes the list.

Built-in managers: `DefaultAssetManager::registered_keys` and `ImmediateAssetManager::
registered_keys`. `liquers-records/src/provider.rs`: `ManifestRecipeProvider` implements
`invalidate_caches` by calling `clear_cache`. `liquers-core/src/recipes.rs`: `RecipeProviderChain`
forwards to every member. `DefaultRecipeProvider` keeps the default: its cache is validated by
bytes on every read.

## Error Handling

A failing step aborts the call with its error (`?`), after the steps before it have taken effect.
Every step is idempotent, so the caller retries the whole call. Per-key failures inside a step
keep their existing handling (the store audit skips unreadable metadata, `verify_stored_versions`
returns store errors). No new error constructor.

## Relevant Commands

None. No command or registry change.

## Documentation Architecture

- `specs/guides/DEPENDENCY_CONSISTENCY_GUIDE.md`: a new section "After an outside change" with the
  call, the meaning of each report field and the guarantee (Phase 1 Q2).
- `specs/reference/ASSETS.md` §Content changed outside Liquers: one paragraph pointing to the
  operation and stating the live-registration rule (AC-2).
- `specs/reference/DEPENDENCIES_STATUS.md` §Consistency policies: the operation as the explicit
  "bring a running manager in line" policy.
- History rows and `reviewed:` bumps. `affects_docs` as in `DESIGN.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/assets.rs` (trait, report, helper, two `registered_keys`), `recipes.rs` (hook, chain), `liquers-records/src/provider.rs` |
| Crates and workflows affected | `liquers-core`, `liquers-records`; no existing call changes |
| Existing tests likely to change | none (new provided methods only) |
| New validation | `liquers-core/tests/store_sync.rs` on both managers; one `liquers-records` unit test |
| Compatibility / data / concurrency / performance / security | Additive. Cost is the sum of the composed steps: a deep hash of every stored value under `root`, which the guide states. No global lock. A concurrent manager write is consistent by construction |
| Recovery | Revert. Steps 1-3 and 5-7 remain callable separately |
| Certainty and open questions | High for the composition; Q1 (levels) is the open contract question |
