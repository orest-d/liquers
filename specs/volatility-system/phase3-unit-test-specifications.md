# Phase 3: Unit Test Specifications - Volatility System

## Overview

This document specifies comprehensive unit tests for the volatility system implementation across liquers-core modules. Tests are organized by module and functionality area, following Liquers testing conventions (see CLAUDE.md).

## Test Naming Conventions

- Sync tests: `#[test] fn test_<module>_<feature>_<condition>()`
- Async tests: `#[tokio::test] async fn test_<module>_<feature>_<condition>()`
- Location: Unit tests in same file as implementation, integration tests in `tests/`
- Error cases: `test_<module>_<feature>_error_<error_type>()`

---

## Module: liquers_core::metadata

### Tests for Status::Volatile Variant

#### Test 1: Status::Volatile has_data()
**What it tests:** `Status::Volatile` variant returns `true` for `has_data()` method (indicating value is available).

**Given:**
- `Status::Volatile` instance

**Expected outcome:**
- `status.has_data()` returns `true`
- Matches behavior of `Status::Ready` and `Status::Partial`

**File location:** `liquers-core/src/metadata.rs` (tests module at end of file)

**Test function signature:**
```rust
#[test]
fn test_status_volatile_has_data() {
    let status = Status::Volatile;
    assert!(status.has_data());
}
```

---

#### Test 2: Status::Volatile is_finished()
**What it tests:** `Status::Volatile` variant returns `true` for `is_finished()` method (indicating asset evaluation is complete).

**Given:**
- `Status::Volatile` instance

**Expected outcome:**
- `status.is_finished()` returns `true`
- Matches behavior of `Status::Ready` and `Status::Expired`
- Indicates asset is in a terminal state

**File location:** `liquers-core/src/metadata.rs` (tests module at end of file)

**Test function signature:**
```rust
#[test]
fn test_status_volatile_is_finished() {
    let status = Status::Volatile;
    assert!(status.is_finished());
}
```

---

#### Test 3: Status::Volatile can_have_tracked_dependencies()
**What it tests:** `Status::Volatile` variant returns `false` for `can_have_tracked_dependencies()` (volatile assets are not revalidated).

**Given:**
- `Status::Volatile` instance

