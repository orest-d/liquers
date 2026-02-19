# Phase 3 Quick Reference: Integration Test Specifications

**Quick Lookup Guide for Phase 4 Implementation**

---

## Test List by Category

### Full Pipeline (Tests 1-3)

| # | Name | Query | Focus | Status Check |
|---|------|-------|-------|--------------|
| 1 | Simple Volatile Command | `"current_time"` | End-to-end volatile command pipeline | Status::Volatile |
| 2 | V Instruction | `"data/v/to_string"` | Explicit volatility via 'v' action | Step::Info present |
| 3 | Volatile Dependency | `"volatile_key"` (phase 2 check) | Dependency chain volatility propagation | plan.is_volatile after phase 2 |

### Volatility Instruction (Test 5)

**Positions tested:**
- `"v"` (alone)
- `"data/v"` (after command)
- `"data/v/to_string"` (middle)
- `"data/to_string/v"` (end)

**Assertion:** All variants have `plan.is_volatile == true`

### Circular Dependencies (Tests 6-8)

| # | Pattern | Error Expected | Stack Depth |
|---|---------|-----------------|-------------|
| 6 | Direct: A → B → A | Yes | 2 |
| 7 | Indirect: A → B → C → A | Yes | 3 |
| 8 | Self: A → A | Yes | 1 (immediate) |

**Key:** All return `Error::general_error("Circular dependency detected...")`

### Serialization (Tests 9-11)

| # | Field | Type | Format |
|---|-------|------|--------|
| 9 | Plan.is_volatile | bool | JSON `"is_volatile": true` |
| 10 | MetadataRecord.is_volatile | bool | Always present, required field |
| 11 | Status enum | Enum variant | Serializes as `"Volatile"` string |

**Pattern:** Serialize → JSON → Deserialize → Verify round-trip

### Concurrency (Tests 12-13)

| # | Scenario | Requests | Volatile IDs | Non-Volatile IDs |
|---|----------|----------|--------------|------------------|
| 12 | All volatile | 5x same volatile query | All unique (5 different) | N/A |
| 13 | Mixed | Alternating vol/non-vol | All unique | All same (cached) |

**Assertion:** `unique_ids.len() == 5 && cached_ids.len() == 1`

### Cross-Module (Tests 14-15)

| # | Pipeline | Layer Count | Key Check |
|---|----------|-------------|-----------|
| 14 | Query → Plan → Context → Command | 4 | Context.is_volatile() matches plan.is_volatile |
| 15 | Query → AssetManager → Interpreter → Asset | 4 | Caching behavior (volatile new, non-volatile cached) |

### Performance (Tests 16-17)

| # | Scenario | Depth/Complexity | Threshold | Focus |
|---|----------|------------------|-----------|-------|
| 16 | Linear chain | 100 recipes deep | < 500ms | Linear recursion performance |
| 17 | Cycle detection | Complex graph | < 100ms | Early termination |

### Corner Cases (Tests 18-22)

| # | Test | Scenario | Validates |
|---|------|----------|-----------|
| 18 | Large chain | 1000-deep dependencies | No stack overflow, < 1000ms |
| 19 | Many volatiles | 10 volatile commands chained | First-detection optimization |
| 20 | State transitions | Volatile → Override | AssetRef::to_override() method |
| 21 | Error propagation | Volatile command throws error | Metadata.is_volatile persists with error |
| 22 | Missing recipe | Phase 2 recipe lookup failure | Graceful error handling |

### End-to-End (Test 23)

**Scenario:** Full production-like test
- 3+ commands (volatile + non-volatile)
- 2+ recipes
- 3 queries (direct volatile, dependent volatile, 'v' instruction)
- Verify: execution, metadata, caching, results

---

## Code Template Quick Reference

### Environment Setup Pattern

```rust
type CommandEnvironment = SimpleEnvironment<Value>;
let mut env = SimpleEnvironment::<Value>::new();

// Register commands
fn my_cmd(_state: &State<Value>) -> Result<Value, Error> { ... }
let cr = &mut env.command_registry;
register_command!(cr, fn my_cmd(state) -> result
    volatile: true // if volatile
)?;

let envref = env.to_ref();
```

### Recipe Pattern

```rust
let mut recipe_provider = env.get_mut_recipe_provider();
let key = Key::new_root("recipe-name");
let recipe = Recipe::new(
    key.clone(),
    Query::from_string("query/string")?,
    true // is_volatile flag
);
recipe_provider.add_recipe(recipe);
```

### Plan Building Pattern

