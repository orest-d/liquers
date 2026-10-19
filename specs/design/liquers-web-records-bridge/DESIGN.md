---
id: LIQUERS-WEB-RECORDS-BRIDGE
kind: design
title: Complete the liquers-web records bridge — sources, streaming and the way back
status: complete
readiness: ready
autofix: not-eligible
area: [web, records]
issues: [WEB-RECORDS-BRIDGE-ONE-WAY-AND-SOURCE-UNMAPPED]
affects_docs: [specs/guides/LANGUAGE-INTEGRATION_GUIDE.md]
created: 2026-10-17
gh_pr: [157]
---
# Complete the liquers-web records bridge — sources, streaming and the way back

Bulk design (`specs/guides/autonomous_bulk_design.md`), implemented 2026-10-18. Readiness, decisions
and consolidated findings are in Phase 1.

## Implementation notes

- `stream` and `materialize` resolve the environment through `environment::shared_env()`, not
  `with_global()`: `with_global()` fails until the first evaluation has shared the pending
  environment, which a page that only registers commands and then streams a source never does.
- Step 1 (refuse a `RecordSource`) was folded into Step 4: the arm went straight from `Ok(None)`
  to the handle.
- `LiquersRecordBatch::from_view` registers the original `Arc<dyn RecordView>`, not the
  materialized `RecordBatch`, so a view crossing back keeps its own type, chunk id and origin.
- Review follow-ups (PR #157): a handle is recognised by the marker being a getter on its
  prototype, so data copying the marker is not taken for a handle; in-memory views list as
  `kind: "memory"` chunks, since their placeholder ids name no query. A source from an explicit
  `Environment` still resolves through the singleton: `WEB-RECORD-SOURCE-RESOLVES-THROUGH-SINGLETON`.
- Quickstart `.wasm` size, measured with and without the `ns-rec` registration line: release
  13,435,801 → 16,533,123 bytes (+3.1 MB, +23%); debug 37.7 MB → 46.6 MB. Registration stays as
  decided; a page that needs a smaller build would need a feature to leave it out.

## Phases

- [Phase 1: High-Level Design](./phase1-high-level-design.md)
- [Phase 2: Architecture](./phase2-architecture.md)
- [Phase 3: Examples and Tests](./phase3-examples.md)
- [Phase 4: Implementation Plan](./phase4-implementation.md)
