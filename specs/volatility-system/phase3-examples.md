# Phase 3: Examples, Test Specifications, and Validation - Volatility System

**Document Status:** Comprehensive test and validation specifications for Phase 2 implementation

**Cross-References:**
- Phase 1: `phase1-high-level-design.md`
- Phase 2: `phase2-architecture.md`
- Integration: `CLAUDE.md` (testing conventions)

---

## Overview Table: All Examples and Tests

| # | Name | Type | Demonstrates | Drafted By |
|---|------|------|--------------|------------|
| 1 | Real-Time Dashboard Timestamp | Primary Use Case | Volatile command, 'v' instruction, no caching | Haiku Agent 1 |
| 2 | Random Sampling | Primary Use Case | Volatile command metadata, fresh evaluation | Haiku Agent 1 |
| 3 | 'v' Instruction Force Volatile | Primary Use Case | User-controlled cache busting | Haiku Agent 1 |
| 4 | Combining Multiple Volatility Sources | Primary Use Case | Multiple volatility triggers, propagation | Haiku Agent 1 |
| 5 | Volatile Dependency Chain | Advanced Scenario | Upward propagation A→B→C | Haiku Agent 2 |
| 6 | Mixed Volatile/Non-Volatile Dependencies | Advanced Scenario | Selective caching, transitive volatility | Haiku Agent 2 |
| 7 | to_override() Status Transitions | Advanced Scenario | Freezing volatile assets, cascading effects | Haiku Agent 2 |
| 8 | Circular Dependency A→B→A | Edge Case | Direct cycle detection | Haiku Agent 3 |
| 9 | Two-Phase Volatility Check | Edge Case | Phase 1 vs Phase 2, lazy discovery | Haiku Agent 3 |
| 10 | Context Propagation Nested | Edge Case | Volatile context inheritance | Haiku Agent 3 |
| 11 | Mixed Dependency Graph | Edge Case | Transitive volatility propagation | Haiku Agent 3 |
| 12 | Volatile to Override Transition | Edge Case | Status transition semantics | Haiku Agent 3 |
| U1-U8 | Status::Volatile Unit Tests | Unit Test | Enum variant behavior (has_data, is_finished, serialization) | Haiku Agent 4 |
| U9-U11 | Plan.is_volatile Unit Tests | Unit Test | Field existence, getter, serialization | Haiku Agent 4 |
| U12-U14 | PlanBuilder Volatility Unit Tests | Unit Test | 'v' instruction, volatile command, optimization | Haiku Agent 4 |
| U15-U20 | Circular Dependency Unit Tests | Unit Test | Detection (2-asset, 3-asset, self), stack management | Haiku Agent 4 |
| U21-U24 | Volatile Dependency Propagation Unit Tests | Unit Test | None volatile, one volatile, mixed, short-circuit | Haiku Agent 4 |
| U25-U29 | Context.is_volatile Unit Tests | Unit Test | Initialization, propagation, contagion | Haiku Agent 4 |
| U30-U38 | AssetRef::to_override() Unit Tests | Unit Test | All status transitions, concurrency | Haiku Agent 4 |
| U39-U44 | Edge Case Unit Tests | Unit Test | Empty plans, multiple sources, nested | Haiku Agent 4 |
| I1 | Simple Volatile Command to Asset | Integration Test | Full pipeline: Query → Plan → Asset | Haiku Agent 5 |
| I2 | Query with 'v' Instruction | Integration Test | Instruction parsing and plan marking | Haiku Agent 5 |
| I3 | Volatile Dependency Chain | Integration Test | Recipe-based propagation | Haiku Agent 5 |
| I4 | AssetManager No Cache | Integration Test | Multiple requests, unique AssetRef IDs | Haiku Agent 5 |
| I5 | 'v' Instruction at Various Positions | Integration Test | Parser flexibility | Haiku Agent 5 |
| I6-I8 | Circular Dependency Detection | Integration Test | Direct, indirect, self-reference | Haiku Agent 5 |
| I9-I11 | Serialization Round-Trip | Integration Test | Plan, MetadataRecord, Status | Haiku Agent 5 |
| I12-I13 | Concurrency Tests | Integration Test | Thread safety, volatile/non-volatile mix | Haiku Agent 5 |
| I14-I15 | Cross-Module Integration | Integration Test | Interpreter → PlanBuilder → Context | Haiku Agent 5 |
| I16-I17 | Performance Tests | Integration Test | Dependency checking (linear, circular) | Haiku Agent 5 |
| I18-I20 | Corner Cases | Integration Test | Large chains, many commands, state transitions | Haiku Agent 5 |
| I21-I22 | Error Handling | Integration Test | Volatile command errors, missing recipes | Haiku Agent 5 |
| I23 | End-to-End Pipeline | Integration Test | Complete production-like scenario | Haiku Agent 5 |

**Total Coverage:** 4 primary use cases, 3 advanced scenarios, 5 edge cases, 44 unit tests, 23 integration tests

---

## Part 1: Primary Use Cases (Conceptual Examples)

These examples demonstrate real-world scenarios where volatility matters. All examples are conceptual code showing data flow and behavior.

