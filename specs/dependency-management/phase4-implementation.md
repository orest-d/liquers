# Phase 4: Implementation Plan - dependency-management

## Overview

**Feature:** dependency-management

**Architecture:** Replace the skeletal `DependencyManagerImpl<V,D>` with `DependencyManager<E>` (scc-based, generic over Environment to store WeakAssetRef), add `DependencyKey`/`DependencyRecord`/`DependencyRelation`/`PlanDependency`/`ExpiredDependents<E>`, extend `DefaultAssetManager` with cascade expiration hooks, and update `plan.rs`/`context.rs` to produce typed dependency records.

**Estimated complexity:** High

**Prerequisites:**
- Phase 1, 2, 3 approved
- All open questions resolved
- `WeakAssetRef<E>` already defined at `liquers-core/src/assets.rs:683`
- `scc` and `blake3` already in `liquers-core/Cargo.toml`

---

## Implementation Steps

### Step 1: Add `ErrorType` variants and constructors

**File:** `liquers-core/src/error.rs`

**Action:**
- Add `DependencyVersionMismatch` and `DependencyCycle` variants to the `ErrorType` enum (must be explicit — no `_ =>` arm anywhere in the codebase that matches `ErrorType`)
- Add typed constructor methods to `Error`

**Code changes:**
```rust
// MODIFY: ErrorType enum — add after ExecutionError
DependencyVersionMismatch,
DependencyCycle,

// NEW: Add to impl Error
// `Error::key` is set from `DependencyKey` via `Key::try_from` (succeeds for `-R/` keys).
// `Error::query` is set to `Query::from(&key_str)` as a default, and callers may override
// it with `.with_query(&actual_query)` at the error site if available.
pub fn dependency_version_mismatch(key: &DependencyKey, msg: impl Into<String>) -> Self {
    let store_key = Key::try_from(key).ok();
    let query = store_key.as_ref().map(|k| Query::from(k));
    Error {
        error_type: ErrorType::DependencyVersionMismatch,
        message: format!("Dependency version mismatch for '{}': {}", key.as_str(), msg.into()),
        position: Position::unknown(),
        query,
        key: store_key,
        command_key: None,
    }
}

pub fn dependency_cycle(key: &DependencyKey) -> Self {
    let store_key = Key::try_from(key).ok();
    let query = store_key.as_ref().map(|k| Query::from(k));
    Error {
        error_type: ErrorType::DependencyCycle,
        message: format!("Dependency cycle detected involving '{}'", key.as_str()),
        position: Position::unknown(),
        query,
        key: store_key,
        command_key: None,
    }
}
```

**Important:** After adding `DependencyVersionMismatch` and `DependencyCycle`, find all `match error_type` / `match self.error_type` exhaustive match statements in the codebase and add explicit arms for the new variants. Run `cargo check` to surface them.

**Note on `classify_persistence_error`:** The function at `assets.rs:884` currently uses a `_ =>` default arm in its match on `ErrorType`. This violates the project's no-default-arm rule (new `ErrorType` variants won't trigger compile errors). While fixing it is not strictly required for this step, the agent should either:
- Replace the `_ =>` arm with explicit variants (preferred), or
- At minimum, add explicit arms for `DependencyVersionMismatch` and `DependencyCycle` mapping to `PersistenceStatus::NotPersisted`, alongside the existing `_ =>` arm (less preferred; leaves the rule violation in place for a future cleanup pass).

**Validation:**
```bash
cargo check -p liquers-core
```

**Rollback:**
```bash
git checkout liquers-core/src/error.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/error.rs`, `specs/dependency-management/phase2-architecture.md` (Error Handling section), `specs/dependency-management/phase3-examples.md` (ErrorType usage in examples)
- **Rationale:** Small, targeted change. Pattern matches existing constructors exactly.

---

### Step 1b: Extend `CommandMetadata` with version fields

**File:** `liquers-core/src/command_metadata.rs`

**Status:** Implemented.

**Fields added to `CommandMetadata`:**
- `#[serde(skip)] pub metadata_version: u128` — blake3 hash of the serializable fields, computed by the registry at registration time. Not serialized.
- `pub impl_version: u128` — implementation version set by the registering code via the `register_command!` macro or by direct assignment before calling `add_command()`. Serialized as a 32-char lowercase hex string; skipped when zero.

**Implementation:**
```rust
// CommandMetadata struct — fields added after existing fields
/// Metadata version: blake3 hash of the serializable fields, computed at registration.
/// Not serialized — deterministically recomputable from the other fields.
#[serde(skip)]
pub metadata_version: u128,

/// Implementation version: set by the registering code via register_command! macro
/// or by direct assignment before calling add_command(). Serialized as 32-char hex; skipped when zero.
#[serde(with = "hex_u128_serde")]
#[serde(skip_serializing_if = "u128_is_zero")]
#[serde(default)]
pub impl_version: u128,

// CommandMetadataRegistry — helper + updated add_command()
fn calculate_metadata_version(command: &CommandMetadata) -> u128 {
    let mut cm = command.clone();
    cm.impl_version = 0;  // Zero out impl_version before hashing (it is serialized)
    match serde_json::to_vec(&cm) {
        Ok(json) => {
            let hash = blake3::hash(&json);
            u128::from_be_bytes(hash.as_bytes()[0..16].try_into().unwrap_or([0u8; 16]))
        }
        Err(_) => 0,
    }
}

pub fn add_command(&mut self, command: &CommandMetadata) -> &mut Self {
    let key = command.key();
    let mut command_to_store = command.to_owned();
    // Preserve impl_version from any previously registered command with the same key
    if let Some(existing) = self.get(key.clone()) {
        command_to_store.impl_version = existing.impl_version;
    }
    command_to_store.metadata_version = Self::calculate_metadata_version(&command_to_store);
    // Upsert: update existing entry or push new one
    if let Some(existing) = self.get_mut(key) {
        *existing = command_to_store;
    } else {
        self.commands.push(command_to_store);
    }
    self
}
```