```rust
let plan = make_plan(envref.clone(), "query/string").await?;
assert!(plan.is_volatile);
// Verify Step::Info present
let has_info = plan.steps.iter().any(|step| {
    matches!(step, Step::Info(msg) if msg.contains("Volatile"))
});
assert!(has_info);
```

### Evaluation Pattern

```rust
let state = evaluate_plan(envref.clone(), &plan).await?;
let metadata = state.metadata();
assert_eq!(metadata.status, Status::Volatile);
assert!(metadata.is_volatile());
```

### Concurrency Pattern

```rust
let envref = Arc::new(env.to_ref());
let mut handles = vec![];
for _ in 0..N {
    let env_clone = envref.clone();
    let handle = tokio::spawn(async move {
        // Make request in spawned task
        env_clone.get_asset_manager().get_asset_from_query(&query).await
    });
    handles.push(handle);
}
// Collect results and verify
```

### Circular Dependency Pattern

```rust
let result = make_plan(envref.clone(), "key-a").await;
assert!(result.is_err());
let error = result.unwrap_err();
assert!(error.message().contains("Circular dependency"));
```

### Serialization Pattern

```rust
let original = /* create object */;
let json = serde_json::to_string(&original)?;
assert!(json.contains("expected_field"));
let restored: Type = serde_json::from_str(&json)?;
assert_eq!(restored.field, original.field);
```

---

## Validation Assertions Checklist

### After `make_plan()` for volatile query:
- [ ] `plan.is_volatile == true`
- [ ] Plan contains at least one `Step::Info`
- [ ] `Step::Info` message explains volatility source

### After `evaluate_plan()` for volatile query:
- [ ] `metadata.is_volatile() == true`
- [ ] `metadata.status == Status::Volatile`
- [ ] Command executed without error

### For circular dependency queries:
- [ ] `make_plan()` returns `Err`
- [ ] Error message contains "Circular dependency"
- [ ] Response time < 100ms (no hang)

### For concurrent volatile requests:
- [ ] All AssetRef.id values are unique
- [ ] No AssetRefs are reused/cached
- [ ] All assets evaluate successfully

### For serialization:
- [ ] is_volatile field present in JSON
- [ ] Field value matches after round-trip
- [ ] No `#[serde(default)]` needed (required field)

### For non-volatile queries:
- [ ] `plan.is_volatile == false`
- [ ] Second request returns SAME cached AssetRef
- [ ] AssetRef.id identical for repeated queries

---

## Performance Baselines

| Test | Operation | Threshold | Measured |
|------|-----------|-----------|----------|
| 16 | 100-deep linear chain dependency check | < 500ms | TBD |
| 17 | Circular dependency detection (complex graph) | < 100ms | TBD |

**Baseline measurement command:**
```bash
cargo test -p liquers-core --test volatility_integration \
  test_dependency_checking_performance_linear_chain -- --nocapture
```

---

## Common Implementation Patterns

### Pattern: Two-Phase Volatility Checking

**Phase 1 (PlanBuilder::build):**
```rust
// Check each Step::Action for volatile command
// Check for 'v' instruction
// Set plan.is_volatile = true if found
```

**Phase 2 (make_plan after build):**
```rust
// Call find_dependencies(envref, &plan, &mut stack)
// For each dependency key, get recipe and check recipe.volatile
// If any volatile: update plan.is_volatile and add Step::Info
```

### Pattern: Stack-Based Cycle Detection

```rust
async fn find_dependencies<E: Environment>(
    envref: EnvRef<E>,
    plan: &Plan,
    stack: &mut Vec<Key>,  // Track path
) -> Result<HashSet<Key>, Error> {
    for key_ref in plan.references() {
        if stack.contains(&key_ref) {
            return Err(Error::general_error(
                format!("Circular dependency: {:?}", key_ref)
            ));
        }
        stack.push(key_ref.clone());
        // Recurse
        stack.pop();
    }
    Ok(dependencies)
}
```

### Pattern: Volatile Asset Non-Caching

```rust
// In AssetManager::get_asset_from_query()
let plan = make_plan(query).await?;
if plan.is_volatile {
    // Always create new AssetRef
    let asset_ref = AssetRef::new(/* ... */);
    // Do NOT add to internal cache
    return Ok(asset_ref);
} else {
    // Use normal caching
    self.get_or_create_cached(key)
}
```

---

## Test Execution Commands