### Example 1: Real-Time Dashboard with Current Timestamp

**Scenario:** A data dashboard displays financial market data alongside a "last updated" timestamp. The timestamp command always produces different output, so it must NEVER be cached.

**Query:**
```
/financial/data/v/timestamp
```

**Command Definition:**
```rust
// File: liquers-lib/src/commands.rs

use liquers_macro::register_command;
use chrono::Local;

fn current_timestamp(state: &State<Value>) -> Result<Value, Error> {
    let now = Local::now().to_rfc3339();
    Ok(Value::from(format!("Last updated: {}", now)))
}

// Register with volatile: true metadata
let cr = env.get_mut_command_registry();
register_command!(cr,
    fn current_timestamp(state) -> result
    label: "Current Timestamp"
    volatile: true  // KEY: Mark as volatile
)?;
```

**Execution Flow:**
1. User query includes 'v' to get volatile result
2. **Phase 1:** Build plan (checks commands and 'v' instruction)
   - `plan.is_volatile = true` (due to 'v' instruction)
3. **Phase 2:** Check asset dependencies
   - Step::Info added: "Volatile due to instruction 'v' at position X"
4. AssetManager checks: `plan.is_volatile = true`
   - Result: CREATES NEW AssetRef (no caching!)
5. Second request for same query:
   - AssetManager creates NEW AssetRef again
   - Fresh evaluation, different timestamp

**Expected Output:**
```
Timestamp 1: Last updated: 2026-02-17T10:23:45+00:00
Timestamp 2: Last updated: 2026-02-17T10:23:46+00:00  (1 second later!)
```

**Why Volatility Matters:**
- Without 'v' instruction: AssetManager would cache first result, dashboard shows stale timestamp
- With volatility system: Each request forces fresh evaluation, always shows current time

---

### Example 2: Volatile Command - Random Sampling

**Scenario:** A data analysis pipeline includes a "random sample" command that selects N random rows from a dataset. Different outputs on each execution.

**Query:**
```
/data/customers/random_sample/50
```

**Command Definition:**
```rust
use rand::seq::SliceRandom;
use polars::prelude::*;

fn random_sample(state: &State<Value>, sample_size: usize) -> Result<Value, Error> {
    let df = state.try_into_dataframe()?;
    let n_rows = df.height();

    if sample_size > n_rows {
        return Err(Error::general_error(
            format!("Sample size {} exceeds dataset size {}", sample_size, n_rows)
        ));
    }

    let mut indices: Vec<usize> = (0..n_rows).collect();
    let mut rng = rand::thread_rng();
    indices.shuffle(&mut rng);

    let sampled_df = df.slice(0, sample_size);
    Ok(Value::from(sampled_df))
}

// Register with volatile: true
register_command!(cr,
    fn random_sample(state, sample_size: usize = 100) -> result
    namespace: "data"
    volatile: true  // KEY: Mark as volatile
)?;
```

**Execution Flow:**
1. **Phase 1:** Build plan
   - Found volatile command: `random_sample`
   - `plan.is_volatile = true`
   - Added Step::Info("Volatile due to command 'data/random_sample'")
2. **Phase 2:** Check asset dependencies (skipped, already volatile)
3. First execution:
   - AssetManager creates NEW AssetRef (asset_id = 12345)
   - Does NOT cache it
4. Second execution (same query):
   - AssetManager creates ANOTHER NEW AssetRef (asset_id = 12346)
   - Does NOT reuse cache from first execution
5. Results are different: Sample 1 ≠ Sample 2

**Why Volatility Matters:**
- Without volatility marking: First execution caches 50 random customers, second returns SAME 50 (cached)
- With volatility marking: Each execution produces fresh random subset, truly non-deterministic

---

### Example 3: 'v' Instruction - Forcing Volatile on Non-Volatile Query

**Scenario:** User has a deterministic query but wants to force fresh evaluation every time by adding 'v' instruction (cache-busting).

**Query:**
```
Normal:          /sales/by_region/group_by_q/sum
Forced volatile: /sales/by_region/group_by_q/sum/v
```

**Execution Comparison:**

**WITHOUT 'v' instruction:**
```
Query: sales_data/group_by_q/sum
plan.is_volatile: false
  - No 'v' instruction found
  - No volatile commands detected
Result 1: DataFrame { shape: (5, 2), ... }
Asset ID: 1000
Result 2: DataFrame { shape: (5, 2), ... }
Asset ID: 1000 (SAME! Cached)
```

**WITH 'v' instruction:**
```
Query: sales_data/group_by_q/sum/v
plan.is_volatile: true
  - 'v' instruction found
  - Added Step::Info: 'Volatile due to instruction 'v''
Result 3: DataFrame { shape: (5, 2), ... }
Asset ID: 2000
Result 4: DataFrame { shape: (5, 2), ... }
Asset ID: 2001 (NEW! Not cached)
```

**Why Volatility Matters:**
- Without 'v': Results are deterministic and safely cached, efficient for repeated queries
- With 'v': User forces fresh evaluation each time, useful for cache-busting or debugging
- Control is in user's hands via query syntax

