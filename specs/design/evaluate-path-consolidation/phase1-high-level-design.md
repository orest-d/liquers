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

`liquers-core` only. Primarily `assets.rs` (evaluation bodies, harnesses, managers) and
`context.rs` (`Context::apply`, `EnvRef::evaluate*`); Phase 2 widened this by two single-line
projections — `interpreter.rs`, where `apply_plan` already holds the authoritative payload gate and
so is where the requirement is recorded, and `recipes.rs`, where the recipe-preview `AssetInfo`
projects `is_volatile` and `expires` but not `payload_required`. No dependent crate gains a
dependency; the goal is that `liquers-lib`, `liquers-axum`, `liquers-web` and `liquers-py` compile
unchanged.

## Documentation Intent

**Reference:** Rewrite `specs/reference/ASSET_LIFECYCLE.md` — it already *is* the evaluation-path
catalogue (§2 entry points, §3 Paths A–D, §6 `evaluate_and_store` vs `evaluate_immediately`
asymmetry), and its own Overview names as a purpose "identifying code duplication and
responsibility boundaries" — a purpose this design completes, leaving most of its body false at
HEAD (§6 is already partly stale: it claims immediate evaluation never collects dependencies, which
HEAD does). Phase 2 settles the scope: a rewrite into the flow-and-public-surface reference, with
the audit content promoted to `archive/`. A new reference file would split one subject in two.

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
it is evaluated. The qualifier matters and Phase 3 had to learn it the hard way: they are thin *in
evaluation logic*, not in construction, and construction decides what concurrent access means. Two
`apply` calls build two separate ad-hoc assets and legitimately run the body twice; two `get_asset`
calls converge on one mapped asset and must run it once. "One evaluation path" is not "every entry
point is interchangeable", and `AssetData` already carries those facts (`initial_state`, `is_volatile`,
`save_in_background`, `payload_path`, `expiration_time`).

## Recorded Execution Facts (requirement added 2026-09-03)

An asset must **know** that its evaluation depends on a payload, be **executed as such**, and
expose that fact in metadata and `AssetInfo`.

The destination fields already exist and are already plumbed: `MetadataRecord.payload_required`
(`metadata.rs:913`), `AssetInfo.payload_required` (`:716`), the setter `set_payload_required`
(`:1386`), legacy-JSON extraction (`:1532`) and a round-trip test (`:2826`). **Nothing sets them.**
No evaluation path calls the setter, so every evaluated asset reports `PayloadRequirement::None`,
including one that could not have run without a payload. Filed as
`ASSET-PAYLOAD-REQUIREMENT-NOT-RECORDED` (P2, M) and scheduled into this design.

The requirement is computed twice today and discarded both times — `Context::apply` calls
`query.requires_payload` only to choose an entry point, and `Recipe::to_plan_for_key` only to
reject a keyed recipe. That is the same duplication this design exists to remove.

**Design consequence.** Resolve the payload requirement *before* evaluation, symmetrically with
volatility, which `AssetData` already models this way:

| Volatility (exists) | Payload requirement (to add) |
|---|---|
| `AssetData.is_volatile` | `AssetData.payload_required: PayloadRequirement` |
| `resolve_volatility_before_evaluation()` | *(Phase 2 revised: projected at `apply_plan`, beside the gate that already reads it, rather than joining a pre-evaluation pass that `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` reports as misplaced)* |
| reaches `MetadataRecord.is_volatile` → `AssetInfo` | reaches `MetadataRecord.payload_required` → `AssetInfo` |

The pairing is natural: a `PayloadRequirement::Required` command is already marked volatile at
registration, so both facts come from the same plan walk.

Once recorded, the payload precondition has one home — `payload_required.is_required() &&
payload.is_none()` is an error, checked in the one body from the recorded field — and the
per-entry-point pre-checks disappear.

**Property to preserve:** what makes an asset non-reproducible is the *requirement*, not whether a
payload happened to be in scope. A plain query evaluated through `EnvRef::evaluate_immediately` has
a payload available that no command consumes; it stays reproducible.

## Resolved Questions (2026-09-03)

1. **Store-target ownership for a query asset.** Non-keyed (query) assets are not stored; they need
   no owner. The write predicate is therefore `AssetRef::bound_owner_key().is_some()` — which
   already exists and is already what `DependencyManager::track_asset` uses. It replaces
   `recipe.key().or(recipe.store_to_key())` in `save_to_store` (`assets.rs:2447`, `:2479`) and in
   `AssetData`'s metadata-save path (`:833`, `:861`). Note this keeps recipe-defined resources
   storable: a `recipes.yaml` action chain has `key() == None` and is identified by
   `store_to_key()`, which `bound_owner_key` accepts as recipe identity — while an ad-hoc `apply`
   asset built from a bare-key recipe fails the owner check and writes nothing.