**Expected outcome:**
- `status.can_have_tracked_dependencies()` returns `false`
- Matches behavior of `Status::Expired` (both are terminal states that won't be revalidated)
- Ensures volatile assets are not subject to dependency tracking

**File location:** `liquers-core/src/metadata.rs` (tests module at end of file)

**Test function signature:**
```rust
#[test]
fn test_status_volatile_cannot_have_tracked_dependencies() {
    let status = Status::Volatile;
    assert!(!status.can_have_tracked_dependencies());
}
```

---

#### Test 4: Status::Volatile serialization
**What it tests:** `Status::Volatile` variant serializes and deserializes correctly as JSON.

**Given:**
- `Status::Volatile` instance
- Round-trip serialization via `serde_json`

**Expected outcome:**
- Serializes to JSON string `"Volatile"`
- Deserializes back to `Status::Volatile` variant
- No data loss in round-trip

**File location:** `liquers-core/src/metadata.rs` (tests module at end of file)

**Test function signature:**
```rust
#[test]
fn test_status_volatile_serialization() {
    let status = Status::Volatile;
    let json = serde_json::to_string(&status).expect("serialize");
    assert_eq!(json, "\"Volatile\"");

    let deserialized: Status = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized, Status::Volatile);
}
```

---

#### Test 5: MetadataRecord.is_volatile() with volatile status
**What it tests:** `MetadataRecord::is_volatile()` helper returns `true` when status is `Status::Volatile`.

**Given:**
- `MetadataRecord` with `is_volatile = false` and `status = Status::Volatile`

**Expected outcome:**
- `record.is_volatile()` returns `true`
- Status variant takes precedence when set

**File location:** `liquers-core/src/metadata.rs` (tests module at end of file)

**Test function signature:**
```rust
#[test]
fn test_metadata_record_is_volatile_with_volatile_status() {
    let mut record = MetadataRecord::default();
    record.status = Status::Volatile;
    record.is_volatile = false;

    assert!(record.is_volatile());
}
```

---

#### Test 6: MetadataRecord.is_volatile() with volatile flag
**What it tests:** `MetadataRecord::is_volatile()` helper returns `true` when `is_volatile` flag is set (even if status is not Volatile yet).

**Given:**
- `MetadataRecord` with `is_volatile = true` and `status = Status::Processing` (in-flight asset)

**Expected outcome:**
- `record.is_volatile()` returns `true`
- Flag indicates volatility for assets still being evaluated

**File location:** `liquers-core/src/metadata.rs` (tests module at end of file)

**Test function signature:**
```rust
#[test]
fn test_metadata_record_is_volatile_with_flag() {
    let mut record = MetadataRecord::default();
    record.is_volatile = true;
    record.status = Status::Processing;

    assert!(record.is_volatile());
}
```

---

#### Test 7: MetadataRecord.is_volatile() false when neither set
**What it tests:** `MetadataRecord::is_volatile()` returns `false` when both flag and status indicate non-volatility.

**Given:**
- `MetadataRecord` with `is_volatile = false` and `status = Status::Ready`

**Expected outcome:**
- `record.is_volatile()` returns `false`

**File location:** `liquers-core/src/metadata.rs` (tests module at end of file)

**Test function signature:**
```rust
#[test]
fn test_metadata_record_is_volatile_false() {
    let record = MetadataRecord::default(); // is_volatile = false, status = None
    assert!(!record.is_volatile());
}
```

---

#### Test 8: MetadataRecord.is_volatile field in serialization
**What it tests:** `MetadataRecord.is_volatile` field is serialized and required during deserialization.

**Given:**
- `MetadataRecord` with `is_volatile = true`
- Round-trip JSON serialization

**Expected outcome:**
- `is_volatile` field appears in JSON as `"is_volatile": true`
- Deserialization requires field (no default)
- Value is preserved

**File location:** `liquers-core/src/metadata.rs` (tests module at end of file)

**Test function signature:**
```rust
#[test]
fn test_metadata_record_is_volatile_field_serialization() {
    let mut record = MetadataRecord::default();
    record.is_volatile = true;
    record.status = Status::Processing;

    let json = serde_json::to_string(&record).expect("serialize");
    assert!(json.contains("\"is_volatile\":true"));

    let deserialized: MetadataRecord = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.is_volatile, true);
}
```

---

## Module: liquers_core::plan

### Tests for Plan.is_volatile Field

#### Test 9: Plan.is_volatile field exists and defaults to false
**What it tests:** `Plan` struct has `is_volatile: bool` field that defaults to `false`.

**Given:**
- Empty Plan created via constructor

**Expected outcome:**
- `plan.is_volatile` field exists and is accessible
- Defaults to `false` for non-volatile plans
- Can be read and written

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[test]
fn test_plan_is_volatile_field_default() {
    let plan = Plan::new();
    assert!(!plan.is_volatile);
}
```

---

#### Test 10: Plan.is_volatile getter method
**What it tests:** `Plan::is_volatile()` getter method returns field value.

**Given:**
- Plan with `is_volatile = true`

**Expected outcome:**
- `plan.is_volatile()` returns `true`
- Getter encapsulates access to field

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[test]
fn test_plan_is_volatile_getter() {
    let mut plan = Plan::new();
    plan.is_volatile = true;
    assert!(plan.is_volatile());
}
```

---

#### Test 11: Plan.is_volatile field in serialization
**What it tests:** `Plan.is_volatile` field is serialized and required during deserialization.

**Given:**
- Plan with `is_volatile = true`
- Round-trip JSON serialization

**Expected outcome:**
- `is_volatile` field appears in JSON
- Deserialization requires field
- Value is preserved

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[test]
fn test_plan_is_volatile_field_serialization() {
    let mut plan = Plan::new();
    plan.is_volatile = true;

    let json = serde_json::to_string(&plan).expect("serialize");
    assert!(json.contains("\"is_volatile\":true"));

    let deserialized: Plan = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.is_volatile, true);
}
```

---

### Tests for PlanBuilder Volatility Marking

#### Test 12: PlanBuilder marks volatile for 'v' instruction
**What it tests:** `PlanBuilder` detects `v` instruction and marks plan as volatile.

**Given:**
- Query with `v` instruction: `"data/v/to_text"`
- PlanBuilder processes the query

**Expected outcome:**
- Resulting `Plan.is_volatile = true`
- `Step::Info` added explaining volatility source
- Message indicates "Volatile due to instruction 'v'"

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Setup code:**
```rust
// Register command metadata
let mut cmr = CommandMetadataRegistry::new();
register_test_command(&mut cmr, "to_text", false); // non-volatile command

// Parse query with 'v' instruction
let query = parse_query("data/v/to_text").expect("parse");
let mut pb = PlanBuilder::new(query, &cmr);
let plan = pb.build().expect("build");
```

**Expected structure:**
```rust
assert!(plan.is_volatile);
// Find Step::Info in plan indicating volatility reason
let has_info = plan.steps.iter().any(|s| {
    match s {
        Step::Info(msg) if msg.contains("Volatile due to instruction 'v'") => true,
        _ => false,
    }
});
assert!(has_info);
```

**Test function signature:**
```rust
#[tokio::test]
async fn test_plan_builder_marks_volatile_for_v_instruction() {
    // ... setup code ...

    let query = parse_query("data/v/to_text").expect("parse");
    let mut pb = PlanBuilder::new(query, &cmr);
    let plan = pb.build().expect("build");

    assert!(plan.is_volatile);
}
```

---

#### Test 13: PlanBuilder marks volatile for volatile command
**What it tests:** `PlanBuilder` detects volatile command via `CommandMetadata.volatile` flag and marks plan as volatile.

**Given:**
- Query calling a volatile command: `"random/to_text"`
- `random` command registered with `volatile = true`
- PlanBuilder processes the query

**Expected outcome:**
- Resulting `Plan.is_volatile = true`
- `Step::Info` added explaining volatility source
- Message indicates "Volatile due to command 'random'"

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Setup code:**
```rust
// Register volatile command
let mut cmr = CommandMetadataRegistry::new();
register_test_command(&mut cmr, "random", true); // volatile=true

