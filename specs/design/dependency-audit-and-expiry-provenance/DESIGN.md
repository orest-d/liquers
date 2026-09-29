---
id: DEPENDENCY-AUDIT-AND-EXPIRY-PROVENANCE
kind: design
title: Dependency audit correctness, audit policy, expiry provenance, outside-change detection and external asset managers
status: draft
workflow: liquers-project
phase: examples
area: [core/assets]
gh_pr: []
affects_docs: [DEPENDENCIES_STATUS, ASSETS, ASSET_LIFECYCLE, DOC_03_ASSETS_EXECUTION_LIFECYCLE, DOC_04_ENVIRONMENT_CONTEXT_EVALUATION, ENVIRONMENT_CONFIG, COMMAND_REGISTRATION_GUIDE, ENVIRONMENT_CONSTRUCTION_GUIDE, STORE_IMPLEMENTATION_GUIDE, UNITTEST_GUIDE]
issues: [AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION, DEPENDENCY-AUDIT-POLICY-NOT-EXPRESSIBLE, EXPIRY-RECORDS-NO-REASON, DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED, STALE-DEPENDENCY-PATH-HAS-NO-END-TO-END-TEST, IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES, ASSET-MANAGER-TRAIT-CANNOT-BE-IMPLEMENTED-OUTSIDE-CORE, STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS]
created: 2026-09-28
superseded_by:
---
# dependency-audit-and-expiry-provenance Design Tracking

**Created:** 2026-09-28

## Phase Status

- [x] Phase 1: High-Level Design (approved 2026-09-28)
- [x] Phase 2: Solution & Architecture (approved 2026-09-29)
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
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

**Phase 2 gate, second round (2026-09-28).** The owner brought
`ASSET-MANAGER-TRAIT-CANNOT-BE-IMPLEMENTED-OUTSIDE-CORE` into scope and said
`DependencyManagerAccess` may be public. That became Part F. The graph type is made public but
opaque, with its methods narrowed to `pub(crate)`. Five lifecycle primitives are made public with
documented contracts. `refresh_command_versions` gets a default body. A from-scratch external
manager in `tests/` runs the shared manager scenarios. This widens the design beyond the
Phase 1 scope, and that is recorded here rather than by editing the approved Phase 1.

**Workflow switched to `liquers-project` (2026-09-29)**, at the owner's request, while Phase 2 was
awaiting approval. The design was started under `liquers-designer`. The switch added what the
new workflow requires and the old one did not: a Documentation Intent section in Phase 1 (added
after that phase's approval and marked as such), a Known-Issue Preflight and a Documentation
Architecture in Phase 2, `affects_docs`, and a mandatory Phase 5. The empty Phase 3 and 4 templates
were replaced with the `liquers-project` templates. Nothing already written was changed in meaning.

**Phase 2 gate, third round (2026-09-29).** A glossary and worked examples were added to Phase 1,
with code-level traces in Phase 2. `STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS` was brought
in as Part G, following the owner's solution: content-hash versions flagged by bit 127, verified on
read and on demand; a `Source` or `Override` always taken as input; a recipe-backed value converted to
`Override` by default, or deleted under the opt-in `Corrupted` policy. Only content hashes are
flagged, so command versions do not change on upgrade.

**Phase 3 drafted 2026-09-29.** Conceptual code, by the owner's choice. Five parallel drafters
(two examples, pitfalls, unit and integration test plans) were merged by a synthesizer, which
corrected the drafts against Phase 2. In particular, an audit *expires* edges recorded as unknown,
because those are the attribution edges of results reached through intermediate queries. The
synthesis found four gaps in Phase 2, which were settled and recorded there: policy accessors,
`external_change_action` returning `Option`, `on_load` treating a recorded unknown as compatible,
and direct-vs-transitive audit reasons. Three reviewers found nothing blocking. They raised three
fixes, all applied: `VersionVerification` was not declared in Phase 2; the decision-table tests
ignored the `Option`; and problem 6 had no example, now pitfall 11. The reviewer that ran the code
check validated all five example queries with `liquers-validate`.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