**Note on `impl_version`:** This field is set by the registering code at command registration time via the `register_command!` macro or by direct assignment before calling `add_command()`. `CommandMetadataRegistry` preserves the existing value on re-registration but does not set it itself.

**Note on `metadata_version` hash stability:** Because `impl_version` is serialized, it must be zeroed before hashing to ensure `metadata_version` reflects only the command's interface/documentation fields, not its implementation stamp.

**Validation:**
```bash
cargo check -p liquers-core
```

**Rollback:**
```bash
git checkout liquers-core/src/command_metadata.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/command_metadata.rs` (CommandMetadata struct at line 703, add_command at line 951), `specs/dependency-management/phase2-architecture.md` (command_metadata.rs section)
- **Rationale:** Small, targeted struct extension with standard serde attribute. Hash computation follows the same blake3 pattern used in `Version::from_bytes`.

---

### Step 1c: Add Plan-to-metadata helpers

**Files:** `liquers-core/src/plan.rs`, `liquers-core/src/metadata.rs`

**Action:**
- Add `Plan::to_metadata_record() -> MetadataRecord` — creates a `MetadataRecord` from plan fields, excluding `steps`. Status is set to `Status::Submitted`.
- Add `Plan::update_metadata_record(&self, mr: &mut MetadataRecord)` — updates an existing record from the same fields.
- Add `Metadata::from_plan(plan: &Plan) -> Metadata` — convenience wrapper.
- Add `Metadata::update_from_plan(&mut self, plan: &Plan)` — updates whichever variant (`MetadataRecord` or `LegacyMetadata`).

**Conversion rules (all methods):**
- `mr.query = plan.query.clone()`
- `mr.is_volatile = plan.is_volatile`
- `mr.expires = plan.expires.clone()`
- `plan.error` → if `Some(e)`, call `mr.with_error(e.clone())`; also sets status to `Error`
- `plan.init_steps` → for each step:
  - `Step::Info(msg)` → `mr.info(msg)`
  - `Step::Warning(msg)` → `mr.warning(msg)`
  - `Step::Error(msg)` → `mr.error(msg)` (sets status to `Error` via `add_log_entry`)
  - all other variants → skip
- Status is set to `Status::Submitted` initially (before processing `init_steps`; error steps will override to `Status::Error`)
- `plan.dependencies` is NOT copied into `mr.dependencies` here (those are `PlanDependency`, not `DependencyRecord`; DependencyRecords are written later by the asset manager after evaluation)

**`update_metadata_record`** applies the same transformations but preserves existing log entries (appends rather than replacing).

**Code changes:**
```rust
// In plan.rs — impl Plan
pub fn to_metadata_record(&self) -> MetadataRecord {
    let mut mr = MetadataRecord::new();
    mr.with_status(Status::Submitted);
    mr.with_query(self.query.clone());
    mr.is_volatile = self.is_volatile;
    mr.expires = self.expires.clone();
    if let Some(error) = &self.error {
        mr.with_error(error.clone());
    }
    for step in &self.init_steps {
        match step {
            Step::Info(msg) => { mr.info(msg); }
            Step::Warning(msg) => { mr.warning(msg); }
            Step::Error(msg) => { mr.error(msg); }
            _ => {}
        }
    }
    mr
}

pub fn update_metadata_record(&self, mr: &mut MetadataRecord) {
    mr.with_query(self.query.clone());
    mr.is_volatile = self.is_volatile;
    mr.expires = self.expires.clone();
    if let Some(error) = &self.error {
        mr.with_error(error.clone());
    }
    for step in &self.init_steps {
        match step {
            Step::Info(msg) => { mr.info(msg); }
            Step::Warning(msg) => { mr.warning(msg); }
            Step::Error(msg) => { mr.error(msg); }
            _ => {}
        }
    }
}

// In metadata.rs — impl Metadata
pub fn from_plan(plan: &Plan) -> Self {
    Metadata::MetadataRecord(plan.to_metadata_record())
}

pub fn update_from_plan(&mut self, plan: &Plan) {
    match self {
        Metadata::MetadataRecord(mr) => plan.update_metadata_record(mr),
        Metadata::LegacyMetadata(_) => {
            // Replace legacy metadata with a fresh record derived from the plan
            *self = Metadata::from_plan(plan);
        }
    }
}
```

**Note:** `metadata.rs` imports `Plan` from `crate::plan` for the `Metadata` methods. Alternatively, keep the `Metadata` methods in `plan.rs` as inherent methods or a trait impl to avoid the import direction going metadata→plan. Preferred: put `Plan::to_metadata_record` and `Plan::update_metadata_record` in `plan.rs`; put `Metadata::from_plan` and `Metadata::update_from_plan` in `plan.rs` as well (in an `impl Metadata` block), keeping `plan.rs` as the single file that knows about both types.

**Validation:**
```bash
cargo check -p liquers-core
```

**Rollback:**
```bash
git checkout liquers-core/src/plan.rs liquers-core/src/metadata.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/plan.rs` (Plan struct, Step enum), `liquers-core/src/metadata.rs` (MetadataRecord, Metadata, Status)
- **Rationale:** Mechanical field mapping; straightforward match on init_steps variants.

