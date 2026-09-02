---
id: EVALUATE-PATH-CONSOLIDATION-PHASE1
kind: design
title: "Phase 1: High-level design — one evaluation path"
status: draft
phase: high-level
area: [core/assets, core/plan]
created: 2026-09-02
---
# Phase 1: High-Level Design — Evaluation Path Consolidation

## Feature Name

Unified evaluation path (`evaluate-path-consolidation`), resolving `CORE-EVALUATE-PATH-CONSOLIDATION`.

## Purpose

`AssetRef` carries two independent recipe-evaluation bodies — `evaluate_and_store` (via
`evaluate_recipe_outcome`) and `evaluate_immediately` — reached through four run harnesses
(`run`, `run_inline`, `run_immediately`, `run_immediately_inline`) and six manager entry points
(`get_asset`, `apply`, `apply_immediately`, twice over). They diverge on key-owner delegation,
recipe-provider resolution, the payload boundary check, status finalization, persistence and
dependency-manager registration, so a behaviour fixed on one stays broken on the other. Replace
them with **one** evaluation body parameterized by an explicit policy, leaving every entry point
a thin wrapper.

## Core Interactions

### Query System
None. Query syntax, parsing and `Key` encoding are untouched; only which code evaluates a plan changes.

### Store System
Persistence becomes one policy flag on the unified body (`save_to_store` unchanged), instead of a
property of which method was called. The `Context::apply`-writes-to-the-store surprise recorded in
`CONTEXT-APPLY-BARE-KEY-ILL-DEFINED` becomes expressible as policy rather than an accident of path.

### Command System
No new or changed commands. Command execution still runs through `Environment::apply_recipe` →
`interpreter::apply_plan`; the hook's contract is unchanged.

### Asset System
The centre of the change: `AssetRef::evaluate_*`, the run harnesses, `AssetManager`'s three
evaluation entry points and both managers (`DefaultAssetManager`, `ImmediateAssetManager`).
Dependency recording (`take_pending_dependencies` → metadata, `dm.track_asset`) moves into the
single body so it no longer depends on the entry point. `Context::apply` gains a defined,
documented dependency contract instead of silently recording nothing.

### Value Types
None.

### Web/API
No route or handler changes. `liquers-axum` and `liquers-web` call the same entry points; the
wasm/inline manager must keep its spawn-free property, which the policy split must preserve.

### UI
No widget changes. `liquers-lib/src/ui/runner.rs` is the main `apply_immediately`/payload consumer
and is the behavioural canary for the payload path.

## Crate Placement

`liquers-core` only — `assets.rs` (evaluation bodies, harnesses, managers) and `context.rs`
(`Context::apply`, `EnvRef::evaluate*`). No dependent crate gains a dependency; the goal is that
`liquers-lib`, `liquers-axum`, `liquers-web` and `liquers-py` compile unchanged.

## Documentation Intent

**Reference:** Extend `specs/reference/ASSET_LIFECYCLE.md` — it already *is* the evaluation-path
catalogue (§2 entry points, §3 Paths A–D, §6 `evaluate_and_store` vs `evaluate_immediately`
asymmetry). After this change it must describe one path plus a policy table, and §6's asymmetry
table and §7 Issues 3/5 become obsolete (§6 is already partly stale: it claims immediate evaluation
never collects dependencies, which HEAD does). A new reference file would split one subject in two.

**Guide:** Neither. Choosing an entry point is "what the system is", not a repeatable task; it
belongs in the reference. Reconsider in Phase 5 if the policy table proves to need worked examples.

**Other documents to create:** None.

**Specific documents to update:** `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`
(§"Public entry-point contract", §"Persistence contract", §"Conflicts and unresolved gaps");
`specs/reference/ASSETS.md` (§Overview, §AssetManager); `specs/reference/DEPENDENCIES_STATUS.md`
if the wait/`Dependencies` contract shifts; `specs/README.md` capability line for evaluation;
`specs/issues/CORE-EVALUATE-PATH-CONSOLIDATION.md` and `specs/issues/ASSETS-FIX1.md` (status and
`design:` links). The `//!` module documentation of `assets.rs` (its entry-point table) is code and
changes with the implementation.

Audience: framework developers and coding agents working in `liquers-core`. After this, "what does
evaluating a query do, and what differs between entry points" must be answerable from
`ASSET_LIFECYCLE.md` alone, without reading this design folder.

## Open Questions

1. What is the policy axis set? Candidates: persist, register in the dependency manager, admit a
   payload, delegate to the key owner, finalize status here vs in the harness, queued vs inline.
   Are these independent, or are only three or four combinations legal?
2. Should `Context::apply` record a dependency edge (making it uniform), or is "applies a query to
   a state, records nothing" a deliberate contract to be documented and kept? The issue's
   "dependency recording that does not depend on the entry point" implies the former; the rejected
   `CONTEXT-APPLY-BARE-KEY-ILL-DEFINED` shows `apply` is deliberately permissive.
3. Do ad-hoc/immediate assets get `dm.track_asset` registration? They are not persisted and not in
   any map, so registration may create entries nothing can invalidate.
4. Does the payload boundary (keys reject payload recipes) stay a key-path check, or become a
   policy precondition checked once in the unified body?
5. Can the four run harnesses collapse to two (spawn vs inline) with the body as a parameter, given
   `run_with_future` is `#[cfg(not(wasm32))]` and `run_with_future_inline` is not?
6. Is `INLINE-PATH-LACKS-EXECUTE-ONCE` (P2, accepted) a prerequisite, a co-delivery, or out of
   scope? Consolidation makes the shared claim primitive cheaper, but folding it in widens the
   blast radius. (Phase 2 known-issue preflight decides.)

## References

- `specs/issues/CORE-EVALUATE-PATH-CONSOLIDATION.md` (P1, L, accepted) — the issue being designed
- `specs/issues/ASSETS-FIX1.md` — same conclusion from the marker-inventory direction
- `specs/issues/INLINE-PATH-LACKS-EXECUTE-ONCE.md`, `specs/issues/CONTEXT-APPLY-BARE-KEY-ILL-DEFINED.md`
- `specs/reference/ASSET_LIFECYCLE.md` §2, §3, §6, §7 · `specs/reference/ASSETS.md`
- `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`
- `specs/design/keyed-recipe-ownership/`, `specs/design/dependency-scheduling/`,
  `specs/design/keyed-delegation-hand-off/` — prior work in the same code
