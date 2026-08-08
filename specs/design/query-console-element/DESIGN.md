---
id: QUERY-CONSOLE-ELEMENT
kind: design
title: Query console UI element
status: complete
area: [lib/ui]
gh_pr: []
issues: []
created: 2026-03-02
superseded_by:
---
# query-console-element Design Tracking

**Created:** 2026-02-14


## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (approved)
- [x] Phase 3: Examples & Testing (approved)
- [x] Phase 4: Implementation Plan (approved)
- [ ] Implementation Complete

## Notes

- Phase 1 approved in earlier session
- Phase 2 rewritten to use Option A: AppRunner-monitored AssetSnapshot pattern
  - Widget is passive (no notification_rx, no poll_state_fn, no background tasks)
  - AppRunner monitors assets via `monitoring: HashMap<UIHandle, MonitoredAsset<E>>`
  - Pushes `UpdateMessage::AssetUpdate(AssetSnapshot)` on each change
  - Auto-stop monitoring when element removed from AppState
- Phase 3 was drafted earlier but is OUT OF DATE with the final Phase 2 design; must be updated after Phase 2 approval

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