---

### Step 2: Rewrite `dependencies.rs`

**File:** `liquers-core/src/dependencies.rs`

**Action:** Replace the entire file. The existing `DependencyManagerImpl` and all its doctests (lines 176-197 in the old file) are removed along with all old types (`StringDependency`, `DependencyList`, `Dependency` trait, `DependencyRecord<V,D>`). The new file defines `DependencyRelation`, `PlanDependency`, `ExpiredDependents`, and `DependencyManager<E>` plus all 26 unit tests from Phase 3.

**Note:** `Version`, `DependencyKey`, and `DependencyRecord` are defined in `crate::metadata` and imported from there — they are **not** re-defined in this file.

**Code structure (new file):**
```rust
// Imports
use std::collections::VecDeque;
use crate::assets::{AssetRef, WeakAssetRef};
use crate::command_metadata::CommandKey;
use crate::context::Environment;
use crate::error::{Error, ErrorType};
use crate::metadata::{DependencyKey, DependencyRecord, Version};
use crate::query::{Key, Query};

// --- DependencyRelation ---
// Note: has Serialize/Deserialize because PlanDependency is stored in Plan.dependencies
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum DependencyRelation {
    StateArgument,
    ParameterLink(String),
    DefaultLink(String),
    RecipeLink(String),
    OverrideLink(String),
    EnumLink(String),
    ContextEvaluate(String),
    CommandMetadata,
    CommandImplementation,
    Recipe,
}

// --- PlanDependency ---
// Note: has Serialize/Deserialize because it is stored in Plan.dependencies
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlanDependency {
    pub key: DependencyKey,
    pub relation: DependencyRelation,
}

// --- ExpiredDependents ---
pub struct ExpiredDependents<E: Environment> {
    pub keys: Vec<DependencyKey>,
    pub assets: Vec<WeakAssetRef<E>>,
}

// --- DependencyManager ---
// pub(crate): not part of the public API. Users interact via DefaultAssetManager methods.
pub(crate) struct DependencyManager<E: Environment> {
    versions: scc::HashMap<DependencyKey, Version>,
    keyed_dependents: scc::HashMap<DependencyKey, scc::HashSet<DependencyKey>>,
    dependent_assets: scc::HashMap<DependencyKey, Vec<WeakAssetRef<E>>>,
    expiration_lock: tokio::sync::Mutex<()>,
}

impl<E: Environment> DependencyManager<E> {
    pub fn new() -> Self
    pub async fn register_version(&self, key: &DependencyKey, version: Version)
    pub async fn add_dependency(&self, dependent: &DependencyKey, dependency: &DependencyKey, version: Version) -> Result<(), Error>

    /// Register an asset and all its dependencies into the DM.
    /// - Only processes assets in Ready/Source/Override state.
    /// - For keyed assets: registers the asset's own version, then loads
    ///   DependencyRecords from the asset's metadata via `load_from_records`.
    /// - For non-keyed (query) assets: registers as a dependent_asset (weak ref)
    ///   on each of its metadata dependencies.
    pub async fn track_asset(&self, asset: &AssetRef<E>)

    pub async fn add_dependent_asset(&self, dependency: &DependencyKey, dependent: WeakAssetRef<E>)
    pub async fn would_create_cycle(&self, dependent: &DependencyKey, dependency: &DependencyKey) -> bool
    pub async fn expire(&self, key: &DependencyKey) -> ExpiredDependents<E>
    pub async fn remove(&self, key: &DependencyKey)
    pub async fn version_consistent(&self, key: &DependencyKey, expected: Version) -> bool
    pub async fn get_version(&self, key: &DependencyKey) -> Option<Version>
    pub async fn load_from_records(&self, dependent: &DependencyKey, records: &[DependencyRecord])
}
```

**Key implementation notes:**

- **Version 0 semantics:** `Version(0)` is a sentinel meaning "version unknown". Any comparison involving `Version(0)` must be treated as matching. Specifically:
  - `version_consistent(key, expected)` returns `true` if `expected == Version(0)` OR if the stored version is `Version(0)`.
  - `add_dependency(dependent, dependency, Version(0))` skips the `version_consistent` check entirely and just registers the edge. No error is returned.
  - In `expire()` (cascade): before cascading from a key, check `get_version(key)`; if it is `Some(Version(0))`, skip that key's cascade (its dependents are not invalidated since we don't know the real version).
  - This ensures that assets loaded without a known content hash do not cause spurious cascade expiration.

- `expire()`: acquire `expiration_lock`, then iterative BFS using `VecDeque`. For each expired key: skip if version is `Version(0)` (see above); remove from `versions`, collect `keyed_dependents` entries (BFS frontier), collect `dependent_assets` entries (prune dead WeakAssetRefs). Remove from `keyed_dependents` and `dependent_assets` maps. Return `ExpiredDependents`.

- `would_create_cycle()`: iterative BFS over `keyed_dependents` starting from `dependency`; returns true if `dependent` is reachable.

- `add_dependency()`: if `version != Version(0)`, check `version_consistent` first (return `dependency_version_mismatch` if not); check `would_create_cycle` (return `dependency_cycle` if true); insert `dependent` into `keyed_dependents[dependency]`.

- `track_asset()`:
  1. Await the asset's state; check that status is `Ready`, `Source`, or `Override` — return early otherwise.
  2. If the asset has a `Key`: compute `dep_key = DependencyKey::from(&key)`; call `register_version(&dep_key, asset_version)` where `asset_version` comes from `metadata.version.unwrap_or(Version(0))`; call `load_from_records(&dep_key, &metadata.dependencies)`.
  3. If the asset has no `Key` (query asset): for each `DependencyRecord` in its metadata, call `add_dependent_asset(&dep_record.key, asset.downgrade())`.

