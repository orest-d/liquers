# Phase 3: Edge Case Examples - Volatility System

## Overview

This document provides conceptual code examples demonstrating edge cases and error scenarios in the volatility system. Each example shows:
1. **Edge case scenario** - The problematic or interesting situation
2. **Conceptual code** - Query, plan building, and asset graph representation
3. **Expected behavior** - Error messages, state transitions, or propagation results
4. **How the system handles it** - Mechanisms that ensure correctness

---

## Edge Case 1: Circular Dependency Detection in Asset Graph

### Scenario

User attempts to define a query that creates a circular dependency through recipe references. This should be detected during the Phase 2 dependency analysis and reject the query before execution begins.

```
Recipe A references Key B
Recipe B references Key C
Recipe C references Key A  <- CIRCULAR
```

### Conceptual Code

#### Step 1: Define recipes in environment

```rust
// RecipeProvider or config-driven setup
let mut env = SimpleEnvironment::new();

// Recipe A: depends on B
let recipe_a = Recipe {
    query: parse_query("b/to_upper").ok(),
    volatile: false,
    // ...
};
env.register_recipe(Key::new().segment("a"), recipe_a);

// Recipe B: depends on C
let recipe_b = Recipe {
    query: parse_query("c/reverse").ok(),
    volatile: false,
    // ...
};
env.register_recipe(Key::new().segment("b"), recipe_b);

// Recipe C: depends on A - CREATES CYCLE
let recipe_c = Recipe {
    query: parse_query("a/length").ok(),
    volatile: false,
    // ...
};
env.register_recipe(Key::new().segment("c"), recipe_c);
```

#### Step 2: Attempt to build plan that references A

```rust
// User submits query that requires resolving key A
let query = parse_query("a/add_suffix-_done").ok();

// Phase 1: Build plan from query (succeeds - no direct circular refs yet)
let mut plan = PlanBuilder::new(query, cmr).build()?;
// plan.is_volatile = false (no commands marked volatile, no 'v' instruction)
// plan.steps = [
//   Step::Resource(Key("a")),
//   Step::Action("add_suffix", [Value::String("_done")])
// ]

// Phase 2: find_dependencies() called with plan
let mut stack = Vec::new();
let deps = find_dependencies(envref, &plan, &mut stack).await?;
// stack = []
// dependencies processing:
//   1. Step::Resource(Key("a")) found
//   2. stack contains "a"? No -> add to stack: ["a"]
//   3. Get recipe for key "a" from env
//   4. Resolve recipe_a's query "b/to_upper"
//   5. Find dependencies of recipe_a's plan:
//      a. Step::Resource(Key("b")) found
//      b. stack contains "b"? No -> add to stack: ["a", "b"]
//      c. Get recipe for key "b" from env
//      d. Resolve recipe_b's query "c/reverse"
//      e. Find dependencies of recipe_b's plan:
//         i. Step::Resource(Key("c")) found
//         ii. stack contains "c"? No -> add to stack: ["a", "b", "c"]
//         iii. Get recipe for key "c" from env
//         iv. Resolve recipe_c's query "a/length"
//         v. Find dependencies of recipe_c's plan:
//             - Step::Resource(Key("a")) found
//             - stack contains "a"? YES! ✗ CYCLE DETECTED
```

#### Step 3: Error handling

```rust
// find_dependencies returns error on cycle detection
Err(Error::general_error(
    "Circular dependency detected: key Key(\"a\") appears in dependency chain".to_string()
))

// This error propagates back through has_volatile_dependencies()
// All the way to make_plan()
// Query evaluation aborts before execution starts
```

### Expected Behavior

**Query submission:**
```
Query: "a/add_suffix-_done"
```

**Immediate error response (during plan building):**
```
{
  "error": "Circular dependency detected: key Key(\"a\") appears in dependency chain",
  "type": "GeneralError",
  "context": "Phase 2 dependency analysis"
}
```

**State of system:**
- Query rejected
- No AssetRefs created
- No side effects
- Next query can proceed normally

### How the System Handles It Correctly

1. **Deferred dependency resolution** - Phase 1 (plan build) completes without recursion. Circular refs don't appear yet because recipes aren't evaluated.

2. **Explicit cycle detection** - Phase 2 calls `find_dependencies()` which maintains a `stack: Vec<Key>` tracking the current dependency chain. Each time a new key is encountered, check `if stack.contains(key)` before recursing.

