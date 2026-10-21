---
id: AGENT-MEMORY-DERIVED
kind: design
title: Agent memory — templates, indexes and context packs
workflow: liquers-project
status: draft
phase: high-level
area: [lib/commands, docs]
issues: [AGENT-MEMORY-HAS-NO-INDEXES-OR-CONTEXT-PACKS]
affects_docs: []
created: 2026-10-17
---
# Agent memory — templates, indexes and context packs

## Phases

- [Phase 1](./phase1-high-level-design.md) — written 2026-10-17
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

## Notes

Split from [`agent-memory-mvp`](../agent-memory-mvp/) on 2026-10-17, at the user's decision on its
Phase 1 decision 12 ("two designs"). This design takes execution-plan memory D, E and F: templates
in `memory_new` (the template step of mvp AC-4), `index.csv` (mvp AC-10), context packs (mvp AC-21)
and `index.md` (mvp AC-22), with mvp decisions 6, 7 and 10. It has no source issue of its own: its
source is `AGENT-MEMORY-SERVICE`, which stays with `agent-memory-mvp`.

Its Phase 1 is reviewed now but proceeds after `agent-memory-mvp`, since everything here runs on
that design's collections and MCP server, and after `liquers-mustache` is implemented.
