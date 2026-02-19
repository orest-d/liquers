# Phase 3: Advanced Scenario Examples - Volatility System

This document provides conceptual code examples demonstrating volatile dependency propagation, status transitions, and override usage in complex scenarios.

## Scenario 1: Volatile Dependency Propagation Chain

### Scenario Description

A query chain demonstrates how volatility "infects" dependent assets:
- Query A: `timestamp/add-child-query-B` - timestamps are volatile (time changes)
- Query B: `transform/add-child-query-C` - depends on volatile A
- Query C: `aggregate-results` - depends on volatile B

Each asset knows it's volatile upfront before evaluation begins. When a consumer requests Asset B, they get a fresh AssetRef even if B was created seconds ago, because B depends on volatile A.

### Conceptual Code Example

```rust
// Environment setup with volatile command
let env = SimpleEnvironment::new().await;
let cr = env.get_mut_command_registry();

// Register 'timestamp' command (volatile) and 'transform' command (non-volatile)
register_command!(cr,
    async fn timestamp(state) -> result
    volatile: true
    namespace: "core"
    label: "Get current timestamp"
)?;

register_command!(cr,
    async fn transform(state, multiplier: f64 = 2.0) -> result
    namespace: "data"
    label: "Transform value"
)?;

// Test case: Build query chain
let query_a = Query::parse("core/timestamp").unwrap();
let query_b = Query::parse("data/transform/q/core/timestamp").unwrap();
let query_c = Query::parse("data/aggregate/q/data/transform/q/core/timestamp").unwrap();

// Phase 1: Build plans - check commands only
let plan_a = make_plan(env.clone(), &query_a).await.unwrap();
let plan_b = make_plan(env.clone(), &query_b).await.unwrap();
let plan_c = make_plan(env.clone(), &query_c).await.unwrap();

// Verify Phase 1 results
assert!(plan_a.is_volatile);  // 'timestamp' command is volatile
assert!(!plan_b.is_volatile); // 'transform' command is non-volatile, no 'v' instruction
assert!(!plan_c.is_volatile); // 'aggregate' command is non-volatile, no 'v' instruction

// Phase 2: Check dependencies
let plan_b = make_plan(env.clone(), &query_b).await.unwrap();
let plan_c = make_plan(env.clone(), &query_c).await.unwrap();

// After Phase 2: Dependencies checked
// - plan_b depends on plan_a (volatile) → plan_b.is_volatile becomes true
// - plan_c depends on plan_b (volatile) → plan_c.is_volatile becomes true
assert!(plan_b.is_volatile);  // Now volatile due to dependency on 'timestamp'
assert!(plan_c.is_volatile);  // Now volatile due to dependency on volatile 'transform'

// Evaluate all assets
let asset_a = env.get_asset_from_query(&query_a).await.unwrap();
let asset_b = env.get_asset_from_query(&query_b).await.unwrap();
let asset_c = env.get_asset_from_query(&query_c).await.unwrap();

// Key insight: Each asset is created fresh because they all have volatile dependencies
// - asset_a is Volatile because 'timestamp' is volatile
// - asset_b is Volatile because it depends on volatile asset_a
// - asset_c is Volatile because it depends on volatile asset_b

// Metadata propagation
let meta_a = asset_a.get_metadata().await;
let meta_b = asset_b.get_metadata().await;
let meta_c = asset_c.get_metadata().await;

assert!(meta_a.status == Status::Volatile);
assert!(meta_a.is_volatile);

assert!(meta_b.status == Status::Volatile);
assert!(meta_b.is_volatile);  // Volatile due to dependency, not because command is volatile

assert!(meta_c.status == Status::Volatile);
assert!(meta_c.is_volatile);

// Requesting same query again gets a NEW AssetRef (not cached)
let asset_b_again = env.get_asset_from_query(&query_b).await.unwrap();
assert_ne!(asset_b.id(), asset_b_again.id());  // Different AssetRef objects
```

### Data Flow Diagram

