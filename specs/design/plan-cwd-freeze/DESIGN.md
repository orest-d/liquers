---
id: PLAN-CWD-FREEZE
kind: design
title: Freeze CWD in the plan and cut correct predecessor boundaries
workflow: liquers-project
status: draft
phase: high-level
area: [core/plan, core/query, core/context, core/assets]
gh_pr: []
issues: [CORE-RECIPES-EXPAND-PREDECESSORS-CRASH, CORE-PLAN-POLICY-AND-DEFAULTS]
affects_docs: []
created: 2026-08-14
superseded_by:
---
# plan-cwd-freeze Design Tracking

**Created:** 2026-08-14

## Phase Status

- [x] Phase 1: High-Level Design (awaiting approval)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Phase 1 established by experiment. Enabling `disable_expand_predecessors()` in `Recipe::to_plan`
gives 11 failures in `cargo test -p liquers-core --lib` from four causes; the named test does not
panic and fails on the documented `payload: required` rule.

Rescoped after discussion. The root problem is not the boundary but that CWD-relative operands are
resolved in three places with three cursors that must agree. `Plan::freeze_cwd` collapses them, and
dissolves the boundary's CWD failures (R3) as a consequence. `plan-relative-resolution`
phase 2 §"Future Plan Normalization and Optimization" anticipated this pass; it blocked *removing*
`SetCwd`, not rewriting operands, and those are separable.

Verified during Phase 1:
- `resolve_query_scoped` already canonicalizes: relative operands become per-folder keys, absolute
  ones pass through, so a shared input keeps one cache entry across folders.
- `-R-key/.` plans to `Step::UseKeyValue` and normalizes to the CWD as a key value.
- A *default* link is invisible to the cache key; an explicit link is not.
- `Context::get_cwd_key`/`set_cwd_key` are `pub` with zero users outside `liquers-core`.
- Privatizing the accessors closes CWD *observation* but not *use*: a command can hand a relative
  query to `Context::evaluate`, which resolves it against the live CWD (`context.rs:423`).

Folder renamed from `predecessor-evaluation-boundary`; nothing referenced the old slug.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
