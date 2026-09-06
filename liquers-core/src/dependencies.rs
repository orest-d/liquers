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

impl<E: Environment> ExpiredDependents<E> {
    /// Create an empty ExpiredDependents structure
    pub fn new() -> Self {
        ExpiredDependents {
            keys: Vec::new(),
            assets: Vec::new(),
        }
    }
    /// Returns true if there are no expired dependents
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.assets.is_empty()
    }
}
// ---------------------------------------------------------------------------
// ScheduleNode — schedule-time dependency-graph participation
// ---------------------------------------------------------------------------

/// How an asset participates in dependency-graph bookkeeping at schedule time.
///
/// Only keyed assets are real graph nodes. A non-keyed asset (an expression /
/// ad-hoc query) is NOT a node; it stands for the set of keyed assets that depend
/// on it (its *attribution set*), and its own dependency edges are attributed onto
/// those keyed ancestors. See [`DependencyManager::register_scheduled_dependency`].
#[derive(Debug, Clone)]
pub(crate) enum ScheduleNode {
    /// Keyed asset: a real graph node, identified by its key.
    Keyed(DependencyKey),
    /// Non-keyed asset (expression), identified by its query key; NOT a graph node.
    Expression(DependencyKey),
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
    /// Keyed dependents: for key K, the keyed assets that depend on K **and the version each of
    /// them observed for K** when the edge was recorded.
    ///
    /// The expected version is what lets [`Self::register_version`] be precise — expiring only the
    /// dependents a change actually affects — and what lets [`Self::missing_versions`] tell an
    /// edge that wants a concrete version from one that never knew any. It is caller-trusted:
    /// nothing here validates that a stored expectation was ever true, because
    /// [`Self::add_dependency`] deliberately records without comparing.
    ///
    /// **Last writer wins** on an edge's version: a later observation is the fresher one.
    keyed_dependents: scc::HashMap<DependencyKey, scc::HashMap<DependencyKey, Version>>,
    /// Untracked dependents: for key K, the WeakAssetRefs of query/ad-hoc assets depending on K.
    dependent_assets: scc::HashMap<DependencyKey, Vec<WeakAssetRef<E>>>,
    /// Serializes cascade expiration to prevent concurrent interleaved updates.
    expiration_lock: tokio::sync::Mutex<()>,
    /// Transient (schedule-time) attribution set: for expression Q, the keyed assets
    /// that (directly or through expression chains) depend on Q.
    expression_dependents: scc::HashMap<DependencyKey, scc::HashSet<DependencyKey>>,
    /// Transient: keyed dependencies discovered so far for expression Q.
    expression_keyed_deps: scc::HashMap<DependencyKey, scc::HashSet<DependencyKey>>,
    /// Transient: expression dependencies of expression Q (to propagate late-joining
    /// keyed dependents down expression chains).
    expression_expr_deps: scc::HashMap<DependencyKey, scc::HashSet<DependencyKey>>,
}

impl<E: Environment> DependencyManager<E> {
    pub fn new() -> Self {
        DependencyManager {
            versions: scc::HashMap::new(),
            keyed_dependents: scc::HashMap::new(),
            dependent_assets: scc::HashMap::new(),
            expiration_lock: tokio::sync::Mutex::new(()),
            expression_dependents: scc::HashMap::new(),
            expression_keyed_deps: scc::HashMap::new(),
            expression_expr_deps: scc::HashMap::new(),
        }
    }

    /// Register (or update) the version for a dependency key.
    ///
    /// If the version changes, all transitive dependents are expired and returned.
    pub async fn register_version(
        &self,
        key: &DependencyKey,
        version: Version,
    ) -> ExpiredDependents<E> {
        let mut version_changed = false;
        match self.versions.entry_async(key.clone()).await {
            scc::hash_map::Entry::Occupied(mut entry) => {
                version_changed = *entry.get() != version;
                *entry.get_mut() = version;
            }
            scc::hash_map::Entry::Vacant(entry) => {
                entry.insert_entry(version);
            }
        }

        if version_changed {
            self.expire_stale_dependents(key, version).await
        } else {
            ExpiredDependents::new()
        }
    }

    /// Expire the dependents of `key` that a change to `version` actually affects.
    ///
    /// A dependent is **spared only on positive evidence** — its edge records a concrete version
    /// equal to the new one, so it already observed exactly this content. Everything else is
    /// expired, including an edge recording `Version::unknown()`: no evidence either way is not
    /// evidence of safety, and [`Self::propagate_attribution`] records *every* attribution edge
    /// that way, so sparing them would drop every keyed dependent reached through a non-keyed
    /// expression out of the cascade.
    ///
    /// The property to hold on to, which is stronger than any enumeration of cases: **this expires
    /// a subset of what an unconditional cascade expires, and removes a dependent from that set
    /// only on positive evidence.**
    ///
    /// Weak-reference dependents — query assets — record no expectation at all, so they are always
    /// expired, exactly as before.
    async fn expire_stale_dependents(
        &self,
        key: &DependencyKey,
        version: Version,
    ) -> ExpiredDependents<E> {
        let mut frontier = Vec::new();
        for (dependent, expected) in self.snapshot_dependent_edges(key).await {
            if expected.is_unknown() || expected != version {
                self.remove_edge(key, &dependent).await;
                frontier.push(dependent);
            }
            // else: KEEP the edge. Dropping a spared dependent's edge would silently unhook it
            // from every future invalidation — it would pass a first-order test and fail only on
            // the *second* version change.
        }
        let seed_assets = self.take_dependent_assets(key).await;
        self.expire_from_frontier(frontier, seed_assets).await
    }

