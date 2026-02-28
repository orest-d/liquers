# Phase 2: Solution & Architecture - Dependency Management System

## Overview

The redesign replaces the skeletal `DependencyManagerImpl` with a fully async-friendly `DependencyManager<E>` (using `scc::HashMap` for concurrent access). `DependencyManager<E>` is generic over the environment to track both keyed dependents (`DependencyKey`) and untracked dependents (`WeakAssetRef<E>`) for cascade expiration. `DefaultAssetManager` gains `dependency_manager` and `max_dependency_retries` fields and hooks `set_state`, `set_binary`, and `remove` to trigger cascade expiration. `MetadataRecord` gains a `dependencies` field for persistence. `Context::evaluate` gains cycle detection and runtime dependency recording. The plan layer gains `PlanDependency` (key + relation) replacing the current raw `HashSet<Key>`.

---

## Data Structures

### `Version` (redesigned)

```rust
/// A version identifies a point-in-time snapshot of an asset.
/// Uses i128 internally for total ordering and efficient comparison.
/// Serialized as a 32-character lowercase hex string for JSON safety
/// (u128/i128 exceeds JavaScript Number precision).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(i128);
```

**Ownership:** `Copy` — cheap to pass by value everywhere.

**Serialization:** Custom `Serialize`/`Deserialize` as 32-char hex string (not i128 JSON number — JS cannot represent u128 exactly). Round-trips: `Version(v)` → `"000...ff"` → `Version(v)`.

**Constructors:**
```rust
impl Version {
    pub fn new(v: i128) -> Self;
    /// Hash bytes via BLAKE3; interpret first 16 bytes as i128 (big-endian).
    /// Uses copy_from_slice — not index-by-index.
    pub fn from_bytes(bytes: &[u8]) -> Self;
    /// Nanoseconds since UNIX_EPOCH cast to i128. `as_nanos()` returns u128;
    /// cast to i128 preserves the bit pattern (safe for comparison within an epoch).
    /// Uses unwrap_or_default() — no unwrap in lib code.
    pub fn from_time_now() -> Self;
    /// Nanoseconds since UNIX_EPOCH for a specific time (e.g. file modification time).
    /// Enables stable, reproducible versions for assets loaded from files.
    /// Uses SystemTime::duration_since(UNIX_EPOCH).ok().unwrap_or_default().
    pub fn from_specific_time(time: std::time::SystemTime) -> Self;
    /// Randomly generated (using system entropy).
    pub fn new_unique() -> Self;
}
```

---

### `DependencyKey` (new)

```rust
/// A string parseable as a valid Liquers query, identifying a dependency target.
/// Conventions:
///   `-R/<key>`                             → asset data + metadata
///   `-R-recipe/<key>`                      → asset recipe
///   `-R-dir/<key>`                         → directory listing (reserved)
///   `ns-dep/command_metadata-<r>-<ns>-<c>` → command metadata
///   `ns-dep/command_implementation-<r>-<ns>-<c>` → command implementation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]    // serializes as plain string in JSON/YAML
pub struct DependencyKey(String);
```

**Ownership:** Owned `String` inside; cloned when inserted into maps.

**Serialization:** `#[serde(transparent)]` — appears as plain string. Consistent with how `Key` and `Query` are stored in metadata JSON.

**Conversions** (use `From`/`TryFrom` stdlib traits, not only ad-hoc constructors):
```rust
impl DependencyKey {
    pub fn new(s: impl Into<String>) -> Self;
    pub fn as_str(&self) -> &str;
    /// Parse to Query (infallible by construction invariant).
    pub fn to_query(&self) -> Result<Query, Error>;
}

/// Asset data dependency: `-R/<key>`
impl From<&Key> for DependencyKey { ... }
/// Fallible: only `-R/<key>` keys can become a Key.
impl TryFrom<&DependencyKey> for Key { type Error = Error; ... }
impl From<&Query> for DependencyKey { ... }

// Named constructors for non-key dependency types:
impl DependencyKey {
    pub fn from_recipe_key(key: &Key) -> Self;          // `-R-recipe/<key>`
    pub fn from_dir_key(key: &Key) -> Self;             // `-R-dir/<key>`
    /// `ns-dep/command_metadata-{key}` where `{key}` is `CommandKey::to_string()` = `realm-namespace-name`.
    pub fn for_command_metadata(key: &CommandKey) -> Self;
    /// `ns-dep/command_implementation-{key}` where `{key}` is `CommandKey::to_string()`.
    pub fn for_command_implementation(key: &CommandKey) -> Self;
}

impl std::fmt::Display for DependencyKey { ... }
```

