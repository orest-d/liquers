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

//FIXME: For these two error constructors, key is available, so it should be set in the Error::key
//FIXME: Similarly, the query should be initialized by default using key.into(), though it should be set (using with_query) to the query where where the error occured (if available)
// NEW: Add to impl Error
pub fn dependency_version_mismatch(key: &DependencyKey, msg: impl Into<String>) -> Self {
    Error {
        error_type: ErrorType::DependencyVersionMismatch,
        message: format!("Dependency version mismatch for '{}': {}", key.as_str(), msg.into()),
        position: Position::unknown(),
        query: None,
        key: None,
        command_key: None,
    }
}

pub fn dependency_cycle(key: &DependencyKey) -> Self {
    Error {
        error_type: ErrorType::DependencyCycle,
        message: format!("Dependency cycle detected involving '{}'", key.as_str()),
        position: Position::unknown(),
        query: None,
        key: None,
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

### Step 2: Rewrite `dependencies.rs`

**File:** `liquers-core/src/dependencies.rs`

**Action:** Replace the entire file. The existing `DependencyManagerImpl` and all its doctests (lines 176-197 in the old file) are removed along with all old types (`StringDependency`, `DependencyList`, `Dependency` trait, `DependencyRecord<V,D>`). The new file defines all new types plus `DependencyManager<E>` implementation plus all 26 unit tests from Phase 3.

**Code structure (new file):**
```rust
// Imports
use std::collections::VecDeque;
use crate::assets::WeakAssetRef;
use crate::command_metadata::CommandKey;
use crate::context::Environment;
use crate::error::{Error, ErrorType};
use crate::query::{Key, Query};

// --- Version ---
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(u128);
// Custom serde: Serialize as 32-char lowercase hex; Deserialize from 32-char hex
impl serde::Serialize for Version { ... }  // format!("{:032x}", self.0)
impl<'de> serde::Deserialize<'de> for Version { ... }  // parse hex as u128

impl Version {
    pub fn new(v: u128) -> Self
    pub fn from_bytes(bytes: &[u8]) -> Self  // blake3 hash → first 16 bytes → u128
    pub fn from_time_now() -> Self            // SystemTime::now nanos as u128 (as_nanos() returns u128)
    pub fn from_specific_time(time: std::time::SystemTime) -> Self
    pub fn new_unique() -> Self              // AtomicU64 counter + SystemTime nanos (no rand crate)
}

// --- DependencyKey ---
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DependencyKey(String);

impl DependencyKey {
    pub fn new(s: impl Into<String>) -> Self
    pub fn as_str(&self) -> &str
    pub fn to_query(&self) -> Result<Query, Error>
    pub fn from_recipe_key(key: &Key) -> Self      // `-R-recipe/{key}`
    pub fn from_dir_key(key: &Key) -> Self          // `-R-dir/{key}`
    pub fn for_command_metadata(key: &CommandKey) -> Self   // format!("ns-dep/command_metadata-{}", key)
    pub fn for_command_implementation(key: &CommandKey) -> Self  // format!("ns-dep/command_implementation-{}", key)
}
impl From<&Key> for DependencyKey { ... }       // format!("-R/{}", key.encode())
impl TryFrom<&DependencyKey> for Key { ... }    // only "-R/..." keys succeed
impl From<&Query> for DependencyKey { ... }
impl std::fmt::Display for DependencyKey { ... }

// --- DependencyRecord ---
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyRecord {
    pub key: DependencyKey,
    pub version: Version,
}

// --- DependencyRelation (plan-only, no serde) ---
#[derive(Debug, Clone, PartialEq, Eq)]
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

// --- PlanDependency (plan-only, no serde) ---
#[derive(Debug, Clone, PartialEq, Eq)]
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
pub struct DependencyManager<E: Environment> {
    versions: scc::HashMap<DependencyKey, Version>,
    keyed_dependents: scc::HashMap<DependencyKey, scc::HashSet<DependencyKey>>,
    untracked_dependents: scc::HashMap<DependencyKey, Vec<WeakAssetRef<E>>>,  //TODO: rename to dependent_assets, also the methods
    expiration_lock: tokio::sync::Mutex<()>,
}

impl<E: Environment> DependencyManager<E> {
    pub fn new() -> Self
    pub async fn register_version(&self, key: &DependencyKey, version: Version)
    pub async fn add_dependency(&self, dependent: &DependencyKey, dependency: &DependencyKey, version: Version) -> Result<(), Error>
    //TODO: There should be a convenience function track_asset taking AssetRef as an argument.
    //TODO: Only assets that qualify for tracking (Ready, Source, Override) should be processed.
    //TODO: It should be checked whether the asset is a keyed asset, and automatically extract dependencies and their versions from asset metadata and register them.
    //TODO: Non-keyed assets should be registered as a dependent asset (downgrading to weak ref).
    //TODO: This should be used by asset manager to register all the created assets.
    //TODO: Verify if there are some potential pitfalls - e.g. violating consistency , aske questions if unclear.

    //TODO: Rename to add_dependent_asset 
    pub async fn add_untracked_dependent(&self, dependency: &DependencyKey, dependent: WeakAssetRef<E>) 
    pub async fn would_create_cycle(&self, dependent: &DependencyKey, dependency: &DependencyKey) -> bool
    pub async fn expire(&self, key: &DependencyKey) -> ExpiredDependents<E>
    pub async fn remove(&self, key: &DependencyKey)
    pub async fn version_consistent(&self, key: &DependencyKey, expected: Version) -> bool
    pub async fn get_version(&self, key: &DependencyKey) -> Option<Version>
    pub async fn load_from_records(&self, dependent: &DependencyKey, records: &[DependencyRecord])
}
```

**Key implementation notes:**

- `expire()`: acquire `expiration_lock`, then iterative BFS using `VecDeque`. For each expired key: remove from `versions`, collect `keyed_dependents` entries (BFS frontier), collect `untracked_dependents` entries (prune dead WeakAssetRefs). Remove from `keyed_dependents` and `untracked_dependents` maps. Return `ExpiredDependents`.
- `would_create_cycle()`: iterative BFS over `keyed_dependents` starting from `dependency`; returns true if `dependent` is reachable.
- `add_dependency()`: check `version_consistent` first (return `dependency_version_mismatch` if not); check `would_create_cycle` (return `dependency_cycle` if true); insert `dependent` into `keyed_dependents[dependency]`.
- `from_bytes`: use `copy_from_slice` not index-by-index: `u128::from_be_bytes(hash[0..16].try_into().unwrap_or([0u8;16]))`
- `from_time_now` / `from_specific_time`: use `.ok().unwrap_or_default()` not `unwrap()`; `as_nanos()` returns `u128` directly — no cast needed
- `new_unique`: `rand` is **not** in `liquers-core/Cargo.toml` and must not be added. Use a rand-free approach combining `AtomicU64` counter with `SystemTime`:
  ```rust
  static UNIQUE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
  pub fn new_unique() -> Self {
      let nanos = std::time::SystemTime::now()
          .duration_since(std::time::UNIX_EPOCH)
          .ok()
          .unwrap_or_default()
          .as_nanos();  // returns u128 directly
      let counter = UNIQUE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as u128;
      Version(nanos.wrapping_shl(64) | counter)
  }
  ```
  This guarantees uniqueness within a process (monotonic counter) and approximate uniqueness across processes (nanosecond timestamp in the high bits).
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
- **Knowledge:** `specs/dependency-management/phase2-architecture.md` (full), `specs/dependency-management/phase3-examples.md` (unit tests section), `liquers-core/src/assets.rs` (lines 677-705 for WeakAssetRef), `liquers-core/src/error.rs` (for Error constructors), `liquers-core/src/query.rs` (Key::encode format), `liquers-core/src/command_metadata.rs` (CommandKey::Display format), `liquers-core/Cargo.toml`
- **Rationale:** This is the largest single step — full new implementation. Sonnet needed for correct async scc patterns, BFS iterative algorithm, and custom serde.

---

### Step 3: Extend `MetadataRecord` with dependencies field

**File:** `liquers-core/src/metadata.rs`

**Action:**
- Add `dependencies: Vec<DependencyRecord>` to `MetadataRecord` struct
- Use `#[serde(default)]` for backward compatibility (old records without the field deserve to an empty Vec)
- Import `DependencyRecord` from `crate::dependencies`

**Code changes:**
```rust
// ADD: import
use crate::dependencies::DependencyRecord;

// MODIFY: MetadataRecord struct — add field
#[serde(default)]
pub dependencies: Vec<DependencyRecord>,
```

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
- Add `dependency_manager` and `max_dependency_retries` fields to `DefaultAssetManager<E>`
- Initialize in `new()`: `DependencyManager::new()` and `max_dependency_retries: 3`
TODO: Note that the command metadata and command implementation versions also need to be loaded into Dependency Manager at initialization. There should be a function that would update all versions from the command metadata registry. 
- Add `dependency_manager()` accessor (for tests)
- Modify `remove()`: call `self.dependency_manager.remove(&DependencyKey::from(key)).await`
- Modify `set_binary()`: for `Ready`/`Source`/`Override` states — call `dm.register_version()` then `dm.expire()` cascade; convert `ExpiredDependents.keys` to `Key` via `TryFrom`, call `expire()` on those; upgrade `ExpiredDependents.assets` weak refs, call expiration on live assets
- Modify `set_state()`: same cascade logic as `set_binary()` — after state is set and status is `Ready`/`Source`/`Override`, register version and cascade-expire
- **Cascade trigger condition:** Cascade expiration triggers only when the version genuinely changes. In `set_state`/`set_binary`, after the state is stored, compute the new `Version` (e.g., `Version::from_time_now()`), compare with `dm.get_version()` for this key. If the version is the same (no change), skip cascade. If the version is new or the key was not previously tracked, register the new version and cascade-expire dependents.
- Add `register_plan_dependencies()`: iterate `PlanDependency` slice; for each, resolve version from DM; call `dm.add_dependency()` if version is known; skip command metadata / recipe deps (no Key conversion needed)
- **Call site for `register_plan_dependencies()`:** In `AssetRef::evaluate_recipe()` (~line 1234), after the recipe is resolved but before `envref.apply_recipe()` is called (~line 1279). The plan is built from the recipe, `find_dependencies` is called on it, and the resulting `Vec<PlanDependency>` is passed to `envref.get_asset_manager().register_plan_dependencies(key, &plan_deps)`. This requires adding a plan-build step inside `evaluate_recipe()` (the commented-out plan-build at lines 1272-1275 is the natural insertion point). If the asset has no key (query asset), skip the DM registration but still build the plan for volatility detection.
- **Call site for `load_from_records()`:** In `AssetData::try_fast_track()` (~line 449), after successfully loading from store (after `self.metadata = metadata;` at line 501). Extract `MetadataRecord.dependencies` from the loaded metadata, compute `DependencyKey::from(&key)`, and call `dm.load_from_records(&dep_key, &metadata_record.dependencies)`. Since `try_fast_track` is on `AssetData` (not `DefaultAssetManager`), the DM must be accessed via the envref: `self.get_envref().get_asset_manager().dependency_manager().load_from_records(...)`. Also register the loaded asset's own version: `dm.register_version(&dep_key, Version::from_time_now())`.
FIXME: Loaded asset should have a version in the metadata, which should be used. Note: Using the `from_time_now()` would effectively almost destroy persistance, since every loaded asset would get a new version and would lead to expiration of all the dependents.
TODO: It is the responsibility of the store to always assure the version is correct - either by creating it from the file update time or recalculating the hash.
TODO: The consistency between store and asset manager version is an open problem. For now stick to blake3 hash both in store and in asset manager.
- Add `evaluate_with_retry()`: retry loop up to `max_dependency_retries`; match `ErrorType::DependencyVersionMismatch` → `tokio::task::yield_now().await` then retry; other errors propagate immediately
TODO: Add a `version()` method to the `AssetData` and `AssetRef`

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
    // NEW:
    dependency_manager: DependencyManager<E>,
    max_dependency_retries: u32,
}

// MODIFY: new()
let manager = DefaultAssetManager {
    // ...existing fields...
    dependency_manager: DependencyManager::new(),
    max_dependency_retries: 3,
};

// NEW: accessor
pub fn dependency_manager(&self) -> &DependencyManager<E> {
    &self.dependency_manager
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
- Change `find_dependencies` return type from `HashSet<Key>` to `Vec<PlanDependency>`
- Update the function body to construct `PlanDependency { key: DependencyKey::from(&key), relation: DependencyRelation::StateArgument }` (or the appropriate relation) for each dependency found
- Update **all 6 callers/call sites** of `find_dependencies`:
  - **4 recursive calls inside `find_dependencies` itself** (at ~lines 1567, 1612, 1679, 1686): these return `Vec<PlanDependency>` now, so change `dependencies.extend(indirect_deps)` to work with `Vec<PlanDependency>` instead of `HashSet<Key>`. The internal `dependencies` variable changes from `HashSet<Key>` to `Vec<PlanDependency>`.
  - **`has_volatile_dependencies`** (~line 1722): change `for key in dependencies` to `for pd in &dependencies { let Ok(key) = Key::try_from(&pd.key) else { continue }; ... }`
  - **`has_expirable_dependencies`** (~line 1756): same pattern as above
- Map `ParameterValue` variants to the appropriate `DependencyRelation` variants (see Phase 2 `find_dependencies` notes)

**Current signature (to change):**
```rust
pub(crate) fn find_dependencies<'a, E: Environment>(
    envref: EnvRef<E>,
    plan: &'a Plan,
    stack: &'a mut Vec<Key>,
    cwd: Option<Key>,
) -> Pin<Box<dyn Future<Output = Result<HashSet<Key>, Error>> + Send + 'a>>
```

**New signature:**
```rust
pub(crate) fn find_dependencies<'a, E: Environment>(
    envref: EnvRef<E>,
    plan: &'a Plan,
    stack: &'a mut Vec<Key>,
    cwd: Option<Key>,
) -> Pin<Box<dyn Future<Output = Result<Vec<PlanDependency>, Error>> + Send + 'a>>
```

**Updated caller pattern:**
```rust
// In has_volatile_dependencies:
let dependencies = find_dependencies(envref.clone(), plan, &mut stack, None).await?;
for pd in &dependencies {
    let Ok(key) = Key::try_from(&pd.key) else { continue };
    // ... recipe check ...
}
```

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
