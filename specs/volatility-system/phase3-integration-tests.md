# Phase 3: Integration Test Specifications - Volatility System

**Document Status:** Integration test specifications for Phase 2 implementation

**Prepared for:** Phase 4 Implementation Plan

**Test Coverage:** Full pipeline, corner cases, performance, cross-module interactions

**Location:** `liquers-core/tests/volatility_integration.rs` (main test file)

---

## Overview

This document specifies comprehensive integration tests covering the complete volatility system as designed in Phase 2. Tests validate:
1. Full pipeline: Query → Plan → Asset → State with volatile status tracking
2. Volatility instruction (`v`) integration into plan building
3. Volatile dependency chain propagation
4. AssetManager caching behavior (volatile assets never cached)
5. Corner cases: memory, concurrency, serialization
6. Performance: dependency checking efficiency, circular dependency detection
7. Cross-module interactions: Interpreter → PlanBuilder → AssetManager

---

## Full Pipeline Tests

### Test 1: Simple Volatile Command Query to Asset

**Test name:** `test_volatile_query_to_asset_simple`

**Scenario:** Execute a query containing a volatile command (e.g., `current_time`), verify the entire pipeline marks the result as volatile.

**Components involved:**
- Parser (`parse_query`)
- PlanBuilder (Phase 1 volatility check via CommandMetadata)
- Interpreter (`make_plan`, `evaluate_plan`)
- Context (is_volatile flag propagation)
- AssetManager (volatile asset creation)
- MetadataRecord (is_volatile field)

**Execution flow:**
1. Register a volatile command: `fn current_time(_state: &State<Value>) -> Result<Value, Error> { ... }` with `volatile: true` in metadata
2. Parse query: `"current_time/to_string"`
3. Call `make_plan()` - Phase 1 check detects volatile command, sets `plan.is_volatile = true`, adds `Step::Info("Volatile due to command 'current_time'")`
4. Call `evaluate_plan()` - Context initialized with `is_volatile: true`
5. Command executes within volatile context
6. AssetManager creates new AssetRef for volatile asset
7. Asset completes with `Status::Volatile`
8. Verify metadata: `metadata.is_volatile() == true`, `metadata.status == Status::Volatile`

**Validation criteria:**
- Plan.is_volatile == true
- Step::Info present with reason string
- Context.is_volatile() == true during evaluation
- AssetRef returned from AssetManager is new (not cached)
- Final Status is Volatile
- MetadataRecord.is_volatile == true
- MetadataRecord.status == Status::Volatile
- Subsequent request for same query returns NEW AssetRef (not cached copy)

**File location:** `liquers-core/tests/volatility_integration.rs`

**Test code template:**
```rust
#[tokio::test]
async fn test_volatile_query_to_asset_simple() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // Register volatile command
    fn current_time(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("2026-02-17T10:00:00Z".to_string()))
    }

    let cr = &mut env.command_registry;
    // Register with volatile: true metadata
    register_command!(cr, fn current_time(state) -> result
        volatile: true
    )?;

    let envref = env.to_ref();

    // Make plan
    let plan = make_plan(envref.clone(), "current_time").await?;
    assert!(plan.is_volatile, "Plan should be marked volatile");

    // Verify Step::Info present
    let has_info_step = plan.steps.iter().any(|step| {
        matches!(step, Step::Info(msg) if msg.contains("Volatile"))
    });
    assert!(has_info_step, "Plan should contain Step::Info explaining volatility");

    // Evaluate plan
    let state1 = evaluate_plan(envref.clone(), &plan).await?;
    let metadata1 = state1.metadata();
    assert!(metadata1.is_volatile(), "Metadata should indicate volatility");
    assert_eq!(metadata1.status, Status::Volatile, "Status should be Volatile");

    // Get same query again - should return NEW AssetRef
    let state2 = evaluate_plan(envref.clone(), &plan).await?;
    // Both are valid volatile values but from different asset evaluations
    assert_eq!(state1.try_into_string()?, state2.try_into_string()?);

    Ok(())
}
```

---

### Test 2: Query with 'v' Instruction

**Test name:** `test_v_instruction_marks_plan_volatile`

**Scenario:** Execute a query with explicit `v` instruction (e.g., `"data/v/to_string"`). Verify volatility is marked despite command not being volatile.

**Components involved:**
- Parser (handles `v` as action instruction)
- PlanBuilder (Phase 1: detects `v` action, marks plan volatile)
- Interpreter (make_plan with volatility tracking)
- Step::Info (documents volatility source)

**Execution flow:**
1. Parse query: `"data/v/to_string"` where `data` produces non-volatile output and `to_string` is non-volatile command
2. Call `make_plan()` - Phase 1 detects Step::Action with name "v", calls `mark_volatile("Volatile due to instruction 'v'")`
3. Verify plan.is_volatile == true
4. Verify Step::Info contains reason
5. Verify plan can be executed with is_volatile context

**Validation criteria:**
- Plan.is_volatile == true
- Step::Info added explaining 'v' instruction
- Plan steps include exactly one Step::Action with name "v"
- Context initialized with is_volatile: true
- Result status is Volatile

