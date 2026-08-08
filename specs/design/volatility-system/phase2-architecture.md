# Phase 2: Solution & Architecture - Volatility System

## Overview

Comprehensive volatility tracking implemented through coordinated changes across liquers-core modules. Adds `volatile` fields to Status enum (new variant), MetadataRecord, Plan, and Context structures. Volatility computed upfront during plan building via existing `IsVolatile<E>` trait, then propagated through metadata to consumers. AssetManager modified to return new AssetRef for volatile assets instead of existing copies.

## Data Structures

### Modified Enum: Status

**File:** `liquers-core/src/metadata.rs` (line ~11-47)

```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub enum Status {
    None,
    Directory,
    Recipe,
    Submitted,
    Dependencies,
    Processing,
    Partial,
    Error,
    Storing,
    Ready,
    Expired,
    Cancelled,
    Source,
    Override,
    // NEW VARIANT:
    Volatile,  // Asset has volatile value (use once, then expires like Ready but volatile)
}
```

**Variant semantics:**
- `Volatile`: Asset HAS a volatile value. Semantically like `Ready` (data is available) but signals "use once, then consider expired". Value is valid but should not be cached long-term.

**`has_data()` update:**
```rust
pub fn has_data(&self) -> bool {
    match self {
        Status::Ready => true,
        Status::Volatile => true,  // NEW: Volatile has data
        Status::Partial => true,
        Status::Expired => true,
        Status::Source => true,
        Status::Override => true,
        // ... other variants false
    }
}
```

**`is_finished()` update:**
```rust
pub fn is_finished(&self) -> bool {
    match self {
        Status::Ready => true,
        Status::Volatile => true,  // NEW: Volatile is finished state
        Status::Error => true,
        Status::Expired => true,
        Status::Cancelled => true,
        // ... other variants false
    }
}
```

**`can_have_tracked_dependencies()` update:**
```rust
pub fn can_have_tracked_dependencies(&self) -> bool {
    match self {
        Status::Ready => true,
        Status::Volatile => false,  // NEW: Like Expired, volatile is terminal - no revalidation needed
        Status::Partial => true,
        Status::Storing => true,
        // ... other variants false
    }
}
```

**Rationale:** Volatile assets are valid when created (dependencies were valid at that moment) and meant to be used once, then discarded. Like Expired, they don't benefit from dependency tracking since they won't be revalidated.

**Ownership:** Status is `Copy` (1 byte enum) - pass by value

**Serialization:** Already derives `Serialize, Deserialize`

**No default match arm:** All existing match statements on Status must be updated to explicitly handle `Volatile` variant (compiler will enforce)

### Modified Struct: MetadataRecord

**File:** `liquers-core/src/metadata.rs` (line ~470-515)

```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MetadataRecord {
    pub log: Vec<LogEntry>,
    #[serde(with = "query_format")]
    pub query: Query,
    #[serde(with = "option_key_format")]
    pub key: Option<Key>,
    pub status: Status,
    pub type_identifier: String,
    pub data_format: Option<String>,
    pub message: String,
    pub title: String,
    pub description: String,
    pub is_error: bool,
    pub error_data: Option<Error>,
    pub media_type: String,
    pub filename: Option<String>,
    pub unicode_icon: String,
    pub file_size: Option<u64>,
    pub is_dir: bool,
    pub progress: Vec<ProgressEntry>,
    pub updated: String,
    #[serde(default)]
    pub children: Vec<AssetInfo>,

    // NEW FIELD:
    /// If true, this value is known to be volatile even if status is not yet Volatile.
    /// Useful for in-flight assets (Submitted, Dependencies, Processing) where final
    /// value will be volatile when ready.
    pub is_volatile: bool,
}
```

**Ownership:** MetadataRecord fields are owned. `is_volatile` is `bool` (Copy, 1 byte)

**Serialization:**
- `is_volatile` is always serialized (no `#[serde(default)]`)
- Field is required in all serialized MetadataRecord instances

**Helper method:**
```rust
impl MetadataRecord {
    /// Returns true if the value is or will be volatile
    pub fn is_volatile(&self) -> bool {
        self.is_volatile || self.status == Status::Volatile
    }
}
```

### Modified Struct: Plan

