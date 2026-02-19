# Phase 3 Summary: Integration Test Specifications for Volatility System

**Date:** 2026-02-17
**Status:** Complete - Ready for Phase 4 Implementation
**Document:** `specs/volatility-system/phase3-integration-tests.md`

---

## Deliverable Overview

Comprehensive integration test specifications covering the complete volatility system architecture defined in Phase 2. The document specifies **23 distinct integration tests** organized into **8 test categories**, with full code templates, validation criteria, and cross-module interaction patterns.

**Total Lines:** ~800 specification lines
**Test Categories:** 8
**Individual Tests:** 23
**Test Sub-variants:** 4 (within parameterized tests)
**Estimated Test Code:** ~1500 lines of Rust when implemented

---

## Test Categories & Coverage

### 1. Full Pipeline Tests (3 tests)

**Purpose:** Validate complete query-to-asset-to-state pipeline with volatility tracking at each layer.

| Test | Focus | Validates |
|------|-------|-----------|
| Test 1: Simple Volatile Command | Single volatile command execution | Plan.is_volatile, Context.is_volatile, Status::Volatile, MetadataRecord.is_volatile |
| Test 2: 'v' Instruction | Explicit volatility marking | Phase 1 volatility check detects 'v' action |
| Test 3: Volatile Dependency Propagation | Transitive volatility through recipes | Phase 2 dependency checking, circular detection |

**Key Interactions:**
- Parser → PlanBuilder → Interpreter → AssetManager → Metadata
- Two-phase volatility determination (commands during build, dependencies after)

---

### 2. Volatility Instruction Tests (1 family with 4 sub-tests)

**Purpose:** Validate 'v' instruction works at any position in query string.

**Test 5:** Position variations
- `"v"` alone
- `"data/v"` (middle)
- `"data/v/to_string"` (middle)
- `"data/command/v"` (end)

**Validates:** Parser correctly interprets 'v' as action regardless of position

---

### 3. Circular Dependency Detection (3 tests)

**Purpose:** Verify dependency cycle detection prevents infinite recursion during Phase 2 checking.

| Test | Scenario | Validation |
|------|----------|-----------|
| Test 6: Direct Circular | A → B → A | Stack-based detection, immediate error |
| Test 7: Indirect Chain | A → B → C → A | Arbitrary depth cycle detection |
| Test 8: Self-Reference | A → A | Immediate self-cycle detection |

**Key Implementation:** `find_dependencies()` uses `&mut Vec<Key>` stack to detect cycles before recursing, prevents exponential time complexity.

---

### 4. Serialization Round-Trip Tests (3 tests)

**Purpose:** Verify volatility fields preserved through JSON serialization lifecycle.

| Test | Field | Scope |
|------|-------|-------|
| Test 9: Plan | Plan.is_volatile | JSON encode → decode → verify |
| Test 10: MetadataRecord | MetadataRecord.is_volatile + Status::Volatile | Both in-flight and completed states |
| Test 11: Status Enum | New Status::Volatile variant | Serialization format, behavior methods |

**Validates:**
- is_volatile always serialized (required field, no `#[serde(default)]`)
- Status::Volatile serializes as string "Volatile"
- Round-trip preserves exact values
- Helper method `MetadataRecord::is_volatile()` combines field + status checks

---

### 5. Concurrency Tests (2 tests)

**Purpose:** Verify thread-safety and correct caching behavior under concurrent access.

| Test | Scenario | Validates |
|------|----------|-----------|
| Test 12: Concurrent Volatile | 5 tasks request same volatile query | All get unique AssetRef.id (no caching) |
| Test 13: Mixed Volatile/Non-Volatile | Alternating volatile and non-volatile requests | Volatile always new, non-volatile cached |

**Key Assertion:** `volatile_ids.all_unique() && non_volatile_ids.all_same()`

---

### 6. Cross-Module Integration (2 tests)

**Purpose:** Validate coordinated behavior across Interpreter, PlanBuilder, Context, and AssetManager.

| Test | Pipeline | Validates |
|------|----------|-----------|
| Test 14: Interpreter → PlanBuilder → Context | Query parsing through context initialization | Volatility propagates through layers |
| Test 15: AssetManager → Interpreter → Query Flow | Query to final volatile asset | Caching decisions, metadata accuracy |