---

### `DependencyRecord` (redesigned)

```rust
/// Persistent dependency record: what this asset depends on and at which version.
/// Stored in MetadataRecord.dependencies and DependencyManager.
/// Does NOT carry DependencyRelation — that is plan-level only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyRecord {
    pub key: DependencyKey,
    pub version: Version,
}
```

**Serialization:** Full `Serialize, Deserialize` (persisted in `MetadataRecord`).

---

### `DependencyRelation` (new, plan-only)

```rust
/// Why a dependency exists. Only present at plan level; not stored in DependencyManager
/// or MetadataRecord. For ContextEvaluate dependencies the relation is not recoverable
/// after execution (no plan entry).
/// Derives Serialize/Deserialize for plan caching and debugging; uses
/// adjacently-tagged format for readable JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum DependencyRelation {
    /// Input state entering the action depends on the asset.
    StateArgument,
    /// Named parameter links to another asset via query.
    ParameterLink(String),
    /// Named parameter uses a default that links to another asset.
    DefaultLink(String),
    /// Named parameter links via recipe override.
    RecipeLink(String),
    /// Named parameter links via override link.
    OverrideLink(String),
    /// Named parameter links via enum value mapping.
    EnumLink(String),
    /// Dependency created dynamically via context::evaluate(query).
    /// Stores the encoded query string (not Query) for Hash + simpler serde.
    ContextEvaluate(String),
    /// Dependency on the command's metadata registration.
    CommandMetadata,
    /// Dependency on the command's implementation.
    CommandImplementation,
    /// Dependency on the recipe itself (separate from the asset's data).
    Recipe,
}
```

**No default match arm:** All consumers must handle all variants.

**Not serialized:** Plan-only; no `Serialize/Deserialize` derives.

---

### `PlanDependency` (new, plan-only)

```rust
/// A dependency with relation, as known at plan-analysis time (no version yet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanDependency {
    pub key: DependencyKey,
    pub relation: DependencyRelation,
}
```

**Not serialized:** Lives only in `Plan` structures.

---

### `DependencyManager` (redesigned)

Uses `scc::HashMap` directly (matching `DefaultAssetManager` pattern) — no wrapping `RwLock`. An `expiration_lock: tokio::sync::Mutex<()>` serializes cascade expiration to prevent interleaved map updates being visible to concurrent readers.

`DependencyManager<E>` is **generic over the environment** because it stores `WeakAssetRef<E>` for untracked dependents (query assets and ad-hoc assets created via `apply`/`evaluate_immediately`).

**Circular dependency note:** `dependencies.rs` will import `WeakAssetRef<E>` from `assets.rs`, and `assets.rs` will import `DependencyManager<E>` from `dependencies.rs`. This is an intra-crate mutual reference — Rust allows this within a single crate (unlike cross-crate circular deps, which are forbidden). No action needed.