- `load_from_records()`: for each `DependencyRecord { key, version }` in `records`, call `add_dependency(dependent, &record.key, record.version)`. Ignore `DependencyVersionMismatch` errors here (the loaded dependency version may have advanced; a subsequent consistency check on evaluation will catch it).

- Include all 26 unit tests from Phase 3 in `#[cfg(test)] mod tests { use super::*; use crate::context::SimpleEnvironment; use crate::value::Value; }`

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core dep
```

**Rollback:**
```bash
git checkout liquers-core/src/dependencies.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** `specs/dependency-management/phase2-architecture.md` (full), `specs/dependency-management/phase3-examples.md` (unit tests section), `liquers-core/src/metadata.rs` (Version, DependencyKey, DependencyRecord already defined here), `liquers-core/src/assets.rs` (AssetRef, WeakAssetRef), `liquers-core/src/error.rs` (for Error constructors), `liquers-core/Cargo.toml`
- **Rationale:** Largest step — full new implementation. Sonnet needed for correct async scc patterns, BFS iterative algorithm, Version 0 semantics, and track_asset async logic.

---

### Step 3: Extend `MetadataRecord` with `version` and `dependencies` fields

**File:** `liquers-core/src/metadata.rs`

**Status (partial):** `dependencies: Vec<DependencyRecord>` is already added. `version: Option<Version>` still needs to be added.

**Action:**
- Add `pub version: Option<Version>` to `MetadataRecord` with `#[serde(default)]`

**Purpose of `version`:** Stores the content-hash version of the asset computed at save time (`Version::from_bytes(content)`). On reload, the dependency manager uses this stored version instead of `Version::from_time_now()`. Assets without a stored version use `None`, which maps to `Version(0)` (unknown) — harmless due to Version 0 semantics.

**Code changes:**
```rust
// MODIFY: MetadataRecord struct — add field after `expiration_time`
/// Content-hash version of this asset, computed at save time as Version::from_bytes(content).
/// None for assets whose version has not been recorded (treated as Version(0) = unknown).
#[serde(default)]
pub version: Option<Version>,
```

**Note:** `Version` and `DependencyRecord` are already defined in this file (done in prior step).

**Validation:**
```bash
cargo check -p liquers-core
```

**Rollback:**
```bash
git checkout liquers-core/src/metadata.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/metadata.rs` (MetadataRecord struct), `specs/dependency-management/phase2-architecture.md` (MetadataRecord section)
- **Rationale:** Trivial field addition with standard serde attribute.

---

### Step 4: Extend `DefaultAssetManager` in `assets.rs`

**File:** `liquers-core/src/assets.rs`

**Action:**
- Add `dependency_manager` and `max_dependency_retries` fields to `DefaultAssetManager<E>`. `DependencyManager<E>` is `pub(crate)`.
- Initialize in `new()`: `DependencyManager::new()` and `max_dependency_retries: 3`
- Add `load_command_versions()`: iterates `CommandMetadataRegistry`, registers `metadata_version` and `impl_version` for every command as `DependencyKey::for_command_metadata(ck)` / `for_command_implementation(ck)`. Called from `Environment::to_ref()` after the envref OnceLock is set. Idempotent — safe to call multiple times (e.g., after dynamic command registration).
- Add `dependency_manager()` accessor (pub(crate), for internal use and tests)
- Modify `remove()`: call `self.dependency_manager.remove(&DependencyKey::from(key)).await`
- Modify `set_binary()` and `set_state()`: for `Ready`/`Source`/`Override` states — compute `Version::from_bytes(content)` (for binary) or `Version::from_time_now()` (for in-memory state); compare with `dm.get_version()`. If new or changed, register version and call `cascade_expire_dependents`. Skip if version unchanged. Skip entirely if `Status::Volatile`. Also store the computed version in the asset's `MetadataRecord.version`.
- **Version for loaded assets:** In `try_fast_track()`, use `metadata_record.version.unwrap_or(Version(0))`. `Version(0)` = unknown, which never causes spurious cascade expiration (Version 0 semantics). The store is responsible for computing and persisting `Version::from_bytes(content)` when storing assets, ensuring the version is stable across reloads.
- **Early metadata from plan:** In `AssetRef::evaluate_recipe()`, after `plan = recipe.to_plan(cmr)?` is built (and `has_volatile_dependencies`/`has_expirable_dependencies` are called), call `asset.set_early_metadata(plan.to_metadata_record())` to publish `Status::Submitted` metadata with plan info (volatility, expiration, init_step log entries) before evaluation starts.
- Add `set_early_metadata()` method to `AssetData`/`AssetRef`: sets metadata from a `MetadataRecord` only when the asset is in an early non-ready state (e.g., `None`, `Submitted`, `Dependencies`). Does not overwrite `Ready`/`Source`/`Override` metadata.
- Add `register_plan_dependencies()`: iterate `PlanDependency` slice; for each, resolve version from DM (use `Version(0)` if not found — edge still registered); call `dm.add_dependency()`; skip if volatile.
- **Call site for `register_plan_dependencies()`:** In `AssetRef::evaluate_recipe()`, after plan analysis, before `apply_recipe()`.
- **Call site for `track_asset()`:** In `DefaultAssetManager::set_state()` and `set_binary()`, after the state is successfully stored with `Ready`/`Source`/`Override` status, call `self.dependency_manager.track_asset(&asset_ref).await`.
- Add `evaluate_with_retry()`: retry loop up to `max_dependency_retries`; match `ErrorType::DependencyVersionMismatch` → `tokio::task::yield_now().await` then retry; other errors propagate immediately.
- Add `version()` accessor to `AssetData` and `AssetRef` returning `Option<Version>` from metadata.