**Key Patterns:**
- `make_plan()` called by AssetManager (now async, breaking change)
- Context initialized with Plan.is_volatile value
- Metadata reflects final state

---

### 7. Performance Tests (2 tests)

**Purpose:** Ensure dependency checking is efficient and scales linearly (not exponentially).

| Test | Scenario | Performance Threshold |
|------|----------|---------------------|
| Test 16: Linear Chain Dependency | 100-deep linear chain: A → B → ... → Z | < 500ms |
| Test 17: Circular Dependency Detection | Complex graph with cycles, early termination | < 100ms (error detection) |

**Validates:** No exponential blowup, early cycle detection termination, no stack overflow

---

### 8. Corner Cases & Error Handling (5 tests)

**Purpose:** Validate system robustness in edge cases and error conditions.

| Test | Scenario | Validates |
|------|----------|-----------|
| Test 18: Large Dependency Chain (1000-deep) | Memory limits, performance scaling | No stack overflow, < 1000ms |
| Test 19: Many Volatile Commands | 10 chained volatile commands | Correct first-detection, no double-checks |
| Test 20: Volatile Status Transitions | Volatile → Override state change | AssetRef::to_override() method |
| Test 21: Volatile Command Error | Error in volatile command execution | Error propagation with is_volatile=true |
| Test 22: Missing Recipe in Phase 2 | Dependency check on non-existent recipe | Graceful error handling |

---

### 9. End-to-End Integration (1 test)

**Purpose:** Complete production-like scenario exercising all components.

**Test 23:** End-to-end volatile query execution
- 3+ commands (volatile and non-volatile)
- 2+ recipes with dependencies
- 3 queries (direct volatile, dependent volatile, 'v' instruction)
- Verify: execution, metadata accuracy, caching behavior, result values

---

## Key Design Patterns Validated

### Pattern 1: Two-Phase Volatility Determination

**Phase 1 (during build):**
```
Check commands via CommandMetadata.volatile
Check for 'v' instruction
→ Set plan.is_volatile (assuming non-volatile dependencies)
```

**Phase 2 (after build):**
```
find_dependencies(envref, plan, &mut stack) → Result<HashSet<Key>, Error>
  - Stack-based cycle detection
  - Recursive recipe resolution
  - Return error on cycle found
has_volatile_dependencies(envref, plan) → Result<bool, Error>
  - Calls find_dependencies()
  - Checks each key.recipe.volatile
  - Updates plan.is_volatile if needed
```

### Pattern 2: Volatile Asset Non-Caching

```rust
AssetManager::get_asset_from_query(query: &Query) -> Result<AssetRef<E>, Error>
  1. make_plan(query).await → Plan with is_volatile field
  2. if plan.is_volatile:
       - Create new AssetRef
       - NEVER add to internal cache
     else:
       - Use existing cache behavior
```

### Pattern 3: Context Volatility Propagation

```rust
Context::new(envref, is_volatile: bool) → Context<E>
  - Initialize with Plan.is_volatile value

Context::with_volatile(&self, is_volatile: bool) → Context<E>
  - Child context inherits or overrides volatility
  - Used for nested evaluate() calls
```

### Pattern 4: Metadata Dual Representation

```rust
Status::Volatile          // Asset has volatile value (terminal state, like Ready)
MetadataRecord.is_volatile // Flag indicating volatility even in-flight (Submitted, Dependencies, etc.)

is_volatile() helper:     // Returns true if either condition met
  self.is_volatile || self.status == Status::Volatile
```

---

## Test File Structure

**Location:** `liquers-core/tests/volatility_integration.rs`