**File location:** `liquers-core/tests/volatility_integration.rs`

**Test code template:**
```rust
#[tokio::test]
async fn test_v_instruction_marks_plan_volatile() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // Register non-volatile commands
    fn data(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("test-data"))
    }

    fn to_string(state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from(state.try_into_string()?))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn data(state) -> result)?;
    register_command!(cr, fn to_string(state) -> result)?;

    let envref = env.to_ref();

    // Make plan with 'v' instruction
    let plan = make_plan(envref.clone(), "data/v/to_string").await?;
    assert!(plan.is_volatile, "Plan should be marked volatile due to 'v' instruction");

    // Verify Step::Info
    let has_v_step_info = plan.steps.iter().any(|step| {
        matches!(step, Step::Info(msg) if msg.contains("'v'"))
    });
    assert!(has_v_step_info, "Plan should explain volatility from 'v' instruction");

    // Verify 'v' action is in plan
    let has_v_action = plan.steps.iter().any(|step| {
        matches!(step, Step::Action { action_name, .. } if action_name == "v")
    });
    assert!(has_v_action, "Plan should contain 'v' action step");

    Ok(())
}
```

---

### Test 3: Volatile Dependency Propagation

**Test name:** `test_volatile_dependency_chain_propagation`

**Scenario:** Query depends on a recipe marked as volatile. Verify volatility propagates through dependency chain.

**Components involved:**
- RecipeProvider (stores recipes with volatile flag)
- PlanBuilder (Phase 2: find_dependencies, has_volatile_dependencies)
- Interpreter (two-phase volatility check)
- Circular dependency detection (stack tracking)

**Execution flow:**
1. Register two recipes: `key1 = "data"` (volatile: false) and `key2 = "key1/transform"` (volatile: true)
2. Parse query: `"key2/final_command"` where final_command is non-volatile
3. Call `make_plan()`:
   - Phase 1: No volatile commands detected, is_volatile = false initially
   - Phase 2: `find_dependencies()` traverses plan, finds reference to key2
   - Retrieves recipe for key2, finds recipe.volatile == true
   - Calls `has_volatile_dependencies()`, returns true
   - Updates plan.is_volatile = true, adds Step::Info explaining dependency
4. Verify plan.is_volatile == true after Phase 2
5. Context initialized with is_volatile: true
6. Evaluation produces volatile result

**Validation criteria:**
- Phase 1: plan.is_volatile initially false
- Phase 2: dependency checking finds key2.recipe.volatile == true
- Phase 2: plan.is_volatile updated to true
- Step::Info added: "Volatile due to dependency on volatile key: ..."
- Context.is_volatile() == true during evaluation
- Final Status is Volatile
- No circular dependency errors

**File location:** `liquers-core/tests/volatility_integration.rs`

**Test code template:**
```rust
#[tokio::test]
async fn test_volatile_dependency_chain_propagation() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // Register commands
    fn data(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("data-value"))
    }

    fn transform(state: &State<Value>) -> Result<Value, Error> {
        let input = state.try_into_string()?;
        Ok(Value::from(format!("transformed-{}", input)))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn data(state) -> result)?;
    register_command!(cr, fn transform(state) -> result)?;

    // Create recipes
    let mut recipe_provider = env.get_mut_recipe_provider();

    let key1 = Key::new_root("data-key");
    let recipe1 = Recipe::new(key1.clone(), Query::from_string("data")?, false);
    recipe_provider.add_recipe(recipe1);

    let key2 = Key::new_root("volatile-key");
    let recipe2 = Recipe::new(key2.clone(), Query::from_string("data-key/transform")?, true);
    recipe_provider.add_recipe(recipe2);

    let envref = env.to_ref();

    // Make plan that depends on volatile-key
    let plan = make_plan(envref.clone(), "volatile-key").await?;
    assert!(plan.is_volatile, "Plan should be marked volatile from dependency");

    // Verify Step::Info documents dependency
    let has_dependency_info = plan.steps.iter().any(|step| {
        matches!(step, Step::Info(msg) if msg.contains("dependency"))
    });
    assert!(has_dependency_info, "Plan should explain volatility from dependency");

    Ok(())
}
```

---

### Test 4: AssetManager Never Caches Volatile Assets

**Test name:** `test_asset_manager_volatile_no_cache`

**Scenario:** Request same volatile query twice via AssetManager. Verify each request returns NEW AssetRef with unique IDs, not cached copies.

**Components involved:**
- AssetManager (get_asset, get_asset_from_query)
- Asset ID tracking
- Volatile detection logic

**Execution flow:**
1. Register volatile command: `random_value`
2. Parse query: `"random_value"`
3. Call `get_asset_from_query()` first time:
   - make_plan() determines plan.is_volatile == true
   - AssetManager detects volatility
   - Creates new AssetRef with asset_id = N
   - Does NOT add to internal cache
4. Call `get_asset_from_query()` second time (same query):
   - make_plan() determines plan.is_volatile == true
   - AssetManager detects volatility
   - Creates NEW AssetRef with asset_id = N+1 (different from first)
   - Does NOT add to internal cache
