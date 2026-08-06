# query-link-parser Design Tracking

**Created:** 2026-08-04

**Status:** Design complete — ready to implement

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (approved)
- [x] Phase 3: Examples & Testing (approved)
- [x] Phase 4: Implementation Plan (approved)
- [ ] Implementation Complete

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
- **D5** — links are the grammar's first recursive construct; `MAX_LINK_MARKERS = 64` is
  a hypothesis until test C8b passes.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
