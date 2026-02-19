# PlanBuilder Direct Usage Audit

**Date:** 2026-02-19
**Purpose:** Audit all direct `PlanBuilder::new()` usage to ensure Phase 2 volatility checks are applied where needed.

## Summary

- **Total PlanBuilder::new() call sites:** 24
- **Inside make_plan():** 1
- **Inside find_dependencies/has_volatile_dependencies:** 4
- **In Recipe.to_plan() (sync, no envref):** 1
- **In tests:** 18
- **Action required:** 0 (all legitimate usage)

## Detailed Findings

| Location | Function | Async? | EnvRef? | Phase 2 Needed? | Action | Rationale |
|----------|----------|--------|---------|-----------------|--------|-----------|
| interpreter.rs:26 | make_plan() | Yes | Yes | **N/A** | None | This IS make_plan - Phase 2 called after this |
| recipes.rs:142 | Recipe.to_plan() | No | No | No | None | Sync method, no envref available, returns plan for later processing |
| plan.rs:796 | check_parameter_for_volatile_links() | No | Yes | No | None | Phase 1 check only - called during plan building, before Phase 2 |
| plan.rs:1378 | find_dependencies() | Yes | Yes | No | None | Would cause infinite loop - recursively analyzes plans |
| plan.rs:1414 | find_dependencies() | Yes | Yes | No | None | Would cause infinite loop - recursively analyzes plans |
| plan.rs:1472 | find_dependencies() | Yes | Yes | No | None | Would cause infinite loop - recursively analyzes plans |
| plan.rs:1549 | Test: first_test | N/A | N/A | N/A | None | Test code - testing plan building only |
| plan.rs:1574 | Test: first_override | N/A | N/A | N/A | None | Test code - testing override functionality |
| plan.rs:1602 | Test: handle_allow_placeholders | N/A | N/A | N/A | None | Test code - testing placeholder handling |
| plan.rs:1605 | Test: handle_allow_placeholders | N/A | N/A | N/A | None | Test code - testing placeholder handling |
| plan.rs:1608 | Test: handle_allow_placeholders | N/A | N/A | N/A | None | Test code - testing placeholder handling |
| plan.rs:1612 | Test: handle_allow_placeholders | N/A | N/A | N/A | None | Test code - testing placeholder handling |
| plan.rs:1854 | Test: test_plan_split_index | N/A | N/A | N/A | None | Test code - testing plan structure |
| plan.rs:1883 | Test: test_pop_parameter_value | N/A | N/A | N/A | None | Test code - testing parameter handling |
| plan.rs:1905 | Test: test_string_parameter_value | N/A | N/A | N/A | None | Test code - testing parameter handling |
| plan.rs:1934 | Test: test_q_instruction_with_arguments_error | N/A | N/A | N/A | None | Test code - testing error handling |
| plan.rs:1946 | Test: test_plan_builder_mark_volatile | N/A | N/A | N/A | None | Test code - testing mark_volatile() method |
| plan.rs:1984 | Test: test_is_action_volatile | N/A | N/A | N/A | None | Test code - testing is_action_volatile() method |
| plan.rs:2018 | Test: test_plan_builder_sets_is_volatile | N/A | N/A | N/A | None | Test code - testing is_volatile field propagation |
| plan.rs:2023 | Test: test_plan_builder_sets_is_volatile | N/A | N/A | N/A | None | Test code - testing is_volatile field propagation |
| plan.rs:2034 | Test: test_v_instruction_marks_volatile | N/A | N/A | N/A | None | Test code - testing 'v' instruction detection |
| plan.rs:2051 | Test: test_v_instruction_no_action_step | N/A | N/A | N/A | None | Test code - testing 'v' instruction behavior |
| plan.rs:2071 | Test: test_volatile_command_marks_volatile | N/A | N/A | N/A | None | Test code - testing volatile command detection |
| plan.rs:2089 | Test: test_link_parameter_volatile | N/A | N/A | N/A | None | Test code - testing link parameter volatility |

## Key Insights

### Legitimate Direct Usage Patterns

1. **Inside make_plan() (1 location)**
   - This is the PRIMARY location where plans are built
   - Phase 2 check (`has_volatile_dependencies`) is called immediately after
   - No action needed

2. **Inside find_dependencies() (3 locations)**
   - Recursively builds plans to analyze dependency trees
   - Calling `make_plan()` would cause infinite loop (make_plan → has_volatile_dependencies → find_dependencies → make_plan → ...)
   - These plans are analyzed for dependencies only, not executed
   - No action needed

3. **Inside check_parameter_for_volatile_links() (1 location)**
   - Phase 1 volatility check (during plan building)
   - Called BEFORE Phase 2 (has_volatile_dependencies)
   - No envref available in this context
   - No action needed

4. **Recipe.to_plan() (1 location)**
   - Synchronous method without async context
   - No EnvRef available
   - Returns plan for later processing by make_plan() or direct use
   - Legitimate use case for direct plan building with argument/link overrides
   - No action needed

5. **Test code (18 locations)**
   - Tests focus on specific plan building features
   - Tests don't need Phase 2 checks unless explicitly testing volatility
   - No action needed

## Conclusion

**All 24 direct PlanBuilder usage sites are legitimate and should NOT be converted to use make_plan().**

The architecture is correct:
- `make_plan()` is the public API that includes Phase 2 checks
- Direct `PlanBuilder` usage is for:
  - Internal implementation within `make_plan()` itself
  - Recursive analysis (find_dependencies) where Phase 2 would cause infinite loop
  - Phase 1 checks (link parameters) that happen during building
  - Recipe conversion without async context
  - Testing specific plan building features

## Recommendations

1. **No code changes required** - all usage is appropriate
2. **Document pattern**: Consider adding a comment in PlanBuilder docs explaining when to use `make_plan()` vs. direct `PlanBuilder`
3. **Future additions**: Any NEW code that builds plans for execution should use `make_plan()` unless it falls into one of the legitimate patterns above

## Verification

```bash
# Total call sites found
rg "PlanBuilder::new" liquers-core/src/ | wc -l
# Expected: 24
# Actual: 24 ✓

# All assessed in this document: ✓
```
