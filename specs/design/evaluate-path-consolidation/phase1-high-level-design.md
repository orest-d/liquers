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

## Resolved Before Phase 2

Discussion on 2026-09-02/03 rejected the "policy axes" framing of the original questions. Most of
what differs between entry points is not policy at all:

| Concern | Verdict | Where it lands |
|---|---|---|
| Dependency recording | always done | invariant of the one body |
| Status finalization | must be correct and correctly ordered — before use, before storing | invariant, one authority |
| Key-owner delegation | an algorithm preventing one key being evaluated twice | invariant, always applied |
| Payload | a *requirement of the plan*: fail iff a command needs one and none was supplied | uniform precondition |
| Persistence | not switchable — it follows from reproducibility | derived from the asset |
| Queued vs inline | genuine policy, already owned by the asset manager | manager policy |
| Queue capacity, routing, eviction | genuine policy | manager policy, **out of scope** here |

**The reproducibility predicate.** An asset is *reproducible* when its value is determined by its
identity alone: no payload, no supplied initial state, not volatile. One predicate governs three
things the code currently decides in three unrelated places — whether the asset may be shared
through the key/query maps, whether its stored form may be loaded back, and whether it may *be*
someone's dependency. `Context::apply` records no dependency edge because the applied asset is not
reproducible, exactly as a payload asset "may *have* dependencies, it just may not *be* one"
(`Context::schedule_payload_dependency_asset`) — not because `apply` is a special entry point.

HEAD already believes this in three different spellings: `Recipe::key()` returns `None` when
`has_arguments()`; `DependencyManager::track_asset` refuses `Volatile` and everything without a
`bound_owner_key`; `Context` excludes payload assets from the graph. Consolidation states the rule
once instead of rediscovering it per call site.

**Three words where the code has one:**

- **stored** — bytes written under a key; a user may open the file.
- **loadable** (persistent) — the stored form may be read back and reused by the system as this
  asset's value; what `try_fast_track` accepts (`Ready`/`Source`/`Override`, non-stale deps).
- **owned** — this asset is the registered owner of that key, hence the one allowed to write there.

A volatile keyed asset is stored and owned but not loadable — which is what HEAD does already
(it writes with status `Volatile`, which fast-track refuses).

**Derived persistence rule to be specified in Phase 2:** write iff the asset owns a store target
(`key()` or `store_to_key()`, and not delegating — the owner writes, not the delegator); write a
loadable status iff reproducible, otherwise a status fast-track refuses. No flag, no parameter.

**Consequent behaviour change, intentional:** queued `apply` stops writing to the store. Today it
runs `evaluate_and_store` → `save_to_store`, targeting `recipe.key().or(store_to_key())` — the
durable half of `CONTEXT-APPLY-BARE-KEY-ILL-DEFINED`. An ad-hoc asset with a supplied state owns
nothing, so it writes nothing. Phase 2 must confirm no in-tree caller depends on that write.

**Dependency-manager registration** needs no per-path decision: `track_asset` is already
self-limiting on status and ownership, so calling it unconditionally in the one body is safe and
erases the asymmetry.

Consequently the unified body takes **only the asset**. Entry points become constructors that
differ in the asset they build — recipe, initial state, payload, tracked vs ad hoc — never in how
it is evaluated, and `AssetData` already carries those facts (`initial_state`, `is_volatile`,
`save_in_background`, `payload_path`, `expiration_time`).

## Open Questions

1. **Store-target ownership for a non-key query asset carrying a `filename`.** `store_to_key()`
   resolves to `cwd/filename` and HEAD writes there. Such an asset is reproducible, so the value is
   legitimate — but the key belongs to whatever the recipe provider resolves at that path. Owning
   it lets a query plant a value under a key it does not define; not owning it removes `filename:`
   as a persistence mechanism for query assets.
2. **Harness collapse.** Can the four run harnesses become two (spawn vs inline) with the
   evaluation body as a parameter, given `run_with_future` is `#[cfg(not(wasm32))]` and
   `run_with_future_inline` is not? With one body, `run`/`run_immediately` and
   `run_inline`/`run_immediately_inline` differ only in the payload argument.
3. **`INLINE-PATH-LACKS-EXECUTE-ONCE` (P2, accepted): prerequisite, co-delivery, or out of scope?**
   Consolidation makes a shared claim primitive cheaper; folding it in widens the blast radius.
   The Phase 2 known-issue preflight decides.

## References

- `specs/issues/CORE-EVALUATE-PATH-CONSOLIDATION.md` (P1, L, accepted) — the issue being designed
- `specs/issues/ASSETS-FIX1.md` — same conclusion from the marker-inventory direction
- `specs/issues/INLINE-PATH-LACKS-EXECUTE-ONCE.md`, `specs/issues/CONTEXT-APPLY-BARE-KEY-ILL-DEFINED.md`
- `specs/reference/ASSET_LIFECYCLE.md` §2, §3, §6, §7 · `specs/reference/ASSETS.md`
- `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`
- `specs/design/keyed-recipe-ownership/`, `specs/design/dependency-scheduling/`,
  `specs/design/keyed-delegation-hand-off/` — prior work in the same code