---

### Example 4: Combining Multiple Volatility Sources

**Scenario:** Single query that combines multiple sources of volatility.

**Query:**
```
/financial/data/v/random_sample/100/format_report
  ├── Load financial data (non-volatile)
  ├── 'v' instruction (forces volatile)
  ├── random_sample (volatile command)
  └── format_report (non-volatile)
```

**Volatility Propagation:**
```rust
// Plan building:
// PHASE 1: Check commands
//   - load: non-volatile
//   - format_report: non-volatile
//   Result: plan.is_volatile = false

// PHASE 1.5: Check 'v' instruction
//   - Found 'v'!
//   Result: plan.is_volatile = true

// PHASE 1.75: Check for volatile commands
//   - random_sample: VOLATILE COMMAND DETECTED
//   Result: plan.is_volatile already true (stays true)

// PHASE 2: Check dependencies (skipped, already volatile)

// Final Plan:
Plan {
    steps: [
        Step::Action("load"),
        Step::Info("Volatile due to instruction 'v'"),
        Step::Action("random_sample"),
        Step::Action("format_report"),
        Step::Info("Volatile due to command 'random_sample'"),
    ],
    is_volatile: true,  // TRUE - from multiple sources
}
```

**Key Insight - Contagious Volatility:**
Once ANY source marks a plan as volatile:
- That plan is volatile
- All results are volatile
- No caching occurs
- Context marked volatile for nested evaluations
- Any State derived from volatile Context becomes volatile

---

## Part 2: Advanced Scenarios (Conceptual Code)

These scenarios demonstrate volatile dependency propagation, status transitions, and complex interactions.

### Example 5: Volatile Dependency Propagation Chain

**Scenario:** Query chain demonstrates how volatility "infects" dependent assets:
- Query A: `timestamp/add-child-query-B` (volatile)
- Query B: `transform/add-child-query-C` (depends on volatile A)
- Query C: `aggregate-results` (depends on volatile B)

**Conceptual Code:**
```rust
// Setup volatile command
register_command!(cr,
    async fn timestamp(state) -> result
    volatile: true
    namespace: "core"
)?;

// Build query chain
let query_a = Query::parse("core/timestamp").unwrap();
let query_b = Query::parse("data/transform/q/core/timestamp").unwrap();
let query_c = Query::parse("data/aggregate/q/data/transform/q/core/timestamp").unwrap();

// Phase 1: Build plans
let plan_a = make_plan(env.clone(), &query_a).await.unwrap();
let plan_b = make_plan(env.clone(), &query_b).await.unwrap();
let plan_c = make_plan(env.clone(), &query_c).await.unwrap();

// Phase 1 results:
assert!(plan_a.is_volatile);  // 'timestamp' command is volatile
assert!(!plan_b.is_volatile); // 'transform' command is non-volatile
assert!(!plan_c.is_volatile); // 'aggregate' command is non-volatile

// Phase 2: Check dependencies
// - plan_b depends on plan_a (volatile) → plan_b.is_volatile becomes true
// - plan_c depends on plan_b (volatile) → plan_c.is_volatile becomes true
assert!(plan_b.is_volatile);  // Now volatile due to dependency
assert!(plan_c.is_volatile);  // Now volatile due to dependency chain
```

**Data Flow:**
```
Query A: core/timestamp
    ↓
Plan A: is_volatile=true (command 'timestamp' is volatile)
    ↓
Asset A: Status::Volatile, is_volatile=true
    ↓
[Second request for Query A] → NEW AssetRef (never cached)

Query B: data/transform/q/core/timestamp
    ↓
Plan B (Phase 1): is_volatile=false
    ↓
Plan B (Phase 2): finds dependency on Query A → is_volatile=true
    ↓
Asset B: Status::Volatile, is_volatile=true
    ↓
[Second request for Query B] → NEW AssetRef (never cached)
```

**Expected Behavior:**
1. **Upfront knowledge:** Volatility fully determined before evaluation begins
2. **Propagation:** Volatility propagates "upward" through dependency chain
3. **No caching:** Volatile assets NEVER cached in AssetManager
4. **Metadata consistency:** Both `status` and `is_volatile` reflect volatility at all stages

---

### Example 6: Mixed Volatile and Non-Volatile Dependencies

**Scenario:** Query has both volatile and non-volatile dependencies:
- Root: `combine-results` (takes two inputs)
  - Input 1: `query-db/cached-query` (non-volatile, can cache)
  - Input 2: `core/random-number` (volatile)

Root asset should be volatile, but Input 1 can be cached independently.

