# wp2-terminal-outcome Design Tracking

**Created:** 2026-07-16

**Status:** In Progress

## Phase Status

- [~] Phase 1: High-Level Design (drafted, awaiting user approval)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Notes

Central clarification: `State` (backed by `Metadata`, which already stores the typed error in
`error_data`) is the single source of truth for a terminal asset. `AssetOutcome`/`poll_outcome`
from WP-2 are dropped as redundant. Open decision: `get()` keeps `Result<State>` (recommended)
vs. bare `State` — see Phase 1 Open Question 1.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
