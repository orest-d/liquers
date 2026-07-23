# expiration-safety Design Tracking

**Created:** 2026-07-22

**Status:** In Progress

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (approved)
- [x] Phase 3: Examples & Testing (approved)
- [x] Phase 4: Implementation Plan (approved; execution options offered, awaiting choice)
- [ ] Implementation Complete

## Notes

**`async-wasm-refactor` sync (post-Phase-4-approval, pre-execution):** an independent, parallel
effort merged into `main` while this branch was open, touching `liquers-core/src/assets.rs`
heavily (+1153/-279 lines). Re-audited all four phase documents against the merged code:
- Core design validated, unaffected in substance (all three WP-3 fixes still needed, same
  algorithm).
- One improvement applied: `get_any_status`/`to_override` changed from "required, implemented
  per-manager" to "one shared default trait method" — enabled by new primitives
  (`lookup_key_asset`/`get_envref`/`insert_key_asset`) the refactor introduced for its own second
  `AssetManager` implementor, `ImmediateAssetManager`. Strictly less implementation work.
- All file:line citations across Phase 1-4 refreshed against the post-merge file.
- `rust-best-practices` skill is now installed (was missing during original drafting).
- Phase 2, 3, and 4 documents updated in place with sync notes; this file's phase checkboxes
  corrected to reflect actual approval status (were never updated during the original run).

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