```rust
/// Thread-safe, async-friendly dependency manager.
/// Owned by DefaultAssetManager.
/// Uses scc::HashMap for concurrent reads; expiration_lock serializes cascade writes.
/// Generic over E to store WeakAssetRef<E> for untracked dependents.
pub struct DependencyManager<E: Environment> {
    /// Current version per tracked dependency key.
    versions: scc::HashMap<DependencyKey, Version>,
    /// Keyed dependents: for key K, the DependencyKeys of keyed assets that depend on K.
    /// Populated when a keyed (stored) asset registers a plan-level dependency.
    keyed_dependents: scc::HashMap<DependencyKey, scc::HashSet<DependencyKey>>,
    /// Untracked dependents: for key K, the WeakAssetRefs of query/ad-hoc assets depending on K.
    /// WeakAssetRef prevents this map from keeping assets alive past their natural lifetime.
    /// Populated when context::evaluate() records a ContextEvaluate dependency.
    untracked_dependents: scc::HashMap<DependencyKey, Vec<WeakAssetRef<E>>>,
    /// Serializes cascade expiration to prevent concurrent interleaved updates.
    expiration_lock: tokio::sync::Mutex<()>,
}
```

**Ownership:** All methods take `&self` (scc maps are internally synchronized). Cloneable via `Arc<DependencyManager<E>>` when shared across tasks.

**No serialization:** Not persisted; rebuilt from metadata.

**Expired dependency cleanup:** When `expire(key)` is called, `key` and all transitive dependents are removed from `versions`, `keyed_dependents`, and `untracked_dependents`. This ensures the manager only tracks valid (non-expired) dependencies at all times.

---

### `DefaultAssetManager` (extended)

```rust
pub struct DefaultAssetManager<E: Environment> {
    id: std::sync::atomic::AtomicU64,
    envref: std::sync::OnceLock<EnvRef<E>>,
    assets: scc::HashMap<Key, AssetRef<E>>,
    query_assets: scc::HashMap<Query, AssetRef<E>>,
    job_queue: Arc<JobQueue<E>>,
    monitor_tx: mpsc::UnboundedSender<ExpirationMonitorMessage<E>>,
    /// NEW: Dependency tracking and cascade expiration.
    dependency_manager: DependencyManager<E>,
    /// NEW: Max retries on DependencyVersionMismatch during recipe evaluation.
    /// Moved here from DependencyManager (retry policy is an asset manager concern).
    max_dependency_retries: u32,
}
```

---

### `MetadataRecord` (extended)

```rust
pub struct MetadataRecord {
    // ... existing fields ...

    /// NEW: Dependencies of this asset, with their versions at evaluation time.
    /// Empty for volatile assets (they have no tracked dependencies).
    /// Populated when the asset reaches Ready/Source/Override status.
    #[serde(default)]
    pub dependencies: Vec<DependencyRecord>,
}
```

---

## Trait Implementations

### `DependencyManager` public API

```rust
/// Result of expire(): expired keyed dependents and untracked (WeakAssetRef) dependents.
pub struct ExpiredDependents<E: Environment> {
    /// DependencyKeys of keyed assets that were transitively expired.
    /// AssetManager converts each to Key via TryFrom, skipping non-asset keys silently.
    pub keys: Vec<DependencyKey>,
    /// WeakAssetRefs of untracked (query/ad-hoc) assets that were transitively expired.
    /// AssetManager upgrades each weak ref and triggers expiration on those assets directly.
    pub assets: Vec<WeakAssetRef<E>>,
}

impl<E: Environment> DependencyManager<E> {
    pub fn new() -> Self;

    /// Register/update the version of a key.
    /// Used when an asset reaches Ready/Source/Override status.
    pub async fn register_version(&self, key: &DependencyKey, version: Version);

    /// Register a keyed dependency edge: `dependent` (a keyed asset) depends on `dependency`
    /// at `version`. Returns Err if `dependency` is not tracked or version is stale.
    /// Returns Err if adding this edge would create a cycle.
    pub async fn add_dependency(
        &self,
        dependent: &DependencyKey,
        dependency: &DependencyKey,
        version: Version,
    ) -> Result<(), Error>;

    /// Register an untracked dependent: a query/ad-hoc asset (WeakAssetRef) depends on `dependency`.
    /// Called from context::evaluate() when a ContextEvaluate dependency is recorded.
    /// Dead weak refs are pruned lazily on next expiration.
    pub async fn add_untracked_dependent(
        &self,
        dependency: &DependencyKey,
        dependent: WeakAssetRef<E>,
    );

    /// Check if adding the edge dependent→dependency would create a cycle.
    pub async fn would_create_cycle(
        &self,
        dependent: &DependencyKey,
        dependency: &DependencyKey,
    ) -> bool;

    /// Expire a key and all transitive dependents.
    /// Acquires expiration_lock; uses iterative BFS (not recursive — avoids stack overflow
    /// on deep or accidentally cyclic historical graphs).
    /// Removes expired keys from versions, keyed_dependents, and untracked_dependents maps
    /// (DependencyManager only tracks valid, non-expired dependencies).
    /// Returns all expired keyed DependencyKeys and untracked WeakAssetRefs.
    pub async fn expire(&self, key: &DependencyKey) -> ExpiredDependents<E>;

    /// Remove a key from tracking entirely (e.g., deleted asset).
    /// Does NOT cascade expire — caller is responsible for deciding whether to expire.
    pub async fn remove(&self, key: &DependencyKey);

    /// Check if a key's current version matches the expected version.
    /// Used for consistency validation at dependency registration time.
    pub async fn version_consistent(&self, key: &DependencyKey, expected: Version) -> bool;

    /// Get the current version of a key, or None if not tracked.
    pub async fn get_version(&self, key: &DependencyKey) -> Option<Version>;

    /// Load dependencies from persisted MetadataRecord into the manager.
    /// Called when an asset loads from the store. Only registers edges for keys
    /// whose versions are already known (progressive reconstruction).
    pub async fn load_from_records(
        &self,
        dependent: &DependencyKey,
        records: &[DependencyRecord],
    );
}
```

