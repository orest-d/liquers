# PLAN-INIT-SECTION

Status: Draft

## Summary
Extend `Plan` with a new ordered section `init` that is executed before the main execution steps.

`init` is for early diagnostics and metadata shaping, not for full asset computation. It typically contains:
1. `Info` / `Warning` / `Error` steps created during planning.
2. `SetCwd` (or equivalent cwd updates needed by later interpretation).
3. Dependency analysis messages (volatility reasons, dependency list + relation reasons, cycle diagnostics).

The key outcome is early feedback for potentially problematic assets before expensive execution starts.

## Problem
Today, important planning-time findings are mixed into runtime execution flow. This creates two issues:
1. Metadata producers (for example recipe providers) cannot safely run just the diagnostic/metadata-relevant part of a plan.
2. Users get feedback late, often only after partial execution has already started.

This is especially painful for:
1. Volatility diagnostics.
2. Dependency reason inspection.
3. Early cycle reporting.

## Goals
1. Add explicit `Plan::init` sequence executed before `Plan::steps`.
2. Make planning diagnostics first-class and deterministic.
3. Allow soft execution of init-only steps to produce metadata/log feedback without full evaluation.
4. Keep compatibility with existing planner/executor behavior where possible.

## Non-Goals
1. Executing action steps from `init`.
2. Replacing the full interpreter.
3. Changing dependency semantics themselves (this feature exposes/uses them earlier).

## Proposed Model
### Plan shape
Add a new section:
```rust
pub struct Plan {
    pub init: Vec<Step>,
    pub steps: Vec<Step>,
    // existing fields unchanged
}
```

Execution order:
1. Execute `init` in order.
2. If `init` produces a fatal error, stop and surface it.
3. Execute `steps` only if init phase succeeds (or policy allows continuation on warnings).

### Step policy for `init`
`init` should only contain metadata-safe steps:
1. `Info` / `Warning` / `Error`.
2. `SetCwd`.
3. Future metadata-only step types (if added later).

Planning code should not place heavy execution steps (resource fetch, action execution) into `init`.

## Planner Changes
Planner should emit into `plan.init`:
1. Volatility explanation messages.
2. Full dependency list entries with per-dependency reason (`StateArgument`, `ParameterLink`, `Recipe`, `ContextEvaluate`, etc.).
3. Circular dependency diagnostics when detected (including involved key/query path where available).
4. Early cwd-setting steps needed for deterministic relative resolution in later phases.

Planner should continue emitting executable work into `plan.steps`.

## Mini Interpreter (Init Soft Execution)
Introduce a narrow interpreter path that processes only `plan.init` and only metadata-impacting semantics.

### Responsibilities
1. Consume `Info` / `Warning` / `Error` into metadata/log records.
2. Apply cwd updates used by metadata resolution.
3. Stop on fatal `Error` policy and return structured failure.

### Must Not
1. Fetch resources.
2. Execute commands/actions.
3. Mutate asset value payload.

### Suggested API
```rust
pub async fn execute_plan_init<E: Environment>(
    plan: &Plan,
    context: &Context<E>,
    envref: EnvRef<E>,
) -> Result<InitExecutionReport, Error>;
```

`InitExecutionReport` should include:
1. Collected infos/warnings/errors.
2. Final cwd (if changed).
3. Optional dependency explanation snapshot used to populate metadata.

## Metadata Integration
Recipe providers (or other metadata producers) can:
1. Build plan.
2. Soft-execute `plan.init`.
3. Write resulting diagnostics and dependency reasons into metadata.
4. Skip full plan execution when only metadata is requested.

This enables early UX feedback (for example in directory listings or preview UIs) even when asset evaluation is deferred.

## Error and Cycle Handling
When cycle detection occurs during planning:
1. Add cycle diagnostic `Error` into `plan.init`.
2. Soft init execution returns early with that error.
3. Caller receives actionable failure without starting main evaluation.

This keeps cycle reporting deterministic and decoupled from runtime side effects.

## Backward Compatibility
1. Existing plans without `init` behave as before (`init` defaults to empty).
2. Existing interpreters can be incrementally adapted:
   - phase 1: full interpreter executes `init` then `steps`.
   - phase 2: callers that need early metadata can use init-only interpreter.

## Acceptance Criteria
1. `Plan` supports an `init` section and preserves ordering.
2. Planner emits diagnostics and dependency reasons into `init`.
3. Init-only interpreter can run without invoking action/resource execution.
4. Metadata can be produced from recipe + init-only execution.
5. Cycle errors are surfaced during init phase before main execution.

## Suggested Implementation Phases
1. Data model update: add `Plan::init`, defaults, serde/tests.
2. Planner routing: move planning diagnostics from main steps to `init`.
3. Interpreter update: execute `init` before `steps`.
4. Add init-only mini interpreter API and integration points for recipe metadata.
5. Tests:
   - unit tests for plan ordering and init filtering.
   - integration tests for early volatility/dependency diagnostics.
   - cycle-detection test proving no main-step execution after init error.