5. Verify both AssetRefs are valid and evaluate (potentially different values due to randomness)
6. Third request produces AssetRef with asset_id = N+2 (still new, not reusing N or N+1)

**Validation criteria:**
- First AssetRef.id != Second AssetRef.id != Third AssetRef.id
- Each AssetRef is fresh (not retrieved from internal map)
- Non-volatile queries DO return same cached AssetRef
- Volatile asset evaluation succeeds for all three requests

**File location:** `liquers-core/tests/volatility_integration.rs`

**Test code template:**
```rust
#[tokio::test]
async fn test_asset_manager_volatile_no_cache() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // Register volatile command
    static mut COUNTER: u64 = 0;
    fn random_value(_state: &State<Value>) -> Result<Value, Error> {
        unsafe {
            COUNTER += 1;
            Ok(Value::from(format!("random-{}", COUNTER)))
        }
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn random_value(state) -> result
        volatile: true
    )?;

    let envref = env.to_ref();
    let asset_manager = envref.get_asset_manager();

    // Request volatile query 3 times
    let query = Query::from_string("random_value")?;

    let asset1 = asset_manager.get_asset_from_query(&query).await?;
    let id1 = asset1.id();

    let asset2 = asset_manager.get_asset_from_query(&query).await?;
    let id2 = asset2.id();

    let asset3 = asset_manager.get_asset_from_query(&query).await?;
    let id3 = asset3.id();

    // All three should have different IDs (not cached)
    assert_ne!(id1, id2, "First and second request should return different AssetRef");
    assert_ne!(id2, id3, "Second and third request should return different AssetRef");
    assert_ne!(id1, id3, "First and third request should return different AssetRef");

    Ok(())
}
```

---

## Volatility Instruction Tests

### Test 5: 'v' Instruction at Various Positions

**Test name:** `test_v_instruction_position_variations`

**Scenario:** Place 'v' instruction at different positions in query (beginning, middle, end). Verify volatility is detected in all cases.

**Components involved:**
- Parser (handles 'v' at any position)
- PlanBuilder (Phase 1 volatility check per Step::Action)

**Test cases:**
- `"v"` alone - should mark plan volatile
- `"data/v"` - v after first command
- `"data/v/to_string"` - v in middle
- `"data/command/v"` - v at end

**Validation criteria:**
- All variants have plan.is_volatile == true
- All variants have at least one Step::Info with "Volatile due to instruction 'v'"
- All plans execute successfully with volatile context

**File location:** `liquers-core/tests/volatility_integration.rs`

**Test code template:**
```rust
#[tokio::test]
async fn test_v_instruction_position_variations() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    fn data(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("data"))
    }

    fn to_string(state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from(state.try_into_string()?))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn data(state) -> result)?;
    register_command!(cr, fn to_string(state) -> result)?;

    let envref = env.to_ref();

    let test_queries = vec![
        "v",
        "data/v",
        "data/v/to_string",
        "data/to_string/v",
    ];

    for query_str in test_queries {
        let plan = make_plan(envref.clone(), query_str).await?;
        assert!(plan.is_volatile, "Query '{}' should have volatile plan", query_str);

        let has_info = plan.steps.iter().any(|step| {
            matches!(step, Step::Info(msg) if msg.contains("'v'"))
        });
        assert!(has_info, "Query '{}' should have Step::Info for 'v' instruction", query_str);
    }

    Ok(())
}
```

---

## Circular Dependency Detection Tests

### Test 6: Direct Circular Dependency Detection

**Test name:** `test_circular_dependency_direct_detection`

**Scenario:** Create two recipes where A depends on B and B depends on A. Verify make_plan() detects cycle and returns Error.

**Components involved:**
- find_dependencies() (stack-based cycle detection)
- has_volatile_dependencies() (calls find_dependencies)
- Phase 2 of make_plan()

**Execution flow:**
1. Create recipe A: `key_a = Query("key_b/transform")`
2. Create recipe B: `key_b = Query("key_a/transform")`
3. Parse query: `"key_a/final_command"`
4. Call `make_plan()`:
   - Phase 1: no volatile commands, is_volatile = false
   - Phase 2: `find_dependencies()` starts traversal
     - Finds Step::WithKey(key_a) or similar resource reference
     - Pushes key_a onto stack: stack = [key_a]
     - Gets recipe_a, finds it references key_b
     - Recursively processes key_b
     - Pushes key_b onto stack: stack = [key_a, key_b]
     - Gets recipe_b, finds it references key_a
     - Attempts to push key_a onto stack
     - **Detects key_a already in stack** - circular!
     - Returns Error::general_error("Circular dependency detected: key key_a appears in dependency chain")
5. Verify error is returned and plan is not created

**Validation criteria:**
- make_plan() returns Err (not Ok)
- Error message contains "Circular dependency detected"
- Error message mentions the key name (e.g., "key_a")
- Error type is ErrorType::General

**File location:** `liquers-core/tests/volatility_integration.rs`