---

## Sync vs Async Decisions

| Function | Async? | Rationale |
|---|---|---|
| `DependencyManager::register_version` | Yes | scc map insert + expiration_lock; called from async contexts |
| `DependencyManager::add_dependency` | Yes | scc map insert; cycle check needs BFS traversal |
| `DependencyManager::add_untracked_dependent` | Yes | scc map insert |
| `DependencyManager::expire` | Yes | Acquires expiration_lock; BFS traversal + map cleanup |
| `DependencyManager::would_create_cycle` | Yes | BFS traversal over scc maps |
| `DependencyManager::get_version` | Yes | scc map read |
| `DependencyManager::load_from_records` | Yes | scc map inserts |
| `Context::evaluate` | Yes (already) | Extended with cycle check + dep recording |
| Plan `find_dependencies` | Yes | Currently async (Pin<Box<dyn Future>>); recipe lookups inside require async |
| `DependencyKey` constructors | No | Pure string manipulation |
| `Version` constructors | No | Pure value construction |
| `dep` namespace commands | No | Pure computation (hash of metadata) |

---

## Function Signatures

### `liquers-core/src/dependencies.rs`

```rust
// (All types defined here, see Data Structures section)

impl<E: Environment> DependencyManager<E> {
    pub fn new() -> Self;
    pub async fn register_version(&self, key: &DependencyKey, version: Version);
    pub async fn add_dependency(&self, dependent: &DependencyKey, dependency: &DependencyKey, version: Version) -> Result<(), Error>;
    pub async fn add_untracked_dependent(&self, dependency: &DependencyKey, dependent: WeakAssetRef<E>);
    pub async fn would_create_cycle(&self, dependent: &DependencyKey, dependency: &DependencyKey) -> bool;
    pub async fn expire(&self, key: &DependencyKey) -> ExpiredDependents<E>;
    pub async fn remove(&self, key: &DependencyKey);
    pub async fn version_consistent(&self, key: &DependencyKey, expected: Version) -> bool;
    pub async fn get_version(&self, key: &DependencyKey) -> Option<Version>;
    pub async fn load_from_records(&self, dependent: &DependencyKey, records: &[DependencyRecord]);
}
```

### `liquers-core/src/plan.rs` (updated)

```rust
/// Updated signature: returns Vec<PlanDependency> (key + relation) instead of raw HashSet<Key>.
/// Stays async (currently Pin<Box<dyn Future>>; recipe lookups require async).
pub(crate) async fn find_dependencies<'a, E: Environment>(
    envref: EnvRef<E>,
    plan: &'a Plan,
    stack: &mut Vec<Key>,
    current_cwd: Option<Key>,
) -> Result<Vec<PlanDependency>, Error>;
```

