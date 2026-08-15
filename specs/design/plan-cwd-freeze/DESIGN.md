---
id: PLAN-CWD-FREEZE
kind: design
title: Freeze CWD in the plan and cut correct predecessor boundaries
workflow: liquers-project
status: draft
phase: examples
area: [core/plan, core/query, core/context, core/assets]
gh_pr: []
issues: [CORE-RECIPES-EXPAND-PREDECESSORS-CRASH, CORE-PLAN-POLICY-AND-DEFAULTS]
affects_docs: [specs/reference/api/DOC_08_RECIPES_PLANS.md, specs/reference/PROJECT_OVERVIEW.md, specs/reference/PAYLOAD_GUIDE.md]
created: 2026-08-14
superseded_by:
---
# plan-cwd-freeze Design Tracking

**Created:** 2026-08-14

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (approved)
- [x] Phase 3: Examples & Testing (awaiting approval)
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
  query to `Context::evaluate`, which resolves it against the live CWD (`context.rs:423`). This does
  not affect execution once frozen — the interpreter installs each step's frozen CWD, so dynamic
  evaluation resolves against a statically-known value. It affects only what identifies a cut
  boundary's asset (open question 1).
- `resolve_query_from_cwd` and `resolve_key_from_cwd` are the only two dynamic resolution
  functions, with two production call sites in `context.rs`; `CwdCursor::resolve_key` already
  branches on `is_relative`, so a "consumed the CWD" flag has one natural home.
- `PlanBuilder` keeps no forward state: `namespaces_for_query` resolves `ns` through `last_ns`,
  which is a backward `rev().find_map(..)` scan redone per action. CWD needs the same prefix
  knowledge but is ordered and branches at links.

Decided in discussion: `Context::evaluate`/`apply` reject relative queries (nothing identifies
CWD-dynamic commands, so tolerating them forces a CWD into every boundary query and multiplies
cache entries per folder); the cut moves out of `PlanBuilder` into a post-freeze pass, which makes
R2 and R4 unreachable rather than fixed; recipe overrides never enter a query, which holds by
construction since overrides patch only the last action and a cut removes only the predecessor.

Folder renamed from `predecessor-evaluation-boundary`; nothing referenced the old slug.

Phase 2 preflight: no blocker. `PARAMETER-ESCAPING-INCOMPLETE` (P0) closed on `main` mid-Phase 2 via
`parameter-entity-escaping` (PR #34); re-measured after rebasing and all eleven round-trip probes now
pass, so `parse(encode(q)) == q` holds and the concern it raised no longer applies. Two new draft
issues assessed: `QUERY-AST-DISCARDS-ENTITIES` (P3) is mildly favourable — AST and `DependencyKey`
identity are both canonical over decoded semantics — and `RESOURCE-NAME-ASCII-ONLY` (P2) adds no
exposure, since freeze concatenates existing `ResourceName`s rather than parsing new text.

Phase 2 cost to confirm: rejecting relative `evaluate`/`apply` removes a capability
`plan-relative-resolution` explicitly blessed, with four tests pinning it
(`recipe_cwd_resolution.rs` `via_evaluate`/`via_state`/`via_apply`, and `context.rs:1601`). They are
rewritten to take the directory as a `-R-key/.` link, not deleted. No liquers-lib/axum/web command
is affected.

Phase 3: 12-shape equivalence suite is the primary deliverable, discharging the Phase 2 claim that
cutting is policy rather than correctness. E8 is deliberately an *inequivalence* test, pinning the
one case the two forms differ (an undeclared payload command) so the claim stays falsifiable. All
queries checked with `liquers-validate` before being written down. Runnable tests rather than
`examples/*.rs`: there is no user-facing API here, only internal behaviour to pin.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