```
Query A: core/timestamp
    ↓
Plan A: is_volatile=true (command 'timestamp' is volatile)
    ↓
Asset A: created with status=Volatile, is_volatile=true
    ↓
[Second request for Query A] → NEW AssetRef (never cached)

Query B: data/transform/q/core/timestamp
    ↓
Plan B (Phase 1): is_volatile=false (command 'transform' is not volatile)
    ↓
Plan B (Phase 2): finds dependency on Query A
    ├─ Query A → Plan A → is_volatile=true
    └─ Plan B.is_volatile becomes true (dependency is volatile)
    ↓
Asset B: created with status=Volatile, is_volatile=true
    ↓
[Second request for Query B] → NEW AssetRef (never cached)

Query C: data/aggregate/q/data/transform/q/core/timestamp
    ↓
Plan C (Phase 1): is_volatile=false (command 'aggregate' is not volatile)
    ↓
Plan C (Phase 2): finds dependencies on Query B
    ├─ Query B → Plan B → is_volatile=true
    └─ Plan C.is_volatile becomes true
    ↓
Asset C: created with status=Volatile, is_volatile=true
```

### Expected Behavior

1. **Upfront knowledge:** Volatility is fully determined before any asset evaluation begins
2. **Propagation:** Volatility propagates "upward" through the dependency chain:
   - Direct volatile commands make their assets volatile
   - Assets depending on volatile assets become volatile
3. **No caching:** Volatile assets are NEVER cached in AssetManager, ensuring each request triggers re-evaluation
4. **Metadata consistency:** Both `status` and `is_volatile` fields accurately reflect volatility at all stages

---

## Scenario 2: Mixed Volatile and Non-Volatile Dependencies

### Scenario Description

A more realistic scenario where a query has both volatile and non-volatile dependencies:
- Root asset: `combine-results` - takes two inputs
  - Input 1: `query-db/cached-query` - non-volatile (database query, cached)
  - Input 2: `core/random-number` - volatile (random values change each time)

The root asset should be volatile because one dependency is volatile. However, Input 1 can be cached independently since it's non-volatile.

### Conceptual Code Example

```rust
// Register commands
let cr = env.get_mut_command_registry();

register_command!(cr,
    async fn query_db(state) -> result
    namespace: "db"
    label: "Query database"
    // NOT volatile - database queries are deterministic for given inputs
)?;

register_command!(cr,
    async fn random_number(state) -> result
    volatile: true
    namespace: "core"
    label: "Generate random number"
)?;

register_command!(cr,
    async fn combine_results(state, left: Query, right: Query) -> result
    namespace: "data"
    label: "Combine two input queries"
)?;

// Build the query
// Root: combine-results with two subqueries
let query = Query::parse("data/combine-results/q/db/query-db/q/core/random-number").unwrap();

// Phase 1: Build plan
let mut plan = make_plan(env.clone(), &query).await.unwrap();
println!("After Phase 1: plan.is_volatile = {}", plan.is_volatile);
// Output: After Phase 1: plan.is_volatile = false
// Reasoning: Root command 'combine-results' is not volatile, no 'v' instruction

// Phase 2: Check dependencies
let dependencies = find_dependencies(env.clone(), &plan, &mut vec![]).await.unwrap();
println!("Dependencies: {:?}", dependencies);
// Output: Dependencies found for db/query-db and core/random-number

// Check each dependency for volatility
for dep_key in &dependencies {
    if let Some(recipe) = env.get_recipe(dep_key).await.unwrap() {
        println!("Dependency {:?}: volatile = {}", dep_key, recipe.volatile);
    }
}
// Output:
// Dependency Key("db/query-db"): volatile = false
// Dependency Key("core/random-number"): volatile = true

// Since 'core/random-number' is volatile, plan becomes volatile
assert!(plan.is_volatile);

// Now evaluate
let asset_root = env.get_asset_from_query(&query).await.unwrap();

// The root asset should be Volatile
let meta_root = asset_root.get_metadata().await;
assert_eq!(meta_root.status, Status::Volatile);
assert!(meta_root.is_volatile);

// But we can request the non-volatile dependency independently
let query_db_only = Query::parse("db/query-db").unwrap();
let asset_db = env.get_asset_from_query(&query_db_only).await.unwrap();
let meta_db = asset_db.get_metadata().await;
assert_eq!(meta_db.status, Status::Ready);  // Ready, not Volatile!
assert!(!meta_db.is_volatile);

// Requesting db/query-db multiple times returns CACHED AssetRef
let asset_db_2 = env.get_asset_from_query(&query_db_only).await.unwrap();
assert_eq!(asset_db.id(), asset_db_2.id());  // Same AssetRef (cached)

// But requesting the root query again gets NEW AssetRef
let asset_root_2 = env.get_asset_from_query(&query).await.unwrap();
assert_ne!(asset_root.id(), asset_root_2.id());  // Different AssetRef (not cached)
```

