# Phase 1: High-Level Design - Volatility System

## Feature Name

Comprehensive Volatility Tracking and Propagation System

## Purpose

Establish a consistent, complete system for tracking volatility throughout the entire Liquers execution flow (recipe → plan → command → asset → state → metadata). Currently, volatility is computed dynamically via the `IsVolatile<E>` trait, but State metadata lacks volatility information, making it impossible for consumers to determine if a State originated from a volatile source without re-computing from the query. This feature makes volatility a first-class, observable property at every layer.

## Core Interactions

### Query System
Add volatility instruction **`v`** (similar to `q` and `ns`) that explicitly marks the query as volatile, regardless of prior volatility. This sets `Plan.is_volatile = true`. Optionally adds `Step::Info` to clarify where volatility originates (e.g., "Volatile due to instruction 'v' at position X"). Volatility can also be inferred implicitly from query structure (which commands are called).

### Store System
Store CAN store volatile data (and should in some situations), but has no power to trigger re-evaluation. Volatile data in store should be treated as expired. Store itself does not need volatility awareness - volatility is inconsequential at the store layer.

### Command System
Already implemented: `CommandMetadata.volatile` flag (line 767, command_metadata.rs). Commands declare themselves as volatile (e.g., `current_time`, `random`). When a volatile command appears in a plan, `Plan.is_volatile` is set to `true`. Optionally add `Step::Info` to clarify source of volatility.

### Asset System
**Critical behavior change:** AssetManager returns NEW AssetRef for volatile assets (not existing AssetRef copy). Only the AssetRef owner can read volatile data - anyone else requesting it triggers new AssetRef creation and re-evaluation. Volatility is known upfront from query/plan/recipe analysis before execution starts. Add `AssetRef::to_override()` method to turn Volatile status → Override status (only status transition allowed).

### Value Types
Value by itself does not know if it's volatile. State (which contains metadata) does have this information via metadata fields.

### Metadata/State System
**Dual representation of volatility:**
1. **`Status::Volatile` variant:** Asset HAS a volatile value (like expired, use once). Means value is valid but should only be used once, then expires.
2. **`is_volatile: bool` flag in MetadataRecord:** Known to be volatile even during evaluation (Submitted, Dependencies, Processing). Indicates that IF a value is produced, it WILL be volatile.

These serve different purposes: Status is for ready assets, flag is for in-flight assets.

### Plan/Recipe System
Already implemented: `Recipe.volatile` flag (line 40, recipes.rs) simply labels the recipe as volatile. `IsVolatile<E>` trait (lines 316-422, interpreter.rs) computes volatility. Add `is_volatile: bool` field to `Plan` struct (currently only computed on-demand via trait). No new Step variant needed - use existing `Step::Info` for clarity if desired.

### Context System
Add `is_volatile: bool` flag to Context. A volatile Context means "currently evaluating a volatile asset". Side effects of volatile asset evaluation (i.e., `context.evaluate()` called from volatile context) should result in volatile assets. This propagates volatility through nested evaluations. Context volatility is set upfront when evaluation starts, not changed during execution.

## Crate Placement

**liquers-core** - Primary implementation
- `metadata.rs`: Add `Status::Volatile` variant, add `is_volatile: bool` field to `MetadataRecord`
- `plan.rs`: Add `is_volatile: bool` field to `Plan` struct (no new Step variant needed)
- `assets.rs`: Add volatility tracking to `AssetData`, modify AssetManager to return new AssetRef for volatile assets, add `AssetRef::to_override()`
- `context.rs`: Add `is_volatile: bool` flag to Context, propagate to nested `evaluate()` calls
- `interpreter.rs`: Set `Plan.is_volatile` field during plan building, set `MetadataRecord.is_volatile` during evaluation, initialize Context with is_volatile flag
- `query.rs`: Add `v` volatility instruction parsing

No changes to liquers-store, liquers-lib, or liquers-axum.

## Design Decisions (resolved)

1. **Propagation rules:** YES - volatility is contagious. If a volatile State is transformed, output is volatile.
2. **Status::Volatile variant:** YES - add it. Has special meaning: value exists but should only be used once (like expired).
3. **AssetData is_volatile field:** YES - should have is_volatile flag (or use from metadata). Computed from query/plan/recipe BEFORE execution starts.
4. **Dynamic volatility:** NO - Volatility determined upfront, NOT during execution. Once AssetRef returned from AssetManager, volatility is fixed (except via `to_override()`). Simplifies design significantly.
5. **Backward compatibility:** Not critical now. Dependency management handles command/recipe volatility changes (assets become expired automatically).

## Open Questions for Phase 2

1. **AssetManager "new AssetRef" logic:** When exactly does AssetManager create new AssetRef vs. return existing? How to detect if requester is the "owner"? Or should volatile assets simply NEVER be cached in AssetManager?
2. **to_override() semantics:** What happens to dependencies when Volatile → Override? Do they become Override too or stay Volatile?
3. **Volatility computation timing:** At what point is `Plan.is_volatile` field set? During plan building via `IsVolatile<E>` trait? How to ensure it's computed before execution starts?
4. **Step::Info usage:** Should we automatically add `Step::Info` to clarify volatility sources (e.g., "Volatile due to 'v' instruction" or "Volatile due to command 'random'"), or leave that optional?

## References

- `specs/ISSUES.md` - Issue 1: VOLATILE-METADATA
- `liquers-core/src/interpreter.rs` (lines 316-422) - Existing `IsVolatile<E>` trait
- `liquers-core/src/command_metadata.rs` (line 767) - `CommandMetadata.volatile` field
- `liquers-core/src/recipes.rs` (line 40) - `Recipe.volatile` field
- `liquers-core/src/metadata.rs` (lines 470-510) - `MetadataRecord` structure