**Callers to update** (breaking change — update before signature change):
- `has_volatile_dependencies()` (~plan.rs:1722): `for dep in &dependencies { let Ok(key) = Key::try_from(&dep.key) else { continue }; ...recipe_opt(&key)... }`
- `has_expirable_dependencies()` (~plan.rs:1756): same pattern

### `liquers-core/src/context.rs` (updated)

```rust
impl<E: Environment> Context<E> {
    /// Extended: cycle detection + records ContextEvaluate dependency for the calling asset.
    pub async fn evaluate(&self, query: &Query) -> Result<AssetRef<E>, Error>;

    /// NEW: Register a dependency record for the current asset being evaluated.
    /// Called internally by evaluate(); exposed for testing.
    pub(crate) async fn record_dependency(
        &self,
        dep_key: &DependencyKey,
        version: Version,
    ) -> Result<(), Error>;
}
```

### `liquers-core/src/assets.rs` (updated)

```rust
impl<E: Environment> DefaultAssetManager<E> {
    /// NEW: Expose dependency manager for inspection/testing.
    pub fn dependency_manager(&self) -> &DependencyManager<E>;

    /// Updated: after storing Ready state, register version in DM and cascade-expire stale dependents.
    /// Cascade result includes both DependencyKey and WeakAssetRef dependents.
    pub async fn set_state(&self, key: &Key, state: State<E::Value>) -> Result<(), Error>;

    /// Updated: on remove, call DM::remove(key); caller decides whether to cascade.
    pub async fn remove(&self, key: &Key) -> Result<(), Error>;

    /// NEW: Register asset dependencies from plan-level PlanDependency list.
    /// Called at the start of recipe execution; resolves versions from current DM state.
    pub async fn register_plan_dependencies(
        &self,
        dependent_key: &Key,
        plan_deps: &[PlanDependency],
    ) -> Result<(), Error>;

    /// NEW: Retry wrapper — re-evaluates the recipe up to self.max_dependency_retries times
    /// if DependencyManager rejects a stale dependency version.
    async fn evaluate_with_retry(
        &self,
        asset_ref: &AssetRef<E>,
    ) -> Result<State<E::Value>, Error>;
}
```

### `liquers-core/src/command_metadata.rs` (extended)

`CommandMetadata` gains two non-serialized version fields:

```rust
pub struct CommandMetadata {
    // ... existing fields ...

    /// Hash of the serializable CommandMetadata fields (realm, namespace, name, label,
    /// doc, arguments, etc.). Computed by CommandMetadataRegistry::add_command() using
    /// blake3 on the serde_json serialization of the metadata (before this field is set).
    /// Not serialized — deterministically recomputable from the other fields.
    #[serde(skip)]
    pub version: Option<i128>,

    /// Hash of the source module that implements this command.
    /// Set by the registering code (not by CommandMetadataRegistry) using a compile-time
    /// constant embedded by build.rs.
    /// For Rust commands: blake3 hash of the source file containing the command.
    /// For Python commands: blake3 hash of inspect.getsource(func).
    /// Not serialized.
    #[serde(skip)]
    pub impl_version: Option<i128>,
}
```

`CommandMetadataRegistry::add_command()` computes `version` at registration time:
```rust
pub fn add_command(&mut self, command: &CommandMetadata) -> &mut Self {
    let mut cmd = command.to_owned();
    if let Ok(bytes) = serde_json::to_vec(&cmd) {  // serializes without the #[serde(skip)] fields
        let hash = blake3::hash(&bytes);
        let v = i128::from_be_bytes(hash.as_bytes()[0..16].try_into().unwrap_or([0u8; 16]));
        cmd.version = Some(v);
    }
    self.commands.push(cmd);
    self
}
```

### `liquers-lib/src/commands.rs` (dep commands, new)