**Code changes (key fragments):**
```rust
// MODIFY: DefaultAssetManager struct
pub struct DefaultAssetManager<E: Environment> {
    id: std::sync::atomic::AtomicU64,
    envref: std::sync::OnceLock<EnvRef<E>>,
    assets: scc::HashMap<Key, AssetRef<E>>,
    query_assets: scc::HashMap<Query, AssetRef<E>>,
    job_queue: Arc<JobQueue<E>>,
    monitor_tx: mpsc::UnboundedSender<ExpirationMonitorMessage<E>>,
    // NEW: DependencyManager is pub(crate)
    dependency_manager: DependencyManager<E>,
    max_dependency_retries: u32,
}

// MODIFY: new()
let manager = DefaultAssetManager {
    // ...existing fields...
    dependency_manager: DependencyManager::new(),
    max_dependency_retries: 3,
};

// NEW: accessor (pub(crate) — not public API)
pub(crate) fn dependency_manager(&self) -> &DependencyManager<E> {
    &self.dependency_manager
}

// NEW: load command versions into DM (called from to_ref() after envref is set)
pub async fn load_command_versions(&self) {
    let envref = match self.envref.get() {
        Some(e) => e,
        None => return,
    };
    let cmr = envref.get_command_metadata_registry();
    for cmd in cmr.commands() {
        let ck = cmd.key();
        if cmd.metadata_version != 0 {
            self.dependency_manager
                .register_version(&DependencyKey::for_command_metadata(&ck), Version::new(cmd.metadata_version))
                .await;
        }
        if cmd.impl_version != 0 {
            self.dependency_manager
                .register_version(&DependencyKey::for_command_implementation(&ck), Version::new(cmd.impl_version))
                .await;
        }
    }
}

// NEW: cascade helper — call after setting Ready/Source/Override state
async fn cascade_expire_dependents(&self, dep_key: &DependencyKey) {
    let expired = self.dependency_manager.expire(dep_key).await;
    // Expire keyed assets
    for key in &expired.keys {
        if let Ok(k) = Key::try_from(key) {
            if let Some(entry) = self.assets.get_async(&k).await {
                let ar = entry.get().clone();
                drop(entry);
                let _ = ar.expire().await;
            }
        }
    }
    // Expire untracked assets (WeakAssetRef)
    for weak_ref in &expired.assets {
        if let Some(ar) = weak_ref.upgrade() {
            let _ = ar.expire().await;
        }
    }
}

// NEW: register_plan_dependencies
pub async fn register_plan_dependencies(
    &self,
    dependent_key: &Key,
    plan_deps: &[PlanDependency],
) -> Result<(), Error> {
    let dep_key = DependencyKey::from(dependent_key);
    for plan_dep in plan_deps {
        // Skip if version unknown (dep not yet registered in DM — will be skipped)
        if let Some(ver) = self.dependency_manager.get_version(&plan_dep.key).await {
            self.dependency_manager.add_dependency(&dep_key, &plan_dep.key, ver).await?;
        }
    }
    Ok(())
}

// NEW: evaluate_with_retry (private)
async fn evaluate_with_retry(
    &self,
    asset_ref: &AssetRef<E>,
) -> Result<State<E::Value>, Error> {
    for attempt in 0..self.max_dependency_retries {
        match asset_ref.evaluate().await {
            Ok(val) => return Ok(val),
            Err(e) if e.error_type == ErrorType::DependencyVersionMismatch => {
                if attempt + 1 == self.max_dependency_retries {
                    return Err(e);
                }
                tokio::task::yield_now().await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}
```

**Volatile guard:** Before any `dependency_manager` call, check volatility and skip DM operations for volatile assets. Specific guard points:
- In `set_state()` / `set_binary()`: after determining the final status, check `if final_status == Status::Volatile { /* skip DM registration and cascade */ }`.
- In `register_plan_dependencies()`: caller should not invoke this for volatile assets; add a guard at the top: `if self.is_volatile(dependent_key).await? { return Ok(()); }`.
- In `try_fast_track()` (for `load_from_records`): volatile assets never reach `try_fast_track` (they have `ExpirationTime::Immediately` and are created fresh each time), so no guard is needed there.
- In `cascade_expire_dependents()`: no guard needed; the DM itself only contains non-volatile entries.