**Test code template:**
```rust
#[tokio::test]
async fn test_circular_dependency_direct_detection() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    fn transform(state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from(state.try_into_string()?))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn transform(state) -> result)?;

    // Create circular recipes
    let key_a = Key::new_root("key-a");
    let key_b = Key::new_root("key-b");

    let mut recipe_provider = env.get_mut_recipe_provider();

    // Recipe A depends on B
    let recipe_a = Recipe::new(key_a.clone(), Query::from_string("key-b/transform")?, false);
    recipe_provider.add_recipe(recipe_a);

    // Recipe B depends on A - circular!
    let recipe_b = Recipe::new(key_b.clone(), Query::from_string("key-a/transform")?, false);
    recipe_provider.add_recipe(recipe_b);

    let envref = env.to_ref();

    // Attempt to make plan - should detect circular dependency
    let result = make_plan(envref.clone(), "key-a").await;

    assert!(result.is_err(), "make_plan should return error for circular dependency");
    let error = result.unwrap_err();
    let error_msg = error.message();
    assert!(error_msg.contains("Circular dependency"),
        "Error should mention circular dependency, got: {}", error_msg);

    Ok(())
}
```

---

### Test 7: Indirect Circular Dependency (Chain > 2)

**Test name:** `test_circular_dependency_indirect_chain`

**Scenario:** Create three recipes forming a cycle: A → B → C → A. Verify detection works for longer chains.

**Components involved:**
- find_dependencies() (stack-based cycle detection with arbitrary depth)
- Recursive dependency resolution

**Execution flow:**
1. Create recipes:
   - Recipe A (key_a): `Query("key_b/transform")`
   - Recipe B (key_b): `Query("key_c/transform")`
   - Recipe C (key_c): `Query("key_a/transform")`
2. Parse query: `"key_a"`
3. Call `make_plan()` → Phase 2 → `find_dependencies()`:
   - stack = []
   - Push key_a: stack = [key_a]
   - Recursively process key_b: stack = [key_a, key_b]
   - Recursively process key_c: stack = [key_a, key_b, key_c]
   - Recursively process key_a: **stack.contains(key_a) = true** - cycle detected!
   - Return Error
4. Verify error is returned

**Validation criteria:**
- make_plan() returns Error
- Error message contains "Circular dependency"
- Error does not panic or hang (timeout safe)

**File location:** `liquers-core/tests/volatility_integration.rs`

**Test code template:**
```rust
#[tokio::test]
async fn test_circular_dependency_indirect_chain() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    fn transform(state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from(state.try_into_string()?))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn transform(state) -> result)?;

    // Create indirect circular recipes: A -> B -> C -> A
    let key_a = Key::new_root("key-a");
    let key_b = Key::new_root("key-b");
    let key_c = Key::new_root("key-c");

    let mut recipe_provider = env.get_mut_recipe_provider();

    let recipe_a = Recipe::new(key_a.clone(), Query::from_string("key-b/transform")?, false);
    recipe_provider.add_recipe(recipe_a);

    let recipe_b = Recipe::new(key_b.clone(), Query::from_string("key-c/transform")?, false);
    recipe_provider.add_recipe(recipe_b);

    let recipe_c = Recipe::new(key_c.clone(), Query::from_string("key-a/transform")?, false);
    recipe_provider.add_recipe(recipe_c);

    let envref = env.to_ref();

    // Should detect circular dependency
    let result = make_plan(envref.clone(), "key-a").await;
    assert!(result.is_err(), "Should detect circular dependency in A -> B -> C -> A chain");

    Ok(())
}
```

---

### Test 8: Self-Referencing Recipe

**Test name:** `test_circular_dependency_self_reference`

**Scenario:** Recipe A depends on itself. Verify immediate cycle detection.

**Components involved:**
- find_dependencies() (detects self-reference on first recursion)

**Execution flow:**
1. Create recipe: `key_a = Query("key_a/transform")`
2. Call `make_plan("key_a")`:
   - Phase 2: push key_a, stack = [key_a]
   - Get recipe_a
   - Find it references key_a
   - Attempt to push key_a again
   - Detect key_a in stack - immediate cycle
3. Return Error

**Validation criteria:**
- Error returned immediately (no infinite recursion)
- Error message mentions circular dependency
- Response time < 100ms (performance check)

**File location:** `liquers-core/tests/volatility_integration.rs`

---

## Serialization Round-Trip Tests

### Test 9: Plan.is_volatile Serialization Round-Trip

**Test name:** `test_plan_is_volatile_serialization_roundtrip`

**Scenario:** Create plan with is_volatile = true, serialize to JSON, deserialize, verify field preserved.

**Components involved:**
- Plan struct serialization (serde)
- JSON encoding/decoding
- is_volatile field preservation

**Execution flow:**
1. Create plan via make_plan(): `plan.is_volatile = true`
2. Serialize to JSON: `json_str = serde_json::to_string(&plan)?`
3. Verify JSON contains `"is_volatile": true`
4. Deserialize from JSON: `plan2 = serde_json::from_str(&json_str)?`
5. Verify `plan2.is_volatile == true`
6. Verify all other fields match (steps, etc.)

