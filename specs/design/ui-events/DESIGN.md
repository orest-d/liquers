---
id: UI-EVENTS
kind: design
title: UI event delivery to widgets
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/ui]
gh_pr: []
issues: [WEBUI-SUBMIT-QUERY-STATE-NOT-PRESERVED, WEBUI-QUERY-CONSOLE-ENTER-KEY-SUBMIT, WEBUI-MENU-SHORTCUTS-IGNORED]
merged: 2026-10-19
affects_docs: [UI_INTERFACE_FSD]
created: 2026-07-25
superseded_by:
---
# ui-events Design Tracking

**Created:** 2026-07-25. **Resumed:** 2026-10-19 under `specs/guides/autonomous_bulk_design.md`
§11 (bulk design): Phase 1 reviewed and trimmed, acceptance scenarios added, Phases 2-4 written. Not
approved; nothing implemented.

## Scope

How user interaction reaches a `UIElement` in any backend: a shared event vocabulary delivered
through `update()`, declared in `render_web` markup, handled natively by the element or as a
`UiAction` query. Split out of `specs/design/webui-fixes/` (complete), which kept rendering and
invalidation. This first increment fixes the three browser defects that motivated it:

| ID | Issue | Symptom |
|----|-------|---------|
| W2 | WEBUI-SUBMIT-QUERY-STATE-NOT-PRESERVED (leading) | the submitted query never reaches the console; input, history and refresh keep the old one |
| W1 | WEBUI-QUERY-CONSOLE-ENTER-KEY-SUBMIT | Enter in the console input does nothing |
| W5 | WEBUI-MENU-SHORTCUTS-IGNORED (filed 2026-10-19) | `UISpec` menu shortcuts do nothing in the browser |

The sources share one design because they strongly overlap (`overlap.md` T2, T4): each fix depends
on the user-to-element contract this design defines, and a maintainer moved them here from
`webui-fixes`. Egui adoption, per-keystroke sync, an imperative mount hook and event propagation
are non-goals of this increment.

## Phase Status

- [x] Phase 1: High-Level Design — reviewed 2026-10-19; scenarios AC-1 to AC-7
- [x] Phase 2: Solution & Architecture — drafted 2026-10-19
- [x] Phase 3: Examples & Testing — drafted 2026-10-19
- [x] Phase 4: Implementation Plan — drafted 2026-10-19, 8 steps
- [ ] Implementation Complete

**Reviewed 2026-10-30** against main: every symbol named in Phases 2-4 exists as described.
Changes from that review: types renamed `UserEvent` → `UIEvent` (Orest), platform-independence
principle added to Phase 1, `value-accessor` marked abandoned. `ui-asset-snapshot-boundary` stays a
separate design (independent contracts; a small textual overlap in `runner.rs` and
`AssetViewElement::update`).

Readiness `needs-decision`: the public vocabulary shape (Phase 1 leading question) awaits
confirmation; the recommended answer is assumed throughout.

## Notes

- Decided during the `webui-fixes` review and kept: delivery through `update()` with a shared
  vocabulary (`Custom` stays as escape hatch); declarative markup as the way to declare interest;
  field values carried with the event; accelerators never override native HTML key behaviour.
- `value-accessor` is the binding layer beneath this naming layer and is not a prerequisite:
  values travel as `Value`, and async writes go through the runner.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [webui-fixes](../webui-fixes/DESIGN.md) — the rendering/invalidation half
- [value-accessor](../value-accessor/phase1-high-level-design.md) — the binding layer