```rust
/// Returns the CommandMetadata for the named command.
/// The metadata includes the non-serialized `version: Option<i128>` field
/// (pre-computed at registration time by CommandMetadataRegistry::add_command()).
/// Namespace: "dep", registered as dep/command_metadata.
/// Note: this command need not ever be evaluated — DependencyKey::for_command_metadata()
/// produces a query-compatible key that identifies the command metadata as a dependency
/// target without requiring evaluation.
pub fn command_metadata(
    state: &State<Value>,
    realm: String,
    namespace: String,
    name: String,
    context: &Context,
) -> Result<Value, Error>;

/// Returns the implementation version for the named command.
/// For Rust commands: a compile-time constant produced by build.rs hashing the
/// source file containing the command implementation (blake3, embedded as env!(...)).
/// For Python commands: blake3 hash of inspect.getsource(func).
/// Namespace: "dep", registered as dep/command_implementation.
pub fn command_implementation(
    state: &State<Value>,
    realm: String,
    namespace: String,
    name: String,
    context: &Context,
) -> Result<Value, Error>;
```

### `liquers-lib/build.rs` (new)

```rust
// build.rs: embed blake3 hash of commands source file as compile-time constant.
// The hash changes whenever any command implementation in the file changes.
// Uses blake3 (already in liquers-core) — but build.rs cannot use workspace crates directly.
// Use std::collections::hash_map::DefaultHasher OR add blake3 as a build-dependency.
// Preferred: add blake3 as a [build-dependencies] entry (same version already in workspace).
fn main() {
    let src = std::fs::read("src/commands.rs").unwrap_or_default();
    // Use a simple hash: FNV or SipHash via DefaultHasher (std only, no extra dep)
    // OR add blake3 as build-dep. Decision: use blake3 in build-dep for consistency.
    let hash = blake3::hash(&src);
    let v = i128::from_be_bytes(hash.as_bytes()[0..16].try_into().unwrap_or([0u8; 16]));
    println!("cargo:rustc-env=LIQUERS_LIB_COMMANDS_IMPL_HASH={}", v);
    println!("cargo:rerun-if-changed=src/commands.rs");
}
```

`command_implementation` reads this at compile time:
```rust
const COMMANDS_IMPL_HASH: i128 = /* parse env!("LIQUERS_LIB_COMMANDS_IMPL_HASH") */;
```

---

## Integration Points

### `liquers-core/src/dependencies.rs`

**Replace entirely.** New exports:
- `Version`, `DependencyKey`, `DependencyRecord`, `DependencyRelation`, `PlanDependency`, `DependencyManager<E>`, `ExpiredDependents<E>`

**Remove:** `StringDependency`, `DependencyManagerImpl`, `DependencyList`, `Dependency` trait

### `liquers-core/src/metadata.rs`

**Modify:** `MetadataRecord` — add `dependencies: Vec<DependencyRecord>` with `#[serde(default)]`.

**No breaking changes** to existing fields; `#[serde(default)]` ensures backward compatibility.

### `liquers-core/src/assets.rs`

**Modify:** `DefaultAssetManager` struct — add `dependency_manager: DependencyManager<E>` and `max_dependency_retries: u32`.
**Modify:** `DefaultAssetManager::new()` — initialize `DependencyManager::new()`.
**Modify:** `set_state` — register version in DM after Ready; cascade-expire dependents (both keyed and untracked WeakAssetRefs from `ExpiredDependents`).
**Modify:** `remove` — call `DM::remove`.
**Modify:** `evaluate_recipe` / job execution path — wrap with `evaluate_with_retry`.
**Add:** `register_plan_dependencies`, `evaluate_with_retry`, `dependency_manager()`.

### `liquers-core/src/context.rs`

**Modify:** `Context::evaluate` — add cycle check, record `ContextEvaluate` dependency.
**Add:** `Context::record_dependency` (pub(crate)).

### `liquers-core/src/plan.rs`

**Modify:** `find_dependencies` signature → returns `Vec<PlanDependency>`.
**Modify:** Internal matching on `ParameterValue` variants → map each to `DependencyRelation`.

