# Phase 3 Examples - Validation Checklist & Action Items

**Purpose:** Quick reference for addressing validation issues before Phase 4 implementation

---

## Critical Issues - MUST FIX

### Query Syntax Issues (9 items)

- [ ] **Example 1, lines 63-64:** Remove leading `/` from `/financial/data/v/timestamp`
  - **Action:** Change to `financial/data/v/timestamp`

- [ ] **Example 3, lines 178-179:** Fix parameter separator in `group_by_q` to `group_by-q`
  - **Action:** Change both queries to use `-` for parameters

- [ ] **Example 4, line 221:** Remove leading `/` and fix parameter format
  - **Action:** Change `/financial/data/v/random_sample/100/format_report` to proper format

- [ ] **Example 5, lines 290-292:** Verify `Query::parse()` syntax and nested `q` instructions
  - **Action:** Test against actual parser or update with correct syntax

- [ ] **Example 8, line 498:** Fix parameter encoding `_` to `-`
  - **Action:** Change `add_suffix-_done` to `add_suffix-done`

- [ ] **Example 10, line 590:** Clarify parameter with embedded path `'data/store_value`
  - **Action:** Use proper string parameter quoting or simplify

- [ ] **Example 11, line 639:** Fix special character encoding in `': '` parameter
  - **Action:** Use `~.` for spaces: `'~._~'` or use simpler parameter

- [ ] **Example 5, line 291 (duplicate):** Ambiguous `transform/q/core/timestamp` syntax
  - **Action:** Verify against parser or add clearer parameter

- [ ] **Example 6, line 365:** Remove leading `/` from multi-query example
  - **Action:** Format as valid query strings

---

### API Verification Issues (8 items)

- [ ] **Plan.is_volatile field:** Verify exists in `liquers-core/src/plan.rs`
  - **Action:** Check Plan struct definition
  - **If missing:** Add field and serialize/deserialize support

- [ ] **Step::Info variant:** Verify exists in Step enum
  - **Action:** Check plan.rs Step definition
  - **If missing:** Add variant for documentation steps

- [ ] **Status::Volatile variant:** Verify being added in Phase 3
  - **Action:** Confirm in metadata.rs
  - **Implementation needed:** has_data(), is_finished() methods, serialization

- [ ] **Context.is_volatile() method:** Verify getter exists
  - **Action:** Check liquers-core/src/context.rs
  - **If missing:** Add getter method

- [ ] **PlanBuilder.mark_volatile() method:** Verify exists and signature
  - **Action:** Check plan.rs PlanBuilder implementation
  - **If missing:** Implement method

- [ ] **MetadataRecord.is_volatile() helper:** Verify logic (status precedence)
  - **Action:** Check metadata.rs implementation
  - **If missing:** Implement with OR logic for status/flag

- [ ] **AssetRef.to_override() method:** Verify signature and behavior
  - **Action:** Check assets.rs AssetRef implementation
  - **Implementation needed:** Status transition validation, async handling

- [ ] **Context.with_volatile() method:** Verify builder pattern
  - **Action:** Check context.rs
  - **If missing:** Implement with OR logic for propagation

---

### Documentation Clarification Issues (3 items)

- [ ] **Query::parse() vs parse_query():** Document both functions
  - **Action:** Add to PROJECT_OVERVIEW.md or query.rs docs
  - **Question:** Are they aliases? Different behavior?

- [ ] **register_command! 'volatile:' metadata:** Update DSL documentation
  - **Action:** Add to specs/REGISTER_COMMAND_FSD.md
  - **Syntax:** `volatile: true | false`

- [ ] **Context parameter position:** Clarify last-parameter requirement
  - **Action:** Verify against MEMORY.md note about parameter index bug
  - **Update:** Example 10, line 587 if needed

---

## Medium Priority Issues - SHOULD FIX

### Pattern Alignment (2 items)

- [ ] **Context::new() signature:** Verify constructor takes envref and boolean
  - **Current assumption:** `Context::new(envref, plan.is_volatile)`
  - **Action:** Check actual signature in context.rs

- [ ] **Recipe field naming:** Verify `volatile` field exists
  - **Current assumption:** `recipe.volatile: bool`
  - **Action:** Check Recipe struct in query.rs or recipes.rs

---

### Test Specification Clarity (3 items)

- [ ] **make_plan() function:** Verify it exists or create it
  - **Usage:** `let plan = make_plan(env.clone(), &query).await?;`
  - **Action:** Check liquers-core/tests/ for helper functions or add to async_hellow_world.rs

- [ ] **evaluate_plan() function:** Verify it exists or create it
  - **Usage:** `let state = evaluate_plan(envref.clone(), &plan).await?;`
  - **Action:** Check if wrapper for interpreter::evaluate()

- [ ] **Helper functions for tests:** Document required test utilities
  - **Items needed:** parse_key(), parse_query(), find_dependencies()
  - **Action:** Add to test utilities module

---

## Low Priority Issues - NICE TO HAVE

### Code Example Improvements (4 items)

- [ ] **Example 1, line 74:** Add comment explaining why state is unused
  - **Suggested:** `fn current_timestamp(_state: &State<Value>)` with `// Timestamp doesn't depend on input`

- [ ] **Example 2, line 127:** Add comment for state parameter
  - **Suggested:** Add doc comment explaining DataFrame conversion