**File:** `liquers-core/src/plan.rs` (need to find Plan struct definition)

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Plan {
    pub steps: Vec<Step>,
    pub initial_state: Option<State<Value>>,
    // ... other existing fields

    // NEW FIELD:
    /// If true, this plan produces volatile results.
    /// Computed during plan building via IsVolatile<E> trait.
    pub is_volatile: bool,
}
```

**Ownership:** `is_volatile` is `bool` (Copy)

**Serialization:** No default - field is always required

**No Step::Volatile variant needed** - volatility is a property of the entire Plan, not individual steps. Use existing `Step::Info` for debugging/clarity if needed.

### Modified Struct: Context

**File:** `liquers-core/src/context.rs` (need to locate Context struct)

```rust
pub struct Context<E: Environment> {
    // ... existing fields (metadata, cwd, etc.)

    // NEW FIELD:
    /// If true, this context is evaluating a volatile asset.
    /// Propagates to nested evaluations via context.evaluate()
    is_volatile: bool,
}
```

**Ownership:** `is_volatile` is `bool` (Copy)

**Not serialized** - Context is runtime-only, not persisted

**Initialization:**
```rust
impl<E: Environment> Context<E> {
    pub fn new(envref: EnvRef<E>, is_volatile: bool) -> Self {
        Context {
            // ... existing field initialization
            is_volatile,
        }
    }

    pub fn is_volatile(&self) -> bool {
        self.is_volatile
    }

    /// Create child context for nested evaluation, inheriting volatility
    pub fn with_volatile(&self, is_volatile: bool) -> Self {
        let mut new_ctx = self.clone();
        new_ctx.is_volatile = is_volatile || self.is_volatile;  // Propagate if parent is volatile
        new_ctx
    }
}
```

### Modified Struct: AssetData

**File:** `liquers-core/src/assets.rs` (line ~187)

Add volatility tracking field:

```rust
pub struct AssetData<E: Environment> {
    id: u64,
    pub recipe: Recipe,
    envref: EnvRef<E>,
    service_tx: mpsc::UnboundedSender<AssetServiceMessage>,
    service_rx: Arc<Mutex<mpsc::UnboundedReceiver<AssetServiceMessage>>>,
    notification_tx: watch::Sender<AssetNotificationMessage>,
    _notification_rx: watch::Receiver<AssetNotificationMessage>,
    initial_state: State<E::Value>,
    query: Arc<Option<Query>>,
    data: Option<Arc<E::Value>>,
    binary: Option<Arc<Vec<u8>>>,
    pub(crate) metadata: Metadata,
    status: Status,

