---
id: ACTIVE-08
kind: design
title: Design for QUERY-ABSOLUTE-FIELD-NAME-AMBIGUOUS
status: in_review
phase: implementation
readiness: needs-decision
area: [core/query]
issues: [QUERY-ABSOLUTE-FIELD-NAME-AMBIGUOUS]
created: 2026-09-03
---

# Design Tracking

- [x] Phase 1: High-Level Design
- [x] Phase 2: Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan

## Review 2026-10-06

Re-verified against HEAD after the latest merges. **Rewritten: the premise changed.** The issue and the
original design treat `Query::absolute` as a flag with "no semantic meaning". At HEAD it **has**
meaning: `CwdState::resolve_query_scoped` (`liquers-core/src/query.rs` ≈2295) roots the resource
segments of a query with a leading `/` at the logical root, independent of the live CWD, and
`Plan::absolute_query_resource_step_index` (`plan.rs` ≈2214) depends on it.
`PROJECT_OVERVIEW.md` ("an absolute outer query's source resource remains rooted independently of
that live CWD") documents the semantics. The module doc (`query.rs` ≈67) still says "It currently has no semantic
meaning", which is now false. The recommended name is therefore `rooted` (the issue's own
alternative "if the leading `/` is ever given meaning"), and the documentation fix is a prerequisite
that does not wait for the decision. Readiness stays `needs-decision` (a public field rename).