**Validation criteria:**
- Serialized JSON includes is_volatile field
- Deserialized plan has is_volatile == true
- Field value matches exactly
- No data loss in round-trip

**File location:** `liquers-core/tests/volatility_integration.rs`

**Test code template:**
```rust
#[tokio::test]
async fn test_plan_is_volatile_serialization_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    fn current_time(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("2026-02-17T10:00:00Z"))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn current_time(state) -> result
        volatile: true
    )?;

    let envref = env.to_ref();

    // Create volatile plan
    let plan1 = make_plan(envref.clone(), "current_time").await?;
    assert!(plan1.is_volatile);

    // Serialize
    let json = serde_json::to_string(&plan1)?;
    assert!(json.contains("\"is_volatile\":true"), "JSON should contain is_volatile field");

    // Deserialize
    let plan2: Plan = serde_json::from_str(&json)?;
    assert_eq!(plan2.is_volatile, plan1.is_volatile, "Deserialized plan should have same is_volatile");
    assert_eq!(plan2.steps.len(), plan1.steps.len(), "Steps should match");

    Ok(())
}
```

---

### Test 10: MetadataRecord.is_volatile Serialization

**Test name:** `test_metadata_record_is_volatile_serialization`

**Scenario:** Create metadata with is_volatile = true, serialize/deserialize, verify preservation. Test both Status::Volatile and Status::Ready with is_volatile flag.

**Components involved:**
- MetadataRecord struct
- is_volatile field
- status enum variants
- JSON serialization

**Validation criteria:**
- is_volatile field always serialized (no default)
- Both Status::Volatile and Status::Ready can have is_volatile = true
- Round-trip preserves exact values
- MetadataRecord::is_volatile() helper returns correct combined result

**File location:** `liquers-core/tests/volatility_integration.rs`

**Test code template:**
```rust
#[tokio::test]
async fn test_metadata_record_is_volatile_serialization() -> Result<(), Box<dyn std::error::Error>> {
    // Test Status::Volatile with is_volatile = true
    let mut metadata1 = MetadataRecord::new(Query::from_string("test")?, None);
    metadata1.status = Status::Volatile;
    metadata1.is_volatile = true;

    let json1 = serde_json::to_string(&metadata1)?;
    let metadata1_restored: MetadataRecord = serde_json::from_str(&json1)?;
    assert_eq!(metadata1_restored.status, Status::Volatile);
    assert_eq!(metadata1_restored.is_volatile, true);
    assert!(metadata1_restored.is_volatile()); // Helper method

    // Test Status::Ready with is_volatile = true (in-flight asset becoming volatile)
    let mut metadata2 = MetadataRecord::new(Query::from_string("test")?, None);
    metadata2.status = Status::Dependencies;
    metadata2.is_volatile = true; // Will be volatile when ready

    let json2 = serde_json::to_string(&metadata2)?;
    let metadata2_restored: MetadataRecord = serde_json::from_str(&json2)?;
    assert_eq!(metadata2_restored.status, Status::Dependencies);
    assert_eq!(metadata2_restored.is_volatile, true);
    assert!(metadata2_restored.is_volatile()); // Helper includes flag check

    Ok(())
}
```

---

### Test 11: Status Enum with Volatile Variant

**Test name:** `test_status_volatile_variant_serialization`

**Scenario:** Verify Status::Volatile enum variant serializes/deserializes correctly and is included in status checks.

**Components involved:**
- Status enum (new Volatile variant)
- Serialization (serde derives)
- has_data(), is_finished(), can_have_tracked_dependencies() methods

**Validation criteria:**
- Serializes to "Volatile" string
- Deserializes from "Volatile" string
- Status::Volatile.has_data() == true
- Status::Volatile.is_finished() == true
- Status::Volatile.can_have_tracked_dependencies() == false
- Explicit match arm compiles (no default arm)

**File location:** `liquers-core/tests/volatility_integration.rs`

**Test code template:**
```rust
#[test]
fn test_status_volatile_variant() {
    let status = Status::Volatile;

    // Serialization
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"Volatile\"");

    // Deserialization
    let restored: Status = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, Status::Volatile);

    // Behavior checks
    assert!(status.has_data(), "Volatile should have data");
    assert!(status.is_finished(), "Volatile should be finished");
    assert!(!status.can_have_tracked_dependencies(),
        "Volatile should not track dependencies (like Expired)");
}
```

---

## Concurrency Tests

### Test 12: Multiple Threads Requesting Same Volatile Asset

**Test name:** `test_concurrent_volatile_asset_requests`

**Scenario:** Multiple tokio tasks concurrently request same volatile query. Verify each gets independent AssetRef with unique IDs.

**Components involved:**
- AssetManager (thread-safe concurrent map)
- Volatile asset detection
- tokio spawn/join

**Execution flow:**
1. Register volatile command (e.g., timestamp-based random)
2. Spawn 5 concurrent tasks, each calling `asset_manager.get_asset_from_query(&query)`
3. Collect all AssetRef IDs
4. Verify all 5 IDs are unique (no caching/sharing)
5. Verify all 5 assets evaluate successfully