    // NEW FIELD:
    /// If true, this asset is volatile (computed from recipe/plan before execution)
    is_volatile: bool,
}
```

**Ownership:** `is_volatile` is `bool` (Copy)

**Not serialized** - AssetData is runtime-only

**Initialization:** Set `is_volatile` field when creating AssetData from Recipe (computed via `recipe.is_volatile(env)`)

### Modified Trait: AssetRef

**File:** `liquers-core/src/assets.rs`

Add method to convert any state with a recipe to Override status:

```rust
impl<E: Environment> AssetRef<E> {
    /// Convert asset status to Override, preventing re-evaluation.
    /// Behavior depends on current status:
    /// - Directory, Source: No change (ignored)
    /// - None, Recipe, Submitted, Dependencies, Processing, Error, Cancelled:
    ///   Cancel if necessary, set value to Value::none(), set status to Override
    /// - Partial, Storing, Expired, Volatile, Ready:
    ///   Keep existing value, set status to Override
    pub async fn to_override(&self) -> Result<(), Error> {
        let mut data = self.write().await;

        match data.status {
            // Ignore these - no change
            Status::Directory | Status::Source => {
                // No-op
            }

            // In-progress or failed states: cancel, set to none value, mark Override
            Status::None | Status::Recipe | Status::Submitted |
            Status::Dependencies | Status::Processing |
            Status::Error | Status::Cancelled => {
                // Cancel in-flight evaluation using existing cancellation mechanism
                // See existing cancel message handling in liquers-core/src/assets.rs
                // Send cancel message via service_tx channel
                data.data = Some(Arc::new(E::Value::none()));
                data.binary = None;
                data.status = Status::Override;
                data.metadata.set_status(Status::Override);
            }

            // States with data: keep value, mark Override
            Status::Partial | Status::Storing | Status::Expired |
            Status::Volatile | Status::Ready => {
                data.status = Status::Override;
                data.metadata.set_status(Status::Override);
            }

            // Already Override - no-op
            Status::Override => {
                // No-op
            }
        }

        Ok(())
    }
}
```

**Async:** Method is async because it acquires write lock on AssetData

**Error handling:** Returns `Result<(), Error>` - currently always succeeds, signature allows future error cases

**Cancellation:** For in-progress states, uses existing cancellation mechanism (see `liquers-core/src/assets.rs` for existing cancel message handling via `service_tx` channel)

## Trait Implementations

### IsVolatile<E> Trait Extensions

**File:** `liquers-core/src/interpreter.rs` (lines 316-422)

**Current trait:**
```rust
pub(crate) trait IsVolatile<E: Environment> {
    async fn is_volatile(&self, env: EnvRef<E>) -> Result<bool, Error>;
}
```

**No changes to trait definition** - implementations already exist for:
- `ParameterValue`
- `ResolvedParameterValues`
- `Plan` (computes volatility on-demand)
- `Recipe` (checks `recipe.volatile` flag)
- `Query` (delegates to Plan)
- `Step` (checks command metadata, parameters, asset keys)

**New behavior:** Store computed volatility in Plan struct instead of computing on-demand

**Implementation change for Plan:**

```rust
impl<E: Environment> IsVolatile<E> for Plan {
    async fn is_volatile(&self, env: EnvRef<E>) -> Result<bool, Error> {
        // Return cached value - Plan.is_volatile is always set during plan building
        Ok(self.is_volatile)
    }
}
```

**Note:** Since `is_volatile` is required and always set during plan building, we simply return it

## Generic Parameters & Bounds

### No New Generic Types

All modifications use existing generic parameters:
- `Context<E: Environment>`
- `AssetData<E: Environment>`
- `IsVolatile<E: Environment>` trait

**Existing bounds are sufficient** - no new constraints needed

## Sync vs Async Decisions

| Function/Method | Async? | Rationale |
|----------------|--------|-----------|
| `MetadataRecord::is_volatile()` | No | Simple boolean check, no I/O |
| `Context::is_volatile()` | No | Returns cached field, no I/O |
| `Context::with_volatile()` | No | Clones context, no I/O |
| `AssetRef::to_override()` | Yes | Acquires async write lock on AssetData |
| `IsVolatile::is_volatile()` | Yes | Existing trait, already async (may evaluate assets) |
| `Plan.is_volatile` field access | No | Direct field access |

**No new async patterns needed** - all modifications follow existing async conventions

## Function Signatures

### Module: liquers_core::metadata

```rust
impl Status {
    pub fn has_data(&self) -> bool { /* updated to include Volatile */ }
    pub fn is_finished(&self) -> bool { /* updated to include Volatile */ }
    pub fn can_have_tracked_dependencies(&self) -> bool { /* updated to include Volatile */ }
}

impl MetadataRecord {
    pub fn is_volatile(&self) -> bool {
        self.is_volatile || self.status == Status::Volatile
    }
}
```

### Module: liquers_core::plan

```rust
impl Plan {
    /// Set the volatile flag (called during plan building)
    pub fn set_volatile(&mut self, is_volatile: bool) {
        self.is_volatile = is_volatile;
    }

    pub fn is_volatile(&self) -> bool {
        self.is_volatile
    }
}
```

### Module: liquers_core::context

```rust
impl<E: Environment> Context<E> {
    pub fn new(envref: EnvRef<E>, volatile: bool) -> Self { /* updated signature */ }

    pub fn is_volatile(&self) -> bool {
        self.volatile
    }

    pub fn with_volatile(&self, volatile: bool) -> Self {
        // Clone context, set volatile flag
    }
}
```

### Module: liquers_core::assets

```rust
impl<E: Environment> AssetRef<E> {
    pub async fn to_override(&self) -> Result<(), Error> {
        // Convert Volatile → Override status
    }
}

