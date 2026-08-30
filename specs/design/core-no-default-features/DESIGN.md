---
id: CORE-NO-DEFAULT-FEATURES
kind: design
title: liquers-core no-default-features build decision
status: complete
area: [core/store, build]
issues: [CORE-NO-DEFAULT-FEATURES-BROKEN]
gh_pr: []
created: 2026-08-29
superseded_by:
---
# core-no-default-features Design Tracking

Simplified autonomous issue design for `CORE-NO-DEFAULT-FEATURES-BROKEN`.

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation
- [x] Phase 5: Documentation

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

## Outcome

Implemented on branch `core-no-default-features` on 2026-08-30.
`liquers-core` no longer treats `async_store` as a dependency or public-symbol gate: the async
store API is part of every core build, while the `async_store` feature remains as a no-op
compatibility selector. `cargo check -p liquers-core --no-default-features` and
`cargo test -p liquers-core --no-default-features` both pass.