3. **Early termination** - Cycle detection happens before any assets are created. No partial state or cleanup needed.

4. **Stack-based tracking** - Push when entering a key's dependencies, pop when exiting. Handles multiple independent dependency chains without confusion.

---

## Edge Case 2: Two-Phase Volatility Check with Lazy Dependency Discovery

### Scenario

A query contains both:
- An explicit volatile instruction `v`
- References to assets whose recipes are themselves volatile
- Non-volatile commands

The two-phase check must handle:
- Phase 1: Mark volatile due to `v` instruction
- Phase 2: Also verify volatile dependencies exist and document them

### Conceptual Code

#### Step 1: Register volatile recipe

```rust
// Register a recipe that's inherently volatile (e.g., calls current_time)
let volatile_recipe = Recipe {
    query: parse_query("current_time/to_string").ok(),
    volatile: true,  // Explicitly marked
    // ...
};
env.register_recipe(Key::new().segment("timestamp"), volatile_recipe);

// Register normal command (not volatile)
let cmr = env.get_command_metadata_registry();
// current_time command has CommandMetadata { volatile: true, ... }
// to_string command has CommandMetadata { volatile: false, ... }
```

#### Step 2: Build plan with explicit `v` instruction

```rust
// User submits query with both 'v' instruction AND dependency on volatile asset
let query = parse_query("timestamp/v/to_upper").ok();

// Phase 1: PlanBuilder processes steps
let mut pb = PlanBuilder::new(query, cmr);
// Step 1: Step::Resource(Key("timestamp"))
//   - Already volatile from Phase 1? No, skip it (Phase 2 responsibility)
// Step 2: Step::Action("v") <- VOLATILE INSTRUCTION
//   - PlanBuilder checks: action name == "v"? YES
//   - pb.mark_volatile("Volatile due to instruction 'v'")
//   - pb.is_volatile = true
//   - Add Step::Info("Volatile due to instruction 'v'")
// Step 3: Step::Action("to_upper")
//   - Already volatile? Yes (pb.is_volatile = true)
//   - Skip volatility check (optimization)

let mut plan = pb.build()?;
// plan.is_volatile = true (from Phase 1)
// plan.steps = [
//   Step::Resource(Key("timestamp")),
//   Step::Action("v"),
//   Step::Info("Volatile due to instruction 'v'"),
//   Step::Action("to_upper")
// ]
```

#### Step 3: Phase 2 dependency analysis

```rust
// Call has_volatile_dependencies() to verify and document dependencies
// Note: Phase 1 already set plan.is_volatile = true, so Phase 2 short-circuits?
// NO - Phase 2 still runs to document dependencies

// has_volatile_dependencies logic:
// if plan.is_volatile {
//     return Ok(true);  // Already marked, skip phase 2
// }
// ... but in this case we want to document dependencies anyway

// DECISION: Phase 2 always runs for documentation purposes
let mut stack = Vec::new();
let deps = find_dependencies(envref, &plan, &mut stack).await?;
// Stack empty initially
// Step::Resource(Key("timestamp")) found:
//   - Add to dependencies: {Key("timestamp")}
//   - Push to stack: ["timestamp"]
//   - Get recipe for key "timestamp" from env
//   - Check recipe.volatile: true
//   - Plan already marked volatile, but document source:
//     plan.steps.push(Step::Info("Volatile due to dependency on key: Key(\"timestamp\")"))
//   - Pop from stack: []
// Step::Action, Step::Info: no resources, skip

// Result: dependencies verified, documentation added
```

#### Step 4: Context initialization and evaluation

```rust
// Context created with volatile flag from plan
let context = Context::new(envref, plan.is_volatile);  // is_volatile = true

// During evaluation of Step::Resource(Key("timestamp"))
// let asset_ref = asset_manager.get_asset(Key("timestamp")).await?;
// AssetManager checks:
//   - Get recipe for key
//   - Check if recipe.volatile: true
//   - RETURN NEW AssetRef (not cached)
//   - AssetRef created with is_volatile = true
//   - Set asset.metadata.is_volatile = true

// During evaluation of nested Step::Action("current_time")
// This command executes within volatile context
// If it calls context.evaluate() with another query:
//   let nested_ctx = context.with_volatile(false);
//   // But context.is_volatile = true, so:
//   nested_ctx.is_volatile = false || true = true
//   // Propagates volatility downward

// Final asset status: Status::Volatile (not Status::Ready)
// Metadata: is_volatile = true, status = Volatile
```