impl<E: Environment> AssetManager<E> {
    /// Get asset by key. For volatile assets, always returns NEW AssetRef.
    pub async fn get_asset(&self, key: &Key) -> Result<AssetRef<E>, Error> {
        // Check if recipe is volatile
        // If volatile: create new AssetRef, NEVER cache in internal maps
        // If non-volatile: use existing cache behavior
    }

    /// Get asset by query. For volatile queries, always returns NEW AssetRef.
    pub async fn get_asset_from_query(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        // Build plan, check if plan.is_volatile
        // If volatile: create new AssetRef, NEVER cache in internal maps
        // If non-volatile: use existing cache behavior
    }
}
```

### Module: liquers_core::plan (PlanBuilder)

```rust
impl PlanBuilder {
    // NEW FIELD:
    is_volatile: bool,  // Track volatility during plan building

    /// Add a step to the plan, checking for volatility
    fn add_step(&mut self, step: Step) {
        // If already volatile, skip volatility checks (optimization)
        if !self.is_volatile {
            match &step {
                Step::Action { name, .. } => {
                    // Check for 'v' instruction
                    if name == "v" {
                        self.mark_volatile(&format!("Volatile due to instruction 'v'"));
                    }
                    // Check if action's command is volatile via CommandMetadata
                    else if self.is_action_volatile(name) {
                        self.mark_volatile(&format!("Volatile due to command '{}'", name));
                    }
                }
                // ASSUMPTION: All referenced assets/resources are non-volatile during building
                // Step::Evaluate, Step::Plan dependencies checked AFTER build via find_dependencies
                Step::Evaluate(_) | Step::Plan(_) |
                Step::Info(_) | Step::WithKey(_) | Step::Resource(_) => {
                    // No volatility check during build
                }
            }
        }

        self.steps.push(step);
    }

    /// Mark plan as volatile and add explanatory Step::Info
    fn mark_volatile(&mut self, reason: &str) {
        self.is_volatile = true;
        self.steps.push(Step::Info(reason.to_string()));
    }

    /// Helper: check if action command is volatile via CommandMetadata
    fn is_action_volatile(&self, action_name: &str) -> bool {
        // Look up command in registry, check metadata.volatile flag
    }

    /// Build final plan with is_volatile field set (based on commands and 'v' instruction only)
    fn build(mut self) -> Result<Plan, Error> {
        let plan = Plan {
            steps: self.steps,
            is_volatile: self.is_volatile,
            // ... other fields
        };
        Ok(plan)
    }
}

/// Helper function: Find all asset dependencies of a plan (direct and indirect)
/// Returns Error if circular dependency detected
async fn find_dependencies<E: Environment>(
    envref: EnvRef<E>,
    plan: &Plan,
    stack: &mut Vec<Key>,  // Track dependency chain for cycle detection
) -> Result<HashSet<Key>, Error> {
    let mut dependencies = HashSet::new();

    for step in &plan.steps {
        match step {
            Step::Resource(key) | Step::WithKey(key) => {
                // Check for circular dependency
                if stack.contains(key) {
                    return Err(Error::general_error(
                        format!("Circular dependency detected: key {:?} appears in dependency chain", key)
                    ));
                }

                // Add to dependencies
                dependencies.insert(key.clone());

                // Push onto stack
                stack.push(key.clone());

                // Get recipe for this key (if it exists)
                if let Some(recipe) = envref.get_recipe(key).await? {
                    // Recursively find dependencies of this recipe
                    let recipe_plan = recipe_to_plan(&recipe, envref.clone())?;
                    let indirect_deps = find_dependencies(envref.clone(), &recipe_plan, stack).await?;
                    dependencies.extend(indirect_deps);
                }

                // Pop from stack
                stack.pop();
            }
            Step::Evaluate(query) => {
                // Convert query to plan, find its dependencies
                let eval_plan = query_to_plan(query, envref.clone())?;
                let query_deps = find_dependencies(envref.clone(), &eval_plan, stack).await?;
                dependencies.extend(query_deps);
            }
            Step::Plan(nested_plan) => {
                // Find dependencies of nested plan
                let nested_deps = find_dependencies(envref.clone(), nested_plan, stack).await?;
                dependencies.extend(nested_deps);
            }
            _ => {}
        }
    }

    Ok(dependencies)
}

