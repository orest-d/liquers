---
id: STORE-ADAPTERS
kind: design
title: Modular store configuration — metadata layouts and store adapters
workflow: liquers-project
status: complete
readiness: ready
autofix: not-eligible
area: [core/store, store/config, store/backends, web, docs]
issues: [STORE-METADATA-LAYOUT-HARDCODED-PER-STORE, STORE-NO-READ-ONLY-ADAPTER, STORE-METADATA-HANDLING-NOT-COMPOSABLE]
merged: 2026-10-13
affects_docs: [specs/reference/STORE_SEMANTICS.md, specs/reference/STORE_CONFIG_FSD.md, specs/guides/STORE_IMPLEMENTATION_GUIDE.md, specs/guides/STORE_FACTORY_GUIDE.md, specs/reference/ASSETS.md, specs/reference/PROJECT_OVERVIEW.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, specs/reference/REALMS.md, specs/guides/STORE_CONFIG_GUIDE.md]
created: 2026-10-13
---
# Modular store configuration — metadata layouts and store adapters

## Phases

- [Phase 1](./phase1-high-level-design.md) — written 2026-10-13, revised 2026-10-15, **approved 2026-10-15**
- [Phase 2](./phase2-architecture.md) — written 2026-10-15, **approved 2026-10-15** (`proceed all`)
- [Phase 3](./phase3-examples.md) — written 2026-10-15, reviewed (pre-approved)
- [Phase 4](./phase4-implementation.md) — written 2026-10-15, reviewed; approved 2026-10-15 (`proceed all`); implemented (commits a8e457f–9022a38)
- [Phase 5](./phase5-documentation.md) — written 2026-10-15, revised 2026-10-16 after the issue review, **approved 2026-10-16**; design complete

## Notes

Created 2026-10-13 at the user's request, merging three issues into one design:

| Source | Stage it covers |
|---|---|
| `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` (leading: the oldest of three P2 sources) | *store* — where metadata is persisted, and which keys that reserves |
| `STORE-NO-READ-ONLY-ADAPTER` | *policy* — reads that do not write, writes refused |
| `STORE-METADATA-HANDLING-NOT-COMPOSABLE` | the whole pipeline — *create*, *enhance*, *store*, *policy* — and its configuration |

The merge makes the scope `XL` from two `L` sources and an `M` (overlap exclusion E2 would normally
stop that); the leading source keeps `L` until the split decision below is taken. The merge was the
user's decision, and Phase 1 open question 5 keeps a split available at the
Phase 2 gate. The user's earlier decision that the read-only adapter is fixed first
(`agent-memory-mvp` Phase 1 decision 2) becomes this design's first milestone (open question 3).

**2026-10-15 revision.** The user made three requirements explicit before approving Phase 1:
read-only for any store from its configuration; a storage-model interface (per-entry key templates,
grouped records in a folder file or a database, central storage on a store or router) of which the
per-entry templates and the grouped `__metadata__.json` are built; and composable metadata handlers
for four stages (create defaults, stored or not; validate; fill on write; fill on read). See Phase 1
`## Scope Changes`; AC-11 to AC-16 were added.

**2026-10-15, second revision.** A member's own metadata under a central router is a backup, not an
error; write strategies (direct, atomic, transactional with recovery) are the fourth concern
(AC-17 to AC-20); the user decided to keep everything in one design.

**2026-10-15, Phase 2 review.** Realm name `store_config` confirmed. An omitted realm is filled into
the query before planning (no `PlanBuilder` change); every `store_config` command is specified
(Phase 2 §4 catalogue); new documents `REALMS.md` and `STORE_CONFIG_GUIDE.md` are planned.

**2026-10-15, Phase 2 review.** Presets removed at the user's request; each `metadata` field has one
fixed default instead (AC-22 rewritten, Phase 1 Q13 revised, Phase 2 §7 defaults table).

**2026-10-15, Phase 2 review.** Language integration answered (Phase 2 §Language Integration): a
language store takes `read_only` and `metadata` like any store, through optional data-only methods;
compiled chains stay in Rust; language-defined handlers filed as
`LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`. `LANGUAGE-INTEGRATION_GUIDE.md` added to the
documents to update.

**2026-10-15, Phase 2 review.** The language integration guide's update is planned in Phase 2
§Documentation Architecture: the four questions become "design must answer" questions for every
integration, plus `STORE14`–`STORE18`; §Language Integration is reframed as the facts they rely on.

**Pre-approved after Phase 2 on 2026-10-15** (`proceed all`). Phases 3 and 4, the implementation and
Phase 5 run without gates; reviews still run, and decisions only the user can make go to the decision
log in Phase 1 `## Design Readiness`, presented before implementation starts.

**2026-10-15, Phases 3–4 reviewed** (two reviewers plus a consistency pass). Fixed: the planner's
realm-header defect (new step 0), `ReadOnlyStore` over a pipeline (the pipeline reports no data
access), step order, test placement under `store-conformance`, registry export per step, the
asset-manager hook (step 9), `build_store` variants and bare-router refusal, and the AC coverage
gaps. Decision log: Phase 1 `## Design Readiness`.

**2026-10-15, pre-implementation stop passed.** The user decided the five proposals (Phase 1
`## Design Readiness`): the planner fix is part of this design; `defaults` keeps today's `updated`
and warning; command overviews omit realm `store_config`; `front_matter` stays in `liquers-lib`;
the JavaScript step stays last.