### Expected Behavior

**Query submission:**
```
Query: "timestamp/v/to_upper"
```

**Plan after Phase 1 and Phase 2:**
```rust
Plan {
    steps: [
        Step::Resource(Key("timestamp")),
        Step::Action("v"),
        Step::Info("Volatile due to instruction 'v'"),
        Step::Action("to_upper"),
        Step::Info("Volatile due to dependency on key: Key(\"timestamp\")")
    ],
    is_volatile: true,
}
```

**Asset after evaluation:**
```rust
AssetRef {
    status: Status::Volatile,
    data: Some(Arc::new(Value::String("2026-02-17T14:23:45Z".to_uppercase()))),
    metadata: MetadataRecord {
        is_volatile: true,
        status: Status::Volatile,
        // ... other fields
    },
    is_volatile: true,  // AssetData field
}
```

**Behavior on second request for same query:**
- NEW AssetRef created (not cached)
- New evaluation triggered
- Returns fresh timestamp
- Previous asset discarded

### How the System Handles It Correctly

1. **Phase 1 marks due to instruction** - PlanBuilder::mark_volatile() called for `v` instruction, sets flag and adds Step::Info.

2. **Phase 2 documents dependencies** - Even though plan is already volatile, Phase 2 runs to find and document which assets contributed to volatility (for debugging/auditing).

3. **AssetManager doesn't cache volatile** - When asset_manager.get_asset(Key("timestamp")) called, checks recipe.volatile and returns new AssetRef every time, never reusing.

4. **Context propagates volatility** - Any nested queries evaluated within volatile context inherit volatility via context.with_volatile() logic.

5. **Metadata marks state as volatile** - Status::Volatile variant tells consumers "this value should be used once, then consider expired."

---

## Edge Case 3: Context Propagation Through Nested Evaluations

### Scenario

A volatile command triggers nested query evaluation during execution. The nested query references non-volatile dependencies, but must inherit volatility from parent context. Demonstrates that volatility is contagious throughout the evaluation tree.

### Conceptual Code

#### Setup: Register commands with nested evaluation

```rust
// Command that performs context.evaluate() internally (part of command library)
fn complex_transform(
    state: &State<Value>,
    query: String,
    context: &Context,
) -> Result<Value, Error> {
    // Perform a secondary query evaluation within command implementation
    let nested_query = parse_query(&query)?;
    let nested_result = context.evaluate(&nested_query).await?;

    // Transform input state using nested result
    let input = state.try_into_string()?;
    let nested_str = nested_result.try_into_string()?;
    Ok(Value::from(format!("{}-{}", input, nested_str)))
}

// Register both volatile and non-volatile commands
let cmr = env.get_mut_command_registry();
register_command!(cmr, fn volatile_source() -> result
    label: "Volatile Source"
    volatile: true  // This command is volatile
)?;

register_command!(cmr, async fn complex_transform(state, query: String, context) -> result
    label: "Complex Transform"
    volatile: false  // Command itself is not volatile, but may inherit from context
)?;

register_command!(cmr, fn store_value(state, context) -> result
    label: "Store Value"
    volatile: false
)?;
```

#### Step 1: User query that chains volatile + nested evaluation

```rust
// Query: "volatile_source/complex_transform-'data/store_value'
// This means:
// 1. Call volatile_source (marked volatile)
// 2. Pass result to complex_transform with nested query "data/store_value"
//    (which internally calls context.evaluate("data/store_value"))
// 3. Result passed to store_value

let query = parse_query("volatile_source/complex_transform-'data/store_value").ok();

// Phase 1: Build plan
let mut pb = PlanBuilder::new(query, cmr);
let mut plan = pb.build()?;
// plan.steps = [
//   Step::Action("volatile_source"),
//   Step::Action("complex_transform", [Value::Query("data/store_value")]),
//   Step::Action("store_value")
// ]
// Phase 1 volatility check:
//   - Step::Action("volatile_source"): command volatile? YES
//   - pb.mark_volatile("Volatile due to command 'volatile_source'")
//   - plan.is_volatile = true
//   - Step::Action("complex_transform"): already volatile, skip
//   - Step::Action("store_value"): already volatile, skip

plan.is_volatile = true;
```

#### Step 2: Phase 2 dependency analysis