- [ ] **Example 5, line 296:** Document Phase 2 semantics more clearly
  - **Suggested:** Add inline explanation of dependency discovery

- [ ] **Example 8, line 479:** Add comment explaining Recipe struct fields
  - **Suggested:** Document why query is Option<Query>

---

## Implementation Checklist

### Before Phase 4 Implementation Begins

**Preparation Phase (1-2 days):**
- [ ] Run all queries through parser
- [ ] Verify all API signatures
- [ ] Resolve parameter position issues
- [ ] Update documentation stubs

**Verification Phase (2-3 days):**
- [ ] Test against async_hellow_world.rs patterns
- [ ] Verify all 44 unit test templates compile
- [ ] Verify all 23 integration test templates compile
- [ ] Check for missing helper functions

**Documentation Phase (1 day):**
- [ ] Update specs/REGISTER_COMMAND_FSD.md with volatile metadata
- [ ] Update specs/PROJECT_OVERVIEW.md with query examples
- [ ] Update CLAUDE.md with volatility system patterns
- [ ] Document circular dependency detection algorithm

---

## Test Implementation Order

### Week 1: Core Data Structures
- [ ] Status::Volatile variant (U1-U8)
- [ ] MetadataRecord.is_volatile field (U9-U11)
- [ ] Plan.is_volatile field (U12-U14)

### Week 2: Plan Building & Dependencies
- [ ] PlanBuilder volatility checking (U12-U14)
- [ ] Circular dependency detection (U15-U20)
- [ ] Volatile dependency propagation (U21-U24)
- [ ] Context.is_volatile (U25-U29)

### Week 3: Asset Management
- [ ] AssetRef::to_override() transitions (U30-U38)
- [ ] Edge cases (U39-U44)

### Week 4: Integration Tests
- [ ] Full pipeline tests (I1-I5)
- [ ] Circular dependency tests (I6-I8)
- [ ] Serialization tests (I9-I11)

### Week 5: Performance & Validation
- [ ] Concurrency tests (I12-I13)
- [ ] Cross-module tests (I14-I15)
- [ ] Performance tests (I16-I17)
- [ ] Corner cases (I18-I20)
- [ ] Error handling (I21-I23)

---

## File Update Checklist

### Specs to Update

- [ ] `specs/REGISTER_COMMAND_FSD.md`
  - Add `volatile: true/false` metadata syntax

- [ ] `specs/PROJECT_OVERVIEW.md`
  - Update volatility section with query examples
  - Clarify Query::parse() vs parse_query()

- [ ] `CLAUDE.md`
  - Add volatility system section
  - Document volatile command patterns
  - Add query syntax examples

- [ ] `specs/volatility-system/phase4-implementation.md`
  - Reference corrected query examples
  - Update test timelines if needed

### Source Code to Verify/Create

- [ ] `liquers-core/src/metadata.rs`
  - Status enum: add Volatile variant
  - MetadataRecord: add is_volatile field and helper
  - Unit tests: U1-U11

- [ ] `liquers-core/src/plan.rs`
  - Plan struct: add is_volatile field
  - Step enum: verify Info variant exists
  - PlanBuilder: add mark_volatile() method
  - Unit tests: U12-U24

- [ ] `liquers-core/src/context.rs`
  - Context struct: add is_volatile field
  - Context: add getter and with_volatile() builder
  - Unit tests: U25-U29

- [ ] `liquers-core/src/assets.rs`
  - AssetRef: add to_override() async method
  - Status transition validation
  - Unit tests: U30-U38

- [ ] `liquers-core/tests/volatility_integration.rs`
  - Create file with all 23 integration tests
  - Add test helper functions (make_plan, evaluate_plan, etc.)

---

## Validation Sign-Off Criteria

- [ ] All 9 query syntax issues resolved
- [ ] All 8 APIs verified or implemented
- [ ] All 3 documentation issues clarified
- [ ] All 44 unit tests implemented and passing
- [ ] All 23 integration tests implemented and passing
- [ ] No unwrap()/expect() in library code (only tests)
- [ ] All match statements explicit (no `_ =>` arms)
- [ ] Circular dependency detection working correctly
- [ ] All Status::Volatile transitions correct
- [ ] Query parsing accepts all corrected examples

---

## Notes for Implementation Team

### Known Issues
1. Query syntax in examples is pseudo-code - requires parser validation
2. Some API signatures assumed - need verification against codebase
3. Context parameter positioning has known bug (documented in ISSUES.md)

### Quick Reference: Key Patterns
```rust
// Command registration with volatile
register_command!(cr,
    fn my_command(state) -> result
    volatile: true
    namespace: "my"
)?;

// Plan marking
pb.mark_volatile("Reason for volatility");

// Status checking
assert_eq!(metadata.status, Status::Volatile);
assert!(metadata.is_volatile());  // Helper considers both field and status

// Context propagation
let context = Context::new(envref, plan.is_volatile);
let nested_context = context.with_volatile(some_flag);
```

### Help Needed From
- [ ] Core team: API signature verification
- [ ] Parser team: Query syntax validation
- [ ] Test team: Helper function implementation
- [ ] Docs team: REGISTER_COMMAND_FSD.md updates

---

**Last Updated:** 2026-02-17
**Status:** Ready for Phase 4 implementation after checklist completion
**Estimated Resolution Time:** 3-5 days for all issues
