---
id: QUERY-EDITOR-ERGONOMICS
kind: design
title: Query editor ergonomics - action cards, Simple and Advanced modes, icons, shortcuts and a theme proof of concept
workflow: liquers-project
status: in_review
phase: high-level
priority: P2
complexity: L
area: [lib/ui, lib/egui]
issues: [QUERY-CONSOLE-HAS-NO-QUERY-EDITOR-ON-DEMAND]
affects_docs: [specs/reference/UI_INTERFACE_FSD.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md]
created: 2026-11-21
---
# Query editor ergonomics

The query, action, resource and key editors of `query-pipeline-editor` redrawn as framed action
cards that read as human steps, with a Simple mode (the last action) and an Advanced mode (every
segment, as a tree), a two-column form, preset chips, command icons (Phosphor), keyboard shortcuts,
a jump from the query text to the matching field, and a minimal dark/light theme with two
densities as a proof of concept for a later theme design.

Started from Orest's request of 2026-11-21; the reference design and mockups were discussed and
revised with him the same day (label-first headers, preset chips, Phosphor, one design with the
theme, a compact variant, icon definitions recorded for later). Phase 1 written on 2026-11-21.

## Phases

- [Phase 1](./phase1-high-level-design.md): AC-1 to AC-13
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

## Supporting documents

- [Reference design](./reference-design.md): the detailed design, keyboard shortcut table, and the
  basis for later icon, palette and TUI designs
- [Mockups](./mockups/): eleven images, current state and reference design