```rust
// find_dependencies() called
let mut stack = Vec::new();
let deps = find_dependencies(envref, &plan, &mut stack).await?;

// Steps processed:
// Step::Action steps: no Step::Resource or Step::Evaluate, so no asset dependencies

deps = {};  // No external asset dependencies
plan.is_volatile = true;  // Unchanged from Phase 1
```

#### Step 3: Context initialization and evaluation

```rust
// Interpreter creates context with volatile flag from plan
let context = Context::new(envref, plan.is_volatile);
// context.is_volatile = true

// Evaluation begins:
// Step 1: Execute Step::Action("volatile_source")
//   - Call volatile_source() function
//   - Returns Value::String("2026-02-17")
//   - Store in state as input for next step

// Step 2: Execute Step::Action("complex_transform", [Value::Query(...)])
//   - Call complex_transform(state, "data/store_value", context)
//   - INSIDE complex_transform:
//     {
//       let nested_query = parse_query("data/store_value")?;
//
//       // Evaluate nested query WITHIN volatile context
//       // Complex question: Should nested context inherit volatility?
//       // YES - context.evaluate() passes current context forward
//
//       // Create child context for nested evaluation
//       let nested_ctx = context.with_volatile(false);
//       // Inheritance logic:
//       //   nested_ctx.is_volatile = false || context.is_volatile
//       //   nested_ctx.is_volatile = false || true
//       //   nested_ctx.is_volatile = true  <- INHERITED
//
//       // Call context.evaluate(nested_query) with inherited volatile flag
//       // This builds a NEW plan for "data/store_value"
//       let nested_plan = make_plan(envref.clone(), nested_query).await?;
//       // nested_plan.is_volatile = false (no volatile commands)
//
//       // BUT: nested evaluation happens in volatile context
//       // Interpreter sees context.is_volatile = true
//       // Creates nested Context with is_volatile = true
//       let nested_context = Context::new(envref, true);  // From parent context
//
//       // Evaluate nested_plan with nested_context
//       let nested_result = evaluate_plan(envref.clone(), &nested_plan, nested_context).await?;
//       // Even though nested_plan is not inherently volatile,
//       // Metadata of result has is_volatile = true (from nested context)
//
//       // Asset for "data/store_value" created:
//       // - status: Status::Volatile (because context.is_volatile = true)
//       // - metadata.is_volatile = true
//       // - NOT cached in AssetManager (volatile!)
//
//       // Result passed back to outer command
//       Ok(Value::from("2026-02-17-data_value"))
//     }
//   - Output state: Value::String("2026-02-17-data_value")

// Step 3: Execute Step::Action("store_value")
//   - Call store_value(state, context) with context.is_volatile = true
//   - This command may write to store, but marked as volatile
//   - Store knows it's temporary data
```

#### Step 4: Final asset state

```rust
// Main query completes evaluation
// Final asset created for original query
let final_asset = AssetRef {
    status: Status::Volatile,  // Inherited from context
    data: Some(Arc::new(Value::String("2026-02-17-data_value"))),
    metadata: MetadataRecord {
        is_volatile: true,
        status: Status::Volatile,
        log: [
            LogEntry { message: "Evaluated volatile_source" },
            LogEntry { message: "Evaluated complex_transform with nested query" },
            LogEntry { message: "Evaluated store_value" }
        ],
        // ... other fields
    },
    is_volatile: true,
};

// Nested asset for "data/store_value" also created as volatile
// NOT cached anywhere
// Both assets will be considered expired on next request
```

### Expected Behavior

**Query submission:**
```
Query: "volatile_source/complex_transform-'data/store_value'/store_value"
```

**Plan marked volatile:**
- Due to `volatile_source` command (Phase 1)

**Evaluation flow:**
```
1. volatile_source() executes         <- returns "2026-02-17"
2. complex_transform() executes       <- with context.is_volatile = true
   2a. Nested query "data/store_value" evaluated in VOLATILE context
   2b. Creates asset with Status::Volatile (inherited from context)
   2c. Returns "data_value" value
3. store_value() executes with "2026-02-17-data_value"
```

**Final asset:**
```rust
{
    status: Volatile,
    data: "2026-02-17-data_value",
    is_volatile: true,
    metadata.is_volatile: true,
}
```

**On second request for same query:**
- Previous volatile asset NOT reused
- NEW evaluation triggered
- Fresh timestamp and nested query results
- Previous data discarded

### How the System Handles It Correctly

1. **Context carries volatility flag** - Created with `context.is_volatile` from Plan at evaluation start.