**Conceptual Code:**
```rust
// Register commands
register_command!(cr,
    async fn query_db(state) -> result
    namespace: "db"
    // NOT volatile - database queries are deterministic
)?;

register_command!(cr,
    async fn random_number(state) -> result
    volatile: true
    namespace: "core"
)?;

// Build query that references both
let query = Query::parse("data/combine-results/q/db/query-db/q/core/random-number").unwrap();

// Phase 1: Build plan
let mut plan = make_plan(env.clone(), &query).await.unwrap();
// After Phase 1: plan.is_volatile = false (root command not volatile)

// Phase 2: Check dependencies
// Dependencies: [db/query-db (non-volatile), core/random-number (volatile)]
// Found volatile dependency → plan.is_volatile = true
assert!(plan.is_volatile);

// Evaluate
let asset_root = env.get_asset_from_query(&query).await.unwrap();
let meta_root = asset_root.get_metadata().await;
assert_eq!(meta_root.status, Status::Volatile);

// But we can request non-volatile dependency independently
let query_db_only = Query::parse("db/query-db").unwrap();
let asset_db = env.get_asset_from_query(&query_db_only).await.unwrap();
let meta_db = asset_db.get_metadata().await;
assert_eq!(meta_db.status, Status::Ready);  // NOT Volatile!

// Requesting db/query-db multiple times returns CACHED AssetRef
let asset_db_2 = env.get_asset_from_query(&query_db_only).await.unwrap();
assert_eq!(asset_db.id(), asset_db_2.id());  // Same AssetRef (cached)

// But requesting root query again gets NEW AssetRef
let asset_root_2 = env.get_asset_from_query(&query).await.unwrap();
assert_ne!(asset_root.id(), asset_root_2.id());  // Different AssetRef
```

**Expected Behavior:**
1. **Selective caching:** Only non-volatile assets cached; volatile assets always fresh
2. **Propagation rule:** If ANY dependency is volatile, the dependent is volatile
3. **Independent subqueries:** Non-volatile subqueries can be cached when requested independently

---

### Example 7: to_override() Status Transition and Cascading Effects

**Scenario:** Volatile asset converted to Override status via `to_override()`, freezing its value.

**Use Cases:**
- Save volatile asset for audit trail
- Freeze long-running volatile computation result
- Terminate recursive query to prevent infinite re-evaluation

**Conceptual Code:**
```rust
// Scenario 1: Converting completed volatile asset to Override
let query = Query::parse("time/current-time").unwrap();
let asset_ref = env.get_asset_from_query(&query).await.unwrap();

// Asset is volatile
let meta_ready = asset_ref.get_metadata().await;
assert_eq!(meta_ready.status, Status::Volatile);

// Freeze it
asset_ref.to_override().await.unwrap();

let meta_frozen = asset_ref.get_metadata().await;
assert_eq!(meta_frozen.status, Status::Override);
// Value is preserved

// Scenario 2: Converting in-progress asset to Override
// (cancels evaluation and freezes with no value)

// Scenario 3: Cascading effects - what happens to dependents?
let asset_a = env.get_asset_from_query(&Query::parse("time/current-time").unwrap()).await.unwrap();
let query_b = Query::parse("time/transform-time/q/time/current-time").unwrap();
let asset_b = env.get_asset_from_query(&query_b).await.unwrap();

// Before conversion
assert_eq!(asset_a.get_metadata().await.status, Status::Volatile);
assert_eq!(asset_b.get_metadata().await.status, Status::Volatile);

// Convert A to Override
asset_a.to_override().await.unwrap();

// After conversion
assert_eq!(asset_a.get_metadata().await.status, Status::Override);
assert_eq!(asset_b.get_metadata().await.status, Status::Volatile);  // STILL VOLATILE!

// Explanation: to_override() does NOT cascade to dependents
```

**State Transitions Allowed:**
```
Volatile → Override: YES (intentional freeze)
Volatile → Ready: NO (not permitted)
Volatile → Expired: NO (not permitted)
Override → Volatile: NO (not permitted)
Override → Override: YES (idempotent, no-op)
```

**Expected Behavior:**
1. **One-way transition:** `to_override()` is the ONLY permitted transition for volatile assets
2. **Value preservation:** For assets with data, value is preserved; for in-flight, value set to `none()`
3. **Non-cascading:** Converting A to Override does NOT affect dependents
4. **Termination semantics:** Override signals "this value is frozen, do not re-evaluate"

---

## Part 3: Edge Cases (Conceptual Code)

These examples cover corner cases and error scenarios.

### Example 8: Circular Dependency Detection in Asset Graph

**Scenario:** User defines recipes with circular dependency: A → B → C → A

**Conceptual Code:**
```rust
// Define circular recipes
let recipe_a = Recipe {
    query: parse_query("b/to_upper").ok(),
    volatile: false,
};
env.register_recipe(Key::new().segment("a"), recipe_a);

let recipe_b = Recipe {
    query: parse_query("c/reverse").ok(),
    volatile: false,
};
env.register_recipe(Key::new().segment("b"), recipe_b);

let recipe_c = Recipe {
    query: parse_query("a/length").ok(),  // CREATES CYCLE
    volatile: false,
};
env.register_recipe(Key::new().segment("c"), recipe_c);

// Attempt to build plan
let query = parse_query("a/add_suffix-_done").ok();
let result = make_plan(env.clone(), &query).await;

// Phase 2: find_dependencies() called
// stack processing:
//   1. Push "a": ["a"]
//   2. Get recipe_a, find dependency on "b"
//   3. Push "b": ["a", "b"]
//   4. Get recipe_b, find dependency on "c"
//   5. Push "c": ["a", "b", "c"]
//   6. Get recipe_c, find dependency on "a"
//   7. Check: stack.contains("a")? YES! ✗ CYCLE DETECTED

// Expected error
assert!(result.is_err());
// Error: "Circular dependency detected: key Key(\"a\") appears in dependency chain"
```