// Parse query calling volatile command
let query = parse_query("random/to_text").expect("parse");
let mut pb = PlanBuilder::new(query, &cmr);
let plan = pb.build().expect("build");
```

**Expected structure:**
```rust
assert!(plan.is_volatile);
// Find Step::Info indicating volatility reason
let has_info = plan.steps.iter().any(|s| {
    match s {
        Step::Info(msg) if msg.contains("Volatile due to command 'random'") => true,
        _ => false,
    }
});
assert!(has_info);
```

**Test function signature:**
```rust
#[tokio::test]
async fn test_plan_builder_marks_volatile_for_volatile_command() {
    // ... setup code ...

    assert!(plan.is_volatile);
}
```

---

#### Test 14: PlanBuilder skips volatility checks after first volatile mark
**What it tests:** Once plan is marked volatile, subsequent checks are skipped (optimization).

**Given:**
- Query with multiple potential volatility sources: `"random/v/another_volatile_command"`
- Both `random` (volatile) and `another_volatile_command` (volatile) in chain

**Expected outcome:**
- `Plan.is_volatile = true`
- Only ONE `Step::Info` added (from first volatility source)
- Second command check skipped due to optimization

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_plan_builder_optimization_stops_after_volatile_mark() {
    // ... setup code with multiple volatile sources ...

    let plan = pb.build().expect("build");

    assert!(plan.is_volatile);
    // Count Step::Info entries - should be 1
    let info_count = plan.steps.iter().filter(|s| matches!(s, Step::Info(_))).count();
    assert_eq!(info_count, 1);
}
```

---

### Tests for Circular Dependency Detection

#### Test 15: find_dependencies() empty plan
**What it tests:** `find_dependencies()` returns empty set for plan with no asset references.

**Given:**
- Plan with only action steps (no asset dependencies)
- No `Step::Resource` or `Step::WithKey` steps

**Expected outcome:**
- Returns `Ok(HashSet::new())`
- No dependencies found
- No errors

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_find_dependencies_empty_plan() {
    let envref = setup_test_environment().await;
    let plan = Plan::new(); // Empty plan
    let mut stack = Vec::new();

    let deps = find_dependencies(envref, &plan, &mut stack)
        .await
        .expect("should succeed");

    assert!(deps.is_empty());
}
```

---

#### Test 16: find_dependencies() single asset dependency
**What it tests:** `find_dependencies()` returns single key for plan with one asset reference.

**Given:**
- Plan with single `Step::Resource(key)` for asset `data/config`
- Asset has no further dependencies

**Expected outcome:**
- Returns `Ok(HashSet::with([key]))`
- Single dependency found
- Stack properly managed (pushed and popped)

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_find_dependencies_single_asset() {
    let envref = setup_test_environment().await;
    let key = Key::parse("data/config").expect("parse");

    let mut plan = Plan::new();
    plan.steps.push(Step::Resource(key.clone()));

    let mut stack = Vec::new();
    let deps = find_dependencies(envref, &plan, &mut stack)
        .await
        .expect("should succeed");

    assert_eq!(deps.len(), 1);
    assert!(deps.contains(&key));
}
```

---

#### Test 17: find_dependencies() circular dependency A->B->A detection
**What it tests:** `find_dependencies()` detects and returns error for circular dependencies (A depends on B, B depends on A).

**Given:**
- Setup two recipes: A and B
- Recipe A has `Step::Resource(B)` in its plan
- Recipe B has `Step::Resource(A)` in its plan
- Start analysis from key A

**Expected outcome:**
- Returns `Err(Error::general_error(...))`
- Error message contains "Circular dependency detected" and key reference
- Stack properly maintains dependency chain during detection

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Setup code:**
```rust
let envref = setup_test_environment().await;
let key_a = Key::parse("recipe/a").expect("parse");
let key_b = Key::parse("recipe/b").expect("parse");

// Recipe A depends on B
let mut plan_a = Plan::new();
plan_a.steps.push(Step::Resource(key_b.clone()));

// Recipe B depends on A
let mut plan_b = Plan::new();
plan_b.steps.push(Step::Resource(key_a.clone()));

// Setup recipe provider to return these plans
setup_recipe_provider(&envref, key_a.clone(), plan_a).await;
setup_recipe_provider(&envref, key_b.clone(), plan_b).await;
```

**Expected behavior:**
```rust
let mut stack = Vec::new();
let result = find_dependencies(envref, &plan_a, &mut stack).await;

match result {
    Err(err) => {
        let msg = err.to_string();
        assert!(msg.contains("Circular dependency detected"));
    }
    Ok(_) => panic!("Expected circular dependency error"),
}
```

**Test function signature:**
```rust
#[tokio::test]
async fn test_find_dependencies_circular_a_depends_b_depends_a() {
    // ... setup code ...

    let result = find_dependencies(envref, &plan_a, &mut stack).await;
    assert!(result.is_err());
}
```

---

#### Test 18: find_dependencies() circular dependency A->B->C->A detection
**What it tests:** `find_dependencies()` detects circular dependency in longer chain (3+ assets).

**Given:**
- Setup three recipes: A, B, C
- Recipe A depends on B
- Recipe B depends on C
- Recipe C depends on A (closes loop)

