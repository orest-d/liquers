---
id: RECORD-STREAMS-PHASE5-EVIDENCE
kind: analysis
title: Phase 5 evidence log — record streams
workflow: liquers-project
status: draft
area: [lib/value, core/assets, core/recipes]
created: 2026-09-25
---
# Phase 5 evidence log — Record streams

Kept while Phase 4 is executed ([plan](./phase4-implementation.md) §"Phase 5 evidence capture"),
one entry per step: requested versus implemented scope, tests corrected and why, issues filed,
surprises, measurements. Phase 5 synthesizes from this log rather than rediscovering it.

| Step | Commit | Scope as planned? | Tests | Corrections, issues, surprises |
|---|---|---|---|---|
| 0.1 | (this commit) | Yes, plus the `f32`/`u32`/`u8` `TryFrom` impls through checked narrowing (out of range → error, never truncation) | 14 new in `extended.rs`; 17 pass in the module | **Toolchain:** the container had rustc 1.94.1, and default-feature `liquers-lib` builds refuse it (`egui` 0.36 and `sysinfo` 0.39 need 1.95 — `BUILD-SYSINFO-REQUIRES-NEWER-RUSTC`). Updated to stable 1.98.1, as CI uses; `cargo clean` freed 4 GB of stale artifacts. **Plan wording:** issues close as `status: closed`, not `complete` (a design-only status) — corrected in the plan. Pre-existing `_ =>` arms on `try_into_key`/`try_into_bytes` left alone (out of scope) |
| 0.2 | (this commit) | Yes, and wider: JSON reads consult the type identifier (structured variants as their own types; `Array`/`Object`/`Bytes`/`Query`/`Key` in the tagged form `as_bytes` writes, else plain JSON) | 20 per-format tests from the agent, plus the TypeInfo-driven `every_declared_format_round_trips_or_is_recorded_as_unwritable` written by the orchestrator | **Test corrected:** the agent's array/object round trips built their bytes by hand rather than through `as_bytes`, so they could not see that the writer emits a tagged form the plain reader does not accept; the TypeInfo-driven test found it. **Issue filed:** `SIMPLE-VALUE-WRITES-FEWER-FORMATS-THAN-DECLARED` (42 declared pairs the writer refuses, recorded in the test). **Pre-existing red:** `registry_export` failed on a clean tree — every argument's default `TextField` width had changed 20 → 40 without a re-export; the registry was regenerated with a changelog line. `SimpleValue` gains `PartialEq` (for assertions) |
| 1.1 | (this commit) | Yes | Phase 3 §4.2's 10 tests, in `recipes.rs`, unchanged | None. `liquers-py`, `liquers-store`, `liquers-axum` compile unchanged, as decision 6 expected |
| 1.2 | (this commit) | Yes. Seeding differs from the plan's wording: the flags are set on the ad-hoc `key.into()` recipe **before** the asset is built, so `Recipe::get_asset_info` (Step 1.1) carries them into the first `MetadataRecord` — no separate metadata patch | `stored_cached_flags.rs`: 6 scenarios × both managers = 12 tests. Efficacy checked: with `assets.rs` reverted, the 6 that exercise the fix fail | **Issue filed:** `DEFAULT-ASSET-MANAGER-RECIPE-OPT-SKIPS-PAYLOAD-CHECK` — the two managers reject a payload-requiring keyed recipe at different points (pre-existing). `cached: false` skips registration outright (`entry_async`/`map.insert`) rather than registering and removing |