### Data Flow Diagram

```
Query: combine-results/q/db/query-db/q/core/random-number
    ↓
Plan (Phase 1):
  Root step: Action "combine-results" (non-volatile)
  Sub-step: Evaluate query "db/query-db"
  Sub-step: Evaluate query "core/random-number"
  Result: is_volatile=false (no volatile commands at this level)
    ↓
Plan (Phase 2 - Dependency Check):
  Dependencies: [db/query-db, core/random-number]
  Check db/query-db: recipe.volatile = false
  Check core/random-number: recipe.volatile = true
  ✓ Found volatile dependency!
  Result: is_volatile=true (propagated from dependency)
    ↓
Context created with is_volatile=true
    ↓
Asset Root created:
  Status::Volatile (created as volatile)
  metadata.is_volatile = true
  ✗ NOT cached in AssetManager
    ↓
Sub-evaluation: Query db/query-db
  Context created with is_volatile=false (non-volatile dependency)
  Asset DB created:
    Status::Ready (non-volatile)
    metadata.is_volatile = false
    ✓ Cached in AssetManager
    ↓
Sub-evaluation: Query core/random-number
  Context created with is_volatile=true (volatile command)
  Asset Random created:
    Status::Volatile
    metadata.is_volatile = true
    ✗ NOT cached in AssetManager
```

### Expected Behavior

1. **Selective caching:** Only non-volatile assets are cached; volatile assets are always created fresh
2. **Propagation rule:** Volatile status propagates "upward" - if any dependency is volatile, the dependent is volatile
3. **Independent subqueries:** Non-volatile subqueries can be requested independently and will be cached
4. **Context propagation:** When evaluating subqueries, context inherits volatility from parent (or can override if needed)

---

## Scenario 3: to_override() Status Transition and Cascading Effects

### Scenario Description

A volatile asset is being used but the consumer wants to "freeze" it - prevent further re-evaluation. Using `to_override()` converts the asset to Override status, effectively stopping the volatility propagation at that point.

This is useful when:
- A volatile asset has been generated and the consumer wants to "save" it
- A long-running volatile computation has completed; we want to use this specific result
- A recursive query needs to be terminated to prevent infinite re-evaluation

### Conceptual Code Example

