//! Dependency management for the Liquers asset system.
//!
//! This module defines the runtime dependency graph that tracks which assets depend on
//! which other assets (or commands). When an asset changes, the dependency manager
//! identifies all transitively affected dependents so they can be expired.
//!
//! Pure data types (`Version`, `DependencyKey`, `DependencyRecord`) live in `crate::metadata`.
//! This module defines the relationship/graph types and the `DependencyManager<E>`.

use std::collections::VecDeque;

use crate::assets::WeakAssetRef;
use crate::context::Environment;
use crate::error::Error;
use crate::metadata::{DependencyKey, DependencyRecord, Version};

// ---------------------------------------------------------------------------
// DependencyRelation — plan-level typed edge label
// ---------------------------------------------------------------------------

/// Describes *how* a plan step depends on another asset or command.
/// Stored in `Plan.dependencies` alongside `DependencyKey`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum DependencyRelation {
    /// Input state entering the action depends on the asset.
    StateArgument,
    /// Named parameter links to another asset via query.
    ParameterLink(String),
    /// Named parameter uses a default that links to another asset.
    DefaultLink(String),
    /// Named parameter links via recipe link.
    RecipeLink(String),
    /// Named parameter links via override link.
    OverrideLink(String),
    /// Named parameter links via enum value mapping.
    EnumLink(String),
    /// Dependency created dynamically via `Context::evaluate(query)`.
    ContextEvaluate(String),
    /// Dependency on the command's metadata registration.
    CommandMetadata,
    /// Dependency on the command's implementation.
    CommandImplementation,
    /// Dependency on the recipe itself (separate from the asset's data).
    Recipe,
}

// ---------------------------------------------------------------------------
// PlanDependency — single entry in Plan.dependencies
// ---------------------------------------------------------------------------

/// A single dependency entry in a `Plan`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlanDependency {
    pub key: DependencyKey,
    pub relation: DependencyRelation,
}

impl PlanDependency {
    pub fn new(key: DependencyKey, relation: DependencyRelation) -> Self {
        Self { key, relation }
    }
}

// ---------------------------------------------------------------------------
// ExpiredDependents — result of cascade expiration
// ---------------------------------------------------------------------------

/// Result of a cascade expiration: lists all transitively expired entities.
pub struct ExpiredDependents<E: Environment> {
    /// `DependencyKey`s of keyed assets that were transitively expired.
    pub keys: Vec<DependencyKey>,
    /// `WeakAssetRef`s of untracked (query/ad-hoc) assets that were transitively expired.
    pub assets: Vec<WeakAssetRef<E>>,
}

// ---------------------------------------------------------------------------
// DependencyManager<E>
// ---------------------------------------------------------------------------

/// Runtime dependency graph.
///
/// Not part of the public API — users interact via `DefaultAssetManager` methods.
pub(crate) struct DependencyManager<E: Environment> {
    /// Current version per tracked dependency key.
    versions: scc::HashMap<DependencyKey, Version>,
    /// Keyed dependents: for key K, the DependencyKeys of keyed assets that depend on K.
    keyed_dependents: scc::HashMap<DependencyKey, scc::HashSet<DependencyKey>>,
    /// Untracked dependents: for key K, the WeakAssetRefs of query/ad-hoc assets depending on K.
    dependent_assets: scc::HashMap<DependencyKey, Vec<WeakAssetRef<E>>>,
    /// Serializes cascade expiration to prevent concurrent interleaved updates.
    expiration_lock: tokio::sync::Mutex<()>,
}

impl<E: Environment> DependencyManager<E> {
    pub fn new() -> Self {
        DependencyManager {
            versions: scc::HashMap::new(),
            keyed_dependents: scc::HashMap::new(),
            dependent_assets: scc::HashMap::new(),
            expiration_lock: tokio::sync::Mutex::new(()),
        }
    }

    /// Register (or update) the version for a dependency key.
    pub async fn register_version(&self, key: &DependencyKey, version: Version) {
        match self.versions.entry_async(key.clone()).await {
            scc::hash_map::Entry::Occupied(mut entry) => {
                *entry.get_mut() = version;
            }
            scc::hash_map::Entry::Vacant(entry) => {
                entry.insert_entry(version);
            }
        }
    }