**How System Handles It:**
1. **Deferred resolution:** Phase 1 completes without recursion, circular refs don't appear yet
2. **Explicit cycle detection:** Phase 2 maintains `stack: Vec<Key>` tracking dependency chain
3. **Early termination:** Cycle detected before assets created, no partial state
4. **Stack-based tracking:** Push when entering, pop when exiting

---

### Example 9: Two-Phase Volatility Check with Lazy Dependency Discovery

**Scenario:** Query contains both 'v' instruction AND volatile dependency.

**Conceptual Code:**
```rust
// Register volatile recipe
let volatile_recipe = Recipe {
    query: parse_query("current_time/to_string").ok(),
    volatile: true,
};
env.register_recipe(Key::new().segment("timestamp"), volatile_recipe);

// Build plan with 'v' instruction AND dependency on volatile asset
let query = parse_query("timestamp/v/to_upper").ok();

// Phase 1: Check commands and instructions
let mut pb = PlanBuilder::new(query, cmr);
// Step 1: Step::Resource(Key("timestamp")) - skip (Phase 2 responsibility)
// Step 2: Step::Action("v") - VOLATILE INSTRUCTION
//   pb.mark_volatile("Volatile due to instruction 'v'")
//   pb.is_volatile = true
// Step 3: Step::Action("to_upper") - already volatile, skip
let mut plan = pb.build()?;
// plan.is_volatile = true (from Phase 1)

// Phase 2: Check dependencies (still runs for documentation)
let deps = find_dependencies(envref, &plan, &mut stack).await?;
// Found Key("timestamp"), check recipe.volatile: true
// Add Step::Info("Volatile due to dependency on key: Key(\"timestamp\")")

// Final Plan has BOTH volatility sources documented
```

**How System Handles It:**
1. **Phase 1 marks due to instruction:** 'v' detected, sets flag immediately
2. **Phase 2 documents dependencies:** Even though already volatile, Phase 2 runs for auditing
3. **Multiple sources documented:** Both 'v' instruction and volatile dependency recorded

---

### Example 10: Context Propagation Through Nested Evaluations

**Scenario:** Volatile command triggers nested query evaluation during execution. Nested query references non-volatile dependencies but must inherit volatility from parent context.

**Conceptual Code:**
```rust
// Command that performs context.evaluate() internally
fn complex_transform(
    state: &State<Value>,
    query: String,
    context: &Context,
) -> Result<Value, Error> {
    let nested_query = parse_query(&query)?;
    let nested_result = context.evaluate(&nested_query).await?;

    let input = state.try_into_string()?;
    let nested_str = nested_result.try_into_string()?;
    Ok(Value::from(format!("{}-{}", input, nested_str)))
}

// Setup
register_command!(cmr, fn volatile_source() -> result volatile: true)?;
register_command!(cmr, async fn complex_transform(state, query: String, context) -> result)?;

// Execute query
let query = parse_query("volatile_source/complex_transform-'data/store_value").ok();
let plan = make_plan(envref.clone(), &query).await?;

// Phase 1 volatility check:
// - volatile_source: command volatile → plan.is_volatile = true

// Context initialized with is_volatile: true
let context = Context::new(envref, plan.is_volatile);

// Evaluation:
// 1. Execute volatile_source() → returns "2026-02-17"
// 2. Execute complex_transform() with context.is_volatile = true
//    INSIDE complex_transform:
//      - Create child context for nested evaluation
//        nested_ctx = context.with_volatile(false)
//        // Inheritance: nested_ctx.is_volatile = false || true = true
//      - Evaluate "data/store_value" in VOLATILE context
//      - Result marked as volatile (inherited from context)
```

**How System Handles It:**
1. **Context carries volatility flag:** Created with `is_volatile` from Plan at start
2. **Propagation via context.with_volatile():** Child contexts inherit via OR logic
3. **Nested assets inherit volatility:** Assets created in volatile context become volatile
4. **No caching for volatile assets:** AssetManager checks context.is_volatile
5. **Contagion rule:** Any asset produced within volatile context becomes volatile

---

### Example 11: Mixed Volatility in Dependency Graph

**Scenario:** Complex query with multiple dependencies, some volatile and some not.

**Conceptual Code:**
```rust
// Setup recipes
let recipe_factorial = Recipe {
    query: parse_query("5/factorial").ok(),
    volatile: false,  // Pure computation
};
env.register_recipe(Key::new().segment("factorial5"), recipe_factorial);

let recipe_current_time = Recipe {
    query: parse_query("current_time/to_string").ok(),
    volatile: true,  // Volatile
};
env.register_recipe(Key::new().segment("timestamp"), recipe_current_time);

let recipe_combined = Recipe {
    query: parse_query("factorial5/append-': '/timestamp").ok(),
    volatile: false,  // But transitively depends on volatile key!
};
env.register_recipe(Key::new().segment("combined"), recipe_combined);

// Build plan
let query = parse_query("combined/to_upper").ok();
let mut plan = make_plan(envref.clone(), &query).await?;

// Phase 1: plan.is_volatile = false (root command not volatile)

// Phase 2: Dependency resolution
// Dependencies found: [factorial5 (non-volatile), timestamp (volatile)]
// ✓ Found volatile dependency!
// Result: plan.is_volatile = true
assert!(plan.is_volatile);
```