2. **Harness collapse.** To be resolved by this design; the mapping is below.
3. **`INLINE-PATH-LACKS-EXECUTE-ONCE`** — co-delivery with this work.

## Method Mapping (Phase 2 input)

### `AssetRef` evaluation and run layer

| Today | Becomes | Note |
|---|---|---|
| `resolve_volatility_before_evaluation()` | resolves volatility **and** `payload_required` | one pre-evaluation pass, one plan walk |
| *(no field)* | **`AssetData.payload_required`** → `MetadataRecord` → `AssetInfo` | `ASSET-PAYLOAD-REQUIREMENT-NOT-RECORDED` |
| `evaluate_recipe_outcome()` (private) | **`evaluate(payload)`** — the one body | absorbs delegation, provider resolution, dep collection, install, finalize, persist, DM track |
| `evaluate_recipe()` (pub) | removed | no caller outside `assets.rs` and its tests |
| `evaluate_and_store()` (pub) | `evaluate(None)` | |
| `evaluate_immediately(payload)` (pub) | `evaluate(Some(payload))` | |
| `run_with_future(fut)` `cfg(not(wasm32))` | unchanged | genuine platform difference: spawned psm loop |
| `run_with_future_inline(fut)` | unchanged | spawn-free psm via `futures::join!` |
| `run()` | `run(None)` | |
| `run_immediately(payload)` | `run(Some(payload))` | |
| `run_inline()` | `run_inline(None)` | |
| `run_immediately_inline(payload)` | `run_inline(Some(payload))` | |

Three evaluation bodies become one; four run entry points become two; the two harnesses stay two,
because spawn-vs-inline is a real platform split, not a duplication.

### `AssetManager` trait and its two implementations

| Today | Becomes | Note |
|---|---|---|
| `get_asset(query)` ×2 | unchanged | resolve through the maps, then schedule |
| `get(key)` ×2 | unchanged | |
| `apply(recipe, state)` ×2 | **`apply(recipe, state, payload)`** ×2 | |
| `apply_immediately(recipe, state, payload)` ×2 | removed — same method | |
| `get_dependency_asset_with_payload(...)` | unchanged role; its `run_immediately(payload)` becomes `run(Some(payload))` | |
| `get_dependency_asset`, `drain_dependencies`, `wait_for_dependency` | unchanged | dependency scheduling is a separate concern |
| `eval_mode()` | unchanged, but consulted only for *reproducible* assets | see the decision point below |

Six evaluation entry-point implementations (3 methods × 2 managers) become four.

### Public and `Context` layer

| Today | Becomes | Note |
|---|---|---|
| `EnvRef::evaluate(query)` | unchanged | |
| `EnvRef::evaluate_immediately(query, payload)` | unchanged signature; delegates to `apply(recipe, State::new(), Some(payload))` | |
| `Context::evaluate` / `get_dependency_state` | unchanged | |
| `Context::apply(query, to)` | the `requires_payload` branch disappears: one call, `manager.apply(recipe, to, self.payload.clone())` | payload inheritance stops depending on a pre-check |
| `save_to_store` target `recipe.key().or(store_to_key())` | `bound_owner_key()` | question 1 |

## Open Questions

All six original questions are resolved (see above). One decision remains, and it belongs to
Phase 2 rather than to the user:

### Decision point for Phase 2

Does `apply` under a queued manager still enqueue, or always evaluate inline? Proposal: **inline**,
extending the rationale already written for payload dependencies — such an asset is unshared,
unreused and unpersisted, so a queue slot buys nothing, and taking one while the parent holds one
is the deadlock pattern in `ASSETS-FIX1` #17. This *adds* the "completes before returning"
guarantee to `apply` rather than removing one, so no caller loses a property, and it derives from
reproducibility instead of being a policy. It is the one row that changes an observable guarantee,
so it is called out rather than assumed.

## References

- `specs/issues/CORE-EVALUATE-PATH-CONSOLIDATION.md` (P1, L, accepted) — the issue being designed
- `specs/issues/ASSETS-FIX1.md` — same conclusion from the marker-inventory direction
- `specs/issues/INLINE-PATH-LACKS-EXECUTE-ONCE.md`, `specs/issues/CONTEXT-APPLY-BARE-KEY-ILL-DEFINED.md`
- `specs/reference/ASSET_LIFECYCLE.md` §2, §3, §6, §7 · `specs/reference/ASSETS.md`
- `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`
- `specs/design/keyed-recipe-ownership/`, `specs/design/dependency-scheduling/`,
  `specs/design/keyed-delegation-hand-off/` — prior work in the same code
