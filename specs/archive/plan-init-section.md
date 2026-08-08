# PLAN-INIT-SECTION

Status: Draft

## Summary
`Plan` keeps executable steps in `steps`, and now also exposes planning-time diagnostics/metadata in dedicated fields.

## Plan Fields
`Plan` contains:
1. `query` (existing)
2. `is_volatile` (existing)
3. `expires` (existing)
4. `steps` (existing, execution steps)
5. `init_steps: Vec<Step>` (new)
6. `error: Option<Error>` (new)
7. `dependencies: Vec<PlanDependency>` (new)

## Meaning Of New Fields
1. `init_steps`
Contains only `Step::Info`, `Step::Warning`, `Step::Error` generated during:
1. plan creation
2. volatility checks
3. cyclic dependency checks
4. dependency analysis

These are diagnostics for transparency before execution, not executable planning steps.

2. `error`
Holds a planning/analysis error when available (for example cyclic dependency detection).
The error is also mirrored in `init_steps` as `Step::Error(...)`.

3. `dependencies`
Plan-level dependency list represented as `Vec<PlanDependency>`.
`PlanDependency` follows dependency-management architecture meaning: dependency target + dependency relation.

## Metadata Update Goal
Metadata can be updated using the initial plan metadata fields (`query`, `is_volatile`, `expires`, `init_steps`, `error`, `dependencies`) without executing `steps`.

## AsyncRecipeProvider Integration
`AsyncRecipeProvider::get_asset_info` builds the plan and populates asset info from it:
1. `asset_info.is_volatile` from `plan.is_volatile`
2. `asset_info.is_error` from `plan.error` / init diagnostics
3. `asset_info.message` set to `plan.error.message` when available

This provides early planning transparency in asset listings and previews.