```bash
# Run all volatility integration tests
cargo test -p liquers-core --test volatility_integration

# Run specific test
cargo test -p liquers-core --test volatility_integration test_volatile_query_to_asset_simple

# Run with output (nocapture)
cargo test -p liquers-core --test volatility_integration -- --nocapture

# Run single-threaded (for debugging)
cargo test -p liquers-core --test volatility_integration -- --test-threads=1

# Run performance tests only
cargo test -p liquers-core --test volatility_integration test_dependency_checking_performance

# Run with all features
cargo test -p liquers-core --test volatility_integration --all-features
```

---

## Integration Test File Structure

```
liquers-core/tests/volatility_integration.rs
├── Imports and setup
├── Helper functions for test setup
├── Full Pipeline Tests
│   ├── test_volatile_query_to_asset_simple
│   ├── test_v_instruction_marks_plan_volatile
│   └── test_volatile_dependency_chain_propagation
├── Volatility Instruction Tests
│   └── test_v_instruction_position_variations (4 queries)
├── Circular Dependency Tests
│   ├── test_circular_dependency_direct_detection
│   ├── test_circular_dependency_indirect_chain
│   └── test_circular_dependency_self_reference
├── Serialization Tests
│   ├── test_plan_is_volatile_serialization_roundtrip
│   ├── test_metadata_record_is_volatile_serialization
│   └── test_status_volatile_variant_serialization
├── Concurrency Tests
│   ├── test_concurrent_volatile_asset_requests
│   └── test_concurrent_volatile_and_nonvolatile_requests
├── Cross-Module Tests
│   ├── test_interpreter_plan_builder_context_pipeline
│   └── test_asset_manager_interpreter_query_flow
├── Performance Tests
│   ├── test_dependency_checking_performance_linear_chain
│   └── test_circular_dependency_detection_performance
├── Corner Cases
│   ├── test_large_dependency_chain_volatile_root
│   ├── test_many_volatile_commands_single_plan
│   ├── test_volatile_status_state_transitions
│   ├── test_error_propagation_volatile_command
│   └── test_volatile_dependency_error_recipe_not_found
└── End-to-End Test
    └── test_end_to_end_volatile_query_execution
```

---

## Quick Debugging Checklist

If a test fails:

1. **Check plan.is_volatile field**
   - Is `make_plan()` correctly identifying volatility?
   - Are commands registered with `volatile: true`?
   - Is Phase 2 dependency checking running?

2. **Check Status::Volatile handling**
   - Does Status enum have Volatile variant?
   - Are all match statements explicit (no `_ =>`)?
   - Do has_data(), is_finished() methods handle Volatile?

3. **Check MetadataRecord.is_volatile**
   - Is field always serialized?
   - Is is_volatile() helper returning correct value?
   - Does serialization preserve the field?

4. **Check Context initialization**
   - Is Context::new() being called with is_volatile from Plan?
   - Is Context::is_volatile() returning correct value?

5. **Check AssetManager caching**
   - For volatile queries: Are IDs unique?
   - For non-volatile queries: Are IDs identical?
   - Are volatile queries bypassing the cache?

6. **Check circular dependency detection**
   - Is stack tracking working correctly?
   - Is cycle detected before infinite recursion?
   - Does error message contain query information?

---

## Key Files and Line Numbers (Phase 2 Architecture)

| Module | File | Lines | Component |
|--------|------|-------|-----------|
| Status enum | `liquers-core/src/metadata.rs` | ~11-47 | Add Volatile variant |
| MetadataRecord | `liquers-core/src/metadata.rs` | ~470-515 | Add is_volatile field |
| Plan struct | `liquers-core/src/plan.rs` | TBD | Add is_volatile field |
| PlanBuilder | `liquers-core/src/plan.rs` | TBD | Phase 1 & 2 checking |
| Context struct | `liquers-core/src/context.rs` | TBD | Add is_volatile field |
| AssetData | `liquers-core/src/assets.rs` | ~187 | Add is_volatile field |
| AssetManager | `liquers-core/src/assets.rs` | TBD | Volatile caching logic |
| IsVolatile trait | `liquers-core/src/interpreter.rs` | ~316-422 | Existing, update Plan impl |

---

## Notes

- All tests are async (`#[tokio::test]`)
- All tests follow CLAUDE.md conventions (no unwrap/expect in library-like tests)
- Tests use Result<(), Box<dyn std::error::Error>> return type
- Full code templates provided in phase3-integration-tests.md
- Expected implementation: ~1500 lines of Rust test code

---

**Last Updated:** 2026-02-17
**Status:** Ready for Phase 4 Implementation