**Organization:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Test setup helpers (async environment, command registration, recipe creation)

    // Full Pipeline Tests (3)
    #[tokio::test]
    async fn test_volatile_query_to_asset_simple() { ... }

    #[tokio::test]
    async fn test_v_instruction_marks_plan_volatile() { ... }

    #[tokio::test]
    async fn test_volatile_dependency_chain_propagation() { ... }

    // ... remaining 20 tests organized by category

    // Parametrized test families where applicable
    async fn test_v_instruction_position_variations() { ... }
}
```

**Test Execution:**
```bash
cargo test -p liquers-core --test volatility_integration
cargo test -p liquers-core --test volatility_integration test_circular_dependency
cargo test -p liquers-core --test volatility_integration -- --nocapture --test-threads=1
```

---

## Integration with Phase 4 Implementation

### Implementation Roadmap

1. **Modify existing modules** (per Phase 2 architecture):
   - `liquers-core/src/metadata.rs` - Add Status::Volatile, is_volatile field
   - `liquers-core/src/plan.rs` - Add is_volatile field, two-phase checking
   - `liquers-core/src/context.rs` - Add is_volatile flag, with_volatile()
   - `liquers-core/src/assets.rs` - Add AssetData.is_volatile, to_override(), volatile caching
   - `liquers-core/src/interpreter.rs` - make_plan() async, evaluate_plan() context init

2. **Implement test file**:
   - Create `liquers-core/tests/volatility_integration.rs`
   - Implement all 23 test specifications
   - Target: ~1500 lines of test code

3. **Validate**:
   - All tests compile: `cargo check -p liquers-core`
   - All tests pass: `cargo test -p liquers-core`
   - No clippy warnings: `cargo clippy -p liquers-core -- -D warnings`
   - Performance baselines met (Tests 16-17)

### Breaking Changes Identified

1. **`make_plan()` becomes async** - All call sites must add `.await`
2. **`Context::new()` signature changes** - Added `is_volatile: bool` parameter
3. **Status enum gets new variant** - All match statements must explicitly handle `Status::Volatile`

### Documentation Updates Needed

- Update `specs/PROJECT_OVERVIEW.md` if core concepts change
- Document `make_plan()` API change in migration guide
- Add volatility section to API documentation

---

## Validation Criteria Summary

### Coverage Metrics
- **Full pipeline:** 3 tests (parser → plan → asset → metadata)
- **Instruction syntax:** 4 sub-tests ('v' at different positions)
- **Dependency handling:** 3 tests (direct cycle, indirect chain, self-ref)
- **Serialization:** 3 tests (Plan, MetadataRecord, Status)
- **Concurrency:** 2 tests (same volatile, mixed volatile/non-volatile)
- **Cross-module:** 2 tests (interpreter→planner, assetmgr→interpreter)
- **Performance:** 2 tests (linear chain, cycle detection)
- **Corner cases:** 5 tests (large chains, many commands, state transitions, errors)
- **End-to-end:** 1 test (production scenario)

**Total Test Specification Coverage:** 23 tests + 4 sub-variants = ~27 distinct test scenarios

### Test Quality Standards (CLAUDE.md compliance)

- ✅ No `unwrap()` / `expect()` in test code (library code patterns)
- ✅ Error handling via `Result<T, Error>` with `?` operator
- ✅ Explicit match arms (no default `_ =>` arms on Status/Step)
- ✅ Async tests use `#[tokio::test]` macro
- ✅ Test modules organized at end of files with `#[cfg(test)]`
- ✅ Helper functions for setup (environment, command registration)
- ✅ Clear assertions with descriptive error messages

---

## Key Specifications Addressed

### From Phase 1 Open Questions

1. **"AssetManager 'new AssetRef' logic"** ✅
   - Volatile assets NEVER cached in internal maps
   - Each request creates new AssetRef with unique ID
   - Tests verify IDs are distinct (Test 4, 12, 13)

2. **"Volatility computation timing"** ✅
   - Phase 1: during PlanBuilder.build() (commands + 'v' instruction)
   - Phase 2: after build via find_dependencies() (async, recipe-based)
   - make_plan() is now async, awaits both phases
   - Tests verify both phases execute (Test 1-3, 14-15)

3. **"Step::Info usage"** ✅
   - Always added when marking plan volatile (spec confirmed)
   - Tests verify Step::Info present in all volatile plans (Test 2, 3, 5)
   - Format documents source: "Volatile due to command X" or "Volatile due to instruction 'v'"

