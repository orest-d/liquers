---
id: STORE-CONFIG-URI
kind: design
title: Configuring a store from a URI
status: in_review
phase: architecture
area: [store/config, store/backends, core/store]
gh_pr: []
issues: [STORE-CONFIG-FROM-URI]
created: 2026-08-29
superseded_by:
---
# Configuring a store from a URI

Design tracking for [`issues/STORE-CONFIG-FROM-URI.md`](../../issues/STORE-CONFIG-FROM-URI.md),
prepared under [`guides/autonomous_issue_fixing.md`](../../guides/autonomous_issue_fixing.md). No
`workflow:` marker: this is a simplified transitional design whose required phases are the two
written here plus whatever the approval gate authorizes. It is **not** opted into the
`liquers-project` artifact and approval contract.

## Phase status

- [x] Phase 1: High-level design — [`phase1-high-level-design.md`](./phase1-high-level-design.md)
- [x] Phase 2: Solution and architecture — [`phase2-architecture.md`](./phase2-architecture.md)
- [ ] Approval gate (§5 of the autonomous procedure) — **awaiting a decision**
- [ ] Phase 3: Examples, reproduction and tests
- [ ] Phase 4: Implementation plan and execution
- [ ] Phase 5: Documentation

## Why this folder exists, and its second purpose

The issue asks for `uri: s3://bucket/data?region=eu-central-1` as an alternative to `type:` plus a
`config:` map. That is the stated deliverable.

**The second purpose is a compatibility audit**, and it is why this design was written now rather
than when the feature is built: [`design/store-factories-in-core/`](../store-factories-in-core/) is
at its Phase 3 gate and is about to fix the shape of `StoreFactory`, `StoreTypeInfo` and
`StoreRouterBuilder`. If URI support would later demand a change to any of them, it is far cheaper
to know that before the factory design is approved than after it ships. So this design's Phase 2
carries an explicit §"What this requires of `store-factories-in-core`" and reports gaps as findings
against **that** design, not this one.

**Result of the audit: no conflict, and two additive fields to anticipate.** Both are
`#[serde(default)]` fields on structs Liquers owns — `StoreConfig::uri` and
`StoreTypeInfo::uri_schemes` — so neither is a breaking change and neither needs to land now. The
`StoreFactory` trait needs **no** signature change, because the URI can travel inside the
`StoreConfig` the factory already receives. Details and the one genuine trap are in Phase 2.

## Relationship to `store-factories-in-core`

This design **depends on** that one and does not modify it. It assumes first-wins chaining, a
factory that describes the store types it claims, and a builder that holds one factory. Those are
exactly the properties that make the unified-URI direction safe: scheme resolution goes through the
Liquers chain rather than OpenDAL's registry, so a browser build keeps its `fetch`-backed `http`
without any special case.

Findings this audit produced for that design are recorded in its
[`phase3-examples.md`](../store-factories-in-core/phase3-examples.md) §"S3 two ways" and in its
`DESIGN.md`; nothing in its phase documents, front-matter or workflow marker is changed by this
folder.

## Notes

- **Direction: unified URI** (maintainer decision). The scheme is extracted and interpreted as a
  store type through an explicit mapping, then routed through the normal chain. The rejected
  alternative — naming an interpreter per entry, `schemes: opendal` beside `uri: "s3://…"` — is
  safer and simpler but messier and less ergonomic.
- **The namespaces stay distinct.** A URI scheme is OpenDAL's vocabulary; a store type is ours. A
  mapping remains even where the two names coincide.
- **Backwards compatibility is not a constraint**, so harmonization is allowed where it reduces
  surprise. Phase 2 recommends *against* one specific harmonization — see the `fs://` trap.
- Nothing here is implemented.
