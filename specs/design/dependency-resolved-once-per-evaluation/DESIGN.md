---
id: DEPENDENCY-RESOLVED-ONCE-PER-EVALUATION
kind: design
title: Evaluation session — each dependency evaluated once per evaluation, at any chain depth
workflow: liquers-project
status: in_review
phase: architecture
area: [core/assets, core/plan]
issues: [FAILING-UPSTREAM-RERUNS-EXPONENTIALLY-AT-QUEUE-CAPACITY, EVALUATING-A-DEEP-CHAIN-TOP-DOWN-OVERFLOWS-THE-STACK, REFUSED-DEPENDENCY-RECOMPUTED-TWICE-DURING-A-DEPENDENT-EVALUATION]
merged: 2026-10-12
affects_docs: [reference/DEPENDENCIES_STATUS.md, reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md, guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md]
created: 2026-10-12
---
# dependency-resolved-once-per-evaluation Design Tracking

Merged on 2026-10-12, by maintainer decision, with `deep-chain-evaluation-stack-depth`, which is now
`superseded` by this design. The decision followed two findings: a failing upstream re-runs
exponentially when the queue is at capacity, and a memo per asset run is not enough to stop it
(Phase 1, example 2). Both halves need one shared mechanism, the **evaluation session**.

| Former design | Source issue (now owned here) | Became |
|---|---|---|
| (this design, compact draft of 2026-10-12) | `FAILING-UPSTREAM-RERUNS-EXPONENTIALLY-AT-QUEUE-CAPACITY` (leading) | AC-1 to AC-3, AC-5 to AC-8 |
| (this design, compact draft of 2026-10-12) | `REFUSED-DEPENDENCY-RECOMPUTED-TWICE-DURING-A-DEPENDENT-EVALUATION` | AC-4 |
| `deep-chain-evaluation-stack-depth` | `EVALUATING-A-DEEP-CHAIN-TOP-DOWN-OVERFLOWS-THE-STACK` | AC-9 to AC-14 (AC-14 replaces its inline AC-4) |

Converted from the compact to the full form at the merge: the combined scope is `L`, although the
leading issue alone is `M`. `readiness` (the draft's `ready`, then `needs-decision`) was cleared at
the merge and is reassessed after Phase 4.

## Phase Status

- [x] Phase 1: High-Level Design (approved 2026-10-12, with the review fixes: AC-14, session rules 1-6)
- [ ] Phase 2: Architecture (in review)
- [ ] Phase 3: Examples and Tests
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