**Validation criteria:**
- All 5 AssetRef.id values are unique
- No two tasks get same cached AssetRef
- All assets complete evaluation
- Execution completes without deadlock/race conditions
- Timing: test completes within reasonable time (< 5 seconds)

**File location:** `liquers-core/tests/volatility_integration.rs`

**Test code template:**
```rust
#[tokio::test]
async fn test_concurrent_volatile_asset_requests() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    static mut COUNTER: u64 = 0;
    fn volatile_cmd(_state: &State<Value>) -> Result<Value, Error> {
        unsafe {
            COUNTER += 1;
            Ok(Value::from(COUNTER.to_string()))
        }
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn volatile_cmd(state) -> result
        volatile: true
    )?;

    let envref = Arc::new(env.to_ref());
    let asset_manager = envref.get_asset_manager();
    let query = Query::from_string("volatile_cmd")?;

    // Spawn 5 concurrent tasks
    let mut handles = vec![];
    for _ in 0..5 {
        let am = asset_manager.clone();
        let q = query.clone();
        let handle = tokio::spawn(async move {
            let asset = am.get_asset_from_query(&q).await?;
            Ok::<u64, Box<dyn std::error::Error>>(asset.id())
        });
        handles.push(handle);
    }

    // Collect IDs
    let mut ids = vec![];
    for handle in handles {
        let id = handle.await??;
        ids.push(id);
    }

    // Verify all unique
    let mut unique_ids = ids.clone();
    unique_ids.sort();
    unique_ids.dedup();
    assert_eq!(unique_ids.len(), 5, "All 5 AssetRef IDs should be unique");

    Ok(())
}
```

---

### Test 13: Concurrent Volatile and Non-Volatile Requests

**Test name:** `test_concurrent_volatile_and_nonvolatile_requests`

**Scenario:** Multiple tasks request volatile and non-volatile queries concurrently. Verify volatile always creates new, non-volatile uses cache appropriately.

**Components involved:**
- AssetManager mixed request handling
- Caching vs. non-caching behavior
- Concurrent access

**Execution flow:**
1. Register both volatile and non-volatile commands
2. Spawn tasks alternating between volatile and non-volatile queries
3. Volatile queries (expected IDs: 1, 3, 5) should all be different
4. Non-volatile queries (expected IDs: 2, 4) should share cached AssetRef (same ID)
5. Collect and verify pattern

**Validation criteria:**
- Volatile request IDs all unique
- Non-volatile request IDs identical (cached)
- No race conditions
- Proper isolation between volatile/non-volatile request types

**File location:** `liquers-core/tests/volatility_integration.rs`

---

## Cross-Module Integration Tests

### Test 14: Interpreter → PlanBuilder → Context Pipeline

**Test name:** `test_interpreter_plan_builder_context_pipeline`

**Scenario:** Full integration of interpreter, plan builder, and context initialization with volatile commands.

**Components involved:**
- Interpreter (make_plan, evaluate_plan)
- PlanBuilder (Phase 1 & 2 volatility detection)
- Context initialization (is_volatile flag propagation)
- Command execution

**Execution flow:**
1. Setup environment with volatile and non-volatile commands
2. Parse three queries:
   - "non_volatile_cmd/to_string" (non-volatile)
   - "volatile_cmd/to_string" (volatile)
   - "non_volatile_cmd/v/to_string" (volatile via 'v')
3. For each query:
   - Call `make_plan()` - evaluates volatility
   - Verify plan.is_volatile correct
   - Call `evaluate_plan()` - initializes Context
   - Verify Context.is_volatile() matches plan.is_volatile
   - Verify command executes with correct context
4. Verify metadata reflects volatility

**Validation criteria:**
- Plan volatility correctly detected for all three cases
- Context initialized with correct is_volatile value
- Context volatility propagates to nested evaluate() calls if any
- Metadata reflects final volatility status

**File location:** `liquers-core/tests/volatility_integration.rs`

---

### Test 15: AssetManager → Interpreter → Query Evaluation Flow

**Test name:** `test_asset_manager_interpreter_query_flow`

**Scenario:** Complete flow from query string through AssetManager and Interpreter to final volatile asset.

**Components involved:**
- Query parsing
- Interpreter (make_plan)
- AssetManager (volatile detection, new AssetRef creation)
- Asset evaluation
- Metadata tracking

**Execution flow:**
1. Parse query: `"query_string"`
2. Call `asset_manager.get_asset_from_query()`:
   - make_plan() called internally
   - Volatility determined
   - If volatile: new AssetRef created
   - If non-volatile: cached AssetRef returned/created
3. Await asset completion
4. Verify Status and MetadataRecord
5. Call same method again - verify caching behavior

**Validation criteria:**
- AssetManager correctly uses make_plan() result
- Volatile assets bypass cache
- Non-volatile assets use cache
- Metadata updated correctly
- No double-evaluation of non-volatile assets

**File location:** `liquers-core/tests/volatility_integration.rs`

---

## Performance Tests

### Test 16: Dependency Checking Performance (Linear Chain)

**Test name:** `test_dependency_checking_performance_linear_chain`

