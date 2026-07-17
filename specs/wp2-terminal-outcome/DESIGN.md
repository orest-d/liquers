# wp2-terminal-outcome Design Tracking

**Created:** 2026-07-16

**Status:** In Progress

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (approved)
- [~] Phase 3: Examples & Testing (drafted, conceptual; awaiting user approval)
- [ ] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Notes

Central clarification: `State` (backed by `Metadata`, which already stores the typed error in
`error_data`) is the single source of truth for a terminal asset. `AssetOutcome`/`poll_outcome`
from WP-2 are dropped as redundant. Resolved contract: a **single** `get() -> Result<State,
Error>` where `Err` = delivery failure only and `Ok` = rich terminal `State` (value or error);
ergonomics via `State::value_state()` + error-checked value extraction and private `State.data`.
Two Phase-2 audits carried in: (1) `Err`-vs-error-`State` reclassification, (2) `get()` caller
migration to `.value_state()?`.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
