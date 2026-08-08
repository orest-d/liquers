---
id: EXPIRATION-SAFETY
kind: design
title: Timing and race safety in asset expiration
status: complete
area: [core/assets]
gh_pr: [11]
issues: []
created: 2026-03-02
superseded_by:
---
# expiration-safety Design Tracking

**Created:** 2026-07-22


## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (approved)
- [x] Phase 3: Examples & Testing (approved)
- [x] Phase 4: Implementation Plan (approved)
- [x] Implementation Complete

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

**Close-out (Phase 4 approved; implementation verified green):**
- Implementation landed on this branch (commits `37fae94` Steps 1–3, `ec4a8d3` Steps 4–5),
  including one fix for a gap the four-phase design missed: `AssetRef::expire()` now rewrites the
  *persisted* store metadata to `Expired`, not just the in-memory status (otherwise an evicted
  keyed asset could fast-track stale bytes back in).
- Verification: `liquers-core` — 326 unit tests, **27/27 WP-3 integration tests (0 ignored)**,
  6 `manager_parametric` (both `DefaultAssetManager` and `ImmediateAssetManager`). `cargo check -p
  liquers-py` clean. (The `NotPersisted` retry-branch test was un-ignored with a self-contained
  `RecipesOnlyFailingSetStore` mock — serves `recipes.yaml`, fails all `set`.)
  Full-workspace run initially hit an `ld` SIGBUS linking large egui UI targets at 99% disk; after
  freeing disk, the affected `liquers-lib` targets (`query_console_integration`,
  `ui_shortcuts_integration`) link and pass (6 + 7 tests) — the failure was environmental, not a
  WP-3 defect (those targets reference no WP-3 API; zero `error[E…]` anywhere).
- `specs/archive/EXPIRATION-SAFETY.md` and its original implementation plan marked **Closed**
  (Phase 4 Step 7).

**Remaining (non-blocking):** exposing `get_any_status`/`to_override` on the web API is confirmed
wanted and is now tracked as `EXPIRATION-RECOVERY-WEB-API` in `specs/archive/2026-08-08-issues.md` (a follow-up in
`liquers-axum`, out of this core WP's scope). The previously-`#[ignore]`'d retry-branch test is now
implemented and passing — no tests are skipped.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