**scc lock-safety note:** Never hold an scc entry guard across `.await`. Pattern: `let x = entry.get().clone(); drop(entry); use x.await`.

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core
```

**Rollback:**
```bash
git checkout liquers-core/src/assets.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/assets.rs` (full — especially `DefaultAssetManager::new()` at line 2132, `set_state` at 2875, `remove` at 2714, `set_binary` at 2749), `specs/dependency-management/phase2-architecture.md` (DefaultAssetManager section, Cascade Expiration design note, Retry Logic design note), `liquers-core/src/dependencies.rs` (just written in Step 2)
- **Rationale:** Complex scc patterns — requires attention to lock-before-await rules and cascade expiration logic.

---

### Step 5: Update `find_dependencies` in `plan.rs`

**File:** `liquers-core/src/plan.rs`

TODO: Be ready to use the init section of the plan (To Be Done).

**Action:**
- The return type is already `Vec<PlanDependency>` (done). The function body needs extending.
- Update the function to handle currently-incomplete step variants:

**`Step::Action { realm, ns, action_name, parameters, .. }`**
For each action, add command dependencies:
```rust
let ck = CommandKey::new(realm, ns, action_name);
dependencies.insert(PlanDependency::new(
    DependencyKey::for_command_metadata(&ck),
    DependencyRelation::CommandMetadata,
));
dependencies.insert(PlanDependency::new(
    DependencyKey::for_command_implementation(&ck),
    DependencyRelation::CommandImplementation,
));
```
Then traverse each `ParameterValue` in `parameters` recursively (helper function `collect_param_deps`):
```rust
fn collect_param_deps(pv: &ParameterValue, out: &mut HashSet<PlanDependency>) {
    match pv {
        ParameterValue::ParameterLink(name, query, _) =>
            out.insert(PlanDependency::new(DependencyKey::from(query), DependencyRelation::ParameterLink(name.clone()))),
        ParameterValue::DefaultLink(name, query) =>
            out.insert(PlanDependency::new(DependencyKey::from(query), DependencyRelation::DefaultLink(name.clone()))),
        ParameterValue::OverrideLink(name, query) =>
            out.insert(PlanDependency::new(DependencyKey::from(query), DependencyRelation::OverrideLink(name.clone()))),
        ParameterValue::EnumLink(name, query, _) =>
            out.insert(PlanDependency::new(DependencyKey::from(query), DependencyRelation::EnumLink(name.clone()))),
        ParameterValue::MultipleParameters(vec) =>
            for pv in vec { collect_param_deps(pv, out); }
        _ => {}
    }
}
```

**`Step::GetAsset*(key)` — also add Recipe dependency when recipe is found:**
```rust
// After adding the StateArgument dependency:
if let Ok(Some(_recipe)) = envref.get_recipe_provider().recipe_opt(&resolved_key, envref.clone()).await {
    dependencies.insert(PlanDependency::new(
        DependencyKey::from_recipe_key(&resolved_key),
        DependencyRelation::Recipe,
    ));
    // ... existing recursive recipe-plan traversal for cycle detection ...
}
```

**`Step::GetAssetRecipe(key)`:**
```rust
let resolved_key = /* resolve relative to cwd */;
if stack.contains(&resolved_key) {
    return Err(Error::general_error(...).with_key(&resolved_key));
}
dependencies.insert(PlanDependency::new(
    DependencyKey::from_recipe_key(&resolved_key),
    DependencyRelation::Recipe,
));
```

**`Step::Evaluate(query)` and `Step::Plan(nested_plan)` — full expansion:**

Queries and inline plans cannot themselves be tracked as dependency keys (they have no stable `Key`). Instead, recursively collect all their keyed (`-R/` prefix) `PlanDependency` entries and add them as `StateArgument` dependencies of the current plan:
```rust
Step::Evaluate(query) => {
    let cmr = envref.get_command_metadata_registry();
    let eval_plan = PlanBuilder::new(query.clone(), cmr).build()?;
    let sub_deps = find_dependencies(envref.clone(), &eval_plan, stack, current_cwd.clone()).await?;
    for dep in sub_deps {
        // Only promote keyed (Key-convertible) deps; non-keyed queries are transient
        if Key::try_from(&dep.key).is_ok() {
            dependencies.insert(PlanDependency::new(dep.key, DependencyRelation::StateArgument));
        }
        // Non-keyed: command metadata/impl deps are still relevant — include them
        else {
            dependencies.insert(dep);
        }
    }
}
Step::Plan(nested_plan) => {
    let sub_deps = find_dependencies(envref.clone(), nested_plan, stack, current_cwd.clone()).await?;
    for dep in sub_deps {
        if Key::try_from(&dep.key).is_ok() {
            dependencies.insert(PlanDependency::new(dep.key, DependencyRelation::StateArgument));
        } else {
            dependencies.insert(dep);
        }
    }
}
```

**Note on `DependencyRelation`/`PlanDependency` import:** These types are now in `crate::dependencies`. Add `use crate::dependencies::{DependencyRelation, PlanDependency};` to `plan.rs` imports (alongside the existing `use crate::metadata::DependencyKey;`).

**Callers `has_volatile_dependencies` and `has_expirable_dependencies`:** Already use the `Key::try_from(&pd.key)` pattern — no further changes needed.

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core
```

**Rollback:**
```bash
git checkout liquers-core/src/plan.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/plan.rs` (lines 1518–1800, find_dependencies + callers), `specs/dependency-management/phase2-architecture.md` (plan.rs section + DependencyRelation variants), `liquers-core/src/dependencies.rs` (DependencyRelation, PlanDependency, DependencyKey)
- **Rationale:** Requires careful mapping of existing plan step types to `DependencyRelation` variants; recursive function with type change propagation.

---

### Step 6: Extend `Context::evaluate` in `context.rs`

**File:** `liquers-core/src/context.rs`

**Action:**
- Add `pending_dependencies: Arc<tokio::sync::Mutex<Vec<DependencyRecord>>>` field to `Context<E>`
- Initialize in `Context::new()` (or wherever Context is constructed): `pending_dependencies: Arc::new(tokio::sync::Mutex::new(Vec::new()))`
- Extend `Context::evaluate()`:
  1. Obtain the current asset's `DependencyKey` (from `self.assetref`)
  2. Compute the evaluated query's `DependencyKey`
  3. Check `dm.would_create_cycle(&current_dep_key, &query_dep_key).await` — return `Error::dependency_cycle` if true
  4. Call the original evaluation (`envref.get_asset_manager().get_asset(query).await`)
  5. After success, get the evaluated asset's version from DM
  6. If version known, call `dm.add_untracked_dependent(&query_dep_key, current_asset.downgrade()).await`
  7. Push `DependencyRecord { key: query_dep_key, version }` to `self.pending_dependencies`