/// Check if plan has volatile dependencies
async fn has_volatile_dependencies<E: Environment>(
    envref: EnvRef<E>,
    plan: &mut Plan,
) -> Result<bool, Error> {
    // Only check if plan is not already marked volatile
    if plan.is_volatile {
        return Ok(true);
    }

    // Find all dependencies
    let mut stack = Vec::new();
    let dependencies = find_dependencies(envref.clone(), plan, &mut stack).await?;

    // Check each dependency key for volatility
    for key in dependencies {
        if let Some(recipe) = envref.get_recipe(&key).await? {
            if recipe.volatile {
                plan.is_volatile = true;
                plan.steps.push(Step::Info(
                    format!("Volatile due to dependency on volatile key: {:?}", key)
                ));
                return Ok(true);
            }
        }
    }

    Ok(false)
}
```

**Two-Phase Volatility Check:**

**Phase 1 (during build):**
- Check commands via `CommandMetadata.volatile`
- Check for `v` instruction
- **Assume all referenced assets are non-volatile**
- Set `plan.is_volatile` based on these checks only

**Phase 2 (after build):**
- Call `find_dependencies()` to get all asset dependencies (Keys)
- Recursively resolve dependencies using recipes from environment
- Track dependency chain (`stack`) to detect circular dependencies
- Check each key's recipe for volatility
- If any dependency is volatile, update `plan.is_volatile` and add Step::Info

**Note:** The `v` instruction already uses existing action syntax - no parser changes needed.

### Module: liquers_core::interpreter

```rust
pub async fn make_plan<E: Environment, Q: TryToQuery>(
    envref: EnvRef<E>,
    query: Q,
) -> Result<Plan, Error> {
    let rquery = query.try_to_query();
    let cmr = envref.get_command_metadata_registry();
    let mut pb = PlanBuilder::new(rquery?, cmr);

    // Phase 1: Build plan, check commands and 'v' instruction
    let mut plan = pb.build()?;

    // Phase 2: Check asset dependencies for volatility
    has_volatile_dependencies(envref, &mut plan).await?;

    Ok(plan)
}