    /// Report that `key` has no durable version, expiring the dependents that expected one.
    ///
    /// The companion to [`Self::register_version`] for the audit flow: when the asset manager
    /// cannot resolve a version for a gap that [`Self::missing_versions`] reported, this is how it
    /// says so. `Version::unknown()` cannot express it — unknown is *compatible with anything*, so
    /// registering it would confirm the dependents rather than invalidate them.
    ///
    /// The asymmetry with `register_version` is deliberate. That is a *change* event, so anything
    /// not provably unaffected is affected. This is an *audit finding* that a key has no durable
    /// version, which does not contradict an edge that never expected one — so an edge recording
    /// `Version::unknown()` is spared here, and weak-reference dependents, which record no
    /// expectation at all, are not touched.
    pub(crate) async fn report_no_version(&self, key: &DependencyKey) -> ExpiredDependents<E> {
        let mut frontier = Vec::new();
        for (dependent, expected) in self.snapshot_dependent_edges(key).await {
            if !expected.is_unknown() {
                self.remove_edge(key, &dependent).await;
                frontier.push(dependent);
            }
        }
        self.expire_from_frontier(frontier, Vec::new()).await
    }

    /// Synchronous counterpart of [`Self::register_version`], for the uncontended startup path.
    ///
    /// Returns `true` when the stored version differed from `version`, i.e. when the caller must
    /// arrange a cascade. It deliberately does **not** return [`ExpiredDependents`]: computing
    /// those requires [`Self::expire_dependents`], which is asynchronous, so a synchronous
    /// registration cannot produce them. Splitting detection from application is what lets asset
    /// manager startup be synchronous — see
    /// [`AssetManager::start`](crate::assets::AssetManager::start).
    ///
    /// At first startup the `versions` map is empty, so every key inserts `Vacant` and this always
    /// returns `false`. A later re-registration through
    /// [`AssetManager::refresh_command_versions`](crate::assets::AssetManager::refresh_command_versions)
    /// can report `true`, and
    /// [`AssetManager::refresh_command_versions_and_expire`](crate::assets::AssetManager::refresh_command_versions_and_expire)
    /// applies the cascade for it.
    pub fn register_version_sync(&self, key: &DependencyKey, version: Version) -> bool {
        match self.versions.entry_sync(key.clone()) {
            scc::hash_map::Entry::Occupied(mut entry) => {
                let version_changed = *entry.get() != version;
                *entry.get_mut() = version;
                version_changed
            }
            scc::hash_map::Entry::Vacant(entry) => {
                entry.insert_entry(version);
                false
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
                stored.matches(&expected)
            }
            None => false,
        }
    }

    /// Get the currently registered version for `key`, if any.
    pub async fn get_version(&self, key: &DependencyKey) -> Option<Version> {
        self.versions.get_async(key).await.map(|entry| {
            let v = *entry.get();
            drop(entry);
            v
        })
    }

    /// Register a dependency edge: `dependent` depends on `dependency` at `version`.
    ///
    /// **Records; it does not verify.** The two are different jobs: recording is in-memory, happens
    /// on every edge, and is policy-free, while verifying that a recorded version still holds may
    /// need a store read and is meaningful only on load or on demand. Fusing them put verification
    /// on the hot path and left nowhere to stand for a policy — see
    /// `specs/design/keyed-expiry-cascade-fix/` and `DEPENDENCY-AUDIT-POLICY-NOT-EXPRESSIBLE`.
    ///
    /// Verification now happens in [`Self::register_version`], which is an event that already
    /// means "something changed", and in the audit flow that
    /// [`AssetManager::trigger_dependency_audit`](crate::assets::AssetManager) drives. This
    /// function performs **no I/O**.
    ///
    /// The `version` is stored on the edge as the dependent's expectation. It is caller-trusted:
    /// nothing here checks that it was ever true.
    ///
    /// Returns `Err` only if a cycle would be created.
    pub async fn add_dependency(
        &self,
        dependent: &DependencyKey,
        dependency: &DependencyKey,
        version: Version,
    ) -> Result<ExpiredDependents<E>, Error> {
        // Cycle check
        if self.would_create_cycle(dependent, dependency).await {
            return Err(Error::dependency_cycle(dependent));
        }

        // Insert the edge, recording the version the dependent observed. Last writer wins:
        // `insert_async` fails on a duplicate key, so an existing edge is updated explicitly
        // rather than left pinned to its first-ever observation.
        let entry = self
            .keyed_dependents
            .entry_async(dependency.clone())
            .await
            .or_insert(scc::HashMap::new());
        let edges = entry.get();
        if edges.insert_async(dependent.clone(), version).await.is_err() {
            if let Some(mut existing) = edges.get_async(dependent).await {
                *existing.get_mut() = version;
            }
        }
        drop(entry);

        Ok(ExpiredDependents::new())
    }

    /// Register an asset (via `AssetRef`) and all its dependencies into the DM.
    ///
    /// - Only processes assets in Ready/Source/Override state.
    /// - For keyed assets: registers the asset's own version, then loads
    ///   `DependencyRecord`s from the asset's metadata via `load_from_records`.
    /// - For non-keyed (query) assets: registers as a `dependent_asset` (weak ref)
    ///   on each of its metadata dependencies.
    pub async fn track_asset(&self, asset: &crate::assets::AssetRef<E>) -> ExpiredDependents<E> {
        let mut expired = ExpiredDependents::new();
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
            | crate::metadata::Status::Volatile => return expired,
        }