    /// Check whether the stored version for `key` matches `expected`.
    ///
    /// **Version 0 semantics:** `Version(0)` means "unknown" and always matches.
    /// Returns `false` if the key is not registered at all.
    pub async fn version_consistent(&self, key: &DependencyKey, expected: Version) -> bool {
        if expected == Version::new(0) {
            return true;
        }
        match self.versions.get_async(key).await {
            Some(entry) => {
                let stored = *entry.get();
                drop(entry);
                stored == Version::new(0) || stored == expected
            }
            None => false,
        }
    }

    /// Get the currently registered version for `key`, if any.
    pub async fn get_version(&self, key: &DependencyKey) -> Option<Version> {
        self.versions
            .get_async(key)
            .await
            .map(|entry| {
                let v = *entry.get();
                drop(entry);
                v
            })
    }

    /// Register a dependency edge: `dependent` depends on `dependency` at `version`.
    ///
    /// **Version 0 semantics:** If `version == Version(0)`, skip the version-consistency
    /// check and just register the edge. No error is returned.
    ///
    /// Returns `Err` if the version is inconsistent or a cycle would be created.
    pub async fn add_dependency(
        &self,
        dependent: &DependencyKey,
        dependency: &DependencyKey,
        version: Version,
    ) -> Result<(), Error> {
        // Version 0 — skip consistency check
        if version != Version::new(0) {
            if !self.version_consistent(dependency, version).await {
                return Err(Error::dependency_version_mismatch(
                    dependency,
                    format!(
                        "expected version {}, but stored version differs",
                        version
                    ),
                ));
            }
        }

        // Cycle check
        if self.would_create_cycle(dependent, dependency).await {
            return Err(Error::dependency_cycle(dependent));
        }

        // Insert the edge
        let entry = self
            .keyed_dependents
            .entry_async(dependency.clone())
            .await
            .or_insert(scc::HashSet::new());
        let _ = entry.get().insert_async(dependent.clone()).await;
        drop(entry);

        Ok(())
    }

    /// Register an asset (via `AssetRef`) and all its dependencies into the DM.
    ///
    /// - Only processes assets in Ready/Source/Override state.
    /// - For keyed assets: registers the asset's own version, then loads
    ///   `DependencyRecord`s from the asset's metadata via `load_from_records`.
    /// - For non-keyed (query) assets: registers as a `dependent_asset` (weak ref)
    ///   on each of its metadata dependencies.
    pub async fn track_asset(&self, asset: &crate::assets::AssetRef<E>) {
        let status = asset.status().await;
        match status {
            crate::metadata::Status::Ready
            | crate::metadata::Status::Source
            | crate::metadata::Status::Override => {}
            crate::metadata::Status::None
            | crate::metadata::Status::Directory
            | crate::metadata::Status::Recipe
            | crate::metadata::Status::Submitted
            | crate::metadata::Status::Dependencies
            | crate::metadata::Status::Processing
            | crate::metadata::Status::Partial
            | crate::metadata::Status::Error
            | crate::metadata::Status::Storing
            | crate::metadata::Status::Expired
            | crate::metadata::Status::Cancelled
            | crate::metadata::Status::Volatile => return,
        }

        let lock = asset.data.read().await;
        let key_opt = lock.recipe.key().ok().flatten();
        let metadata = lock.metadata.clone();
        let weak_ref = asset.downgrade();
        drop(lock);

        // Extract dependencies and version from metadata
        let (deps, version) = match &metadata {
            crate::metadata::Metadata::MetadataRecord(mr) => {
                let v = mr.version.unwrap_or(Version::new(0));
                (mr.dependencies.clone(), v)
            }
            crate::metadata::Metadata::LegacyMetadata(_) => (Vec::new(), Version::new(0)),
        };

        if let Some(key) = key_opt {
            // Keyed asset: register version and load dependency records
            let dep_key = DependencyKey::from(&key);
            self.register_version(&dep_key, version).await;
            self.load_from_records(&dep_key, &deps).await;
        } else {
            // Query asset: register as dependent_asset on each dependency
            for dep_record in &deps {
                self.add_dependent_asset(&dep_record.key, weak_ref.clone())
                    .await;
            }
        }
    }