**Expected outcome:**
- Returns `Err(Error::general_error(...))`
- Error message contains "Circular dependency detected"
- Detects cycle regardless of chain length

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_find_dependencies_circular_three_asset_cycle() {
    // Setup A->B->C->A
    let envref = setup_test_environment().await;
    let key_a = Key::parse("recipe/a").expect("parse");
    let key_b = Key::parse("recipe/b").expect("parse");
    let key_c = Key::parse("recipe/c").expect("parse");

    // ... setup recipes with dependencies ...

    let mut stack = Vec::new();
    let result = find_dependencies(envref, &plan_a, &mut stack).await;

    assert!(result.is_err());
    if let Err(err) = result {
        assert!(err.to_string().contains("Circular dependency detected"));
    }
}
```

---

#### Test 19: find_dependencies() self-dependency A->A detection
**What it tests:** `find_dependencies()` detects self-referential circular dependency.

**Given:**
- Recipe A depends on itself: `Step::Resource(A)`

**Expected outcome:**
- Returns `Err(Error::general_error(...))`
- Error message indicates circular dependency
- Stack contains A when cycle is detected

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_find_dependencies_self_dependency() {
    let envref = setup_test_environment().await;
    let key_a = Key::parse("recipe/a").expect("parse");

    let mut plan_a = Plan::new();
    plan_a.steps.push(Step::Resource(key_a.clone())); // A depends on A

    let mut stack = Vec::new();
    let result = find_dependencies(envref, &plan_a, &mut stack).await;

    assert!(result.is_err());
}
```

---

#### Test 20: find_dependencies() stack management and cleanup
**What it tests:** `find_dependencies()` properly maintains stack for multiple branches and cleans up correctly.

**Given:**
- Complex plan with multiple independent dependencies
- No circular dependencies
- Stack should be pushed/popped for each branch

**Expected outcome:**
- Stack is empty after function returns
- All dependencies found and returned
- Stack properly tracks and unwinds each branch

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_find_dependencies_stack_management() {
    let envref = setup_test_environment().await;
    let key_a = Key::parse("data/a").expect("parse");
    let key_b = Key::parse("data/b").expect("parse");
    let key_c = Key::parse("data/c").expect("parse");

    let mut plan = Plan::new();
    plan.steps.push(Step::Resource(key_a.clone()));
    plan.steps.push(Step::Resource(key_b.clone()));
    plan.steps.push(Step::Resource(key_c.clone()));

    let mut stack = Vec::new();
    let deps = find_dependencies(envref, &plan, &mut stack)
        .await
        .expect("should succeed");

    // Stack should be empty after function returns
    assert!(stack.is_empty());

    // All three keys should be in dependencies
    assert_eq!(deps.len(), 3);
}
```

---

### Tests for Volatile Dependency Propagation

#### Test 21: has_volatile_dependencies() plan with no volatile dependencies
**What it tests:** `has_volatile_dependencies()` returns false when no dependencies are volatile.

**Given:**
- Plan with asset dependencies
- All referenced recipes have `volatile = false`

**Expected outcome:**
- `has_volatile_dependencies()` returns `Ok(false)`
- `plan.is_volatile` remains unchanged (false)
- No `Step::Info` added

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_has_volatile_dependencies_none_volatile() {
    let envref = setup_test_environment().await;
    let mut plan = Plan::new();
    plan.is_volatile = false;

    // Add dependency on non-volatile recipe
    let key = Key::parse("data/config").expect("parse");
    plan.steps.push(Step::Resource(key.clone()));

    setup_recipe_provider(&envref, key, false).await; // volatile=false

    let result = has_volatile_dependencies(envref, &mut plan)
        .await
        .expect("should succeed");

    assert!(!result);
    assert!(!plan.is_volatile);
}
```

---

#### Test 22: has_volatile_dependencies() plan with one volatile dependency
**What it tests:** `has_volatile_dependencies()` returns true and updates plan when dependency is volatile.

**Given:**
- Plan with asset dependency
- Referenced recipe has `volatile = true`
- `plan.is_volatile = false` initially

**Expected outcome:**
- `has_volatile_dependencies()` returns `Ok(true)`
- `plan.is_volatile` updated to `true`
- `Step::Info` added explaining volatility source
- Info message indicates "Volatile due to dependency on volatile key"

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_has_volatile_dependencies_one_volatile() {
    let envref = setup_test_environment().await;
    let mut plan = Plan::new();
    plan.is_volatile = false;

    let key = Key::parse("volatile/source").expect("parse");
    plan.steps.push(Step::Resource(key.clone()));

    setup_recipe_provider(&envref, key.clone(), true).await; // volatile=true

    let result = has_volatile_dependencies(envref, &mut plan)
        .await
        .expect("should succeed");

    assert!(result);
    assert!(plan.is_volatile);

    // Check for Step::Info
    let has_info = plan.steps.iter().any(|s| {
        match s {
            Step::Info(msg) if msg.contains("Volatile due to dependency") => true,
            _ => false,
        }
    });
    assert!(has_info);
}
```

---

#### Test 23: has_volatile_dependencies() plan already marked volatile
**What it tests:** `has_volatile_dependencies()` returns true immediately if plan is already volatile (short-circuit).

**Given:**
- Plan with `is_volatile = true` already set
- Asset dependencies may or may not be volatile (irrelevant)

**Expected outcome:**
- `has_volatile_dependencies()` returns `Ok(true)` immediately
- No dependency checking performed (optimization)
- No modification to plan

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_has_volatile_dependencies_already_volatile_short_circuits() {
    let mut plan = Plan::new();
    plan.is_volatile = true;

    let result = has_volatile_dependencies(Arc::new(SimpleEnvironment::new()), &mut plan)
        .await
        .expect("should succeed");

    assert!(result);
    // Plan unchanged
    assert!(plan.is_volatile);
}
```