2. **Propagation via context.with_volatile()** - Child contexts inherit parent volatility via `new_ctx.is_volatile = override || self.is_volatile`.

3. **Nested assets inherit volatility** - When context.evaluate() called, new assets created in that context inherit volatile status from context flag.

4. **No caching for volatile assets** - AssetManager checks both recipe.volatile AND context.is_volatile when deciding whether to cache.

5. **Contagion rule implemented** - Any asset produced within a volatile context becomes volatile, even if its plan itself isn't marked volatile.

---

## Edge Case 4: Mixed Volatility in Dependency Graph

### Scenario

A complex query references multiple asset dependencies, some volatile and some not. The plan must correctly:
- Mark itself volatile if ANY dependency is volatile
- Document all volatility sources
- Create non-cached AssetRef for entire plan

### Conceptual Code

#### Setup: Mixed recipe environment

```rust
// Non-volatile recipe (pure computation)
let recipe_factorial = Recipe {
    query: parse_query("5/factorial").ok(),
    volatile: false,
};
env.register_recipe(Key::new().segment("factorial5"), recipe_factorial);

// Volatile recipe (depends on current state)
let recipe_current_time = Recipe {
    query: parse_query("current_time/to_string").ok(),
    volatile: true,
};
env.register_recipe(Key::new().segment("timestamp"), recipe_current_time);

// Non-volatile recipe depending on other recipes
let recipe_combined = Recipe {
    query: parse_query("factorial5/append-': '/timestamp").ok(),
    volatile: false,  // But transitively depends on volatile key!
};
env.register_recipe(Key::new().segment("combined"), recipe_combined);
```

#### Step 1: Query references mixed dependencies

```rust
// Query requires both volatile and non-volatile keys
// "combined" recipe includes "factorial5" (non-volatile) and "timestamp" (volatile)
let query = parse_query("combined/to_upper").ok();

// Phase 1: Build plan
let mut pb = PlanBuilder::new(query, cmr);
let mut plan = pb.build()?;
// plan.steps = [
//   Step::Resource(Key("combined")),
//   Step::Action("to_upper")
// ]
// Phase 1 volatility check:
//   - Step::Resource: not an action, skip volatility check
//   - Step::Action("to_upper"): not volatile
//   plan.is_volatile = false  // <- CURRENTLY NON-VOLATILE
```

#### Step 2: Phase 2 discovers transitive volatility

```rust
// find_dependencies() called
let mut stack = Vec::new();
let deps = find_dependencies(envref, &plan, &mut stack).await?;

// Dependency resolution:
// Step::Resource(Key("combined")) found:
//   - stack is empty, add "combined": ["combined"]
//   - Get recipe for "combined" from env
//   - recipe.query = "factorial5/append-': '/timestamp"
//   - Parse and resolve recipe query:
//     - Step::Resource(Key("factorial5"))
//       - stack: ["combined", "factorial5"]
//       - recipe.volatile = false (OK, continue)
//       - Pop: ["combined"]
//     - Step::Action("append", [...])
//       - No resource, skip
//     - Step::Resource(Key("timestamp"))
//       - stack: ["combined", "timestamp"]
//       - Get recipe for "timestamp"
//       - recipe.volatile = true <- FOUND VOLATILE DEPENDENCY
//       - Add Step::Info: "Volatile due to dependency on key: Key(\"timestamp\")"
//       - Pop: ["combined"]
//   - Pop: []

// has_volatile_dependencies() result:
// Checked all dependencies:
//   - "factorial5": not volatile
//   - "timestamp": volatile <- FOUND
// plan.is_volatile = true  // Updated!
// plan.steps.push(Step::Info("Volatile due to dependency on volatile key: Key(\"timestamp\")"))

deps = {Key("factorial5"), Key("timestamp")};
plan.is_volatile = true;
```

#### Step 3: Asset creation with correct volatility