**Scenario:** Create a long linear dependency chain (e.g., 100 recipes deep: A → B → C → ... → Z). Measure performance of dependency checking.

**Components involved:**
- find_dependencies() (recursive traversal)
- Stack-based cycle detection
- Recipe lookup performance

**Execution flow:**
1. Create 100 recipes forming linear chain: key_0 → key_1 → ... → key_99
2. Start performance timer
3. Call `make_plan("key_0")` which triggers Phase 2 dependency checking
4. Stop timer
5. Verify:
   - No circular dependency error
   - plan.is_volatile correct (depends on final recipe)
   - Execution time < 500ms (threshold for acceptable performance)

**Validation criteria:**
- Completes without error
- Time < 500ms (no exponential blowup)
- No excessive memory allocation
- No stack overflow

**File location:** `liquers-core/tests/volatility_integration.rs`

**Test code template:**
```rust
#[tokio::test]
async fn test_dependency_checking_performance_linear_chain() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    fn dummy(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("dummy"))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn dummy(state) -> result)?;

    // Create 100-deep linear chain
    let mut recipe_provider = env.get_mut_recipe_provider();
    for i in 0..100 {
        let key = Key::new_root(&format!("key-{}", i));
        let next_query = if i == 0 {
            "dummy".to_string()
        } else {
            format!("key-{}/dummy", i - 1)
        };
        let recipe = Recipe::new(key, Query::from_string(&next_query)?, false);
        recipe_provider.add_recipe(recipe);
    }

    let envref = env.to_ref();

    // Time the dependency checking
    let start = std::time::Instant::now();
    let plan = make_plan(envref, "key-99").await?;
    let elapsed = start.elapsed();

    assert!(!plan.is_volatile); // Linear chain of non-volatile recipes
    assert!(elapsed.as_millis() < 500,
        "Dependency checking should complete in < 500ms, took {:?}", elapsed);

    Ok(())
}
```

---

### Test 17: Circular Dependency Detection Performance

**Test name:** `test_circular_dependency_detection_performance`

**Scenario:** Create complex dependency graph with cycles. Measure that cycle detection completes quickly without exponential blowup.

**Components involved:**
- find_dependencies() (stack-based cycle detection)
- Early termination on cycle found

**Execution flow:**
1. Create dependency graph with 50 recipes where some form cycles
2. Create query that eventually hits a cycle
3. Start performance timer
4. Call `make_plan()` - Phase 2 should detect cycle quickly
5. Stop timer
6. Verify error returned within 100ms

**Validation criteria:**
- Error returned (circular dependency detected)
- Time < 100ms (early termination working)
- No exponential time complexity

**File location:** `liquers-core/tests/volatility_integration.rs`

---

## Corner Case Tests

### Test 18: Very Large Dependency Chain with Volatile at Root

**Test name:** `test_large_dependency_chain_volatile_root`

**Scenario:** 1000-deep dependency chain where the root is volatile. Verify volatility propagates to entire chain without performance degradation.

**Components involved:**
- Dependency traversal
- Volatility propagation
- Performance under deep nesting

**Execution flow:**
1. Create 1000-level deep dependency chain
2. Mark root recipe as volatile: true
3. Call `make_plan()` on leaf
4. Phase 2 traversal should find volatile root
5. Verify plan.is_volatile == true
6. Verify completion time acceptable

**Validation criteria:**
- plan.is_volatile == true
- Execution time < 1000ms
- No stack overflow
- No memory exhaustion

**File location:** `liquers-core/tests/volatility_integration.rs`

---

### Test 19: Many Volatile Commands in Single Plan

**Test name:** `test_many_volatile_commands_single_plan`

**Scenario:** Create plan with 10 volatile commands chained together. Verify each contributes to volatility and context propagates correctly.

**Components involved:**
- PlanBuilder (Phase 1 checks each action)
- Multiple Step::Info entries
- Context initialization

**Execution flow:**
1. Register 10 commands, all marked volatile: true
2. Build query: `"vol_cmd1/vol_cmd2/.../vol_cmd10"`
3. Call `make_plan()`
4. Verify:
   - plan.is_volatile == true (detected on first command)
   - Multiple Step::Info entries (or at least one)
   - All 10 Step::Action items in plan steps
5. Context initialized with is_volatile: true

**Validation criteria:**
- plan.is_volatile == true
- Plan contains all 10 actions
- Step::Info present
- No performance degradation from multiple volatile checks

**File location:** `liquers-core/tests/volatility_integration.rs`

---

### Test 20: Volatile Status State Transitions

**Test name:** `test_volatile_status_state_transitions`

**Scenario:** Verify Status::Volatile can transition to Override via AssetRef::to_override(), and validate other transition rules.

**Components involved:**
- Status enum (Volatile variant)
- AssetRef::to_override() method
- Status transition logic

**Execution flow:**
1. Create asset with Status::Volatile
2. Call `asset_ref.to_override()`
3. Verify Status changed to Override
4. Test other transitions:
   - Ready → Override (data preserved)
   - Volatile → Override (data preserved)
   - Expired → Override (data preserved)
5. Verify Status::Volatile cannot transition to other states (only Override allowed)