**How System Handles It:**
1. **Phase 1 conservatively assumes non-volatile:** Only looks at direct commands/instructions
2. **Phase 2 discovers transitive volatility:** Recursively resolves all asset dependencies
3. **Early updates to plan.is_volatile:** Once any volatile dependency found, plan updated
4. **Context volatility propagates:** All assets created during evaluation inherit flag
5. **Multiple sources documented:** Each volatility source adds Step::Info

---

### Example 12: Volatile Status Transition via to_override()

**Scenario:** Convert volatile asset to Override, preventing re-evaluation while preserving value.

**Conceptual Code:**
```rust
// Create volatile asset
let query = parse_query("current_time/to_string").ok();
let asset_ref = AssetManager::get_asset_from_query(query).await?;

// Asset after evaluation:
// status: Status::Volatile
// data: "2026-02-17T14:23:45Z"

// Call to_override() to freeze
asset_ref.to_override().await?;

// After to_override():
// status: Status::Override
// data: "2026-02-17T14:23:45Z" (PRESERVED)
// is_volatile: true (preserved for audit)

// Subsequent access
let asset_ref2 = AssetManager::get_asset_from_query(query).await?;
// Override status may allow caching (manager's choice)
// Value is SAME as first request (not refreshed)
```

**Error Cases:**
```rust
// Case 1: Processing state → cancels evaluation, freezes with null
// Case 2: Error state → keeps error_data, freezes in Error state
// Case 3: Already Override → no-op (idempotent)
// Case 4: Directory/Source → no-op (unchanged)
```

**How System Handles It:**
1. **One-way transition:** Only method for volatile assets, no reverse
2. **Preserves existing value:** For Ready/Volatile/Expired, keeps data
3. **Handles in-flight cancellation:** For Processing, sends cancel message
4. **Idempotent:** Multiple calls safe
5. **Async and safe:** Uses existing RwLock pattern

---

## Part 4: Unit Test Specifications

Comprehensive unit tests organized by module. See `phase3-unit-test-specifications.md` for complete details.

### Test Coverage Summary

| Category | Count | Key Tests |
|----------|-------|-----------|
| **Status::Volatile** | 4 | has_data(), is_finished(), can_have_tracked_dependencies(), serialization |
| **MetadataRecord.is_volatile** | 4 | Field behavior, helper method, serialization |
| **Plan.is_volatile** | 3 | Field existence, getter, serialization |
| **PlanBuilder volatility** | 3 | 'v' instruction, volatile command, optimization |
| **Circular dependencies** | 5 | 2-asset cycle, 3-asset cycle, self-reference, stack management |
| **Volatile dependencies** | 4 | None volatile, one volatile, mixed, short-circuit |
| **Context.is_volatile** | 5 | Initialization, getter, propagation, contagion |
| **AssetRef::to_override()** | 9 | All status transitions, concurrency |
| **Edge cases** | 4 | Empty plans, context-only, multiple sources, nested |
| **Total Unit Tests** | **44** | Comprehensive coverage |

### Key Unit Tests (Samples)

**Test 1: Status::Volatile has_data()**
```rust
#[test]
fn test_status_volatile_has_data() {
    let status = Status::Volatile;
    assert!(status.has_data());
}
```

**Test 12: PlanBuilder marks volatile for 'v' instruction**
```rust
#[tokio::test]
async fn test_plan_builder_marks_volatile_for_v_instruction() {
    let query = parse_query("data/v/to_text").expect("parse");
    let mut pb = PlanBuilder::new(query, &cmr);
    let plan = pb.build().expect("build");

    assert!(plan.is_volatile);
    // Verify Step::Info contains "Volatile due to instruction 'v'"
}
```

**Test 17: Circular dependency A→B→A detection**
```rust
#[tokio::test]
async fn test_find_dependencies_circular_a_depends_b_depends_a() {
    // Setup recipes with A→B, B→A circular dependency
    let result = find_dependencies(envref, &plan_a, &mut stack).await;

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("Circular dependency detected"));
}
```

**Test 30: AssetRef.to_override() on Volatile status**
```rust
#[tokio::test]
async fn test_asset_ref_to_override_from_volatile() {
    let asset_ref = create_test_asset(Status::Volatile, Value::from("test_data")).await;

    asset_ref.to_override().await.expect("should succeed");

    let data = asset_ref.read().await;
    assert_eq!(data.status, Status::Override);
    // Value preserved
}
```

**File Location:** `liquers-core/src/` (tests modules at end of each file)

---

## Part 5: Integration Test Specifications

Full pipeline tests covering query → plan → asset → state flow. See `phase3-integration-tests.md` for complete details.

