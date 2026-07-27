# Phase 1: High-Level Design - payload-nested-evaluation-inheritance

## Feature Name

Payload / Nested-Evaluation Boundary (resolves ISSUES.md: PAYLOAD-NESTED-EVALUATION-INHERITANCE)

## Purpose

`specs/PAYLOAD_GUIDE.md` and `specs/PROJECT_OVERVIEW.md` state that nested evaluations
inherit the parent's payload; `liquers_core::context` does not implement that — `Context::evaluate`,
`Context::get_dependency_state`, and `Context::apply` schedule dependencies through the `AssetManager`
without forwarding `Context::payload`, proven by
`test_payload_not_inherited_in_nested_evaluation` (`liquers-core/tests/injection.rs`). This
feature picks one authoritative payload boundary and makes docs, rustdoc, and behavior agree.

## Core Interactions

### Query System
No change to parsing/Key encoding. Relevant only in that a query has no per-payload
identity, so payload cannot be encoded into the cache key that identifies a dependency.

### Store System
Not touched.

### Command System
No new commands. Affects any command using `injected` payload/newtype parameters
inside a nested `Context::evaluate` / `get_dependency_state` / `apply` call.

### Asset System
Central to the decision: `AssetManager::get_dependency_asset` resolves nested dependencies
through the same query/key-based cache as top-level `get_asset` (`liquers-core/src/assets.rs:2683-2690`).
Only `apply_immediately`/`evaluate_immediately` are ad hoc, uncached, and payload-bearing today
(`context.rs:57-62`). Forwarding payload into cached nested dependencies risks either being silently
dropped by the cache or leaking one evaluation's payload-dependent result into another's (the doc
at `context.rs:29` already asserts "Asset evaluation should not depend on the user").

### Value Types
None.

### Web/API
None directly; `liquers-axum` request-scoped payloads (if any) are indirectly affected by
whichever boundary is chosen, since it determines whether request context can reach nested
dependency commands.

### UI
None.

## Crate Placement

**liquers-core** only (`src/context.rs`, `src/assets.rs`) plus documentation
(`specs/PAYLOAD_GUIDE.md`, `specs/PROJECT_OVERVIEW.md`, `liquers_core::context` rustdoc) and the
existing non-inheritance test in `liquers-core/tests/injection.rs`. No downstream crate change is
expected unless Phase 2 introduces a new explicit forwarding API that `liquers-lib`/`liquers-axum`
should adopt.

## Open Questions

1. **The required decision itself**: implement inheritance (with explicit cloning/caching/asset-sharing
   rules) vs. keep payload strictly asset-local and correct the docs. Given that nested dependencies
   are resolved through the shared, query-keyed cache (`get_dependency_asset` → `get_asset`), inheriting
   payload there without changing cache identity would make cached results depend on an untracked
   input — this leans Phase 2 toward **asset-local payload + an explicit, non-cached forwarding
   operation** for callers that genuinely need it, but the tradeoff needs to be argued through, not
   assumed.
2. If an explicit forwarding operation is added, does it reuse `Context::apply` (already ad hoc,
   uncached) or need a new `Context` method?
3. Does `Context::apply`'s existing non-forwarding behavior change, or does it stay as the natural
   home for an explicit "apply with parent payload" variant?

## References

- `specs/ISSUES.md` — Issue: PAYLOAD-NESTED-EVALUATION-INHERITANCE
- `specs/PAYLOAD_GUIDE.md` (inheritance claims to correct)
- `specs/PROJECT_OVERVIEW.md` (inheritance claims to correct)
- `liquers-core/src/context.rs:1-81` (module rustdoc already documents current non-inheriting behavior)
- `liquers-core/src/assets.rs:2630-2714` (`AssetManager` trait: `get_dependency_asset`, `apply`, `apply_immediately`)
- `liquers-core/tests/injection.rs::test_payload_not_inherited_in_nested_evaluation`
