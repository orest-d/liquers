# query-validation Design Tracking

**Created:** 2026-08-01

**Status:** Design complete (all 4 phases approved) — implementation not started

## Phase Status

- [x] Phase 1: High-Level Design — approved
- [x] Phase 2: Solution & Architecture — approved
- [x] Phase 3: Examples & Testing — approved
- [x] Phase 4: Implementation Plan — approved
- [ ] Implementation Complete

## Notes

Design approved 2026-08-02. Implementation follows `phase4-implementation.md`, steps 1–10
(including 8b). Landed ahead of implementation, as a separate concern: the `println!` →
`eprintln!` conversion across liquers-core and liquers-lib, plus the stdout rule in `CLAUDE.md`.

Reviews run during design: 2 (Phase 2), 3 (Phase 3), 5 (Phase 4, incl. a cross-phase Opus pass).
The final review found five blocking issues, the most serious being that `-R` as a short flag for
`--registry-file` would have silently swallowed resource queries, which begin `-R/`.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