```rust
// Setup: Volatile recursive query detection
let env = SimpleEnvironment::new().await;
let cr = env.get_mut_command_registry();

register_command!(cr,
    async fn current_time(state) -> result
    volatile: true
    namespace: "time"
    label: "Get current time"
)?;

register_command!(cr,
    async fn transform_time(state, format: String = "seconds") -> result
    namespace: "time"
    label: "Transform time value"
)?;

// Scenario 1: Converting in-progress volatile asset to Override
println!("=== Scenario 1: In-Progress Asset to Override ===");

let query = Query::parse("time/transform-time/q/time/current-time").unwrap();
let asset = env.get_asset_from_query(&query).await.unwrap();

// Asset is being evaluated (status might be Processing or Dependencies)
let meta_before = asset.get_metadata().await;
println!("Before to_override(): status = {:?}, is_volatile = {}",
    meta_before.status, meta_before.is_volatile);

// Call to_override()
asset.to_override().await.unwrap();

let meta_after = asset.get_metadata().await;
println!("After to_override(): status = {:?}, is_volatile = {}",
    meta_after.status, meta_after.is_volatile);
// Output:
// Before to_override(): status = Processing, is_volatile = true
// After to_override(): status = Override, is_volatile = true

// Key insight: Asset is still marked as volatile (was), but status is now Override
// This signals "this value should not be re-evaluated, period"

// Scenario 2: Converting completed volatile asset to Override
println!("\n=== Scenario 2: Ready Volatile Asset to Override ===");

// Wait for asset to complete
tokio::time::sleep(Duration::from_millis(100)).await;

let meta_ready = asset.get_metadata().await;
assert_eq!(meta_ready.status, Status::Volatile);
println!("Asset ready with status: {:?}", meta_ready.status);

// Now freeze it
asset.to_override().await.unwrap();

let meta_frozen = asset.get_metadata().await;
assert_eq!(meta_frozen.status, Status::Override);
println!("After to_override(): status changed to Override");

// Value is preserved
let value_before = /* get value from asset */;
asset.to_override().await.unwrap();
let value_after = /* get value from asset */;
assert_eq!(value_before, value_after);  // Value unchanged
println!("Value preserved: {:?}", value_after);

// Scenario 3: Cascading effects - what happens to dependents?
println!("\n=== Scenario 3: Cascading Effects ===");

// Original volatile asset
let asset_a = env.get_asset_from_query(&Query::parse("time/current-time").unwrap()).await.unwrap();

// Dependent query
let query_b = Query::parse("time/transform-time/q/time/current-time").unwrap();
let asset_b = env.get_asset_from_query(&query_b).await.unwrap();

println!("Asset A status: {:?}", asset_a.get_metadata().await.status);
println!("Asset B status: {:?}", asset_b.get_metadata().await.status);
// Output:
// Asset A status: Volatile
// Asset B status: Volatile (depends on volatile A)

// Convert A to Override
asset_a.to_override().await.unwrap();
println!("After converting A to Override:");
println!("Asset A status: {:?}", asset_a.get_metadata().await.status);
println!("Asset B status: {:?}", asset_b.get_metadata().await.status);
// Output:
// After converting A to Override:
// Asset A status: Override
// Asset B status: Volatile (still volatile! to_override does NOT cascade)

// Explanation: to_override() only affects the target asset's status
// Dependencies remain unchanged. If B wants to be overridden too, must call to_override() on B

// Scenario 4: Requesting overridden asset still gets same AssetRef
println!("\n=== Scenario 4: Overridden Asset Behavior ===");

asset_a.to_override().await.unwrap();

// Request same query again
let asset_a_again = env.get_asset_from_query(&Query::parse("time/current-time").unwrap()).await.unwrap();

println!("Original asset A ID: {}", asset_a.id());
println!("Re-requested asset ID: {}", asset_a_again.id());

// Are they the same? That depends on AssetManager caching strategy for Override status
// Current design: Volatile assets are NEVER cached
// Override is not Volatile, so it CAN be cached (manager's choice)
// To ensure fresh requests always get fresh assets: to_override() + no caching

// Scenario 5: Converting Error or Cancelled to Override
println!("\n=== Scenario 5: Failed Asset to Override ===");

// Simulate a failed asset
let query_bad = Query::parse("time/nonexistent-command").unwrap();
let asset_bad = env.get_asset_from_query(&query_bad).await.unwrap();

tokio::time::sleep(Duration::from_millis(100)).await;

let meta_error = asset_bad.get_metadata().await;
println!("Failed asset status: {:?}", meta_error.status);
// Output: Failed asset status: Error

// Convert Error to Override
asset_bad.to_override().await.unwrap();

let meta_override = asset_bad.get_metadata().await;
println!("After to_override() on Error: {:?}", meta_override.status);
// Output: After to_override() on Error: Override

// Error message is preserved in metadata
if let Some(error) = &meta_override.error_data {
    println!("Error still recorded: {:?}", error);
}

// Scenario 6: No-op conversions
println!("\n=== Scenario 6: No-op Conversions ===");

// Directory assets are not affected
let asset_dir = env.get_asset_from_query(&Query::parse("dir//").unwrap()).await.unwrap();
let meta_dir_before = asset_dir.get_metadata().await;
asset_dir.to_override().await.unwrap();
let meta_dir_after = asset_dir.get_metadata().await;
assert_eq!(meta_dir_before.status, meta_dir_after.status);
println!("Directory status unchanged by to_override()");

// Source assets are not affected
let asset_src = env.get_asset_from_query(&Query::parse("src//some-file").unwrap()).await.unwrap();
let meta_src_before = asset_src.get_metadata().await;
asset_src.to_override().await.unwrap();
let meta_src_after = asset_src.get_metadata().await;
assert_eq!(meta_src_before.status, meta_src_after.status);
println!("Source status unchanged by to_override()");

// Already Override - no-op
let asset_already = env.get_asset_from_query(&query).await.unwrap();
asset_already.to_override().await.unwrap();
let meta_first = asset_already.get_metadata().await;
assert_eq!(meta_first.status, Status::Override);
asset_already.to_override().await.unwrap();  // Call again
let meta_second = asset_already.get_metadata().await;
assert_eq!(meta_first.status, meta_second.status);
println!("Already-Override status unchanged by second to_override()");
```

