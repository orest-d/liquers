# Phase 2: Solution & Architecture — Prune stored derived values

## Overview

A provided `AssetManager::prune_store(root, policy)` lists every key under `root`
(`listdir_keys_deep`), reads each stored record, classifies the key with an explicit decision, and
removes the selected ones through `remove_computed` (or `remove` for opted-in orphans). It returns
a report. Being a provided method, it works on both built-in managers and on an external manager
that implements `remove_computed`.

Rejected alternatives:
- **Deleting store objects directly** (`store.remove`): bypasses the key mutation lock and the
  live-asset handling of `remove`, so a concurrent override could be deleted (AC-8 fails).
- **A store-level operation** (`AsyncStore::prune`): a store does not know recipes or the manager's
  registrations, so it cannot tell derived from user data.
- **Recording last access on read** (question 1 b): a store write on every fast-track load, or a
  throttle table, and a new serialized field.
- **Running inside `collect_garbage`**: collection is frequent and in-memory; a store walk is slow
  and should be scheduled by the operator.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `ASSETS-API-ADMIN-OPERATIONS` | draft, designed | P3 | provides `remove_computed` | yes, or add it in step 1 | no | `requires` |
| `CORE-ASSET-GC` | accepted, designed | P3 | registered keys are skipped; collection makes them prunable | no | no | none |
| `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` | draft | P3 | an in-flight uncached evaluation may write back after a prune; harmless (the value is current) | no | no | none |
| `RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED` | draft | P2 | an edited recipe is not an orphan; its stale values are pruned only by age | no | no | none |

## Interfaces

```rust
// liquers-core/src/assets.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PrunePolicy {
    /// Prune computed values whose stored record is older than this.
    pub older_than: std::time::Duration,
    /// Also remove computed values whose recipe is gone (question 3). Default false.
    #[serde(default)] pub remove_orphans: bool,
    /// Report only. Default false.
    #[serde(default)] pub dry_run: bool,
}
impl PrunePolicy {
    pub fn older_than(d: std::time::Duration) -> Self;
    pub fn with_remove_orphans(self, yes: bool) -> Self;
    pub fn with_dry_run(self, yes: bool) -> Self;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PruneKeepReason { UserValue, NoMetadata, Recent, UnknownAge, Registered,
                           NotComputed, Directory, Volatile }

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PruneReport {
    pub pruned: Vec<Key>,
    pub orphans: Vec<Key>,                       // found; removed only when remove_orphans
    pub kept: Vec<(Key, PruneKeepReason)>,
    pub failed: Vec<(Key, Error)>,
}

// AssetManager<E>, provided:
async fn prune_store(&self, root: &Key, policy: &PrunePolicy) -> Result<PruneReport, Error>;
```

Async because it lists, reads metadata and removes through the store. `Serialize` on the report so
a later admin route or command can return it unchanged.

## Integration Points

`liquers-core/src/assets.rs`, provided method on `AssetManager<E>`:

1. `let now = chrono::Utc::now()`; `keys = self.listdir_keys_deep(root).await?` (the only `?`).
2. Per key, `classify(key)`: `store.is_dir` → `Directory`; `self.lookup_key_asset(key).is_some()`
   → `Registered`; `store.get_metadata(key)`: missing or `LegacyMetadata` with no status →
   `NoMetadata`; then an explicit `match` over `Status` with no `_` arm: `Source` | `Override` →
   `UserValue`; `Volatile` → `Volatile`; `Recipe` | `None` | in-flight | `Directory` |
   `Cancelled` → `NotComputed`; `Ready` | `Expired` | `Error` → age check. Age: parse `updated`
   with `chrono::DateTime::parse_from_rfc3339`; failure → `UnknownAge`; newer than `older_than` →
   `Recent`. Then `recipe_provider.has_recipes(key, envref)`: true → prune candidate; false →
   orphan.
3. Unless `dry_run`: candidates → `self.remove_computed(key)`; orphans with `remove_orphans` →
   `self.remove(key)`. A per-key `Err` goes to `failed`.
4. Keys are processed sequentially in sorted order (bounded work per call; no concurrency knob).

`remove_computed` re-decides under the key mutation lock, so a key that changed status between
classification and removal is refused (AC-8) and lands in `failed` with `StatusConflict`.

## Error Handling

Only the listing can fail the call (`Err` from `listdir_keys_deep`). Every per-key error (metadata
read, `StatusConflict`, read-only store, `KeyNotFound` after a concurrent removal) is recorded in
`report.failed` and the pass continues. No new `ErrorType`; no `unwrap`.

## Relevant Commands

None in this design. No `specs/command_registry.yaml` change. A command (`ns-?/prune`) or an
`assets-api` admin route returning `PruneReport` is a follow-up once the policy is agreed
(question 4).

## Documentation Architecture

- `specs/reference/ASSETS.md`: removal section — pruning, its selection rule, what is never
  touched, the orphan option.
- `specs/reference/STORE_SEMANTICS.md` §5 Removal: a pruned key is a metadata-only record with
  status `Recipe` and its version.
- `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md`: "Bounding the store" with the Problem Example.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/assets.rs` (types, provided method); new test file `liquers-core/tests/store_pruning.rs` |
| Crates and workflows affected | `liquers-core`; nothing runs unless called |
| Existing tests likely to change | none |
| New validation | `store_pruning.rs` on queued and inline managers; a read-only mount case |
| Compatibility / data / concurrency / performance / security | data loss is limited to recomputable values (and opted-in orphans); concurrency via `remove_computed`'s lock; a large subtree is O(keys) metadata reads per call; exposing it over HTTP later needs authorisation |
| Recovery | removed data is recomputed on demand; orphan removal is not recoverable (opt-in) |
| Certainty and open questions | high on mechanics (`remove`'s table read at HEAD); questions 1-3 are policy |
