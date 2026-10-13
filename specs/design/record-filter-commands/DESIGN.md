---
id: RECORD-FILTER-COMMANDS
kind: design
title: Record and polars command interoperability (ns-rec filters, sort, shared vocabulary)
workflow: liquers-project
status: in_review
phase: architecture
readiness: ready
autofix: not-eligible
area: [records, lib/commands, lib/polars]
issues: [RECORD-AND-POLARS-COMMANDS-NOT-INTEROPERABLE, RECORD-SOURCE-WRAPPERS-UNSPECIFIED]
merged: 2026-10-13
affects_docs: [reference/RECORD_STREAMS.md, reference/POLARS_COMMAND_LIBRARY.md, guides/RECORD_STREAM_GUIDE.md, reference/SEARCH.md, reference/COMMAND_ALIASES.md]
created: 2026-10-11
---
# Record and polars command interoperability

Split out of [`store-and-asset-search`](../store-and-asset-search/) on 2026-10-11, where the
`ns-rec` filters were AC-27. On 2026-10-13 the design took the whole of its source issue
`RECORD-AND-POLARS-COMMANDS-NOT-INTEROPERABLE` (size L) and was converted from the compact form to
the full form. The compact draft's Phase 1 is now AC-1 to AC-8 of
[Phase 1](./phase1-high-level-design.md). Its Phase 2 notes are kept, unreviewed, at the top of
[Phase 2](./phase2-architecture.md).

Supporting notes: [`conventions.md`](./conventions.md) (polars-first naming, maintainer decision
2026-10-13), [`lazyframe-investigation.md`](./lazyframe-investigation.md) (AC-16), [`schema-example.md`](./schema-example.md) (AC-14).

## Phase Status

| Phase | State |
|---|---|
| 1 High-level | **Approved 2026-10-13** (Revision 4) |
| 2 Architecture | Written and reviewed 2026-10-13; **awaiting `proceed` / `proceed all`** |
| 3 Examples | Not started |
| 4 Implementation | Not started |
| 5 Documentation | Not started |

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