### Data Flow Diagram

```
SCENARIO: Volatile Asset → Override Conversion

Before to_override():
  Asset: {status: Processing, is_volatile: true}
  AssetRef.read/write locks: working
  Metadata: {status: Processing, is_volatile: true}

During to_override():
  asset.to_override() called
    ↓
  Acquires write lock on AssetData
    ↓
  Match on current status:
    - Processing: Cancel in-flight, set value=none(), status=Override
    - Volatile/Ready: Keep value, set status=Override
    - Error: Keep error, set status=Override
    - Directory/Source: No change
    - Already Override: No change

After to_override():
  Asset: {status: Override, is_volatile: true (preserved)}
  Value: preserved or set to none() depending on prior status
  Metadata: {status: Override, is_volatile: true}
  Future requests: May or may not be cached (manager's choice for non-volatile status)

Key Points:
  ✓ Status changed from Volatile → Override
  ✓ is_volatile flag preserved (asset was volatile)
  ✓ Value preserved or initialized to none()
  ✓ Non-cascading: dependent assets unaffected
  ✗ Cannot transition back to Volatile
```

### Expected Behavior

1. **Status transition:** Any asset can be converted to Override status via `to_override()`
2. **Value preservation:** For states with data (Ready, Volatile, Partial), value is preserved; for in-progress states, value is set to `none()`
3. **Non-cascading:** Converting A to Override does NOT affect dependent assets that depend on A
4. **Termination semantics:** Override status signals "this value is frozen, do not re-evaluate"
5. **No-op cases:** Directory and Source assets are not affected; already-Override assets are idempotent
6. **Metadata consistency:** Both `status` and `error_data` are updated appropriately

---

## Key Insights Across Scenarios

### Volatility Propagation Rules

1. **Upward propagation:** Volatility flows from dependencies to dependents
   - If A is volatile and B depends on A, then B becomes volatile
   - If B was non-volatile, it is marked volatile only if any dependency is volatile

2. **Downward non-propagation:** Converting to Override does NOT affect dependents
   - If A becomes Override, B (which depends on A) is unaffected
   - Each asset must explicitly decide whether to be volatile or override

3. **Two-phase determination:** Volatility is determined upfront, never during execution
   - Phase 1: Check commands and 'v' instruction
   - Phase 2: Check asset dependencies
   - No dynamic volatility decisions

### AssetManager Caching Strategy

- **Volatile assets:** NEVER cached, always creates new AssetRef
- **Non-volatile assets:** Cached normally, returns existing AssetRef
- **Override assets:** Not cached (treated as non-volatile but frozen)

### Status::Volatile vs MetadataRecord.is_volatile

- **Status::Volatile:** Asset currently has a volatile value (Ready state with volatile marker)
- **MetadataRecord.is_volatile:** Asset will be or was volatile (even during in-flight states like Processing)
- **Both are true:** Asset is volatile throughout its lifecycle

### to_override() Use Cases

1. **Freeze volatile results:** Lock in a computed volatile value
2. **Stop re-evaluation:** Prevent further async computation
3. **Recursive termination:** Break dependency cycles by freezing intermediate results
4. **Error recovery:** Stop trying to re-evaluate failed assets

---

## Implementation Checklist for Phase 3

- [ ] Add `Status::Volatile` variant to Status enum
- [ ] Update all match statements on Status to handle Volatile (compiler will enforce)
- [ ] Add `is_volatile: bool` field to MetadataRecord with accessor method
- [ ] Add `is_volatile: bool` field to Plan struct
- [ ] Add `is_volatile: bool` field to Context struct with propagation logic
- [ ] Add `is_volatile: bool` field to AssetData struct
- [ ] Implement `AssetRef::to_override()` method with status transition logic
- [ ] Modify `PlanBuilder` to track volatility during plan building
- [ ] Add `find_dependencies()` function for recursive dependency resolution
- [ ] Add `has_volatile_dependencies()` function to check and propagate volatility
- [ ] Modify `make_plan()` to be async and call dependency checking
- [ ] Modify `evaluate_plan()` to initialize Context with Plan.is_volatile
- [ ] Modify AssetManager to check volatility and skip caching for volatile assets
- [ ] Add circular dependency detection with clear error messages
- [ ] Test all scenarios: dependency chains, mixed volatility, overrides, circular dependencies