```rust
// Context initialized with updated volatile flag
let context = Context::new(envref, plan.is_volatile);  // is_volatile = true

// Evaluation starts:
// Step::Resource(Key("combined")):
//   - AssetManager.get_asset(Key("combined"))
//   - Get recipe for "combined"
//   - Check recipe.volatile: false
//   - BUT check plan.is_volatile: true (entire plan volatile)
//   - RETURN NEW AssetRef (not cached)
//
//   During evaluation of "combined" recipe query:
//   - Step::Resource(Key("factorial5")):
//     - Get recipe for "factorial5"
//     - Check recipe.volatile: false
//     - Check context.is_volatile: true
//     - RETURN NEW AssetRef (context is volatile, propagate)
//
//   - Step::Resource(Key("timestamp")):
//     - Get recipe for "timestamp"
//     - Check recipe.volatile: true
//     - RETURN NEW AssetRef (always for volatile recipes)
//
//   - Step::Action("append", ...):
//     - Combines: "120: 2026-02-17T14:23:45Z"
//
// Result of "combined" evaluation:
//   - status: Status::Volatile (from context)
//   - value: "120: 2026-02-17T14:23:45Z"
//   - metadata.is_volatile = true

// Step::Action("to_upper"):
//   - Transforms: "120: 2026-02-17T14:23:45Z" -> "120: 2026-02-17T14:23:45Z"
//   - Result inherits volatility from input state

// Final asset for "combined/to_upper":
//   - status: Status::Volatile
//   - value: "120: 2026-02-17T14:23:45Z"
//   - metadata.is_volatile = true
//   - NOT cached (volatile!)
```

### Expected Behavior

**Query submission:**
```
Query: "combined/to_upper"
```

**Plan after Phase 1 and Phase 2:**
```rust
Plan {
    steps: [
        Step::Resource(Key("combined")),
        Step::Info("Volatile due to dependency on volatile key: Key(\"timestamp\")"),
        Step::Action("to_upper")
    ],
    is_volatile: true,  // Updated from Phase 2
}
```

**Execution trace:**
```
1. Evaluate "combined" (non-volatile recipe, but volatile context)
   1a. Evaluate "factorial5" (non-volatile, result: "120")
   1b. Evaluate "timestamp" (volatile, result: "2026-02-17T14:23:45Z")
   1c. Append: "120: 2026-02-17T14:23:45Z"
   Result: Status::Volatile (inherited from context)

2. Apply "to_upper": "120: 2026-02-17T14:23:45Z"
   Result: Status::Volatile (propagated from input)

Final: "120: 2026-02-17T14:23:45Z" with Status::Volatile
```

### How the System Handles It Correctly

1. **Phase 1 conservatively assumes non-volatile** - Only looks at direct commands/instructions, not recipes.

2. **Phase 2 discovers transitive volatility** - Recursively resolves all asset dependencies and checks each recipe.volatile flag.

3. **Early updates to plan.is_volatile** - Once any volatile dependency found, plan updated and never downgraded.

4. **Context volatility propagates** - All assets created during evaluation inherit context.is_volatile flag.

5. **Multiple sources documented** - Each volatility source (direct command, volatile instruction, volatile dependency) adds Step::Info for auditability.

6. **Consistent caching strategy** - AssetManager never caches volatile assets, regardless of source (recipe.volatile OR context.is_volatile OR plan.is_volatile).

---

## Edge Case 5: Volatile Status Transition via to_override()

### Scenario

An asset with Status::Volatile is converted to Status::Override via the `to_override()` method. This prevents re-evaluation while preserving the existing value. Shows the ONLY legitimate state transition for volatile assets.

### Conceptual Code

#### Setup: Create volatile asset

```rust
// Query: "current_time/to_string"
// Produces volatile asset with current timestamp
let query = parse_query("current_time/to_string").ok();

let context = Context::new(envref, true);  // is_volatile = true
let asset_ref = AssetManager::get_asset_from_query(query).await?;

// Asset after evaluation:
// status: Status::Volatile
// data: "2026-02-17T14:23:45Z"
// metadata.is_volatile: true
```

#### Step 1: Call to_override() to freeze value

```rust
// Consumer decides: "I want to use this timestamp exactly once, then preserve it"
// (e.g., for audit trail or reproducible test)

let result = asset_ref.to_override().await?;
// OK(())

// Internal state change in AssetData:
// Before:
//   status: Status::Volatile
//   data: Some("2026-02-17T14:23:45Z")
//   metadata.is_volatile: true

// After:
//   status: Status::Override    <- Changed
//   data: Some("2026-02-17T14:23:45Z")  <- Preserved
//   metadata.is_volatile: true  <- Unchanged (for audit)

// Key behavior: to_override() does NOT trigger re-evaluation
// It simply marks: "Do not re-evaluate, use this value"
```

#### Step 2: Subsequent access to overridden asset

