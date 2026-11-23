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
- [Review of the current architecture](./current-architecture-review.md) — the fundamental
  problems of the old UI (F1–F11), their three root causes, and what is kept.
- [Master design](./master-design.md) — the over-arching design the D-designs rely on: one
  runtime (session, lifecycle, messages, effects), native and portable views, clients from very
  thin to thick, protocol levels, decision register and a use-case walkthrough. Proposed
  2026-11-23, revised the same day after Orest's review.
- [Spike S1 stage 1 findings](../../archive/2026-11-23-ui-runner-spike.md) — the runner built under
  egui, a terminal, a retained host and a split pair; eight problems found.
- [Plan](./plan.md) — target architecture sketch, designs D1–D21 with dependencies, spikes S1–S4,
  milestones M0–M7 and Orest's decisions.

## Status

- [ ] Idea map reviewed by Orest
- [x] Plan questions answered by Orest, 2026-11-22 (plan §7)
- [x] M0 started: spike S2 done (PR #282), D1 `ui-core-crate` Phase 1 written and its questions answered (2026-11-23)
- [ ] Master design approved by Orest (its §28 questions 1–3; the two main points agreed in principle 2026-11-23)
