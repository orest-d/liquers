---
id: UI-REDESIGN
kind: design
title: UI redesign — separate UI crates, egui, web and ratatui frontends, monolithic and split client-server
workflow: liquers-project
status: draft
phase: high-level
area: [lib/ui, lib/egui, web, axum]
issues: [UI-NOT-SEPARATED-BY-CRATE-AND-TECHNIQUE]
affects_docs: []
created: 2026-11-22
---
# UI redesign — planning

**Planning stage, started 2026-11-22 from Orest's goal list.** This folder is an umbrella: it maps
the ideas and splits the effort into separate designs, each of which gets its own `liquers-project`
folder when its milestone starts. Nothing here is approved or implemented. The investigative design
[`ui-split-elements`](../ui-split-elements/) is its starting point for the model/view split and
the protocol.

## Documents

- [Idea map](./idea-map.md) — every goal (G1–G14) and brainstormed addition (B1–B16), what exists,
  and the coverage table of every open UI-related issue.
- [Plan](./plan.md) — target architecture sketch, designs D1–D21 with dependencies, spikes S1–S4,
  milestones M0–M7 and the questions for Orest.

## Status

- [ ] Idea map reviewed by Orest
- [ ] Plan reviewed by Orest (questions in plan §7 answered)
- [ ] M0 started: spikes S1/S2, and D1 `ui-core-crate` created with Phase 1
