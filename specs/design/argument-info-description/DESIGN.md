---
id: ARGUMENT-INFO-DESCRIPTION
kind: design
title: Per-argument description in ArgumentInfo and register_command!
status: superseded
area: [core/commands, macro]
issues: [ARGUMENT-INFO-HAS-NO-DESCRIPTION]
created: 2026-10-06
superseded_by: command-metadata-descriptions-and-hints
---
# Per-argument description in ArgumentInfo and register_command!

> **Superseded on 2026-10-08.** Merged by maintainer decision (backlog compaction D1) with `command-metadata-command-hints` into
> [`command-metadata-descriptions-and-hints`](../command-metadata-descriptions-and-hints/), which now owns
> `ARGUMENT-INFO-HAS-NO-DESCRIPTION`. The `description` field, macro option and tests moved there
> unchanged; this folder is kept for its reasoning.

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

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