        let key_opt = asset.bound_owner_key().await.ok().flatten();
        let lock = asset.data.read().await;
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
            let mut e = self.register_version(&dep_key, version).await;
            expired.keys.append(&mut e.keys);
            expired.assets.append(&mut e.assets);
            let mut e = self.load_from_records(&dep_key, &deps).await;
            expired.keys.append(&mut e.keys);
            expired.assets.append(&mut e.assets);
        } else {
            // Query asset: register as dependent_asset on each dependency
            for dep_record in &deps {
                self.add_dependent_asset(&dep_record.key, weak_ref.clone())
                    .await;
            }
        }
        expired
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
                let edges = entry.get();
                let mut dependents_vec = Vec::new();
                edges
                    .iter_async(|dk, _version| {
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

    // --- Schedule-time dependency registration (keyed-expansion model) ---

    /// Snapshot the contents of a `DependencyKey -> HashSet<DependencyKey>` map entry.
    async fn snapshot_set(
        &self,
        map: &scc::HashMap<DependencyKey, scc::HashSet<DependencyKey>>,
        key: &DependencyKey,
    ) -> Vec<DependencyKey> {
        let mut out = Vec::new();
        if let Some(entry) = map.get_async(key).await {
            entry
                .get()
                .iter_async(|dk| {
                    out.push(dk.clone());
                    true
                })
                .await;
            drop(entry);
        }
        out
    }

    /// Insert `value` into the set stored at `key` in `map` (lazily creating the entry).
    async fn insert_set(
        &self,
        map: &scc::HashMap<DependencyKey, scc::HashSet<DependencyKey>>,
        key: &DependencyKey,
        value: &DependencyKey,
    ) {
        let entry = map
            .entry_async(key.clone())
            .await
            .or_insert(scc::HashSet::new());
        let _ = entry.get().insert_async(value.clone()).await;
        drop(entry);
    }

    /// Register a scheduled dependency edge (`dependent` depends on `dependency`) under
    /// the keyed-expansion model, performing all cycle checks. Called at schedule time
    /// by `Context::schedule_dependency_asset`.
    ///
    /// Only keyed assets are graph nodes; an expression is expanded onto its attribution
    /// set (the keyed assets that depend on it). Returns `Err(dependency_cycle)` if the
    /// edge — after expansion — would create a cycle. No default match arm.
    pub(crate) async fn register_scheduled_dependency(
        &self,
        dependent: &ScheduleNode,
        dependency: &ScheduleNode,
        version: Version,
    ) -> Result<(), Error> {
        // A = attribution set of `dependent`.
        let attribution: Vec<DependencyKey> = match dependent {
            ScheduleNode::Keyed(k) => vec![k.clone()],
            ScheduleNode::Expression(q) => self.snapshot_set(&self.expression_dependents, q).await,
        };

        match dependency {
            ScheduleNode::Keyed(d) => {
                for r in &attribution {
                    if self.would_create_cycle(r, d).await {
                        return Err(Error::dependency_cycle(r));
                    }
                    let _ = self.add_dependency(r, d, version).await?;
                }
                if let ScheduleNode::Expression(q) = dependent {
                    self.insert_set(&self.expression_keyed_deps, q, d).await;
                }
            }
            ScheduleNode::Expression(dq) => {
                if let ScheduleNode::Expression(q) = dependent {
                    if q == dq {
                        // Direct self-schedule of an expression.
                        return Err(Error::dependency_cycle(q));
                    }
                }
                let origin = match dependent {
                    ScheduleNode::Expression(q) => Some(q.clone()),
                    ScheduleNode::Keyed(_) => None,
                };
                let mut visited = std::collections::HashSet::new();
                self.propagate_attribution(dq, &attribution, origin.as_ref(), &mut visited)
                    .await?;
                if let ScheduleNode::Expression(q) = dependent {
                    self.insert_set(&self.expression_expr_deps, q, dq).await;
                }
            }
        }
        Ok(())
    }

    /// Attribution propagation: join `attribution` (new keyed dependents) onto expression
    /// `expr` and every expression it transitively depends on, registering the implied
    /// keyed edges with cycle checks. `origin` is the originating dependent expression (if
    /// any); re-encountering it means a pure-expression cycle. `visited` bounds the walk.
    /// This is traversal of the attribution bookkeeping, not a second cycle detector —
    /// keyed cycle detection stays in `would_create_cycle`.
    async fn propagate_attribution(
        &self,
        expr: &DependencyKey,
        attribution: &[DependencyKey],
        origin: Option<&DependencyKey>,
        visited: &mut std::collections::HashSet<DependencyKey>,
    ) -> Result<(), Error> {
        if let Some(o) = origin {
            if expr == o {
                return Err(Error::dependency_cycle(expr));
            }
        }
        if !visited.insert(expr.clone()) {
            return Ok(());
        }
        // Every R in the attribution set now depends (through expr) on expr's keyed deps.
        for r in attribution {
            self.insert_set(&self.expression_dependents, expr, r).await;
        }
        let keyed_deps = self.snapshot_set(&self.expression_keyed_deps, expr).await;
        for r in attribution {
            for x in &keyed_deps {
                if self.would_create_cycle(r, x).await {
                    return Err(Error::dependency_cycle(r));
                }
                let _ = self.add_dependency(r, x, Version::unknown()).await?;
            }
        }
        let expr_deps = self.snapshot_set(&self.expression_expr_deps, expr).await;
        for dq in &expr_deps {
            Box::pin(self.propagate_attribution(dq, attribution, origin, visited)).await?;
        }
        Ok(())
    }

    /// Drop the transient schedule-time attribution entries for expression `expr`.
    /// Called when the expression asset reaches a terminal status.
    pub(crate) async fn remove_expression(&self, expr: &DependencyKey) {
        self.expression_dependents.remove_async(expr).await;
        self.expression_keyed_deps.remove_async(expr).await;
        self.expression_expr_deps.remove_async(expr).await;
    }

    /// Cascade-expire a key and all its transitive dependents.
    ///
    /// **Version 0 semantics:** Before cascading from a key, if its stored version
    /// is `Version(0)` (unknown), skip that key's cascade — its dependents are not
    /// invalidated since we don't know the real version.
    ///
    /// Acquires `expiration_lock` to serialize concurrent cascades.
    pub async fn expire(&self, key: &DependencyKey) -> ExpiredDependents<E> {
        self.expire_internal(key, true).await
    }

    /// Cascade-expire transitive dependents of `key`, but keep `key` itself alive.
    pub async fn expire_dependents(&self, key: &DependencyKey) -> ExpiredDependents<E> {
        self.expire_internal(key, false).await
    }

    async fn expire_internal(
        &self,
        key: &DependencyKey,
        include_root: bool,
    ) -> ExpiredDependents<E> {
        if include_root {
            self.expire_from_frontier(vec![key.clone()], Vec::new())
                .await
        } else {
            // Seed from every dependent, and take the root's own bookkeeping down with it: the
            // root stays alive but its dependent lists are now stale.
            let frontier = self.snapshot_dependent_keys(key).await;
            let seed_assets = self.take_dependent_assets(key).await;
            self.keyed_dependents.remove_async(key).await;
            self.expire_from_frontier(frontier, seed_assets).await
        }
    }

    /// The keyed dependents of `key`, without their expected versions.
    async fn snapshot_dependent_keys(&self, key: &DependencyKey) -> Vec<DependencyKey> {
        let mut out = Vec::new();
        if let Some(entry) = self.keyed_dependents.get_async(key).await {
            entry
                .get()
                .iter_async(|dk, _version| {
                    out.push(dk.clone());
                    true
                })
                .await;
            drop(entry);
        }
        out
    }

    /// The keyed dependents of `key` paired with the version each observed.
    async fn snapshot_dependent_edges(&self, key: &DependencyKey) -> Vec<(DependencyKey, Version)> {
        let mut out = Vec::new();
        if let Some(entry) = self.keyed_dependents.get_async(key).await {
            entry
                .get()
                .iter_async(|dk, version| {
                    out.push((dk.clone(), *version));
                    true
                })
                .await;
            drop(entry);
        }
        out
    }

    /// Remove `key`'s weak-reference dependents, returning the ones still alive.
    async fn take_dependent_assets(&self, key: &DependencyKey) -> Vec<WeakAssetRef<E>> {
        let mut out = Vec::new();
        if let Some(entry) = self.dependent_assets.get_async(key).await {
            let assets = entry.get().clone();
            drop(entry);
            for weak in assets {
                if weak.upgrade().is_some() {
                    out.push(weak);
                }
            }
        }
        self.dependent_assets.remove_async(key).await;
        out
    }

    /// Drop one edge, and the outer entry with it when it was the last one.
    ///
    /// The blanket paths remove `keyed_dependents[dependency]` wholesale, which is right when
    /// every dependent has just been invalidated. A *selective* expiry must not: an edge whose
    /// dependent was deliberately spared has to survive, or sparing it would silently unhook it
    /// from every future invalidation.
    async fn remove_edge(&self, dependency: &DependencyKey, dependent: &DependencyKey) {
        let mut now_empty = false;
        if let Some(entry) = self.keyed_dependents.get_async(dependency).await {
            let edges = entry.get();
            edges.remove_async(dependent).await;
            now_empty = edges.is_empty();
            drop(entry);
        }
        if now_empty {
            self.keyed_dependents.remove_async(dependency).await;
        }
    }

    /// The single cascade traversal, shared by every expiry entry point.
    ///
    /// One [`Self::expiration_lock`] hold, one `visited` set, one breadth-first walk. Callers
    /// differ only in the frontier they seed and in whether they seed weak references — which is
    /// what keeps a *selective* expiry from being N separate traversals: those would each carry
    /// their own `visited` set, so a descendant reachable from two frontier entries would be
    /// expired twice and reported twice.
    async fn expire_from_frontier(
        &self,
        frontier: Vec<DependencyKey>,
        seed_assets: Vec<WeakAssetRef<E>>,
    ) -> ExpiredDependents<E> {
        let _lock = self.expiration_lock.lock().await;

        let mut expired_keys = Vec::new();
        let mut expired_assets: Vec<WeakAssetRef<E>> = seed_assets;
        let mut queue = VecDeque::new();
        let mut visited = std::collections::HashSet::new();

        for key in frontier {
            if visited.insert(key.clone()) {
                queue.push_back(key);
            }
        }

        while let Some(current) = queue.pop_front() {
            // Check the version BEFORE removing it. A key registered at `Version(0)` — unknown —
            // does not propagate: without a version, staleness cannot be concluded for its
            // dependents. The key itself is expired either way. An *absent* entry is not the same
            // as a zero one and does not stop the walk.
            //
            // The audit is what turns an unknown into a known one, so the two mechanisms cover
            // each other rather than compete: this branch declines to guess, and
            // `missing_versions` reports the key so something can resolve it.
            let mut skip_cascade = false;
            if let Some(entry) = self.versions.get_async(&current).await {
                let ver = *entry.get();
                drop(entry);
                if ver.is_unknown() {
                    skip_cascade = true;
                }
            }

            // Remove version and record as expired
            self.versions.remove_async(&current).await;
            expired_keys.push(current.clone());

            if !skip_cascade {
                for dk in self.snapshot_dependent_keys(&current).await {
                    if visited.insert(dk.clone()) {
                        queue.push_back(dk);
                    }
                }
            }

            self.keyed_dependents.remove_async(&current).await;
            expired_assets.extend(self.take_dependent_assets(&current).await);
        }

        ExpiredDependents {
            keys: expired_keys,
            assets: expired_assets,
        }
    }

    /// Dependency keys whose version this manager does not know, and which at least one edge
    /// expects at a concrete version.
    ///
    /// "Does not know" is one predicate applied in two places, both `Version::is_unknown()`:
    ///
    /// - an edge expecting `Version(0)` is **unverifiable**, so it is skipped — it makes no demand
    ///   on its target, and this is what keeps an audit over a store written before versions
    ///   existed from reporting everything;
    /// - a node registered at `Version(0)` is **unverified**, so it is reported, exactly like a
    ///   node with no entry at all. A zero gets registered for reasons that are nobody's decision
    ///   — `track_asset` registers one for a `Metadata::LegacyMetadata` record — and without this
    ///   such a key would be permanently invisible to the one mechanism that can resolve it.
    ///
    /// Synchronous and allocation-light by design: this is a scan of two in-memory maps and
    /// performs no I/O. Resolving the gaps is the asset manager's job, through
    /// `AssetManager::version`, and pushing the answers back is
    /// [`Self::register_version`] or [`Self::report_no_version`].
    ///
    /// The result is a snapshot; the graph may change under it. That is fine for a
    /// policy-triggered operation and deliberately takes no lock — locking here would put
    /// contention on the cascade path to protect an operation that tolerates being slightly out
    /// of date.
    pub(crate) fn missing_versions(&self) -> Vec<DependencyKey> {
        let mut out = Vec::new();
        self.keyed_dependents.iter_sync(|dependency, edges| {
            if self.is_missing_version(dependency, edges) {
                out.push(dependency.clone());
            }
            true
        });
        out
    }

    /// The gaps that checking `key` alone requires: the subset of [`Self::missing_versions`]
    /// reachable from `key`'s own recorded dependencies.
    ///
    /// `keyed_dependents` is indexed by *dependency*, so this scans for entries in which `key`
    /// appears as a dependent rather than walking outward from `key`.
    pub(crate) fn missing_versions_for(&self, key: &DependencyKey) -> Vec<DependencyKey> {
        let mut out = Vec::new();
        self.keyed_dependents.iter_sync(|dependency, edges| {
            let expected = edges.read_sync(key, |_, version| *version);
            if let Some(expected) = expected {
                if !expected.is_unknown() && self.version_is_unknown(dependency) {
                    out.push(dependency.clone());
                }
            }
            true
        });
        out
    }

    /// Whether any edge makes a concrete demand on `dependency` that this manager cannot answer.
    fn is_missing_version(
        &self,
        dependency: &DependencyKey,
        edges: &scc::HashMap<DependencyKey, Version>,
    ) -> bool {
        if !self.version_is_unknown(dependency) {
            return false;
        }
        let mut demanded = false;
        edges.iter_sync(|_dependent, expected| {
            if !expected.is_unknown() {
                demanded = true;
                return false; // one concrete expectation is enough
            }
            true
        });
        demanded
    }

    /// `true` when this manager holds no version for `key`, or holds `Version(0)` — which means
    /// the same thing.
    fn version_is_unknown(&self, key: &DependencyKey) -> bool {
        self.versions
            .read_sync(key, |_, version| version.is_unknown())
            .unwrap_or(true)
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
    ) -> ExpiredDependents<E> {
        let mut expired = ExpiredDependents::new();
        for record in records {
            match self
                .add_dependency(dependent, &record.key, record.version)
                .await
            {
                Ok(mut e) => {
                    expired.keys.append(&mut e.keys);
                    expired.assets.append(&mut e.assets);
                }
                Err(_) => {
                    // Cycle or other error — skip gracefully.
                }
            }
        }
        expired
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
    use crate::parse::parse_key;
    use crate::query::Key;
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

    /// `add_dependency` records; it does not verify. A recorded version that disagrees with the
    /// dependency's current one is not an error and expires nothing — that comparison belongs to
    /// `register_version`, which is an event that already means "something changed".
    ///
    /// Replaces `add_dependency_fails_stale_version`, which asserted the inline check.
    #[tokio::test]
    async fn add_dependency_records_a_disagreeing_version_without_expiring() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        dm.register_version(&a, Version::new(1)).await;
        dm.register_version(&b, Version::new(2)).await;

        let expired = dm.add_dependency(&a, &b, Version::new(99)).await.unwrap();

        assert!(expired.keys.is_empty(), "recording must not expire");
        assert_eq!(dm.snapshot_dependent_edges(&b).await, vec![(a, Version::new(99))]);
    }

    /// An unregistered dependency is *not loaded yet*, which is not the same as *changed*.
    ///
    /// Replaces `add_dependency_fails_unregistered_dep`, whose name recorded a behaviour nobody
    /// chose: absence read as staleness. The dependency's absence is now reported by
    /// `missing_versions` and resolved by the audit instead.
    #[tokio::test]
    async fn add_dependency_records_an_unregistered_dependency_without_expiring() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        dm.register_version(&a, Version::new(1)).await;
        // b deliberately not registered

        let expired = dm.add_dependency(&a, &b, Version::new(42)).await.unwrap();

        assert!(expired.keys.is_empty());
        assert_eq!(dm.snapshot_dependent_edges(&b).await, vec![(a, Version::new(42))]);
    }

    /// The enabling fact for per-edge precision: the version `add_dependency` is given is
    /// retained on the edge rather than discarded.
    #[tokio::test]
    async fn add_dependency_stores_the_expected_version_on_the_edge() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        dm.register_version(&b, Version::new(7)).await;
        dm.add_dependency(&a, &b, Version::new(7)).await.unwrap();

        let edges = dm.snapshot_dependent_edges(&b).await;
        assert_eq!(edges, vec![(a, Version::new(7))]);
    }

    /// Last writer wins. `scc`'s `insert_async` fails on a duplicate key rather than overwriting,
    /// so getting this wrong pins an edge to its first-ever observation forever — and a dependent
    /// that has since observed a newer version would then be spared expiry it deserves.
    #[tokio::test]
    async fn add_dependency_overwrites_an_earlier_edge_version() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        dm.add_dependency(&a, &b, Version::new(1)).await.unwrap();
        dm.add_dependency(&a, &b, Version::new(2)).await.unwrap();

        let edges = dm.snapshot_dependent_edges(&b).await;
        assert_eq!(
            edges,
            vec![(a, Version::new(2))],
            "a later observation must replace an earlier one, not be dropped"
        );
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

    // --- Gap reporting ---

    #[tokio::test]
    async fn missing_versions_reports_a_key_with_no_entry() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        dm.add_dependency(&a, &b, Version::new(42)).await.unwrap();

        assert_eq!(dm.missing_versions(), vec![b]);
    }

    /// A zero registered for reasons nobody chose — `track_asset` does it for a `LegacyMetadata`
    /// record — must not be permanently invisible to the audit.
    #[tokio::test]
    async fn missing_versions_reports_a_registered_zero() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        dm.register_version(&b, Version::unknown()).await;
        dm.add_dependency(&a, &b, Version::new(42)).await.unwrap();

        assert_eq!(
            dm.missing_versions(),
            vec![b],
            "a registered zero means unknown, exactly like no entry"
        );
    }

    /// The other half of the symmetry, and what keeps an audit over a store written before
    /// versions existed from reporting every key in it.
    #[tokio::test]
    async fn missing_versions_skips_an_edge_expecting_zero() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        dm.add_dependency(&a, &b, Version::unknown()).await.unwrap();

        assert!(
            dm.missing_versions().is_empty(),
            "an edge that expects nothing demands nothing"
        );
    }

    #[tokio::test]
    async fn missing_versions_omits_a_known_target() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        dm.register_version(&b, Version::new(9)).await;
        dm.add_dependency(&a, &b, Version::new(42)).await.unwrap();

        assert!(
            dm.missing_versions().is_empty(),
            "gap reporting answers 'do I know?', never 'does it match?'"
        );
    }

    #[tokio::test]
    async fn missing_versions_for_restricts_to_one_keys_dependencies() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        let other = DependencyKey::new("-R/other");
        let other_dep = DependencyKey::new("-R/other_dep");
        dm.add_dependency(&a, &b, Version::new(1)).await.unwrap();
        dm.add_dependency(&other, &other_dep, Version::new(1))
            .await
            .unwrap();

        assert_eq!(dm.missing_versions_for(&a), vec![b]);
        assert_eq!(dm.missing_versions_for(&other), vec![other_dep]);
    }

    #[test]
    fn missing_versions_is_empty_on_a_fresh_manager() {
        let dm = DependencyManager::<TestEnv>::new();
        assert!(dm.missing_versions().is_empty());
    }

    /// The self-healing path end to end: a zero is reported, the resolved version is pushed back,
    /// and the dependent whose expectation differs is expired.
    #[tokio::test]
    async fn filling_a_registered_zero_expires_the_stale_dependent() {
        let dm = DependencyManager::<TestEnv>::new();
        let d = DependencyKey::new("-R/d");
        let k = DependencyKey::new("-R/k");
        dm.register_version(&k, Version::unknown()).await;
        dm.add_dependency(&d, &k, Version::new(1)).await.unwrap();
        assert_eq!(dm.missing_versions(), vec![k.clone()]);

        let expired = dm.register_version(&k, Version::new(2)).await;

        assert!(expired.keys.contains(&d));
    }

    #[tokio::test]
    async fn filling_a_registered_zero_keeps_a_matching_dependent() {
        let dm = DependencyManager::<TestEnv>::new();
        let d = DependencyKey::new("-R/d");
        let k = DependencyKey::new("-R/k");
        dm.register_version(&k, Version::unknown()).await;
        dm.add_dependency(&d, &k, Version::new(2)).await.unwrap();

        let expired = dm.register_version(&k, Version::new(2)).await;

        assert!(!expired.keys.contains(&d), "resolved to what it expected");
    }

    // --- Selective expiry on a version change ---

    #[tokio::test]
    async fn filling_a_gap_expires_only_the_dependents_whose_expectation_differs() {
        let dm = DependencyManager::<TestEnv>::new();
        let k = DependencyKey::new("-R/k");
        let stale = DependencyKey::new("-R/stale");
        let fresh = DependencyKey::new("-R/fresh");
        dm.register_version(&k, Version::new(1)).await;
        dm.add_dependency(&stale, &k, Version::new(1)).await.unwrap();
        dm.add_dependency(&fresh, &k, Version::new(2)).await.unwrap();

        let expired = dm.register_version(&k, Version::new(2)).await;

        assert!(expired.keys.contains(&stale), "expectation differs -> expired");
        assert!(
            !expired.keys.contains(&fresh),
            "expectation equals the new version -> spared"
        );
    }

    /// `propagate_attribution` records every attribution edge with `Version::unknown()`, which is
    /// how a keyed asset depending on another *through a non-keyed expression* enters the graph.
    /// Sparing unknown-expecting edges would drop every join and sub-query out of the cascade.
    #[tokio::test]
    async fn filling_a_gap_expires_a_dependent_whose_edge_expects_unknown() {
        let dm = DependencyManager::<TestEnv>::new();
        let k = DependencyKey::new("-R/k");
        let d = DependencyKey::new("-R/d");
        dm.register_version(&k, Version::new(1)).await;
        dm.add_dependency(&d, &k, Version::unknown()).await.unwrap();

        let expired = dm.register_version(&k, Version::new(2)).await;

        assert!(
            expired.keys.contains(&d),
            "no evidence of safety is not evidence of safety"
        );
    }

    /// Sparing a dependent must not unhook it. This fails on the second version change, not the
    /// first, which is exactly why it is worth writing.
    #[tokio::test]
    async fn sparing_a_dependent_keeps_its_edge() {
        let dm = DependencyManager::<TestEnv>::new();
        let k = DependencyKey::new("-R/k");
        let d = DependencyKey::new("-R/d");
        dm.register_version(&k, Version::new(1)).await;
        dm.add_dependency(&d, &k, Version::new(2)).await.unwrap();

        let first = dm.register_version(&k, Version::new(2)).await;
        assert!(!first.keys.contains(&d), "precondition: spared");

        let second = dm.register_version(&k, Version::new(3)).await;
        assert!(
            second.keys.contains(&d),
            "a spared dependent must still be reachable by the next change"
        );
    }

    /// The distinguishing test for one-traversal-not-N-calls: N calls to `expire` would each carry
    /// their own `visited` set, so a descendant reachable from two frontier entries is expired and
    /// reported twice.
    #[tokio::test]
    async fn selective_expiry_visits_a_shared_descendant_once() {
        let dm = DependencyManager::<TestEnv>::new();
        let k = DependencyKey::new("-R/k");
        let d1 = DependencyKey::new("-R/d1");
        let d2 = DependencyKey::new("-R/d2");
        let x = DependencyKey::new("-R/x");
        for (key, v) in [(&k, 1u128), (&d1, 10), (&d2, 20), (&x, 30)] {
            dm.register_version(key, Version::new(v)).await;
        }
        dm.add_dependency(&d1, &k, Version::new(1)).await.unwrap();
        dm.add_dependency(&d2, &k, Version::new(1)).await.unwrap();
        dm.add_dependency(&x, &d1, Version::new(10)).await.unwrap();
        dm.add_dependency(&x, &d2, Version::new(20)).await.unwrap();

        let expired = dm.register_version(&k, Version::new(2)).await;

        let x_count = expired.keys.iter().filter(|key| **key == x).count();
        assert_eq!(x_count, 1, "shared descendant expired exactly once");
    }

    #[tokio::test]
    async fn selective_expiry_evicts_an_emptied_edge_map() {
        let dm = DependencyManager::<TestEnv>::new();
        let k = DependencyKey::new("-R/k");
        let d = DependencyKey::new("-R/d");
        dm.register_version(&k, Version::new(1)).await;
        dm.add_dependency(&d, &k, Version::new(1)).await.unwrap();

        dm.register_version(&k, Version::new(2)).await;

        assert!(
            dm.snapshot_dependent_edges(&k).await.is_empty(),
            "the only edge was expired, so the outer entry must be gone too"
        );
    }

    // --- report_no_version ---

    #[tokio::test]
    async fn report_no_version_expires_dependents_expecting_a_concrete_version() {
        let dm = DependencyManager::<TestEnv>::new();
        let k = DependencyKey::new("-R/k");
        let d = DependencyKey::new("-R/d");
        dm.add_dependency(&d, &k, Version::new(5)).await.unwrap();

        let expired = dm.report_no_version(&k).await;

        assert!(expired.keys.contains(&d));
    }

    /// An audit finding that a key has no durable version does not contradict an edge that never
    /// expected one — and the spared edge must survive, or the next real change misses it.
    #[tokio::test]
    async fn report_no_version_spares_an_unknown_expecting_edge_and_keeps_it() {
        let dm = DependencyManager::<TestEnv>::new();
        let k = DependencyKey::new("-R/k");
        let d = DependencyKey::new("-R/d");
        dm.add_dependency(&d, &k, Version::unknown()).await.unwrap();

        let expired = dm.report_no_version(&k).await;

        assert!(!expired.keys.contains(&d), "no expectation, no contradiction");
        assert_eq!(
            dm.snapshot_dependent_edges(&k).await,
            vec![(d, Version::unknown())],
            "the spared edge must survive"
        );
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

    #[test]
    fn dependency_key_classifies_and_extracts_pure_key() {
        let key = parse_key("a/b.txt").unwrap();
        let dk = DependencyKey::from(&key);

        assert!(dk.is_pure_key());
        assert!(!dk.is_recipe_key());
        assert!(!dk.is_dir_key());
        assert_eq!(dk.key().unwrap(), Some(key));
        assert_eq!(Key::try_from(&dk).unwrap(), parse_key("a/b.txt").unwrap());
        assert_eq!(dk.recipe_key().unwrap(), None);
        assert_eq!(dk.dir_key().unwrap(), None);
        assert_eq!(dk.command_key().unwrap(), None);
    }

    #[test]
    fn dependency_key_classifies_empty_pure_key() {
        let dk = DependencyKey::new("-R");

        assert!(dk.is_pure_key());
        assert_eq!(dk.key().unwrap(), Some(crate::query::Key::new()));
        assert_eq!(Key::try_from(&dk).unwrap(), crate::query::Key::new());
    }

    #[test]
    fn dependency_key_classifies_and_extracts_recipe_key() {
        let key = parse_key("recipes/demo.txt").unwrap();
        let dk = DependencyKey::from_recipe_key(&key);

        assert!(dk.is_recipe_key());
        assert!(!dk.is_pure_key());
        assert!(!dk.is_dir_key());
        assert_eq!(dk.recipe_key().unwrap(), Some(key));
        assert_eq!(dk.key().unwrap(), None);
        assert_eq!(dk.dir_key().unwrap(), None);
        assert_eq!(dk.command_key().unwrap(), None);
    }

    #[test]
    fn dependency_key_classifies_and_extracts_dir_key() {
        let key = parse_key("reports").unwrap();
        let dk = DependencyKey::from_dir_key(&key);

        assert!(dk.is_dir_key());
        assert!(!dk.is_pure_key());
        assert!(!dk.is_recipe_key());
        assert_eq!(dk.dir_key().unwrap(), Some(key));
        assert_eq!(dk.key().unwrap(), None);
        assert_eq!(dk.recipe_key().unwrap(), None);
        assert_eq!(dk.command_key().unwrap(), None);
    }

    #[test]
    fn dependency_key_classifies_and_extracts_command_metadata_key() {
        let ck = CommandKey::new("", "root", "hello");
        let dk = DependencyKey::for_command_metadata(&ck);

        assert!(dk.is_command_metadata());
        assert!(!dk.is_command_implementation());
        assert_eq!(dk.command_key().unwrap(), Some(ck));
        assert_eq!(dk.key().unwrap(), None);
    }

    #[test]
    fn dependency_key_classifies_and_extracts_command_implementation_key() {
        let ck = CommandKey::new("realm", "ns", "hello");
        let dk = DependencyKey::for_command_implementation(&ck);

        assert!(dk.is_command_implementation());
        assert!(!dk.is_command_metadata());
        assert_eq!(dk.command_key().unwrap(), Some(ck));
        assert_eq!(dk.key().unwrap(), None);
    }

    #[test]
    fn dependency_key_command_key_rejects_invalid_format() {
        let dk = DependencyKey::new("ns-dep/command_metadata-broken");
        assert!(dk.command_key().is_err());
    }

    // --- Scheduled dependency (keyed-expansion) tests ---

    #[tokio::test]
    async fn scheduled_keyed_edge_is_registered() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        // a depends on b (unknown version — dynamic schedule)
        dm.register_scheduled_dependency(
            &ScheduleNode::Keyed(a.clone()),
            &ScheduleNode::Keyed(b.clone()),
            Version::unknown(),
        )
        .await
        .expect("first edge should register");
        // The edge a→b is now visible to cycle detection.
        assert!(dm.would_create_cycle(&b, &a).await);
    }

    #[tokio::test]
    async fn scheduled_self_cycle_is_rejected() {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let err = dm
            .register_scheduled_dependency(
                &ScheduleNode::Keyed(a.clone()),
                &ScheduleNode::Keyed(a.clone()),
                Version::unknown(),
            )
            .await
            .expect_err("self-cycle must be rejected");
        assert!(matches!(
            err.error_type,
            crate::error::ErrorType::DependencyCycle
        ));
    }

    #[tokio::test]
    async fn scheduled_dynamic_keyed_mutual_cycle_is_rejected() {
        let dm = DependencyManager::<TestEnv>::new();
        let k1 = DependencyKey::new("-R/k1");
        let k2 = DependencyKey::new("-R/k2");
        dm.register_scheduled_dependency(
            &ScheduleNode::Keyed(k1.clone()),
            &ScheduleNode::Keyed(k2.clone()),
            Version::unknown(),
        )
        .await
        .expect("k1->k2 ok");
        let err = dm
            .register_scheduled_dependency(
                &ScheduleNode::Keyed(k2.clone()),
                &ScheduleNode::Keyed(k1.clone()),
                Version::unknown(),
            )
            .await
            .expect_err("k2->k1 must cycle");
        assert!(matches!(
            err.error_type,
            crate::error::ErrorType::DependencyCycle
        ));
    }

    #[tokio::test]
    async fn scheduled_keyed_through_expression_cycle_is_rejected() {
        let dm = DependencyManager::<TestEnv>::new();
        let k = DependencyKey::new("-R/k");
        let q = DependencyKey::new("q-expr");
        // K depends on expression Q.
        dm.register_scheduled_dependency(
            &ScheduleNode::Keyed(k.clone()),
            &ScheduleNode::Expression(q.clone()),
            Version::unknown(),
        )
        .await
        .expect("K->Q ok");
        // Q's command evaluates K -> cycle (K depends on Q depends on K).
        let err = dm
            .register_scheduled_dependency(
                &ScheduleNode::Expression(q.clone()),
                &ScheduleNode::Keyed(k.clone()),
                Version::unknown(),
            )
            .await
            .expect_err("Q->K must cycle");
        assert!(matches!(
            err.error_type,
            crate::error::ErrorType::DependencyCycle
        ));
    }

    #[tokio::test]
    async fn scheduled_shared_expression_second_parent_cycle_is_rejected() {
        let dm = DependencyManager::<TestEnv>::new();
        let k1 = DependencyKey::new("-R/k1");
        let k2 = DependencyKey::new("-R/k2");
        let q = DependencyKey::new("q-expr");
        // k1 depends on Q; Q depends on k2 (registers k1->k2, Q.keyed_deps={k2}).
        dm.register_scheduled_dependency(
            &ScheduleNode::Keyed(k1.clone()),
            &ScheduleNode::Expression(q.clone()),
            Version::unknown(),
        )
        .await
        .expect("k1->Q");
        dm.register_scheduled_dependency(
            &ScheduleNode::Expression(q.clone()),
            &ScheduleNode::Keyed(k2.clone()),
            Version::unknown(),
        )
        .await
        .expect("Q->k2");
        // k2 now depends on Q -> k2 depends on k2 through Q -> cycle.
        let err = dm
            .register_scheduled_dependency(
                &ScheduleNode::Keyed(k2.clone()),
                &ScheduleNode::Expression(q.clone()),
                Version::unknown(),
            )
            .await
            .expect_err("k2->Q must cycle");
        assert!(matches!(
            err.error_type,
            crate::error::ErrorType::DependencyCycle
        ));
    }

    #[tokio::test]
    async fn scheduled_pure_expression_cycle_is_rejected() {
        let dm = DependencyManager::<TestEnv>::new();
        let q1 = DependencyKey::new("q1-expr");
        let q2 = DependencyKey::new("q2-expr");
        dm.register_scheduled_dependency(
            &ScheduleNode::Expression(q1.clone()),
            &ScheduleNode::Expression(q2.clone()),
            Version::unknown(),
        )
        .await
        .expect("q1->q2");
        let err = dm
            .register_scheduled_dependency(
                &ScheduleNode::Expression(q2.clone()),
                &ScheduleNode::Expression(q1.clone()),
                Version::unknown(),
            )
            .await
            .expect_err("q2->q1 must cycle");
        assert!(matches!(
            err.error_type,
            crate::error::ErrorType::DependencyCycle
        ));
    }

    #[tokio::test]
    async fn remove_expression_clears_transient_attribution() {
        let dm = DependencyManager::<TestEnv>::new();
        let k = DependencyKey::new("-R/k");
        let q = DependencyKey::new("q-expr");
        dm.register_scheduled_dependency(
            &ScheduleNode::Keyed(k.clone()),
            &ScheduleNode::Expression(q.clone()),
            Version::unknown(),
        )
        .await
        .expect("k->Q");
        dm.remove_expression(&q).await;
        // With Q's attribution cleared, Q->k no longer re-attributes k, so no cycle
        // is produced (without the remove this would be a K→Q→K cycle).
        dm.register_scheduled_dependency(
            &ScheduleNode::Expression(q.clone()),
            &ScheduleNode::Keyed(k.clone()),
            Version::unknown(),
        )
        .await
        .expect("Q->k ok after remove_expression");
    }
}