    /// Register a `WeakAssetRef` as a dependent of `dependency`.
    pub async fn add_dependent_asset(
        &self,
        dependency: &DependencyKey,
        dependent: WeakAssetRef<E>,
    ) {
        let mut entry = self
            .dependent_assets
            .entry_async(dependency.clone())
            .await
            .or_insert(Vec::new());
        entry.get_mut().push(dependent);
        drop(entry);
    }

    /// Check whether adding `dependent → dependency` would create a cycle.
    ///
    /// We need to check if `dependency` transitively depends on `dependent`.
    /// The `keyed_dependents` map stores: for each key K, the set of keys that depend on K.
    /// So `keyed_dependents[K]` = {X : X depends on K}.
    ///
    /// Starting from `dependent`, we follow the `keyed_dependents` graph upward:
    /// if `dependent` has dependents, and one of them transitively reaches `dependency`,
    /// that would mean `dependency` depends (transitively) on `dependent`, creating a cycle.
    pub async fn would_create_cycle(
        &self,
        dependent: &DependencyKey,
        dependency: &DependencyKey,
    ) -> bool {
        if dependent == dependency {
            return true;
        }
        // BFS: starting from `dependent`, follow keyed_dependents edges.
        // If we reach `dependency`, it means `dependency` transitively depends on `dependent`.
        let mut queue = VecDeque::new();
        let mut visited = std::collections::HashSet::new();
        queue.push_back(dependent.clone());
        visited.insert(dependent.clone());

        while let Some(current) = queue.pop_front() {
            if let Some(entry) = self.keyed_dependents.get_async(&current).await {
                let set = entry.get();
                let mut dependents_vec = Vec::new();
                set.iter_async(|dk| {
                    dependents_vec.push(dk.clone());
                    true
                })
                .await;
                drop(entry);

                for dk in dependents_vec {
                    if dk == *dependency {
                        return true;
                    }
                    if visited.insert(dk.clone()) {
                        queue.push_back(dk);
                    }
                }
            }
        }
        false
    }

    /// Cascade-expire a key and all its transitive dependents.
    ///
    /// **Version 0 semantics:** Before cascading from a key, if its stored version
    /// is `Version(0)` (unknown), skip that key's cascade — its dependents are not
    /// invalidated since we don't know the real version.
    ///
    /// Acquires `expiration_lock` to serialize concurrent cascades.
    pub async fn expire(&self, key: &DependencyKey) -> ExpiredDependents<E> {
        let _lock = self.expiration_lock.lock().await;

        let mut expired_keys = Vec::new();
        let mut expired_assets: Vec<WeakAssetRef<E>> = Vec::new();
        let mut queue = VecDeque::new();
        let mut visited = std::collections::HashSet::new();

        queue.push_back(key.clone());
        visited.insert(key.clone());

        while let Some(current) = queue.pop_front() {
            // Check version BEFORE removing — Version(0) means "unknown".
            // The key itself is always expired, but if its version was 0,
            // we don't cascade to its dependents (except for the root key).
            let mut skip_cascade = false;
            if current != *key {
                if let Some(entry) = self.versions.get_async(&current).await {
                    let ver = *entry.get();
                    drop(entry);
                    if ver == Version::new(0) {
                        skip_cascade = true;
                    }
                }
            }

            // Remove version and record as expired
            self.versions.remove_async(&current).await;
            expired_keys.push(current.clone());

            if !skip_cascade {
                // Collect keyed dependents (BFS frontier)
                if let Some(entry) = self.keyed_dependents.get_async(&current).await {
                    let set = entry.get();
                    let mut dependents_vec = Vec::new();
                    set.iter_async(|dk| {
                        dependents_vec.push(dk.clone());
                        true
                    })
                    .await;
                    drop(entry);

                    for dk in dependents_vec {
                        if visited.insert(dk.clone()) {
                            queue.push_back(dk);
                        }
                    }
                }
            }

            // Remove the keyed_dependents entry
            self.keyed_dependents.remove_async(&current).await;

            // Collect dependent_assets (prune dead WeakAssetRefs)
            if let Some(entry) = self.dependent_assets.get_async(&current).await {
                let assets = entry.get().clone();
                drop(entry);
                for weak in assets {
                    if weak.upgrade().is_some() {
                        expired_assets.push(weak);
                    }
                }
            }
            self.dependent_assets.remove_async(&current).await;
        }

        ExpiredDependents {
            keys: expired_keys,
            assets: expired_assets,
        }
    }

