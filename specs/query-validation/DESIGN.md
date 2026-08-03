# query-validation Design Tracking

**Created:** 2026-08-01

**Status:** Complete — designed, implemented, tested

## Phase Status

- [x] Phase 1: High-Level Design — approved
- [x] Phase 2: Solution & Architecture — approved
- [x] Phase 3: Examples & Testing — approved
- [x] Phase 4: Implementation Plan — approved
- [x] Implementation Complete

## Notes

Design approved and implemented 2026-08-02, following `phase4-implementation.md` steps 1–10
(including 8b). See that document's "Implementation Notes" for the four places reality differed
from the plan. Landed ahead of implementation, as a separate concern: the `println!` →
`eprintln!` conversion across liquers-core and liquers-lib, plus the stdout rule in `CLAUDE.md`.

Shipped: `liquers_core::validate` (+ 41 tests), the `liquers-validate` and
`export-command-registry` binaries behind a non-default `cli` feature, and
`specs/command_registry.yaml` with a freshness test.

Reviews run during design: 2 (Phase 2), 3 (Phase 3), 5 (Phase 4, incl. a cross-phase Opus pass).
The final review found five blocking issues, the most serious being that `-R` as a short flag for
`--registry-file` would have silently swallowed resource queries, which begin `-R/`.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
