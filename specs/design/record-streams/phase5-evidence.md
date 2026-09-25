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
