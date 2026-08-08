# ui-events Design Tracking

**Created:** 2026-07-25

**Status:** Phase 1 drafted — awaiting review

## Scope

How user interaction reaches a `UIElement` in any backend: the event vocabulary, how an element
declares what it reacts to, and how a handler is expressed (native where that is right,
query-based where the action should be data).

Split out of `specs/webui-fixes/`, which keeps the rendering/invalidation half. Acceptance criteria
are the interaction defects in `specs/archive/2026-08-08-issues.md`: W1 (Enter does not submit), W2 (submitted query
never reaches the element), W5 (accelerators are egui-only).

## Phase Status

- [ ] Phase 1: High-Level Design — drafted, awaiting review
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Notes

- Modelled on the two backends in hand: HTML (typed events, delegation, per-control default
  actions) and egui (`Response` predicates, explicit accelerator consumption).
- Decided during the `webui-fixes` review and carried in: delivery through `update()` with a shared
  vocabulary (`Custom` kept as escape hatch); declarative markup as the normal way to declare
  interest, with an imperative per-element mount hook as an opt-in extension; field values synced
  on dispatch with per-keystroke as a per-field opt-in; both value-only and values+action message
  shapes; accelerators must not override native HTML key behaviour.
- `value-accessor` is the binding layer under this feature's naming layer, and is **not** a
  prerequisite — two composition rules keep the door open (values travel as `Value`; async writes
  go through the runner).

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [webui-fixes](../webui-fixes/DESIGN.md) — the rendering/invalidation half
- [value-accessor](../value-accessor/phase1-high-level-design.md) — the binding layer