// Context initialization - pass volatility from plan
pub async fn evaluate_plan<E: Environment>(
    envref: EnvRef<E>,
    plan: &Plan,
) -> Result<State<E::Value>, Error> {
    // Initialize Context with is_volatile flag from plan
    let context = Context::new(envref.clone(), plan.is_volatile);
    // ... rest of evaluation
}
```

**Two-Phase Approach:**
- **Phase 1**: PlanBuilder computes `is_volatile` based on commands and 'v' instruction only
- **Phase 2**: `has_volatile_dependencies()` checks asset dependencies, updates `plan.is_volatile` if needed
- Context initialized with final `plan.is_volatile` value for evaluation

**Note:** `make_plan()` is now async (required for Phase 2 dependency checking)

## Integration Points

### Crate: liquers-core

**File:** `liquers-core/src/metadata.rs`
- Add `Status::Volatile` variant (line ~11-47)
- Update `Status::has_data()`, `is_finished()`, `can_have_tracked_dependencies()` methods
- Add `is_volatile: bool` field to `MetadataRecord` (line ~470-515)
- Add `MetadataRecord::is_volatile()` helper method

**File:** `liquers-core/src/plan.rs`
- Add `is_volatile: bool` field to `Plan` struct
- Add `Plan::is_volatile()` getter method
- Update `IsVolatile<E> for Plan` implementation to return cached field
- Add `is_volatile: bool` field to `PlanBuilder` struct
- Modify `PlanBuilder::add_step()` to check volatility for Step::Action only (commands and 'v' instruction)
- Add `PlanBuilder::mark_volatile(reason)` to set flag and add Step::Info
- Add helper method `is_action_volatile(action_name) -> bool`
- Modify `PlanBuilder::build()` to set `Plan.is_volatile` from builder state
- Add async function `find_dependencies<E>(envref, plan, stack) -> Result<HashSet<Key>, Error>`
  - Finds all direct and indirect asset dependencies (represented by Keys)
  - Takes `stack: &mut Vec<Key>` for circular dependency detection
  - Recursively resolves dependencies using recipes from environment
  - Returns error if circular dependency detected
- Add async function `has_volatile_dependencies<E>(envref, plan) -> Result<bool, Error>`
  - Calls `find_dependencies()` to get all dependencies
  - Checks each key's recipe for volatility
  - Updates `plan.is_volatile` and adds Step::Info if any dependency is volatile

**File:** `liquers-core/src/context.rs`
- Add `is_volatile: bool` field to `Context` struct
- Update `Context::new()` signature to accept `is_volatile` parameter
- Add `Context::is_volatile()` and `Context::with_volatile()` methods

**File:** `liquers-core/src/assets.rs`
- Add `is_volatile: bool` field to `AssetData` struct (line ~187)
- Modify `AssetManager::get_asset()` to check volatility and return new AssetRef for volatile assets
- Modify `AssetManager::get_asset_from_query()` similarly
- Add `AssetRef::to_override()` method

**File:** `liquers-core/src/interpreter.rs`
- Modify `make_plan()` to be async and call `has_volatile_dependencies()` after building plan
- Modify `evaluate_plan()` to initialize `Context` with `plan.is_volatile` flag
- Update `IsVolatile<E> for Plan` implementation to return `self.is_volatile`
- Audit all `make_plan()` call sites to add `.await` (breaking change)

### Dependencies

**No new dependencies** - all changes use existing Rust std library and liquers-core types

## Relevant Commands

### New Commands

**None** - This is infrastructure/architecture work, not command library work

### Relevant Existing Namespaces

**None specifically** - Volatility affects all commands that declare `CommandMetadata.volatile = true`

Examples of volatile commands (if they exist):
- `core/current_time` - returns current timestamp (always volatile)
- `core/random` - returns random value (always volatile)
- `core/uuid` - generates UUID (volatile if non-deterministic)

**User confirmation:** No command-specific work needed for this feature

## Web Endpoints

**No web endpoint changes** - Volatility is internal to asset management

Existing `/api/query/<query>` endpoint behavior unchanged. Volatility affects caching internally but not HTTP API surface.

## Error Handling

### Error Scenarios

| Scenario | Constructor | Example |
|----------|-------------|---------|
| Circular dependency in plan | `Error::general_error` | `Error::general_error(format!("Circular dependency detected in plan: query {:?} appears in chain", query))` |
| IsVolatile trait evaluation fails | Propagate via `?` | Already handled in existing implementations |

**Note:** `to_override()` currently always succeeds (returns `Result<(), Error>` to allow future error cases)

### Error Propagation

All error handling uses existing patterns:

```rust
// find_dependencies - detect and return error on circular dependencies
async fn find_dependencies<E: Environment>(
    envref: EnvRef<E>,
    plan: &Plan,
    stack: &mut Vec<Key>,
) -> Result<HashSet<Key>, Error> {
    // Check for circular dependency
    if stack.contains(key) {
        return Err(Error::general_error(
            format!("Circular dependency detected: key {:?} appears in dependency chain", key)
        ));
    }
    // ... find dependencies
}

// Propagate errors from dependency checking
has_volatile_dependencies(envref, &mut plan).await?;
```

**No unwrap/expect** - all error cases return `Result<T, Error>`

## Serialization Strategy

### MetadataRecord.is_volatile Field

```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MetadataRecord {
    // ... existing fields

    pub is_volatile: bool,  // Required field - always serialized
}
```

**Serialization:**
- `is_volatile` is always serialized and required during deserialization
- No backward compatibility needed at this stage

### Plan.is_volatile Field

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Plan {
    // ... existing fields

    pub is_volatile: bool,  // Required field - always serialized
}
```

**Serialization:**
- `is_volatile` is always serialized and required during deserialization
- No backward compatibility needed at this stage

### Status Enum

```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub enum Status {
    // ... existing variants
    Volatile,  // Serializes as "Volatile" string
}
```

**Serialization:**
- `Volatile` variant serializes/deserializes as string "Volatile"

### Non-Serializable Fields

**Context.is_volatile:** Not serialized (Context is runtime-only)

**AssetData.is_volatile:** Not serialized (AssetData is runtime-only, reconstructed from recipe)

## Concurrency Considerations

### Thread Safety

**All new fields are `bool` (Copy)** - no synchronization needed for reads

