---
id: DEPENDENCY-AUDIT-AND-EXPIRY-PROVENANCE
kind: design
title: Dependency audit correctness, audit policy and expiry provenance
status: draft
workflow: liquers-designer
phase: architecture
area: [core/assets]
gh_pr: []
issues: [AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION, DEPENDENCY-AUDIT-POLICY-NOT-EXPRESSIBLE, EXPIRY-RECORDS-NO-REASON, DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED, STALE-DEPENDENCY-PATH-HAS-NO-END-TO-END-TEST]
created: 2026-09-28
superseded_by:
---
# dependency-audit-and-expiry-provenance Design Tracking

**Created:** 2026-09-28

## Phase Status

- [x] Phase 1: High-Level Design (approved 2026-09-28)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Notes

Groups five open `core/assets` issues around dependency verification and expiry. Three of them
(`EXPIRY-RECORDS-NO-REASON`, `STALE-DEPENDENCY-PATH-HAS-NO-END-TO-END-TEST`,
`AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION`) were handed off explicitly by
`stale-dependency-status-finalization`. `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION`
is a candidate sixth (Phase 1 open question 5).

**Phase 1 drafted 2026-09-28.** Cited code sites were checked against HEAD before drafting.

**Phase 2 drafted 2026-09-28.** All five Phase 1 questions are settled: membership-only
listing versions; a per-environment `DependencyAuditPolicy { Explicit, OnLoad }`; the reachability
test through a public `Context::schedule_dependency`; log levels per reason; the uncached race
excluded. It found and filed `IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES` (the immediate
manager's deadline check compares status with status) and proposes it for this design's scope. The
multi-agent review could not run because of a spend limit, so both review passes were done inline
(see Phase 2 §"Phase 2 review").

**Phase 2 gate, first round (2026-09-28).** Part E was revised to `Context::submit` plus a public
`Context::wait_for_dependency`, with no new handle type, as the owner asked. The immediate-manager
deadline bug was accepted into scope. `#[non_exhaustive]` is applied to `AuditReport` and
`AuditFinding`, with public constructors, so a future asset manager outside core can still build
them. That check found the `AssetManager` trait is sealed today, filed as
`ASSET-MANAGER-TRAIT-CANNOT-BE-IMPLEMENTED-OUTSIDE-CORE`.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
