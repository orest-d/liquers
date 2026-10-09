---
id: METADATA-ONLY-ENTRY-RELOAD
kind: design
title: Fast track recognizes a metadata-only entry and recomputes without a corruption report
status: complete
readiness: ready
autofix: eligible
area: [core/assets]
issues: [METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED]
created: 2026-10-06
---
# Fast track recognizes a metadata-only entry and recomputes without a corruption report

> **Acceptance scenarios not defined.** This design predates acceptance scenarios
> (`specs/DOCS_STRUCTURE_GUIDE.md` §5.2.1). Consider updating it: state the Phase 1 acceptance
> criteria as `AC-<n>` WHEN/THEN scenarios and cite each from the Phase 3 test that proves it. Remove
> this note when you do; until then `docs_index.py --check` counts this design in its warning.

Produced under [`guides/autonomous_bulk_design.md`](../../guides/autonomous_bulk_design.md):
the first four phases, reviewed without phase approval. Not an approval and not an implementation.

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution and Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan
- [x] Phase 5: Documentation (approved 2026-10-08, maintainer)

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

## Implementation status (2026-10-06)

Phase 4 steps 1–2 are implemented on branch `claude/p3-s-issue-designs-oc1dy6`, at the
maintainer's request ("the file stores behaviour should be fixed"): the `try_fast_track` change of
Phase 2 and `liquers-core/tests/metadata_only_entry_reload.rs`. One deviation from Phase 3: a
metadata-only key **without** a recipe answers like an absent key (an `Error` state "No recipe found"
from `get`, the existing behaviour for a key the store does not hold). It does not answer
`KeyNotFound` from `get`, as Phase 1 acceptance 3 assumed. The test pins the equivalence. The memory
store half depends on `memory-store-metadata-only-entry`. `status` stays as it is until a PR is linked.