**AssetRef::to_override()** uses existing async RwLock pattern:
```rust
pub async fn to_override(&self) -> Result<(), Error> {
    let mut data = self.write().await;  // Exclusive lock
    // ... modify data.status
}
```

**AssetManager modifications** already use internal locking (scc concurrent map) - no additional synchronization needed

**No new shared mutable state introduced**

## Compilation Validation

### Expected to Compile

**Yes** - All changes are additive or extend existing patterns

### Potential Issues

**Match statement exhaustiveness:**
- All existing `match` statements on `Status` enum must add `Status::Volatile` arm
- Compiler will enforce this (no default arms allowed per CLAUDE.md)
- Estimated ~10-15 match sites across liquers-core

**Breaking changes:**
- `Status` enum adds variant (non-breaking for match without `_ =>` default arm)
- `MetadataRecord`, `Plan` structs add field (backward compatible via `#[serde(default)]`)
- `Context::new()` signature change (may require updating callers)

### Validation Commands

```bash
# Check compilation
cargo check -p liquers-core

# Run tests
cargo test -p liquers-core --lib

# Check for exhaustive match warnings
cargo clippy -p liquers-core -- -D warnings
```

## References to liquers-patterns.md

- [x] Crate dependencies: liquers-core only (correct - no cross-crate dependencies)
- [x] Error handling: Returns `Result` for all fallible operations
- [x] No unwrap/expect: All error cases return `Result`
- [x] Match statements: No default arms - compiler enforces Status::Volatile handling
- [x] Serialization: Required fields (no `#[serde(default)]`) for MetadataRecord.is_volatile and Plan.is_volatile
- [x] Async: Follows existing async patterns (AssetRef locks, IsVolatile trait)
- [x] No new ExtValue variants: This is metadata/infrastructure, not value types

## Resolved Design Decisions

### 1. AssetManager Caching Strategy ✅
**Decision:** Volatile assets should NEVER be cached in AssetManager's internal maps.

**Implementation:** `AssetManager::get_asset()` and `get_asset_from_query()` must check if recipe/query is volatile and always create a new AssetRef for volatile assets, bypassing the cache entirely.

### 2. Context Initialization ✅
**Decision:** Context is created by interpreter. Need to audit all `Context::new()` call sites.

**Action:** During implementation, audit all `Context::new()` call sites in `liquers-core/src/interpreter.rs` to ensure `is_volatile` parameter is passed correctly (from Plan.is_volatile or Recipe.volatile).

### 3. Two-Phase Volatility Check with Dependency Detection ✅
**Decision:** Use two-phase approach to avoid recursion during plan building. Recursive references can appear due to recipes. Must add error handling for circular dependencies.

**Implementation - Phase 1 (during build):**
- Check only commands via `CommandMetadata.volatile` and 'v' instruction
- **Assume all referenced assets are non-volatile**
- Set `plan.is_volatile` based on these checks only

**Implementation - Phase 2 (after build):**
- Call `find_dependencies(envref, plan, stack)` to find all asset dependencies (Keys)
- Recursively resolve dependencies using recipes from environment
- Track dependency chain via `stack: &mut Vec<Key>` to detect circular dependencies
- Return `Error::general_error("Circular dependency detected: key {:?} appears in dependency chain", key)` when cycle found
- Call `has_volatile_dependencies()` to check each key's recipe for volatility
- Update `plan.is_volatile` and add Step::Info if any dependency is volatile

### 4. Step::Info Usage ✅
**Decision:** ALWAYS add `Step::Info` when marking plan as volatile.

**Implementation:** `PlanBuilder::mark_volatile(reason)` always adds `Step::Info(reason)` to document why plan became volatile.

### 5. Volatile Status Transition ✅
**Decision:** Confirmed - assets must be CREATED as volatile. No transitions to Volatile status after creation.

**Rationale:** Volatility is determined upfront during plan building, before asset creation. No legitimate use case for transitioning to Volatile after creation identified.

### 6. Cancellation Mechanism ✅
**Decision:** Use existing cancellation mechanism already implemented in codebase.

**Implementation:** `AssetRef::to_override()` should use existing cancellation pattern via `service_tx` channel. Check `liquers-core/src/assets.rs` for existing cancel message handling.