    /// Remove a key from all tracking structures.
    pub async fn remove(&self, key: &DependencyKey) {
        self.versions.remove_async(key).await;
        self.keyed_dependents.remove_async(key).await;
        self.dependent_assets.remove_async(key).await;
    }

    /// Reconstruct dependency edges from persisted `DependencyRecord`s.
    ///
    /// For each record, calls `add_dependency`. Ignores `DependencyVersionMismatch`
    /// errors (the loaded dependency version may have advanced since the record was written).
    pub async fn load_from_records(
        &self,
        dependent: &DependencyKey,
        records: &[DependencyRecord],
    ) {
        for record in records {
            match self
                .add_dependency(dependent, &record.key, record.version)
                .await
            {
                Ok(()) => {}
                Err(e)
                    if e.error_type
                        == crate::error::ErrorType::DependencyVersionMismatch =>
                {
                    // Expected on reload — version may have advanced. Skip.
                }
                Err(_) => {
                    // Cycle or other error — skip gracefully.
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_metadata::CommandKey;
    use crate::metadata::{DependencyKey, DependencyRecord, Version};
    use crate::value::Value;

    type TestEnv = crate::context::SimpleEnvironment<Value>;

    // --- Version tests ---

    #[test]
    fn version_ordering() {
        let v1 = Version::new(1);
        let v2 = Version::new(2);
        assert!(v1 < v2);
        assert!(v2 > v1);
        assert_eq!(v1, Version::new(1));
    }

    #[test]
    fn version_from_bytes_is_deterministic() {
        let data = b"hello world";
        let v1 = Version::from_bytes(data);
        let v2 = Version::from_bytes(data);
        assert_eq!(v1, v2);
    }

    #[test]
    fn version_from_bytes_differs_on_different_data() {
        let v1 = Version::from_bytes(b"hello");
        let v2 = Version::from_bytes(b"world");
        assert_ne!(v1, v2);
    }

    #[test]
    fn version_from_specific_time_is_consistent() {
        let t = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        let v1 = Version::from_specific_time(t);
        let v2 = Version::from_specific_time(t);
        assert_eq!(v1, v2);
    }

    #[test]
    fn version_from_specific_time_respects_order() {
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1);
        let t2 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2);
        let v1 = Version::from_specific_time(t1);
        let v2 = Version::from_specific_time(t2);
        assert!(v1 < v2);
    }

    #[test]
    fn version_new_unique_produces_distinct_values() {
        let v1 = Version::new_unique();
        let v2 = Version::new_unique();
        assert_ne!(v1, v2);
    }

    // --- Register/Get version tests ---

    #[tokio::test]
    async fn version_register_and_get() {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/test");
        let ver = Version::new(42);
        dm.register_version(&key, ver).await;
        assert_eq!(dm.get_version(&key).await, Some(ver));
    }

    #[tokio::test]
    async fn version_get_unregistered_returns_none() {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/missing");
        assert_eq!(dm.get_version(&key).await, None);
    }

    #[tokio::test]
    async fn version_register_update_overwrites() {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/test");
        dm.register_version(&key, Version::new(1)).await;
        dm.register_version(&key, Version::new(2)).await;
        assert_eq!(dm.get_version(&key).await, Some(Version::new(2)));
    }

    #[tokio::test]
    async fn version_consistent_matches() {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/test");
        dm.register_version(&key, Version::new(42)).await;
        assert!(dm.version_consistent(&key, Version::new(42)).await);
    }

    #[tokio::test]
    async fn version_consistent_mismatches() {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/test");
        dm.register_version(&key, Version::new(42)).await;
        assert!(!dm.version_consistent(&key, Version::new(99)).await);
    }

    #[tokio::test]
    async fn version_consistent_unregistered_returns_false() {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/missing");
        assert!(!dm.version_consistent(&key, Version::new(42)).await);
    }

    #[tokio::test]
    async fn version_zero_always_matches() {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/test");
        dm.register_version(&key, Version::new(42)).await;
        // Version(0) as expected always matches
        assert!(dm.version_consistent(&key, Version::new(0)).await);
        // Stored Version(0) matches any expected
        let key2 = DependencyKey::new("-R/unknown");
        dm.register_version(&key2, Version::new(0)).await;
        assert!(dm.version_consistent(&key2, Version::new(999)).await);
    }

    // --- Add dependency tests ---

    #[tokio::test]
    async fn add_dependency_succeeds() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        dm.register_version(&a, Version::new(1)).await;
        dm.register_version(&b, Version::new(2)).await;
        assert!(dm.add_dependency(&a, &b, Version::new(2)).await.is_ok());
    }

    #[tokio::test]
    async fn add_dependency_fails_stale_version() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        dm.register_version(&a, Version::new(1)).await;
        dm.register_version(&b, Version::new(2)).await;
        let result = dm.add_dependency(&a, &b, Version::new(99)).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().error_type,
            crate::error::ErrorType::DependencyVersionMismatch
        );
    }

    #[tokio::test]
    async fn add_dependency_fails_unregistered_dep() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        dm.register_version(&a, Version::new(1)).await;
        // b not registered
        let result = dm.add_dependency(&a, &b, Version::new(42)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn add_dependency_version_zero_skips_check() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        dm.register_version(&a, Version::new(1)).await;
        // b not registered — but Version(0) skips consistency check
        // (cycle check still runs but will pass since no edges exist)
        assert!(dm.add_dependency(&a, &b, Version::new(0)).await.is_ok());
    }

    // --- Expiration tests ---

    #[tokio::test]
    async fn expire_cascade_chain() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        let c = DependencyKey::new("-R/c");
        dm.register_version(&a, Version::new(1)).await;
        dm.register_version(&b, Version::new(2)).await;
        dm.register_version(&c, Version::new(3)).await;
        // c depends on b, b depends on a
        dm.add_dependency(&b, &a, Version::new(1)).await.unwrap();
        dm.add_dependency(&c, &b, Version::new(2)).await.unwrap();

        let expired = dm.expire(&a).await;
        // a, b, c should all be expired
        assert_eq!(expired.keys.len(), 3);
        assert!(expired.keys.contains(&a));
        assert!(expired.keys.contains(&b));
        assert!(expired.keys.contains(&c));
    }

    #[tokio::test]
    async fn expire_removes_from_versions() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        dm.register_version(&a, Version::new(1)).await;
        dm.expire(&a).await;
        assert_eq!(dm.get_version(&a).await, None);
    }

    #[tokio::test]
    async fn expire_single_key_no_dependents() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        dm.register_version(&a, Version::new(1)).await;
        let expired = dm.expire(&a).await;
        assert_eq!(expired.keys.len(), 1);
        assert!(expired.keys.contains(&a));
        assert!(expired.assets.is_empty());
    }

    #[tokio::test]
    async fn expire_nonexistent_key_is_noop() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/ghost");
        let expired = dm.expire(&a).await;
        assert_eq!(expired.keys.len(), 1); // still returns the root key
        assert!(expired.assets.is_empty());
    }

    #[tokio::test]
    async fn expire_multiple_dependents_of_one_key() {
        let dm = DependencyManager::<TestEnv>::new();
        let base = DependencyKey::new("-R/base");
        let d1 = DependencyKey::new("-R/d1");
        let d2 = DependencyKey::new("-R/d2");
        let d3 = DependencyKey::new("-R/d3");
        dm.register_version(&base, Version::new(1)).await;
        dm.register_version(&d1, Version::new(2)).await;
        dm.register_version(&d2, Version::new(3)).await;
        dm.register_version(&d3, Version::new(4)).await;
        dm.add_dependency(&d1, &base, Version::new(1))
            .await
            .unwrap();
        dm.add_dependency(&d2, &base, Version::new(1))
            .await
            .unwrap();
        dm.add_dependency(&d3, &base, Version::new(1))
            .await
            .unwrap();

        let expired = dm.expire(&base).await;
        assert_eq!(expired.keys.len(), 4);
    }

    #[tokio::test]
    async fn expire_skips_version_zero_cascade() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        let c = DependencyKey::new("-R/c");
        dm.register_version(&a, Version::new(1)).await;
        dm.register_version(&b, Version::new(0)).await; // unknown version
        dm.register_version(&c, Version::new(3)).await;
        dm.add_dependency(&b, &a, Version::new(0)).await.unwrap();
        dm.add_dependency(&c, &b, Version::new(0)).await.unwrap();

        let expired = dm.expire(&a).await;
        // a is expired; b has Version(0) so its cascade is skipped; c not reached
        assert!(expired.keys.contains(&a));
        assert!(expired.keys.contains(&b)); // b is in the list (it was a direct dependent)
        // c should NOT be expired because b had Version(0) — cascade stopped
        assert!(!expired.keys.contains(&c));
    }

    // --- Cycle detection tests ---

    #[tokio::test]
    async fn would_create_cycle_true_for_back_edge() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        dm.register_version(&a, Version::new(1)).await;
        dm.register_version(&b, Version::new(2)).await;
        dm.add_dependency(&b, &a, Version::new(1)).await.unwrap();
        // Adding a → b would create cycle: a depends on b depends on a
        assert!(dm.would_create_cycle(&a, &b).await);
    }

    #[tokio::test]
    async fn would_create_cycle_false_for_valid_shortcut() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        let c = DependencyKey::new("-R/c");
        dm.register_version(&a, Version::new(1)).await;
        dm.register_version(&b, Version::new(2)).await;
        dm.register_version(&c, Version::new(3)).await;
        // b depends on a, c depends on b
        dm.add_dependency(&b, &a, Version::new(1)).await.unwrap();
        dm.add_dependency(&c, &b, Version::new(2)).await.unwrap();
        // Adding c → a is a shortcut (not a cycle)
        assert!(!dm.would_create_cycle(&c, &a).await);
    }

    // --- Remove tests ---

    #[tokio::test]
    async fn remove_clears_version() {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/test");
        dm.register_version(&key, Version::new(42)).await;
        dm.remove(&key).await;
        assert_eq!(dm.get_version(&key).await, None);
    }

    #[tokio::test]
    async fn remove_nonexistent_is_noop() {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/ghost");
        dm.remove(&key).await; // should not panic
    }

    // --- Load from records tests ---

    #[tokio::test]
    async fn load_from_records_registers_known() {
        let dm = DependencyManager::<TestEnv>::new();
        let parent = DependencyKey::new("-R/parent");
        let child = DependencyKey::new("-R/child");
        dm.register_version(&parent, Version::new(1)).await;
        dm.register_version(&child, Version::new(2)).await;

        let records = vec![DependencyRecord::new(child.clone(), Version::new(2))];
        dm.load_from_records(&parent, &records).await;

        // parent should now be a dependent of child
        // Expire child → parent should also expire
        let expired = dm.expire(&child).await;
        assert!(expired.keys.contains(&parent));
    }

    #[tokio::test]
    async fn load_from_records_skips_unknown() {
        let dm = DependencyManager::<TestEnv>::new();
        let parent = DependencyKey::new("-R/parent");
        dm.register_version(&parent, Version::new(1)).await;

        let records = vec![DependencyRecord::new(
            DependencyKey::new("-R/nonexistent"),
            Version::new(999),
        )];
        dm.load_from_records(&parent, &records).await;
        // Should not panic or error — gracefully skipped
    }

    #[tokio::test]
    async fn load_from_empty_records_is_noop() {
        let dm = DependencyManager::<TestEnv>::new();
        let parent = DependencyKey::new("-R/parent");
        dm.register_version(&parent, Version::new(1)).await;
        dm.load_from_records(&parent, &[]).await;
        // Nothing should happen
    }

    // --- DependencyKey constructor tests ---

    #[test]
    fn dependency_key_for_command_metadata_format() {
        let ck = CommandKey::new("", "root", "hello");
        let dk = DependencyKey::for_command_metadata(&ck);
        assert!(dk.as_str().starts_with("ns-dep/command_metadata-"));
    }

    #[test]
    fn dependency_key_for_command_implementation_format() {
        let ck = CommandKey::new("", "root", "hello");
        let dk = DependencyKey::for_command_implementation(&ck);
        assert!(dk.as_str().starts_with("ns-dep/command_impl-"));
    }
}