### `liquers-core/src/command_metadata.rs`

**Modify:** `CommandMetadata` struct — add `#[serde(skip)] pub version: Option<i128>` and `#[serde(skip)] pub impl_version: Option<i128>`.
**Modify:** `CommandMetadataRegistry::add_command()` — compute and store `cmd.version` from blake3 hash of serialized fields.

### `liquers-lib/src/commands.rs`

**Add:** `dep` namespace functions (`command_metadata`, `command_implementation`).
**Modify:** Command registration to include `dep` namespace.

### `liquers-lib/build.rs` (new)

**Create:** Embeds blake3 hash of `src/commands.rs` as `LIQUERS_LIB_COMMANDS_IMPL_HASH` compile-time env var.

### `liquers-lib/Cargo.toml`

**Modify:** Add `blake3` as a `[build-dependencies]` entry (same version as in workspace; needed for `build.rs`). No new runtime dependencies.

---

## Relevant Commands

### New Commands (`dep` namespace)

| Command | Namespace | Parameters | Description |
|---|---|---|---|
| `command_metadata` | `dep` | `state`, `realm: String`, `namespace: String`, `name: String`, `context` | Returns the `CommandMetadata` for the named command (includes pre-computed `version: Option<i128>`) |
| `command_implementation` | `dep` | `state`, `realm: String`, `namespace: String`, `name: String`, `context` | Returns the implementation version (compile-time source hash from `build.rs`) |

**Design note:** These commands need not be evaluated for dependency tracking to work. `DependencyKey::for_command_metadata(key)` produces a query-compatible key (`ns-dep/command_metadata-{key}`) that identifies the dependency without execution. If evaluated, `command_metadata` returns the full `CommandMetadata` (with the non-serialized `version` field accessible in memory via the `Value::CommandMetadata` variant or similar). `command_implementation` returns a JSON object with the compile-time source hash.

Registration:
```rust
register_command!(cr,
    fn command_metadata(state, realm: String, namespace: String, name: String, context) -> result
    namespace: "dep"
    label: "Command Metadata"
    doc: "Returns the CommandMetadata for the named command, including pre-computed metadata version"
)?;
register_command!(cr,
    fn command_implementation(state, realm: String, namespace: String, name: String, context) -> result
    namespace: "dep"
    label: "Command Implementation Version"
    doc: "Returns the implementation version hash for the named command's source module"
)?;
```

### Relevant Existing Namespaces

| Namespace | Relevance |
|---|---|
| (none) | No existing namespaces interact with dependency tracking directly |

---

## Error Handling

### New `ErrorType` Variant

Add to `liquers-core/src/error.rs`:
```rust
// In ErrorType enum:
DependencyVersionMismatch,
DependencyCycle,
```

Add constructors:
```rust
impl Error {
    pub fn dependency_version_mismatch(key: &DependencyKey, msg: String) -> Self;
    pub fn dependency_cycle(key: &DependencyKey) -> Self;
}
```

Rationale: version mismatch is a specific recoverable error (retry logic matches on it); `general_error` would not allow programmatic retry detection.

### Error Scenarios

| Scenario | Constructor | Note |
|---|---|---|
| Cycle in context::evaluate | `Error::dependency_cycle` | Specific type; not retried |
| Unknown dependency version | `Error::general_error` | Dependency not yet registered |
| Version inconsistency (stale) | `Error::dependency_version_mismatch` | Triggers retry in AssetManager |
| Max retries exceeded | `Error::general_error` | Terminal failure |
| Invalid DependencyKey format | `Error::general_error` | Bad string format |
| Unknown command in dep namespace | `Error::general_error` | Command not found |

---

## Serialization Strategy

- `Version` → custom `Serialize, Deserialize` (i128 → 32-char lowercase hex string)
- `DependencyKey` → `Serialize, Deserialize` (transparent string newtype)
- `DependencyRecord` → `Serialize, Deserialize` (in `MetadataRecord.dependencies`)
- `DependencyRelation` → **no** serde (plan-only)
- `PlanDependency` → **no** serde (plan-only)
- `DependencyManager` → **no** serde (in-memory, rebuilt from metadata)
- `MetadataRecord.dependencies` → `#[serde(default)]` (backward compat, empty = no deps)