**Validation criteria:**
- Volatile → Override succeeds
- Override → Volatile not allowed (manual check)
- Data preserved through transition
- Other status transitions unaffected

**File location:** `liquers-core/tests/volatility_integration.rs`

---

## Error Handling Tests

### Test 21: Error Propagation from Volatile Command Execution

**Test name:** `test_error_propagation_volatile_command`

**Scenario:** Volatile command throws error during execution. Verify error propagates correctly with volatile metadata.

**Components involved:**
- Command error handling
- MetadataRecord.error_data field
- Status::Error with is_volatile

**Execution flow:**
1. Register volatile command that returns Err
2. Execute query
3. Verify:
   - Status == Error
   - is_volatile == true (should be set before execution)
   - error_data populated
4. Metadata should reflect both error state and volatility

**Validation criteria:**
- Error returned (not panicked)
- MetadataRecord.is_volatile == true
- MetadataRecord.status == Status::Error
- error_data contains error message

**File location:** `liquers-core/tests/volatility_integration.rs`

---

### Test 22: Volatile Dependency Error (Recipe Not Found)

**Test name:** `test_volatile_dependency_error_recipe_not_found`

**Scenario:** Query depends on missing recipe. Verify error handling during Phase 2 dependency checking.

**Components involved:**
- find_dependencies() (recipe lookup)
- Error handling when recipe not found
- Phase 2 error propagation

**Execution flow:**
1. Create plan that references non-existent recipe key
2. Call `make_plan()`
3. Phase 2: `find_dependencies()` attempts to look up recipe
4. Recipe not found - should return error or skip (depends on implementation)
5. Either returns error OR continues without finding volatility
6. Verify no panic

**Validation criteria:**
- Handles missing recipe gracefully
- No panic or unwrap
- Either returns error or completes successfully
- Behavior documented in implementation

**File location:** `liquers-core/tests/volatility_integration.rs`

---

## Full Pipeline Integration Test

### Test 23: End-to-End Volatile Query Execution with Results

**Test name:** `test_end_to_end_volatile_query_execution`

**Scenario:** Complete end-to-end test simulating production usage: register commands, create recipes, build queries, execute with full evaluation, verify results are volatile.

**Components involved:**
- All layers: Parser, PlanBuilder, Interpreter, AssetManager, Context, Metadata
- Command execution
- Asset completion
- Result retrieval

**Execution flow:**
1. Setup environment with:
   - Non-volatile: `data_source` (returns hardcoded data)
   - Non-volatile: `transform` (transforms input)
   - Volatile: `current_timestamp` (returns current time)
2. Create recipes:
   - Recipe A: `data_source/transform` (non-volatile)
   - Recipe B: `current_timestamp` (volatile)
3. Execute three queries:
   - Query 1: Direct volatile command
   - Query 2: Dependent on volatile recipe
   - Query 3: Non-volatile with 'v' instruction
4. For each:
   - Verify plan.is_volatile correct
   - Verify asset evaluation completes
   - Verify metadata reflects volatility
   - Verify Status == Volatile for volatile queries
5. Verify result values are correct
6. Verify subsequent requests create new assets for volatile queries

**Validation criteria:**
- All queries execute successfully
- Volatility marked correctly
- Results have correct values
- Metadata accurate
- Second requests for volatile queries create new assets

**File location:** `liquers-core/tests/volatility_integration.rs`

---

## Test Organization Summary

**File structure:**
```
liquers-core/tests/
├── volatility_integration.rs (new - main integration tests)
│   ├── Full Pipeline Tests (3 tests)
│   ├── Volatility Instruction Tests (1 test family: 4 sub-tests)
│   ├── Circular Dependency Tests (3 tests)
│   ├── Serialization Tests (3 tests)
│   ├── Concurrency Tests (2 tests)
│   ├── Cross-Module Tests (2 tests)
│   ├── Performance Tests (2 tests)
│   ├── Corner Cases (3 tests)
│   ├── Error Handling (2 tests)
│   └── End-to-End (1 test)
│
└── [Total: 23 specification tests + sub-variants]
```

**Test execution:**
```bash
# Run all volatility integration tests
cargo test -p liquers-core --test volatility_integration

# Run specific test family
cargo test -p liquers-core --test volatility_integration test_circular_dependency

# Run with output
cargo test -p liquers-core --test volatility_integration -- --nocapture
```

---

## Implementation Checklist

- [ ] Create `liquers-core/tests/volatility_integration.rs` file
- [ ] Implement Test 1-23 according to specifications
- [ ] Verify all tests compile without warnings
- [ ] Verify all tests execute successfully (green)
- [ ] Measure performance baselines (Tests 16-17)
- [ ] Run full test suite: `cargo test -p liquers-core`
- [ ] Run with coverage if available
- [ ] Document any deviations from spec in implementation notes

---

## References

- Phase 1: `specs/volatility-system/phase1-high-level-design.md`
- Phase 2: `specs/volatility-system/phase2-architecture.md`
- CLAUDE.md: Testing conventions, error handling, async patterns
- Existing test examples: `liquers-core/tests/async_hellow_world.rs`