4. **"Circular dependency handling"** ✅
   - find_dependencies() uses stack-based detection (prevents exponential blowup)
   - Returns Error::general_error on cycle found
   - Tests verify detection at 2-way (Test 6), 3-way (Test 7), and self-reference (Test 8)
   - Performance test ensures early termination (Test 17, < 100ms)

### From Phase 2 Design Decisions

1. **Two-Phase Volatility Check** ✅
   - Tests validate Phase 1 detects commands and 'v' (Tests 1-2)
   - Tests validate Phase 2 detects dependencies (Tests 3, 14-15)
   - Tests validate both phases work together (Test 23)

2. **Status::Volatile Variant** ✅
   - Tests verify Status::Volatile.has_data() == true (Test 11)
   - Tests verify Status::Volatile.is_finished() == true (Test 11)
   - Tests verify Status::Volatile.can_have_tracked_dependencies() == false (Test 11)

3. **MetadataRecord.is_volatile Field** ✅
   - Tests verify serialization with is_volatile always present (Test 10)
   - Tests verify combined check: status == Volatile OR flag == true (Test 10)

4. **AssetRef::to_override() Method** ✅
   - Test 20 validates Volatile → Override transition
   - Validates other states (Ready, Expired, etc.)

---

## Notes for Implementation

### Performance Considerations

- Dependency checking uses stack-based recursion (bounded by recipe depth)
- Early cycle termination prevents exponential time complexity
- Non-volatile query caching unaffected
- Volatile queries create new AssetRef (cannot be optimized via caching by design)

### Thread Safety

- New fields (bool Copy types) are inherently thread-safe
- AssetManager uses existing scc concurrent map (no changes needed)
- AssetRef::to_override() uses existing async RwLock pattern
- Tests verify concurrent requests work correctly (Test 12-13)

### Backward Compatibility

- No breaking changes to Value/ExtValue types
- No new generic parameters
- Status enum changes require explicit match arms (compiler enforced)
- MetadataRecord.is_volatile field always required (no `#[serde(default)]`)
- Plan.is_volatile field always required

### Error Handling

- All error paths return `Result<T, Error>` (no panics expected)
- Circular dependency errors: `Error::general_error("Circular dependency detected...")`
- Command errors propagate through Context.is_volatile context
- Missing recipes during Phase 2 should be handled gracefully (Test 22)

---

## Files Modified by Specification

### New Files
- `liquers-core/tests/volatility_integration.rs` (~1500 lines)

### Modified per Phase 2 Architecture
- `liquers-core/src/metadata.rs` (Status enum, MetadataRecord)
- `liquers-core/src/plan.rs` (Plan struct, PlanBuilder, find_dependencies, has_volatile_dependencies)
- `liquers-core/src/context.rs` (Context struct, is_volatile field)
- `liquers-core/src/assets.rs` (AssetData, AssetManager, AssetRef::to_override)
- `liquers-core/src/interpreter.rs` (make_plan async, evaluate_plan, IsVolatile impl)

### Reference/Documentation
- `specs/PROJECT_OVERVIEW.md` (if core concepts change)
- `CLAUDE.md` (no changes, patterns followed)

---

## Checklist for Phase 4

- [ ] Read phase3-integration-tests.md completely
- [ ] Implement each test in volatility_integration.rs
- [ ] Verify compilation: `cargo check -p liquers-core`
- [ ] Run tests: `cargo test -p liquers-core --test volatility_integration`
- [ ] All tests pass (green)
- [ ] No clippy warnings: `cargo clippy -p liquers-core`
- [ ] Performance baselines met (Tests 16-17 < thresholds)
- [ ] Code review: ensure CLAUDE.md compliance
- [ ] Update migration guide for breaking changes
- [ ] Merge to main branch

---

## Conclusion

Phase 3 delivers comprehensive integration test specifications covering:
- ✅ Full pipeline validation (23 tests)
- ✅ All design patterns from Phase 2 (two-phase checking, circular detection, non-caching)
- ✅ Performance and concurrency validation
- ✅ Serialization and error handling
- ✅ Cross-module interaction testing
- ✅ Code templates ready for Phase 4 implementation
- ✅ Clear validation criteria for each test

**Ready for Phase 4 Implementation.**