---

#### Test 24: has_volatile_dependencies() multiple dependencies with mixed volatility
**What it tests:** `has_volatile_dependencies()` detects volatility when at least one dependency is volatile.

**Given:**
- Plan with multiple dependencies: A (non-volatile), B (volatile), C (non-volatile)

**Expected outcome:**
- `has_volatile_dependencies()` returns `Ok(true)`
- `plan.is_volatile` updated to `true`
- First volatile dependency (B) identified in Step::Info

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_has_volatile_dependencies_mixed_volatility() {
    let envref = setup_test_environment().await;
    let mut plan = Plan::new();
    plan.is_volatile = false;

    let key_a = Key::parse("data/a").expect("parse");
    let key_b = Key::parse("volatile/b").expect("parse");
    let key_c = Key::parse("data/c").expect("parse");

    plan.steps.push(Step::Resource(key_a.clone()));
    plan.steps.push(Step::Resource(key_b.clone()));
    plan.steps.push(Step::Resource(key_c.clone()));

    setup_recipe_provider(&envref, key_a, false).await;
    setup_recipe_provider(&envref, key_b, true).await;
    setup_recipe_provider(&envref, key_c, false).await;

    let result = has_volatile_dependencies(envref, &mut plan)
        .await
        .expect("should succeed");

    assert!(result);
    assert!(plan.is_volatile);
}
```

---

## Module: liquers_core::context

### Tests for Context.is_volatile Field

#### Test 25: Context initializes with is_volatile flag
**What it tests:** `Context::new()` accepts `is_volatile` parameter and stores it.

**Given:**
- `Context::new(envref, true)`

**Expected outcome:**
- Context created successfully
- `context.is_volatile()` returns `true`
- Flag is accessible and correct

**File location:** `liquers-core/src/context.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_context_initializes_with_is_volatile() {
    let envref = setup_test_environment().await;
    let context = Context::new(envref, true);

    assert!(context.is_volatile());
}
```

---

#### Test 26: Context.is_volatile() getter method
**What it tests:** `Context::is_volatile()` method returns current volatility flag.

**Given:**
- Context with `is_volatile = false`

**Expected outcome:**
- `context.is_volatile()` returns `false`
- Method provides access to field

**File location:** `liquers-core/src/context.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_context_is_volatile_getter() {
    let envref = setup_test_environment().await;
    let context = Context::new(envref, false);

    assert!(!context.is_volatile());
}
```

---

#### Test 27: Context.with_volatile() propagates volatility from parent
**What it tests:** `Context::with_volatile()` creates child context inheriting parent volatility.

**Given:**
- Parent context with `is_volatile = true`
- Creating child with `is_volatile = false`

**Expected outcome:**
- Child context has `is_volatile = true` (parent flag takes precedence via OR logic)
- Volatility propagates through nested evaluations
- Contagion semantics: "once volatile, always volatile"

**File location:** `liquers-core/src/context.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_context_with_volatile_propagates_parent_volatility() {
    let envref = setup_test_environment().await;
    let parent = Context::new(envref.clone(), true);

    // Child requests false, but parent is volatile
    let child = parent.with_volatile(false);

    assert!(child.is_volatile());
}
```

---

#### Test 28: Context.with_volatile() child volatile when parent not
**What it tests:** `Context::with_volatile()` allows child to become volatile even if parent is not.

**Given:**
- Parent context with `is_volatile = false`
- Creating child with `is_volatile = true`

**Expected outcome:**
- Child context has `is_volatile = true`
- Individual asset evaluation can be marked volatile

**File location:** `liquers-core/src/context.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_context_with_volatile_child_becomes_volatile() {
    let envref = setup_test_environment().await;
    let parent = Context::new(envref.clone(), false);

    let child = parent.with_volatile(true);

    assert!(child.is_volatile());
}
```

---

#### Test 29: Context.with_volatile() both non-volatile
**What it tests:** `Context::with_volatile(false)` from non-volatile parent remains non-volatile.

**Given:**
- Parent context with `is_volatile = false`
- Creating child with `is_volatile = false`

**Expected outcome:**
- Child context has `is_volatile = false`

**File location:** `liquers-core/src/context.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_context_with_volatile_both_false() {
    let envref = setup_test_environment().await;
    let parent = Context::new(envref.clone(), false);

    let child = parent.with_volatile(false);

    assert!(!child.is_volatile());
}
```

---

## Module: liquers_core::assets

### Tests for AssetRef::to_override()

#### Test 30: AssetRef.to_override() on Volatile status
**What it tests:** `AssetRef::to_override()` converts `Status::Volatile` to `Status::Override`.

**Given:**
- AssetRef with `status = Status::Volatile`
- Asset has value data

**Expected outcome:**
- `to_override()` succeeds with `Ok(())`
- Status changed to `Status::Override`
- Metadata status updated
- Value data preserved

**File location:** `liquers-core/src/assets.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_asset_ref_to_override_from_volatile() {
    let asset_ref = create_test_asset(Status::Volatile, Value::from("test_data")).await;

    asset_ref.to_override().await.expect("should succeed");

    let data = asset_ref.read().await;
    assert_eq!(data.status, Status::Override);
}
```

---

#### Test 31: AssetRef.to_override() on Ready status preserves data
**What it tests:** `AssetRef::to_override()` on `Status::Ready` preserves existing data.

**Given:**
- AssetRef with `status = Status::Ready`
- Asset contains specific value

**Expected outcome:**
- Status changed to `Status::Override`
- Original data value preserved
- No data loss

**File location:** `liquers-core/src/assets.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_asset_ref_to_override_preserves_ready_data() {
    let test_value = Value::from("important_data");
    let asset_ref = create_test_asset(Status::Ready, test_value.clone()).await;

    asset_ref.to_override().await.expect("should succeed");

    let data = asset_ref.read().await;
    assert_eq!(data.status, Status::Override);
    assert_eq!(data.data.as_ref().unwrap().as_ref(), &test_value);
}
```

---

#### Test 32: AssetRef.to_override() on Processing cancels evaluation
**What it tests:** `AssetRef::to_override()` on `Status::Processing` cancels in-flight evaluation.

**Given:**
- AssetRef with `status = Status::Processing`
- Evaluation in progress

**Expected outcome:**
- Status changed to `Status::Override`
- In-flight evaluation cancelled
- Data set to `Value::none()`

**File location:** `liquers-core/src/assets.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_asset_ref_to_override_cancels_processing() {
    let asset_ref = create_test_asset(Status::Processing, Value::none()).await;

    asset_ref.to_override().await.expect("should succeed");

    let data = asset_ref.read().await;
    assert_eq!(data.status, Status::Override);
}
```

---

#### Test 33: AssetRef.to_override() on Error clears error state
**What it tests:** `AssetRef::to_override()` on `Status::Error` replaces error with override.

**Given:**
- AssetRef with `status = Status::Error`

**Expected outcome:**
- Status changed to `Status::Override`
- Error state cleared
- Data set to `Value::none()`

**File location:** `liquers-core/src/assets.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_asset_ref_to_override_clears_error() {
    let asset_ref = create_test_asset(Status::Error, Value::none()).await;

    asset_ref.to_override().await.expect("should succeed");

    let data = asset_ref.read().await;
    assert_eq!(data.status, Status::Override);
}
```

---

#### Test 34: AssetRef.to_override() on Directory is no-op
**What it tests:** `AssetRef::to_override()` on `Status::Directory` does nothing (unchanged).

**Given:**
- AssetRef with `status = Status::Directory`

**Expected outcome:**
- `to_override()` succeeds
- Status remains `Status::Directory`
- No modification

**File location:** `liquers-core/src/assets.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_asset_ref_to_override_directory_no_op() {
    let asset_ref = create_test_asset(Status::Directory, Value::none()).await;

    asset_ref.to_override().await.expect("should succeed");

    let data = asset_ref.read().await;
    assert_eq!(data.status, Status::Directory);
}
```

---

#### Test 35: AssetRef.to_override() on Source is no-op
**What it tests:** `AssetRef::to_override()` on `Status::Source` does nothing (source data unchangeable).

**Given:**
- AssetRef with `status = Status::Source`

**Expected outcome:**
- `to_override()` succeeds
- Status remains `Status::Source`
- No modification

**File location:** `liquers-core/src/assets.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_asset_ref_to_override_source_no_op() {
    let asset_ref = create_test_asset(Status::Source, Value::from("source_data")).await;

    asset_ref.to_override().await.expect("should succeed");

    let data = asset_ref.read().await;
    assert_eq!(data.status, Status::Source);
}
```

---

#### Test 36: AssetRef.to_override() on Override is idempotent
**What it tests:** `AssetRef::to_override()` called on already-Override status is safe (no-op).

**Given:**
- AssetRef with `status = Status::Override`

**Expected outcome:**
- `to_override()` succeeds
- Status remains `Status::Override`
- Idempotent operation

**File location:** `liquers-core/src/assets.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_asset_ref_to_override_already_override_idempotent() {
    let asset_ref = create_test_asset(Status::Override, Value::from("override_data")).await;

    asset_ref.to_override().await.expect("should succeed");

    let data = asset_ref.read().await;
    assert_eq!(data.status, Status::Override);
}
```

---

#### Test 37: AssetRef.to_override() on Expired preserves data
**What it tests:** `AssetRef::to_override()` on `Status::Expired` preserves existing data (expired but available).

**Given:**
- AssetRef with `status = Status::Expired`
- Asset contains value

**Expected outcome:**
- Status changed to `Status::Override`
- Expired data preserved
- No data loss

**File location:** `liquers-core/src/assets.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_asset_ref_to_override_preserves_expired_data() {
    let test_value = Value::from("expired_but_available");
    let asset_ref = create_test_asset(Status::Expired, test_value.clone()).await;

    asset_ref.to_override().await.expect("should succeed");

    let data = asset_ref.read().await;
    assert_eq!(data.status, Status::Override);
    assert_eq!(data.data.as_ref().unwrap().as_ref(), &test_value);
}
```

---

#### Test 38: AssetRef.to_override() multiple calls are thread-safe
**What it tests:** Concurrent calls to `to_override()` on same AssetRef are safe and result in consistent state.

**Given:**
- AssetRef with `status = Status::Volatile`
- Multiple concurrent calls to `to_override()`

**Expected outcome:**
- All calls succeed
- Final status is `Status::Override`
- No race conditions or data corruption

**File location:** `liquers-core/src/assets.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_asset_ref_to_override_concurrent_calls() {
    let asset_ref = Arc::new(create_test_asset(Status::Volatile, Value::from("test")).await);
    let mut handles = vec![];

    for _ in 0..10 {
        let asset = asset_ref.clone();
        let handle = tokio::spawn(async move {
            asset.to_override().await
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.expect("task").expect("to_override");
    }

    let data = asset_ref.read().await;
    assert_eq!(data.status, Status::Override);
}
```

---

## Edge Case Tests

### Empty and Minimal Plans

#### Test 39: Empty plan is non-volatile by default
**What it tests:** Plan with no steps defaults to non-volatile.

**Given:**
- Empty Plan (no steps)

**Expected outcome:**
- `plan.is_volatile = false`
- No dependencies to check

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[test]
fn test_empty_plan_is_non_volatile() {
    let plan = Plan::new();
    assert!(!plan.is_volatile);
}
```

---

#### Test 40: Plan with only context modifiers is non-volatile
**What it tests:** Plan with only context-modifier steps (Info, SetCwd, Filename) is non-volatile.

**Given:**
- Plan with only `Step::Info`, `Step::SetCwd`, `Step::Filename` steps
- No action steps, no resource steps

**Expected outcome:**
- `plan.is_volatile = false`
- No volatility source identified

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[test]
fn test_plan_with_only_context_modifiers_is_non_volatile() {
    let mut plan = Plan::new();
    plan.steps.push(Step::Info("test".to_string()));
    plan.steps.push(Step::SetCwd(Key::parse("data/config").expect("parse")));

    // This plan has no volatility sources
    assert!(!plan.is_volatile);
}
```

---

### Multiple Volatile Sources

#### Test 41: Plan with multiple volatile commands
**What it tests:** Plan calling multiple volatile commands is marked volatile once.

**Given:**
- Query with multiple volatile commands: `"random/current_time/to_text"`
- Both `random` and `current_time` are volatile commands

**Expected outcome:**
- `plan.is_volatile = true`
- Only first volatility source added as Step::Info (optimization)
- Plan processes correctly despite multiple sources

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_plan_multiple_volatile_commands() {
    let mut cmr = CommandMetadataRegistry::new();
    register_test_command(&mut cmr, "random", true);
    register_test_command(&mut cmr, "current_time", true);
    register_test_command(&mut cmr, "to_text", false);

    let query = parse_query("random/current_time/to_text").expect("parse");
    let mut pb = PlanBuilder::new(query, &cmr);
    let plan = pb.build().expect("build");

    assert!(plan.is_volatile);
}
```

---

#### Test 42: Plan with volatile command and 'v' instruction
**What it tests:** Plan with both volatile command AND 'v' instruction is marked volatile.

**Given:**
- Query with volatile command AND 'v' instruction: `"random/v/to_text"`

**Expected outcome:**
- `plan.is_volatile = true`
- Plan recognizes both volatility sources
- Only first one causes marking (due to optimization)

**File location:** `liquers-core/src/plan.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_plan_volatile_command_and_v_instruction() {
    let mut cmr = CommandMetadataRegistry::new();
    register_test_command(&mut cmr, "random", true);
    register_test_command(&mut cmr, "to_text", false);

    let query = parse_query("random/v/to_text").expect("parse");
    let mut pb = PlanBuilder::new(query, &cmr);
    let plan = pb.build().expect("build");

    assert!(plan.is_volatile);
}
```

---

### Nested Volatile Dependencies

#### Test 43: Volatile dependency in nested plan
**What it tests:** Volatility propagates through nested evaluations via Context.

**Given:**
- Parent plan evaluates nested query
- Nested query depends on volatile asset
- Context propagates from parent

**Expected outcome:**
- Nested Context inherits parent volatility
- Both parent and child evaluations respect volatility
- Results correctly marked as volatile

**File location:** `liquers-core/src/context.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_nested_plan_propagates_volatility() {
    let envref = setup_test_environment().await;

    // Parent context is volatile
    let parent_context = Context::new(envref.clone(), true);

    // Create nested context
    let nested = parent_context.with_volatile(false);

    // Nested context should inherit parent's volatility
    assert!(nested.is_volatile());
}
```

---

#### Test 44: Non-volatile parent with volatile nested dependency
**What it tests:** Child evaluation can become volatile even if parent is not.

**Given:**
- Parent context with `is_volatile = false`
- Child evaluation calls `context.evaluate()` with volatile query
- Volatile dependency discovered during evaluation

**Expected outcome:**
- Child context created with `is_volatile = true`
- Child evaluation proceeds as volatile
- Parent remains non-volatile

**File location:** `liquers-core/src/context.rs` (tests module at end of file)

**Test function signature:**
```rust
#[tokio::test]
async fn test_non_volatile_parent_volatile_nested() {
    let envref = setup_test_environment().await;
    let parent_context = Context::new(envref.clone(), false);

    // Nested evaluation can independently be volatile
    let nested = parent_context.with_volatile(true);

    assert!(!parent_context.is_volatile());
    assert!(nested.is_volatile());
}
```

---

## Integration Tests (Recommended in tests/volatility.rs)

#### Integration Test 1: Full flow - volatile query to volatile asset
**What it tests:** Complete flow from query parsing through asset creation with volatility propagated correctly.

**Scenario:**
- Query: `"random/text"`
- `random` command registered as volatile
- Execute full evaluation pipeline

**Expected outcome:**
- Plan marked volatile during building
- Context created with `is_volatile = true`
- Final asset has `status = Status::Volatile`
- Metadata record has `is_volatile = true`

**File location:** `liquers-core/tests/volatility.rs` (new integration test file)

---

#### Integration Test 2: Circular dependency error handling
**What it tests:** Error handling for circular dependencies caught and reported properly.

**Scenario:**
- Setup circular dependency chain
- Call `find_dependencies()`
- Verify error is descriptive

**Expected outcome:**
- `find_dependencies()` returns `Err`
- Error message includes "Circular dependency detected"
- Error message includes key that caused cycle

**File location:** `liquers-core/tests/volatility.rs`

---

#### Integration Test 3: Metadata volatility flag transitions
**What it tests:** `is_volatile` flag in MetadataRecord tracks volatility through asset lifecycle.

**Scenario:**
- In-flight asset: `status = Status::Processing`, `is_volatile = true`
- Asset completes: `status = Status::Volatile`, `is_volatile` remains true (via helper)

**Expected outcome:**
- `record.is_volatile()` returns true at all stages
- Helper method correctly combines status and flag

**File location:** `liquers-core/tests/volatility.rs`

---

## Test Utilities and Helpers

### Recommended Test Fixture Functions

```rust
// In tests module at end of each file or in separate test_utils.rs

async fn setup_test_environment() -> EnvRef<SimpleEnvironment<Value>> {
    let env = SimpleEnvironment::<Value>::new();
    env.to_ref()
}

async fn create_test_asset(status: Status, value: Value) -> AssetRef<SimpleEnvironment<Value>> {
    // Create asset with specified status and value
    // Return AssetRef for testing
}

fn register_test_command(
    cmr: &mut CommandMetadataRegistry,
    name: &str,
    volatile: bool,
) {
    // Register command with specified volatility flag
    // Simplifies test setup
}

async fn setup_recipe_provider(
    envref: &EnvRef<SimpleEnvironment<Value>>,
    key: Key,
    volatile: bool,
) {
    // Configure recipe provider to return recipe for key with specified volatility
}
```

---

## Test Coverage Summary

| Category | Count | Details |
|----------|-------|---------|
| **Status::Volatile** | 4 | has_data, is_finished, can_have_tracked_dependencies, serialization |
| **MetadataRecord.is_volatile** | 4 | Field behavior, helper method, serialization |
| **Plan.is_volatile** | 3 | Field existence, getter, serialization |
| **PlanBuilder volatility** | 3 | 'v' instruction, volatile command, optimization |
| **Circular dependencies** | 5 | Detection (2-asset, 3-asset, self), stack management, early exit |
| **Volatile dependencies** | 4 | No volatile, one volatile, already volatile, mixed |
| **Context.is_volatile** | 5 | Initialization, getter, propagation, both volatile/non-volatile, contagion |
| **AssetRef::to_override()** | 9 | Volatile, Ready, Processing, Error, Directory, Source, Override, Expired, concurrency |
| **Edge cases** | 4 | Empty plan, context-only plan, multiple sources, nested dependencies |
| **Integration** | 3 | Full flow, error handling, lifecycle |
| **Total** | **44** | Comprehensive coverage of volatility system |

---

## Compilation and Validation Checklist

Before implementing tests, verify:

- [ ] All Status enum variants explicitly matched (no `_ =>` default arms)
- [ ] All test functions follow naming convention
- [ ] Async tests use `#[tokio::test]` attribute
- [ ] Error paths use `Result` and `?` operator (no unwrap in library tests)
- [ ] Test helpers use `expect()` only in test code (acceptable)
- [ ] Documentation strings for each test explain "What it tests"
- [ ] Test modules properly gated with `#[cfg(test)]`

---

## References

- **Phase 1 Design:** `specs/volatility-system/phase1-high-level-design.md`
- **Phase 2 Architecture:** `specs/volatility-system/phase2-architecture.md`
- **Testing conventions:** `CLAUDE.md` (sections on Testing and Match Statements)
- **Example tests:** `liquers-core/tests/async_hellow_world.rs`
- **Error handling:** `CLAUDE.md` (Error Handling section)

---

## Notes for Implementer

1. **Match statement validation:** After adding `Status::Volatile` variant, compiler will flag all incomplete match statements. This is desired behavior - ensures all code paths are updated.

2. **Test organization:** Place unit tests at end of implementation files (following CLAUDE.md conventions). Integration tests go in `tests/` directory.

3. **Async patterns:** Use existing test infrastructure (`SimpleEnvironment`, `tokio::test`) - no new patterns needed.

4. **Error testing:** Use `match result { Err(...) => {}, Ok(_) => panic!() }` pattern to verify errors as designed.

5. **Circular dependency testing:** Use stack/Vec to manually verify cycle detection - test framework doesn't need awareness of internal stack mechanics.

6. **Concurrency testing:** Test 44 uses `Arc` and `tokio::spawn` - verify thread safety of `to_override()` async implementation.