---

## Concurrency Considerations

- `DependencyManager<E>` uses `scc::HashMap` for concurrent reads (version checks, cycle detection are non-blocking); `expiration_lock: tokio::sync::Mutex<()>` serializes cascade expiration writes.
- `DefaultAssetManager<E>` accesses `dependency_manager` via `&self` (no extra Arc needed — scc maps are internally synchronized).
- `Context::evaluate` records dependencies to a `Vec<DependencyRecord>` stored in the `Context` (via `Arc<Mutex<Vec<DependencyRecord>>>`), flushed to the asset and DM after evaluation completes.
- No deadlock risk: `expiration_lock` is short-lived (no I/O inside the lock); scc map operations never block.
- Dead `WeakAssetRef` entries in `untracked_dependents` are pruned lazily during `expire()` traversal.

---

## Generic Parameters & Bounds

`DependencyManager<E: Environment>` is now generic. This is necessary because it stores `WeakAssetRef<E>` for untracked dependents. `DependencyKey`, `Version`, `DependencyRecord`, `PlanDependency`, `DependencyRelation`, and `ExpiredDependents<E>` — all other types are concrete or minimally generic.

**Intra-crate mutual reference:** `dependencies.rs` imports `WeakAssetRef<E>` from `assets.rs`; `assets.rs` imports `DependencyManager<E>` from `dependencies.rs`. Rust allows mutual module references within a single crate — no issue.

`Context` gains a new field `pending_dependencies: Arc<tokio::sync::Mutex<Vec<DependencyRecord>>>` — no new bounds required.

---

## Design Notes (from Rust Best Practices Review)

### Volatile Asset Exclusion
Enforced at `AssetManager` call site, NOT inside `DependencyManager`. When `AssetManager` would register a volatile asset as a dependency or dependent, it checks `is_volatile` first and skips the DM call. `DependencyManager` remains policy-free.

### Cascade Expiration + Key Conversion
`DependencyManager::expire()` returns `ExpiredDependents<E>` containing:
- `keys: Vec<DependencyKey>` — `AssetManager` converts each to `Key` via `TryFrom<&DependencyKey> for Key`, silently skipping command metadata and other non-asset keys.
- `assets: Vec<WeakAssetRef<E>>` — `AssetManager` upgrades each weak ref; live assets are expired directly (without a key lookup). Dead weak refs (already dropped) are silently ignored.

All expired entries are removed from `DependencyManager`'s internal maps; only valid (non-expired) dependencies remain tracked.

### Retry Logic
`evaluate_with_retry` in `DefaultAssetManager` wraps recipe evaluation.
`max_dependency_retries` lives on `DefaultAssetManager` (retry policy is an asset manager concern, not DependencyManager's):
```rust
// DefaultAssetManager::evaluate_with_retry
for attempt in 0..self.max_dependency_retries {
    match evaluate_recipe(...).await {
        Ok(val) => return Ok(val),
        Err(e) if e.error_type == ErrorType::DependencyVersionMismatch => {
            if attempt + 1 == self.max_dependency_retries { return Err(e); }
            tokio::task::yield_now().await;
        }
        Err(e) => return Err(e),
    }
}
```

### `all_dependents` — Iterative BFS
Cascade expiration uses iterative BFS (not recursive) to prevent stack overflow on deep or historically-cyclic dependency graphs. Visited set prevents infinite loops even if data is corrupted.

### Removing `Dependency` Trait and Generics
The existing `Dependency` trait, `DependencyManagerImpl<V,D>`, `DependencyList<V,D>`, and `StringDependency` are all removed. The new design minimizes generics: only `DependencyManager<E>` and `ExpiredDependents<E>` are generic (necessary for `WeakAssetRef<E>`); all other types are concrete.