- Add `pub(crate) async fn record_dependency(&self, dep_key: &DependencyKey, version: Version) -> Result<(), Error>` for testing

**Note on key extraction:** The current asset's `Key` may not always be available (query assets have no store key). If the current asset has no associated `Key`, skip the DM cycle check and `add_untracked_dependent` call, but still record to `pending_dependencies` so the dependency is tracked locally. This means query assets get dependency recording in `pending_dependencies` but no DM-level cycle detection. This is acceptable because query assets are transient and do not participate in cascade expiration as dependents via keyed edges.

**Context Clone impl update (REQUIRED):** `Context<E>` has a manual `Clone` implementation at `context.rs:322`. After adding the `pending_dependencies` field, this `Clone` impl **must** be updated to include `pending_dependencies: self.pending_dependencies.clone()`. The `with_volatile()` method at line 228 also constructs a `Context` manually and must include the new field. Failure to update these will cause a compile error (non-exhaustive struct literal).

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core
```

**Rollback:**
```bash
git checkout liquers-core/src/context.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/context.rs` (full — Context struct, evaluate method at line 192, Context::new), `specs/dependency-management/phase2-architecture.md` (context.rs section + Concurrency section), `liquers-core/src/assets.rs` (AssetRef::key or equivalent — how to get Key from AssetRef), `liquers-core/src/dependencies.rs` (DependencyKey, DependencyManager API)
- **Rationale:** Async context with Arc<Mutex> pattern; must not hold lock across await points.

---

### Step 7: Add `dep` namespace commands in `liquers-lib`

**File:** `liquers-lib/src/commands.rs`

**Action:**
- Add `command_metadata` function: looks up `CommandMetadata` from registry (which already has `metadata_version: u128` pre-computed by `add_command()`), serializes and returns it as a JSON `Value`
- Add `command_implementation` function: reads `impl_version` from the registered `CommandMetadata` and returns it as a JSON `Value`
- Register both in the `dep` namespace using `register_command!`

TODO: Use `ValueInterface::from_command_metadata(...)`
**Code:**
```rust
fn command_metadata(
    _state: &State<Value>,
    realm: String,
    namespace: String,
    name: String,
    context: &Context<impl Environment<Value = Value>>,
) -> Result<Value, Error> {
    let ck = CommandKey::new(&realm, &namespace, &name);
    let cmr = context.get_envref_blocking().get_command_metadata_registry();
    let cmd_meta = cmr.get(&ck).ok_or_else(|| {
        Error::general_error(format!("Command not found: {}", ck))
    })?;
    // Serialize CommandMetadata to JSON (metadata_version is #[serde(skip)] — not included;
    // impl_version is included as a 32-char hex string when nonzero).
    let json = serde_json::to_string(cmd_meta)
        .map_err(|e| Error::general_error(e.to_string()))?;
    Ok(Value::from(json))
}

//TODO: For now, the `command_implementation` should be identical to command_metadata
//TODO: These two commands are only formal, they should practically never be called, they only should provide a way to construct a meaningful DependencyKey distinguising command metadata from command implementation
//TODO: However, they should provide meaningful output that would provide helpful information.
//TODO: Reason: All dependencies in the UI might be clickable and clicking on command_implementation DependencyKey should provide a meaningful result - showing the command metadata provides a good description of the command.

fn command_implementation(
    _state: &State<Value>,
    realm: String,
    namespace: String,
    name: String,
    context: &Context<impl Environment<Value = Value>>,
) -> Result<Value, Error> {
    let ck = CommandKey::new(&realm, &namespace, &name);
    let cmr = context.get_envref_blocking().get_command_metadata_registry();
    let cmd_meta = cmr.get(&ck).ok_or_else(|| {
        Error::general_error(format!("Command not found: {}", ck))
    })?;
    let result = serde_json::json!({
        "impl_version": format!("{:032x}", cmd_meta.impl_version),
        "realm": realm,
        "namespace": namespace,
        "name": name,
    });
    Ok(Value::from(result.to_string()))
}