```rust
// Same query requested again: "current_time/to_string"
// Previous request: Status::Volatile (would normally trigger new eval)
// Current request: Status::Override (do not re-evaluate)

let asset_ref2 = AssetManager::get_asset_from_query(query).await?;

// AssetManager behavior with Override status:
// - Check cache for key
// - Found: AssetRef with status = Override
// - Check if Override: YES
// - Return cached asset (do not create new AssetRef)

// Result:
// status: Status::Override
// data: "2026-02-17T14:23:45Z"  <- SAME TIMESTAMP as first request
// metadata.is_volatile: true  <- Preserved for audit

// Important: This is NOT like Expired or Ready caching
// Override is explicit "freeze this value, do not change"
// Useful for reproducible test scenarios or audit trails
```

#### Step 3: Attempting transition back to Volatile

```rust
// Question: Can Override -> Volatile?
// Answer: NO - to_override() is ONE-WAY transition

// Attempting to unfreeze (hypothetical - not allowed):
// asset_ref2.to_volatile();  // ERROR - no such method

// Why? Volatility must be determined upfront from plan
// Cannot change mid-execution

// Override is terminal decision made by consumer
// System respects it
```

#### Step 4: Error cases for to_override()

```rust
// Case 1: Asset in Processing state
// status: Status::Processing
// Calling to_override():
//   - Send cancel message via service_tx
//   - Set value to Value::none()
//   - Set status to Status::Override
//   - Result: In-flight evaluation cancelled, frozen with no value

let processing_asset = // ... asset in Processing state

let result = processing_asset.to_override().await;
// OK(())
// Asset now frozen with null value (evaluation was cancelled)

// Case 2: Asset in Error state
// status: Status::Error
// data: None
// error_data: Some(Error { ... })
// Calling to_override():
//   - Keep error_data intact (for audit)
//   - Keep data as None
//   - Set status to Status::Override
//   - Result: Error frozen, no re-evaluation will occur

let error_asset = // ... asset in Error state

let result = error_asset.to_override().await;
// OK(())
// Asset frozen in Error state, no retry possible

// Case 3: Asset already Override
// status: Status::Override
// Calling to_override():
//   - No-op (already Override)
//   - Return OK(())

let override_asset = // ... asset already Override

let result = override_asset.to_override().await;
// OK(()) - Already in desired state
```

### Expected Behavior

**Operation sequence:**

```
Time T1: Query "current_time/to_string" -> Status::Volatile, data="2026-02-17T14:23:45Z"
Time T1: Call asset_ref.to_override() -> Status::Override, data="2026-02-17T14:23:45Z"
Time T2: Query "current_time/to_string" again -> Status::Override, data="2026-02-17T14:23:45Z"
  (Note: Data is SAME, not refreshed - evaluation did not run)
```

**State transitions allowed:**

```
Volatile -> Override: YES (intentional freeze)
Volatile -> Ready: NO (not permitted)
Volatile -> Expired: NO (not permitted)
Override -> Volatile: NO (not permitted)
Override -> Override: YES (idempotent, no-op)
```

### How the System Handles It Correctly

1. **One-way transition** - `to_override()` is the ONLY permitted state transition for volatile assets. Enforced by having no other transition methods.

2. **Preserves existing value** - For assets with data (Ready, Partial, Expired, Volatile), `to_override()` keeps the data and just changes status.

3. **Handles in-flight cancellation** - For processing assets, sends cancel message and freezes with null value.

4. **Idempotent** - Calling `to_override()` multiple times has same effect as calling once (already Override = no-op).

5. **Async and safe** - Uses existing async RwLock pattern for exclusive access during transition.

6. **For auditability** - Metadata.is_volatile remains true even after Override, so audit trails show original volatility source.

---

## Summary: Validation Checklist

The five edge cases validate that the volatility system:

- ✅ **Circular dependency detection** - Prevents infinite loops before execution
- ✅ **Two-phase checking** - Separates command-based checks (Phase 1) from dependency-based checks (Phase 2)
- ✅ **Context propagation** - Volatility contagious through nested evaluations
- ✅ **Mixed dependencies** - Correctly marks plan volatile if ANY dependency is volatile
- ✅ **Explicit override** - One-way transition to freeze volatile assets
- ✅ **No unwrap/expect** - All error cases return `Result`
- ✅ **Explicit match statements** - All enum matching includes all variants
- ✅ **Async patterns** - Follows existing context locks and trait patterns

All examples follow CLAUDE.md constraints and codebase conventions.