### Test Coverage Summary

| Category | Count | Key Tests |
|----------|-------|-----------|
| **Full Pipeline** | 3 | Volatile command, 'v' instruction, dependency chain |
| **Volatility Instruction** | 1 family | 'v' at various positions |
| **Circular Dependencies** | 3 | Direct (A→B→A), indirect (A→B→C→A), self-reference |
| **Serialization** | 3 | Plan, MetadataRecord, Status round-trip |
| **Concurrency** | 2 | Multiple threads, mixed volatile/non-volatile |
| **Cross-Module** | 2 | Interpreter → PlanBuilder → Context pipeline |
| **Performance** | 2 | Linear chain, circular detection timing |
| **Corner Cases** | 3 | Large chains, many commands, state transitions |
| **Error Handling** | 2 | Volatile command errors, missing recipes |
| **End-to-End** | 1 | Complete production-like scenario |
| **Total Integration Tests** | **23** | Complete system coverage |

### Key Integration Tests (Samples)

**Test I1: Simple Volatile Command Query to Asset**
```rust
#[tokio::test]
async fn test_volatile_query_to_asset_simple() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = SimpleEnvironment::<Value>::new();

    // Register volatile command
    register_command!(cr, fn current_time(state) -> result volatile: true)?;

    let envref = env.to_ref();

    // Make plan
    let plan = make_plan(envref.clone(), "current_time").await?;
    assert!(plan.is_volatile);

    // Evaluate
    let state1 = evaluate_plan(envref.clone(), &plan).await?;
    let metadata1 = state1.metadata();
    assert!(metadata1.is_volatile());
    assert_eq!(metadata1.status, Status::Volatile);

    // Get same query again - should return NEW AssetRef
    let state2 = evaluate_plan(envref.clone(), &plan).await?;
    // Both valid but from different evaluations

    Ok(())
}
```

**Test I4: AssetManager Never Caches Volatile Assets**
```rust
#[tokio::test]
async fn test_asset_manager_volatile_no_cache() -> Result<(), Box<dyn std::error::Error>> {
    let query = Query::from_string("random_value")?;

    // Request volatile query 3 times
    let asset1 = asset_manager.get_asset_from_query(&query).await?;
    let id1 = asset1.id();

    let asset2 = asset_manager.get_asset_from_query(&query).await?;
    let id2 = asset2.id();

    let asset3 = asset_manager.get_asset_from_query(&query).await?;
    let id3 = asset3.id();

    // All three should have different IDs (not cached)
    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);

    Ok(())
}
```

**Test I6: Direct Circular Dependency Detection**
```rust
#[tokio::test]
async fn test_circular_dependency_direct_detection() -> Result<(), Box<dyn std::error::Error>> {
    // Create circular recipes: A→B, B→A
    let recipe_a = Recipe::new(key_a.clone(), Query::from_string("key-b/transform")?, false);
    let recipe_b = Recipe::new(key_b.clone(), Query::from_string("key-a/transform")?, false);

    // Attempt to make plan - should detect circular dependency
    let result = make_plan(envref.clone(), "key-a").await;

    assert!(result.is_err());
    let error_msg = result.unwrap_err().message();
    assert!(error_msg.contains("Circular dependency"));

    Ok(())
}
```

**Test I16: Dependency Checking Performance (Linear Chain)**
```rust
#[tokio::test]
async fn test_dependency_checking_performance_linear_chain() -> Result<(), Box<dyn std::error::Error>> {
    // Create 100-deep linear chain: key-0 → key-1 → ... → key-99
    for i in 0..100 {
        let recipe = Recipe::new(key, Query::from_string(&next_query)?, false);
        recipe_provider.add_recipe(recipe);
    }

    // Time the dependency checking
    let start = std::time::Instant::now();
    let plan = make_plan(envref, "key-99").await?;
    let elapsed = start.elapsed();

    assert!(!plan.is_volatile);
    assert!(elapsed.as_millis() < 500,
        "Dependency checking should complete in < 500ms");

    Ok(())
}
```

**File Location:** `liquers-core/tests/volatility_integration.rs`

---

## Part 6: Corner Cases and Validation

### Corner Cases Covered

1. **Memory:** Large dependency chains (1000-deep), many volatile commands (10+)
2. **Concurrency:** Multiple threads requesting same volatile asset, mixed volatile/non-volatile
3. **Serialization:** Round-trip for Plan, MetadataRecord, Status
4. **Cross-module:** Interpreter → PlanBuilder → Context → AssetManager integration
5. **Error Handling:** Volatile command errors, missing recipes, circular dependencies
6. **Performance:** Linear chains (100+ deep), circular detection timing

### Manual Validation Commands

```bash
# Check compilation
cargo check -p liquers-core

# Run all unit tests
cargo test -p liquers-core --lib

# Run integration tests
cargo test -p liquers-core --test volatility_integration

# Run specific test family
cargo test -p liquers-core test_circular_dependency

# Run with output
cargo test -p liquers-core --test volatility_integration -- --nocapture

# Check for exhaustive match warnings
cargo clippy -p liquers-core -- -D warnings

# Performance baseline measurement
cargo test -p liquers-core test_dependency_checking_performance -- --nocapture
```