// Register:
register_command!(cr,
    fn command_metadata(state, realm: String, namespace: String, name: String, context) -> result
    namespace: "dep"
    label: "Command Metadata"
    doc: "Returns the CommandMetadata for the named command"
)?;
register_command!(cr,
    fn command_implementation(state, realm: String, namespace: String, name: String, context) -> result
    namespace: "dep"
    label: "Command Implementation Version"
    doc: "Returns the impl_version from the registered CommandMetadata for the named command"
)?;
```

**Note on `impl_version` in CommandMetadata:** Set `impl_version` at registration time via the `register_command!` macro or by assigning the field directly on the `CommandMetadata` before calling `add_command()`.

**Note:** `context` must be the **last** parameter in the register_command! DSL (see ISSUES.md for parameter index bug with context). Both function signatures have context as last.

**Validation:**
```bash
cargo check -p liquers-lib
```

**Rollback:**
```bash
git checkout liquers-lib/src/commands.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-lib/src/commands.rs` (existing command registration patterns), `specs/COMMAND_REGISTRATION_GUIDE.md`, `specs/REGISTER_COMMAND_FSD.md`, `specs/ISSUES.md` (context parameter position bug), `specs/dependency-management/phase2-architecture.md` (dep namespace + command_metadata.rs sections), `liquers-core/src/command_metadata.rs` (CommandMetadata struct with new version fields from Step 1b)
- **Rationale:** Standard command registration, well-understood pattern. The `command_metadata` function is now much simpler (no hashing — just registry lookup + serialize).

TODO: `specs/COMMAND_REGISTRATION_GUIDE.md` and `specs/REGISTER_COMMAND_FSD.md` needs to be updated to explain `command_version` macro and recommend to always use it together with `version: auto`.
 
---

### Step 8: Create integration test file

**File:** `liquers-core/tests/dependency_manager_integration.rs`

**Action:**
- Create with the first two compilable integration tests from Phase 3 sketches (Tests 1 and 2)
- Test 1: `set_state_triggers_cascade_expiration` — using `SimpleEnvironment<Value>`, register deps, set_state, verify cascade
- Test 2: `metadata_deserialized_into_dm_on_load` — using MemoryStore, write MetadataRecord with dependencies, load via get(), verify DM edges reconstructed
- **Deferred tests:** Phase 3 integration tests 3 (`concurrent_expiration_serialized`) and 4 (`evaluate_with_retry_succeeds_after_mismatch`) are **deferred to a follow-up pass** after the core implementation is stable. They require more complex test scaffolding (spawning concurrent tasks for test 3, injecting mid-evaluation expiration for test 4) and are better addressed once the basic integration tests pass. Add `// TODO: Phase 3 integration tests 3 and 4 deferred` comment at the end of the test file.

**Validation:**
```bash
cargo test -p liquers-core --test dependency_manager_integration
```

**Rollback:**
```bash
rm liquers-core/tests/dependency_manager_integration.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** `liquers-core/tests/async_hellow_world.rs` (full integration test pattern), `specs/dependency-management/phase3-examples.md` (Integration Tests section), `liquers-core/src/assets.rs` (DefaultAssetManager API), `liquers-core/src/dependencies.rs` (just written)
- **Rationale:** Standard test pattern, closely follows the Phase 3 sketches.

---

## Testing Plan

### Unit Tests

**File:** `liquers-core/src/dependencies.rs` — inline `#[cfg(test)] mod tests`

**Run:** `cargo test -p liquers-core dep`

All 26 tests from Phase 3 are included in Step 2.

### Integration Tests

**File:** `liquers-core/tests/dependency_manager_integration.rs`

**Run:** `cargo test -p liquers-core --test dependency_manager_integration`

### Full Crate Tests

**Run after each step:**
```bash
cargo check -p liquers-core    # after steps 1-6
cargo check -p liquers-lib     # after step 7
cargo test -p liquers-core     # after step 2 (unit tests)
cargo test -p liquers-core     # after step 8 (integration tests)
```

### Manual Validation

```bash
cargo test -p liquers-core dep
cargo test -p liquers-core
cargo test -p liquers-lib
cargo test -p liquers-core --test dependency_manager_integration
```

---

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|------|-------|--------|-----------|
| 1 — error.rs variants | haiku | rust-best-practices | Small, targeted, pattern matches existing |
| 1b — command_metadata.rs version fields | haiku | rust-best-practices | Small struct extension + hash in add_command() |
| 2 — rewrite dependencies.rs | sonnet | rust-best-practices, liquers-unittest | Largest step; scc async patterns + BFS + custom serde |
| 3 — metadata.rs field | haiku | rust-best-practices | Trivial field addition |
| 4 — assets.rs extension | sonnet | rust-best-practices | Complex scc lock-safety + cascade expiration |
| 5 — plan.rs find_dependencies | sonnet | rust-best-practices | Recursive type change + caller updates |
| 6 — context.rs evaluate | sonnet | rust-best-practices | Async Arc<Mutex> + dependency recording |
| 7 — dep commands | haiku | rust-best-practices | Registry lookup + impl_version from CommandMetadata |
| 8 — integration tests | haiku | rust-best-practices, liquers-unittest | Standard test pattern |

---

## Rollback Plan

**Per-step rollback:**
```bash
git checkout liquers-core/src/error.rs                              # Step 1
git checkout liquers-core/src/command_metadata.rs                    # Step 1b
git checkout liquers-core/src/dependencies.rs                        # Step 2
git checkout liquers-core/src/metadata.rs                            # Step 3
git checkout liquers-core/src/assets.rs                              # Step 4
git checkout liquers-core/src/plan.rs                                # Step 5
git checkout liquers-core/src/context.rs                             # Step 6
git checkout liquers-lib/src/commands.rs                             # Step 7
rm liquers-core/tests/dependency_manager_integration.rs              # Step 8
```

**Full feature rollback:**
```bash
git checkout liquers-core/src/error.rs liquers-core/src/command_metadata.rs \
    liquers-core/src/dependencies.rs liquers-core/src/metadata.rs \
    liquers-core/src/assets.rs liquers-core/src/plan.rs \
    liquers-core/src/context.rs liquers-lib/src/commands.rs
rm -f liquers-core/tests/dependency_manager_integration.rs
```

---

## Documentation Updates

- `specs/dependency-management/DESIGN.md` — mark all phases complete after implementation
- No `CLAUDE.md` or `PROJECT_OVERVIEW.md` changes required (no new core concepts; dependency management is an internal implementation detail)

---

## Execution Options

After approval:
- **Execute now** — implement steps 1–8 sequentially using assigned models
- **Create task list** — defer execution
- **Revise plan** — return to Phase 4
- **Exit** — user implements manually
