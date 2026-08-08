# query-link-parser Design Tracking

**Created:** 2026-08-04

**Status:** Complete — designed, implemented, tested and documented

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (approved)
- [x] Phase 3: Examples & Testing (approved)
- [x] Phase 4: Implementation Plan (approved)
- [x] Implementation Complete (2026-08-06)

## Notes

All four phases approved. Design is confined to `liquers-core` (`parse.rs`, one doc
comment in `query.rs`) plus documentation in `specs/`.

Key decisions, for anyone picking this up cold:

- **D+ / in-band parsing** — the embedded query is parsed on the original span, never
  sliced. Nesting, the `~~` escape and absolute inner positions all fall out for free.
- **D3** — the resource/transform shorthand is *rejected* inside a link, not
  reinterpreted. The detector fires exactly when the in-link reading would differ from
  the top-level one.
- **D4a** — `link_query` cannot fail, so a malformed body always surfaces at the
  terminator. Read this before touching the error paths.
- **D5** — links are the grammar's first recursive construct. The bound was designed as
  `MAX_LINK_MARKERS = 64` and **implementation proved that wrong**: parsing is exponential
  in nesting depth, so 64 never finishes and the guard was itself a DoS vector. Now two
  bounds: `MAX_LINK_DEPTH = 8` and `MAX_LINK_MARKERS = 64`. See Phase 4 → Implementation
  Findings, and follow-up `QUERY-LINK-EXPONENTIAL-BACKTRACKING` in `specs/archive/2026-08-08-issues.md`.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