---

## Part 7: Test Execution Plan

### Implementation Order

**Phase 1: Core Data Structures (Week 1)**
1. Add `Status::Volatile` variant (U1-U8)
2. Add `MetadataRecord.is_volatile` field (U9-U11)
3. Add `Plan.is_volatile` field (U12-U14)
4. Add `Context.is_volatile` field (U25-U29)
5. Run unit tests for data structures

**Phase 2: Plan Building (Week 2)**
6. Implement PlanBuilder volatility checking (U12-U14)
7. Implement circular dependency detection (U15-U20)
8. Implement volatile dependency propagation (U21-U24)
9. Run unit tests for plan building

**Phase 3: Asset Management (Week 3)**
10. Modify AssetManager caching behavior
11. Implement `AssetRef::to_override()` (U30-U38)
12. Run unit tests for asset management

**Phase 4: Integration (Week 4)**
13. Run integration tests I1-I5 (full pipeline)
14. Run integration tests I6-I8 (circular dependencies)
15. Run integration tests I9-I11 (serialization)
16. Run integration tests I12-I15 (concurrency, cross-module)

**Phase 5: Performance & Corner Cases (Week 5)**
17. Run integration tests I16-I17 (performance)
18. Run integration tests I18-I20 (corner cases)
19. Run integration tests I21-I23 (error handling, end-to-end)
20. Performance tuning and optimization

### Test Dependencies

```
Data Structure Tests (U1-U29)
    ↓
Plan Building Tests (U12-U24)
    ↓
Asset Management Tests (U30-U38)
    ↓
Integration Tests (I1-I23)
    ↓
Performance & Corner Cases (I16-I20)
```

### Estimated Time/Complexity

| Phase | Time | Complexity | Risk |
|-------|------|------------|------|
| Core Data Structures | 2 days | Low | Low (additive changes) |
| Plan Building | 3 days | Medium | Medium (circular dependency logic) |
| Asset Management | 2 days | Medium | Low (existing patterns) |
| Integration | 3 days | High | Medium (cross-module interactions) |
| Performance & Corner Cases | 2 days | Medium | Low (validation only) |
| **Total** | **12 days** | **Medium** | **Medium** |

---

## Summary: Key Insights Across All Examples

### Volatility Propagation Rules

1. **Upward propagation:** Volatility flows from dependencies to dependents
   - If A is volatile and B depends on A, then B becomes volatile
2. **Downward non-propagation:** Converting to Override does NOT affect dependents
   - If A becomes Override, B (which depends on A) is unaffected
3. **Two-phase determination:** Volatility determined upfront, never during execution
   - Phase 1: Check commands and 'v' instruction
   - Phase 2: Check asset dependencies

### AssetManager Caching Strategy

- **Volatile assets:** NEVER cached, always creates new AssetRef
- **Non-volatile assets:** Cached normally, returns existing AssetRef
- **Override assets:** Not cached (treated as non-volatile but frozen)

### Status::Volatile vs MetadataRecord.is_volatile

- **Status::Volatile:** Asset currently has a volatile value (Ready state with volatile marker)
- **MetadataRecord.is_volatile:** Asset will be or was volatile (even during in-flight states)
- **Both are true:** Asset is volatile throughout its lifecycle

### to_override() Use Cases

1. **Freeze volatile results:** Lock in a computed volatile value
2. **Stop re-evaluation:** Prevent further async computation
3. **Recursive termination:** Break dependency cycles
4. **Error recovery:** Stop trying to re-evaluate failed assets

---

## References

- **Phase 1 Design:** `specs/volatility-system/phase1-high-level-design.md`
- **Phase 2 Architecture:** `specs/volatility-system/phase2-architecture.md`
- **Testing Conventions:** `CLAUDE.md` (sections on Testing and Match Statements)
- **Query DSL:** `liquers-core/src/query.rs` - instruction parsing
- **Plan Builder:** `liquers-core/src/plan.rs` - volatility detection
- **Asset Manager:** `liquers-core/src/assets.rs` - caching behavior
- **Command Metadata:** `liquers-core/src/command_metadata.rs` - volatile flag
- **Example Tests:** `liquers-core/tests/async_hellow_world.rs`

---

## Appendix: Agent Contributions

| Agent | Contribution | Files |
|-------|--------------|-------|
| Haiku Agent 1 | Primary use cases | `PRIMARY_USE_CASE_EXAMPLES.md` |
| Haiku Agent 2 | Advanced scenarios | `phase3-advanced-scenarios.md` |
| Haiku Agent 3 | Edge case examples | `phase3-edge-case-examples.md` |
| Haiku Agent 4 | Unit test specifications | `phase3-unit-test-specifications.md` |
| Haiku Agent 5 | Integration tests | `phase3-integration-tests.md` |
| Synthesizer Agent | Integration & synthesis | `phase3-examples.md` (this document) |

**Document Version:** 1.0
**Last Updated:** 2026-02-17
**Status:** Complete - Ready for Phase 4 Implementation
