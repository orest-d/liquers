---
id: EVALUATE-PATH-CONSOLIDATION-PHASE2
kind: design
title: "Phase 2: Architecture — one evaluation body, one write predicate"
status: draft
phase: architecture
area: [core/assets, core/plan, core/context]
created: 2026-09-03
---
# Phase 2: Solution & Architecture — Evaluation Path Consolidation

## Overview

`AssetRef` grows **one** evaluation body, `evaluate(payload)`, which every entry point reaches.
The two run harnesses stay (spawn vs inline — a platform split, not duplication) but lose their
per-body variants; `AssetManager` loses `apply_immediately`, since a supplied state or payload
already implies inline evaluation. Two facts move from being re-derived per call site to being
recorded on the asset: the **store target** (the key this asset is responsible for, set when the
manager creates it) and the **payload requirement** (projected from the plan into metadata and
`AssetInfo`). `INLINE-PATH-LACKS-EXECUTE-ONCE` is co-delivered as a queue-less claim primitive.

## Known-Issue Preflight

Searched: `specs/index.csv` for open (`draft`/`accepted`/`in_progress`) records whose `area`
includes `core/assets`, `core/context`, `core/plan`, `core/metadata` or `web`; the issues linked
from `DESIGN.md` and Phase 1; and the design folders touching `assets.rs`
(`keyed-recipe-ownership`, `dependency-scheduling`, `dependency-management`, `volatility-system`,
`expiration-*`, `wp2-terminal-outcome`). Terminal records (`closed`, `rejected`, `superseded`) are
excluded from the table; `CONTEXT-APPLY-BARE-KEY-ILL-DEFINED` (rejected) is discussed in prose
because this design changes the behaviour it described.

| Issue | Status | Priority | Relevance and solution impact | First? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `CORE-EVALUATE-PATH-CONSOLIDATION` | accepted | P1 | The subject of this design | — | no | Close on Phase 5 | keep P1 |
| `ASSET-PAYLOAD-REQUIREMENT-NOT-RECORDED` | draft | P2 | Resolved here: the plan's `payload_required` is projected into metadata and `AssetInfo` | no | no | Deliver here; close on Phase 5 | keep P2 |
| `INLINE-PATH-LACKS-EXECUTE-ONCE` | accepted | P2 | The inline harness guards with `is_finished()` only. Consolidation halves the number of places a claim must be installed | no | no | **Co-deliver** (user decision, 2026-09-03) | keep P2 |
| `ASSETS-FIX1` | accepted | P2 | Reaches the same conclusion from the marker inventory. Its remaining Phase-4 items (#16 logging, #17 delegation deadlock) sit in `evaluate_recipe`, which this design rewrites | no | no | Absorb what the rewrite touches; leave the rest recorded | keep P2 |
| `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` | draft | P3 | Names the misplaced volatility/expiration pass in `create_plan_with_init_metadata`. The payload projection must **not** join it | no | no | Project at `apply_plan`, the authoritative gate, not in the misplaced pass | keep P3 |
| `EXTENDED-FAST-TRACK` | accepted | P2 | Its items #23/#24 are "fast-track for `apply()`/`apply_immediately()`". Merging the two methods changes what that work targets | no | no | Keep the scheduling decision derivable, not hard-coded; note the merge in that issue | keep P2 |
| `QUEUED-MANAGER-EVICTION-RACE` | accepted | P2 | Same eviction code the manager entry points call. This design does not touch the eviction sequence | no | no | Independent; either order | keep P2 |
| `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` | draft | P2 | Manager↔environment ownership; this design adds no new strong reference | no | no | Monitor | keep P2 |
| `COMBINED-EXPIRES` | accepted | P2 | Expiration algebra in `try_to_set_ready`, which becomes the single status authority here | no | no | Keep `try_to_set_ready` the only writer of terminal status so the algebra lands in one place | keep P2 |
| `ASSETS-IMPROVEMENTS` | accepted | P2 | Persistence, eviction safety, upload limits — adjacent to the write predicate | no | no | Monitor; the narrowed predicate does not conflict | keep P2 |
| `CORE-TOKIO-REMOVAL` | accepted | P3 | The spawn/inline harness split is exactly its subject. This design keeps both and documents why | no | no | Monitor; documenting the split helps that work | keep P3 |
| `CORE-ASSET-GC` | accepted | P3 | Ad-hoc assets remain unreferenced and uncollected, as today | no | no | Monitor | keep P3 |

**No blocking issue.** No prerequisite work is required before implementation, and no priority
change is recommended.

## The Model

Phase 1 sorted every per-entry-point difference into one of three boxes. Phase 2 fixes the
mechanism for each.

| Box | Members | Mechanism |
|---|---|---|
| **Invariants** of the one body | dependency recording, status finalization, key-owner delegation, payload precondition, volatility resolution, notification | executed unconditionally in `evaluate` |
| **Facts recorded on the asset** | store target, volatility, payload requirement, initial state, payload | fields on `AssetData` / `Metadata`, set at construction or resolved before evaluation |
| **Manager policy** | queued vs inline | `AssetManager::eval_mode`, unchanged |

### Reusability invariant

**Only `evaluate(None)` produces an asset the manager may store and hand out again** (user
decision, 2026-09-03). An asset evaluated with a payload is never inserted into the key map or the
query map, is never returned by `get`/`get_asset`, and can never be requested a second time.

Today this holds only through a chain of reasoning rather than by construction: a command
declaring `payload: required` is marked volatile at registration, a volatile query resolves through
`get_volatile_*_asset`, and those construct a fresh asset that is inserted nowhere. Every link is
true at HEAD, but the invariant is a property of the payload, not of volatility, and a registration
that set `payload: required` without `volatile` would break the chain silently.

Phase 2 states it directly and Phase 3 tests it directly: *no map contains an asset that was
evaluated with a payload*, asserted against the maps rather than inferred from the volatility flag.

#### A latent violation this exposes

`get_dependency_asset_with_payload` (both managers) begins:

```rust
let asset = if let Some(key) = query.key() {
    self.get_resource_asset(&key).await?      // ← non-volatile branch returns the MAP-REGISTERED asset
} else {
    self.get_query_asset(query).await?
};
asset.set_payload_path(payload_path).await;
asset.run_immediately(payload).await?;
```

For a non-volatile key, `get_resource_asset` returns the asset registered in `self.assets`
(`get_nonvolatile_resource_asset` inserts it, `:4281`). Running *that* asset with a payload would
put a payload-evaluated value in the key map — precisely what the invariant forbids.

It is unreachable at HEAD: a pure key query reports `PayloadRequirement::None`
(`interpreter.rs:939`, `Step::GetResource` is not a payload-requiring step), so
`schedule_payload_dependency_asset` never selects the payload path for one; and a keyed recipe that
requires a payload is rejected earlier by `to_plan_for_key`. The branch is therefore dead code that
silently contradicts the invariant, protected by two unrelated facts rather than by design.

**Resolution:** the key branch of the payload path becomes an explicit error — keys are a payload
boundary, so asking for a keyed asset *with* a payload is a caller error, not a case to serve:

```rust
// get_dependency_asset_with_payload
if query.key().is_some() {
    return Err(Error::general_error(format!(
        "Query '{}' is a key and cannot be evaluated with a payload: a payload does not cross a \
         key boundary, so a keyed asset would become unreusable while remaining in the key map.",
        query.encode()
    ))
    .with_query(query));
}
```

Cost: one branch removed and one error added, in two managers. Benefit: the invariant holds by
construction rather than by coincidence, and the failure is named if a future change makes the path
reachable.

### Correction to Phase 1: `bound_owner_key` is not the write predicate

Phase 1 proposed writing iff `AssetRef::bound_owner_key().is_some()`. That is **wrong for volatile
keyed assets**, and the error is worth recording because it nearly removed a behaviour you
explicitly asked to keep.

`bound_owner_key` asks the manager's key map who owns the key. A volatile keyed asset is
**deliberately never registered** (`get_volatile_resource_asset` creates a fresh `AssetRef` and puts
it in no map), so `owned_key_asset` returns `None` and `bound_owner_key` yields `None`. Today such
an asset *does* write — `save_to_store` targets `recipe.key().or(store_to_key())` — producing a
stored-but-not-loadable file, which is exactly "volatile assets can be stored but are not
persistent". The `bound_owner_key` predicate would have silently stopped storing them.

The fix is to record the write target instead of re-deriving it:

```rust
/// The key this asset is responsible for, recorded when a manager creates the asset *for* that
/// key — including a volatile keyed asset, which is deliberately absent from the key map.
///
/// `None` for a query asset and for an ad-hoc `apply` asset, neither of which owns a place in
/// the store. This is the *write* target; `bound_owner_key()` remains the separate question of
/// who is registered as the key's owner, which is what the dependency manager asks.
store_target: Option<Key>,
```

`bound_owner_key` keeps its existing role (dependency-manager registration, delegation identity)
and is not touched.

#### Construction sites

Every site that builds an asset, and the target it records. This is the complete list at HEAD
(`assets.rs`); Phase 4 works from it.

| Site | Line | `store_target` |
|---|---|---|
| `DefaultAssetManager::get_nonvolatile_resource_asset` | 4281 | `Some(key)` |
| `DefaultAssetManager::get_volatile_resource_asset` | 4294 | `Some(key)` — the case `bound_owner_key` would have missed |
| `DefaultAssetManager::get_nonvolatile_query_asset` | 4333 | `None` |
| `DefaultAssetManager::get_volatile_query_asset` | 4346 | `None` |
| `DefaultAssetManager::apply` / `apply_immediately` | 4743, 4759 | `None` |
| `DefaultAssetManager::set_state` | 5065 | `Some(key)` — installs, never evaluates |
| `DefaultAssetManager::create_asset` (public, untracked) | 4245 | `None` |
| `DefaultAssetManager::create_dummy_asset` | 4253 | `None` |
| `ImmediateAssetManager::make_volatile` | 5724 | **parameter** — serves both a key caller (5756) and a query caller (5737), so the target cannot be derived inside it |
| `ImmediateAssetManager::get_resource_asset` (non-volatile branch) | 5765 | `Some(key)` |
| `ImmediateAssetManager::get_query_asset` (non-volatile branch) | 5745 | `None` |
| `ImmediateAssetManager::apply` / `apply_immediately` | 5866, 5901 | `None` |
| `ImmediateAssetManager::set_state` | 6052 | `Some(key)` — installs, never evaluates |
| `AssetData::new_temporary` | 1520 | `None` |

`make_volatile` is the only site that needs a signature change rather than a literal: it is shared
between the keyed and query paths, which is precisely the conflation this field removes.

### The predicate that follows

**Write iff `store_target.is_some()` and this evaluation did not delegate.**

The loadable-vs-stored distinction then needs no separate rule, because of a closure worth stating
explicitly:

> Among assets that reach `evaluate`, a `store_target` is only ever set by a manager creating an
> asset *for a key*. Such an asset has no supplied initial state, and a payload cannot cross a key
> boundary. Therefore `store_target.is_some()` implies the asset is **either reproducible or
> volatile** — and the volatile case already writes status `Volatile`, which `try_fast_track`
> refuses.
>
> The qualifier matters: `AssetManager::set_state(key, state)` also builds a keyed asset *with* a
> supplied state, so it is the one construction that pairs a store target with supplied data. It
> never calls `evaluate` — it installs a value and persists directly — so it is outside the
> closure rather than a counter-example to it. Phase 3 covers it as a corner case.

So the write path needs one predicate and no new status logic. Reproducibility remains the
*explanation* (it is why a query or `apply` asset has no target, why volatile results are not
reused, and why a payload asset may not be a dependency), and its one new mechanical use is the
recorded payload requirement below. It is not, in the end, a second gate on persistence — an
honest downgrade from the Phase 1 sketch.

### Persistence outcomes, today versus after

| Case | `store_target` | Today | After |
|---|---|---|---|
| Keyed, non-volatile (recipe-defined) | `Some(key)` | write `Ready`, fast-trackable | unchanged |
| Keyed, volatile | `Some(key)` | write `Volatile`, not fast-trackable | unchanged |
| Keyed, delegating to the owner | `Some(key)`, delegated | no write (`!delegated`) | unchanged (owner writes) |
| Query asset, no filename | `None` | no write (no target) | unchanged |
| Query asset resolving `store_to_key` | `None` | writes `cwd/filename` | **no write** |
| `apply` with a bare-key recipe | `None` | **writes under that key** | **no write** |
| `apply` whose recipe has a filename | `None` | writes `cwd/filename` | **no write** |
| `apply_immediately` / payload | `None` | no write | unchanged |

Three rows change, all narrowing writes, all of them the durable half of
`CONTEXT-APPLY-BARE-KEY-ILL-DEFINED`. The "query asset resolving `store_to_key`" row is likely
unreachable today (a plain query asset has no cwd, so `store_to_key` yields `None`); the rule makes
it unreachable by construction. Phase 3 must cover all eight rows.

`RecipeEvaluation.delegated` becomes redundant *for persistence* — a delegating asset's owner is
another asset — but the flag is kept, because "did this evaluation hand off?" is also what
suppresses the double dependency-manager registration. The equivalence is not obvious enough to
rely on silently; Phase 3 pins it with a test.

## Data Structures

### `AssetData<E>` — one field added, none removed

```rust
pub struct AssetData<E: Environment> {
    // ... existing fields unchanged ...

    /// Key this asset is responsible for writing, or `None` for a query or ad-hoc asset.
    /// Set at construction by the manager; never inferred from the recipe afterwards, because
    /// provider resolution replaces the recipe mid-evaluation.
    store_target: Option<Key>,
}
```

**Ownership:** owned `Option<Key>`; `Key` is already owned and cloned freely elsewhere in this
struct. No `Arc` — a key is small and the field is read under the existing lock.

**Serialization:** none. `AssetData` is not serialized; the persisted projection is `Metadata`.

**Why a field and not a method:** `bound_owner_key`'s own doc comment records the reason — "provider
evaluation replaces the mutable recipe, so ownership cannot be inferred from `AssetData::recipe`
alone". The same applies to the write target, and re-deriving it from three sources (recipe key,
`store_to_key`, the key map) is what produced the divergence this issue is about.

### Deliberately *not* added: `AssetData.payload_required`

Phase 1 sketched a `payload_required` field mirroring `is_volatile`. **Rejected.** `is_volatile`
exists both as an `AssetData` field and inside `Metadata`, and that duplication is the source of
the two-source-of-truth reads in `try_to_set_ready`
(`lock.is_volatile || metadata_expires.is_volatile()`). Repeating the pattern would add a second
one. `MetadataRecord.payload_required` already exists, round-trips to `AssetInfo`, and lives inside
`AssetData.metadata`, so recording it there *is* the asset knowing it. Reads go through an
accessor:

```rust
impl<E: Environment> AssetRef<E> {
    /// Whether this asset's plan required an evaluation payload.
    pub async fn payload_required(&self) -> PayloadRequirement;
}
```

### `PayloadRequirement`, `Plan.payload_required` — unchanged

Both already exist (`command_metadata.rs:881`, `plan.rs`). `PlanBuilder` already tracks
`payload_required` beside `is_volatile` and `expires`, and `Plan::requires_payload` returns the
cached value. Nothing new is computed; the gap is purely that the computed value never reaches the
asset.

## Function Signatures

### `AssetRef<E>` — three bodies to one, four run entry points to two

**Encapsulation rule (user decision, 2026-09-03):** an `AssetRef` is constructed and managed by
the asset manager. No evaluation entry point on `AssetRef` is public — `evaluate` is private to the
module, `run`/`run_inline` stay `pub(crate)`, and the only public way to evaluate anything remains
`AssetManager` (or `EnvRef`/`Context`, which delegate to it). This *strengthens* the status quo:
the four methods being removed (`evaluate_recipe`, `evaluate_and_store`, `evaluate_immediately`,
and `evaluate_recipe_outcome`'s public wrapper) are `pub` today, and nothing outside `assets.rs`
uses them. The only public addition is a read accessor.

```rust
impl<E: Environment> AssetRef<E> {
    /// The single evaluation body. Every entry point reaches evaluation through this.
    ///
    /// Private: evaluation is entered through the asset manager, never on a handle a caller
    /// happens to hold.
    ///
    /// Resolves execution facts, resolves the recipe (delegating to the key's owner when one is
    /// registered), applies it through `Environment::apply_recipe`, records observed
    /// dependencies, installs the value, finalizes status, persists when this asset owns a store
    /// target, and registers with the dependency manager.
    async fn evaluate(&self, payload: Option<E::Payload>) -> Result<(), Error>;

    /// Spawning harness (native): unchanged.
    #[cfg(not(target_arch = "wasm32"))]
    async fn run_with_future<Fut>(&self, evaluate_future: Fut) -> Result<(), Error>
    where
        Fut: std::future::Future<Output = Result<(), Error>>;

    /// Spawn-free harness (wasm and inline managers): unchanged.
    async fn run_with_future_inline<Fut>(&self, evaluate_future: Fut) -> Result<(), Error>
    where
        Fut: core::future::Future<Output = Result<(), Error>>;

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) async fn run(&self, payload: Option<E::Payload>) -> Result<(), Error>;

    pub(crate) async fn run_inline(&self, payload: Option<E::Payload>) -> Result<(), Error>;

    pub async fn payload_required(&self) -> PayloadRequirement;
}
```

Removed: `evaluate_recipe`, `evaluate_recipe_outcome`, `evaluate_and_store`, `evaluate_immediately`,
`run_immediately`, `run_immediately_inline`. The first four are `pub` but have no caller outside
`assets.rs` and its tests; they are removed rather than deprecated, since `liquers-core` publishes
no stability guarantee for asset internals and leaving two names for one body is the defect being
fixed.

### `evaluate` — the invariant order

1. `resolve_volatility_before_evaluation()` — unchanged, recipe-level.
2. Resolve the recipe. Key recipe → ask `owned_key_asset`: another owner means **hand off**
   (`record_dependency_on_asset` is a no-op for the same node, then `wait_for_dependency`);
   otherwise resolve through the recipe provider and adopt the resolved recipe's identity and
   title/description.
3. Build the context, install `payload` when present, call `envref.apply_recipe(...)`. The payload
   precondition is **already** enforced inside `apply_plan`, which calls itself "the authoritative
   gate … covers every execution path"; nothing new is added, and the per-entry-point pre-check in
   `Context::apply` is deleted as redundant.
4. `take_pending_dependencies()` → metadata. Unconditional, for every entry point — the asymmetry
   the issue names.
5. Install data, type identifier and type name.
6. `try_to_set_ready()` — the single status authority, before persistence and before the
   `ValueProduced` notification.
7. `ValueProduced` notification, after finalization. This is a **behaviour improvement for the
   payload path**, which today notifies while the status is still `None`/`Processing`.
8. Persist iff `store_target.is_some() && !delegated`.
9. `dependency_manager().track_asset(self)` — unconditional; already self-limiting on status and
   `bound_owner_key`, so ad-hoc assets register nothing.

### `AssetManager<E>` — three evaluation entry points to two

```rust
/// Applies a recipe to a supplied initial state, with an optional execution payload.
///
/// A supplied state or a payload makes the result non-reproducible: it is in no map, is never
/// reused, and owns no store target. Evaluation therefore completes before this returns and
/// consumes no job-queue slot, on both managers.
async fn apply(
    &self,
    recipe: Recipe,
    to: State<E::Value>,
    payload: Option<E::Payload>,
) -> Result<AssetRef<E>, Error>;

#[deprecated(note = "use `apply(recipe, to, payload)`; every apply now evaluates before returning")]
async fn apply_immediately(
    &self,
    recipe: Recipe,
    to: State<E::Value>,
    payload: Option<E::Payload>,
) -> Result<AssetRef<E>, Error> {
    self.apply(recipe, to, payload).await
}
```

`apply_immediately` becomes a deprecated **default** method rather than being deleted: only two
implementors exist and both are in `liquers-core`, but an out-of-tree `AssetManager` should get a
deprecation warning rather than a compile error. In-tree callers are migrated in the same change.

`get_asset`, `get`, `get_dependency_asset`, `get_dependency_asset_with_payload`,
`drain_dependencies`, `wait_for_dependency`, `eval_mode` and every keyed-mutation method are
unchanged.

### Decision settled: `apply` evaluates inline

Phase 1 left one decision. **Settled: inline, on both managers.** The rationale already exists in
the codebase for the payload case — such an asset "is volatile by construction, so it is fresh,
unshared, and never persisted or reused, and there is nothing to gain by queueing it" — and it
extends unchanged to any non-reproducible asset. Taking a queue slot while the parent holds one is
the deadlock shape `ASSETS-FIX1` #17 describes.

This *adds* the "completes before returning" guarantee to `apply` rather than removing one, so no
caller loses a property. `DefaultAssetManager::apply` uses `run(payload)` (spawning harness, as its
`apply_immediately` does today); `ImmediateAssetManager::apply` uses `run_inline(payload)`.

It does not preclude `EXTENDED-FAST-TRACK`: when execution classes arrive, the scheduling decision
is derived from the asset's recorded facts, which is where that work wants it.

### `Context` and `EnvRef`

```rust
impl<E: Environment> Context<E> {
    /// Applies a query to a supplied state as an ad-hoc asset.
    ///
    /// The context's payload is inherited unconditionally; whether a payload is *required* is
    /// settled inside `apply_plan`, not by a pre-check here.
    pub async fn apply(&self, query: &Query, to: State<E::Value>) -> Result<AssetRef<E>, Error>;
}
```

The body loses its `requires_payload` branch and its duplicated error message, becoming one call:
`manager.apply(recipe, to, self.payload.clone())`. `EnvRef::evaluate`, `EnvRef::evaluate_immediately`,
`Context::evaluate` and `Context::get_dependency_state` keep their signatures;
`evaluate_immediately` delegates to `apply(recipe, State::new(), Some(payload))`.

### Payload requirement — where it is recorded

One projection, at the point that already reads the value:

```rust
// interpreter::apply_plan, beside the existing authoritative gate
if plan.payload_required.is_required() {
    context.set_payload_required().await?;   // new Context method, mirrors set_expires
}
```

`Context::set_payload_required` writes `MetadataRecord.payload_required` through the asset's
metadata, exactly as `set_expires` writes the expiration. Every environment reaching evaluation
through `apply_plan` gets it, so a custom `Environment::apply_recipe` that uses the standard
interpreter needs no change.

A second, independent gap is fixed in the same change: `AsyncRecipeProvider::get_asset_info`
(`recipes.rs:526`) projects `plan.is_volatile` and `plan.expires` into the recipe preview but not
`plan.payload_required`. One line, beside the other two.

Deliberately **not** placed in `create_plan_with_init_metadata`, whose analysis pass
`RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` already reports as misplaced.

### Co-delivery: `INLINE-PATH-LACKS-EXECUTE-ONCE`

```rust
/// Atomic execute-once claim, available on both targets.
///
/// `RunClaim` becomes the queued specialization: same atomic status transition, but a `Drop`
/// repair that re-submits through the job queue. The inline guard resets the asset to a
/// re-runnable status instead, since it has no queue to re-submit to.
pub(crate) async fn try_claim_for_run_inline(&self) -> Result<Option<InlineRunClaim<'_>>, Error>;
```

Scope discipline, from that issue's own evidence: a claim that only *refuses* a second caller is
the wrong answer — the second caller must **wait**, which `run_with_future_inline` already
improvises through its `select!` between `wait_to_finish()` and the evaluation future. The claim
makes that correct rather than improvised. Consolidation is what makes this cheap: with one body,
the claim is installed in two harnesses instead of four run paths.

## Trait Implementations

No new trait is introduced. One existing trait changes, in two implementors, both in
`liquers-core`.

### Trait: `AssetManager<E>`

**Implementor: `DefaultAssetManager<E>`** (`assets.rs:4384`) — queued, native.

| Method | Change |
|---|---|
| `apply` | gains `payload: Option<E::Payload>`; body becomes construct-ad-hoc + `run(payload)`. It no longer calls `job_queue.submit`, per the inline decision |
| `apply_immediately` | removed from the impl; the deprecated trait default forwards to `apply` |
| `get_asset`, `get` | unchanged, including their stale-terminal eviction loops |
| `get_dependency_asset_with_payload` | unchanged except `run_immediately(payload)` → `run(Some(payload))` |
| `get_resource_asset` / `get_volatile_resource_asset` / `get_nonvolatile_*` | set `store_target` at construction |

**Implementor: `ImmediateAssetManager<E>`** (`assets.rs:5777`) — inline, wasm-capable.

| Method | Change |
|---|---|
| `apply` | gains the payload parameter; body becomes construct-ad-hoc + `run_inline(payload)` — which is what its `apply_immediately` already does |
| `apply_immediately` | removed from the impl |
| `get_asset`, `get` | unchanged, including lazy expiration-on-access |
| `get_dependency_asset_with_payload` | unchanged except `run_immediately_inline(payload)` → `run_inline(Some(payload))` |
| `make_volatile` / `get_resource_asset` | set `store_target` at construction |

Both keep `eval_mode()`, and neither gains or loses a trait bound. `AssetManager` is reached as
`Arc<E::AssetManager>` through an associated type, never as `dyn AssetManager`, so object safety
does not constrain the signature change.

### Traits deliberately untouched

`Environment` (`apply_recipe` keeps its signature — the payload projection happens below it, in
`apply_plan`), `AsyncRecipeProvider` (one default-method body changes, no signature does),
`AsyncStore`, `ValueInterface`, `CommandExecutor`.

## Error Handling

Every error on this path is `liquers_core::error::Error`, built with a typed constructor. No new
error type, no new `ErrorType` variant, and no `Error::new`.

| Condition | Constructor | Where | Change |
|---|---|---|---|
| Plan requires a payload, none supplied | `Error::general_error(...).with_query(&plan.query)` | `interpreter::apply_plan` | unchanged — already the authoritative gate |
| Same condition, pre-checked per entry point | `Error::general_error(...).with_query(&query)` | `Context::apply`, `Context::schedule_payload_dependency_asset` | `Context::apply`'s copy is **deleted** as redundant; the nested-scheduling copy stays, because it must fail before an asset is created |
| Keyed recipe requiring a payload | propagated from `Recipe::to_plan_for_key` | `AssetManager::recipe_opt`, recipe resolution in `evaluate` | unchanged |
| Recipe missing for a key | `Error::key_not_found(&key)` | recipe provider | unchanged |
| Dependency cycle | `Error::dependency_cycle(&dep_key)` | `record_dependency_on_asset`, payload path | unchanged |
| Evaluation failure | existing `fail_asset` routine: `metadata.with_error`, `Status::Error`, `ErrorOccurred` notification | `evaluate` | one routine instead of two — the immediate path currently leaves finalization to the harness |
| Persistence failure | recorded in `PersistenceStatus` / `last_persistence_error`, not returned | `persist_with_status_tracking` | unchanged |

The failure path is the second place the two bodies diverge today: `evaluate_and_store` has an
explicit error arm that clears data, sets `Status::Error` and notifies, while `evaluate_immediately`
returns the error and lets `finish_run_with_result` run `fail_asset`. After consolidation there is
one arm, and it is the harness's — `evaluate` propagates with `?` and `finish_run_with_result`
remains the single failure authority, matching `try_to_set_ready` being the single success
authority.

No `unwrap()` or `expect()` is introduced; `store_target` is an `Option<Key>` read directly, and
every `Result` on the path propagates with `?`.

## Sync vs Async

Everything on this path is already async and stays async. The two harnesses differ only in how the
service-message loop is driven — `tokio::spawn` + `tokio::select!` natively, `futures::join!` +
`futures::select!` inline — and that difference is `#[cfg(not(target_arch = "wasm32"))]`, not a
feature. Keeping both is deliberate: collapsing them would either give up the spawned loop natively
or fake a spawn on wasm, and `CORE-TOKIO-REMOVAL` is the issue that owns changing that.

No lock is held across an `.await` that is not already held so today; `evaluate` follows the
existing discipline of taking the write lock for the data/metadata assignment only, and letting
`try_to_set_ready` take its own.

## Integration Points

| Crate / module | Change |
|---|---|
| `liquers-core/src/assets.rs` | one private `evaluate`; two `pub(crate)` run entry points; `store_target` field and its 14 construction sites; `save_to_store` target; `apply` merge in both managers; the payload path's key branch becomes an error; inline claim |
| `liquers-core/src/context.rs` | `Context::apply` loses the payload branch; `Context::set_payload_required` added |
| `liquers-core/src/interpreter.rs` | one projection line in `apply_plan` beside the existing gate |
| `liquers-core/src/recipes.rs` | one projection line in `AsyncRecipeProvider::get_asset_info` |
| `liquers-core/src/metadata.rs` | none — the fields and accessors already exist |
| `liquers-lib`, `liquers-axum`, `liquers-web`, `liquers-py` | expected to compile unchanged; `liquers-lib/src/ui/runner.rs` is the one in-tree `apply_immediately` caller and moves to `apply` |

## Relevant Commands

**No new commands, and no command namespace is involved** — confirmed by the user, 2026-09-03. This is core runtime plumbing below the
command layer: no `register_command!` signature changes, so `specs/command_registry.yaml` does not
regenerate, and no query syntax changes, so nothing needs `liquers-validate`. The command-facing
surface that *does* change is `PayloadRequirement`, which commands already declare through
`payload: required` in `register_command!` — this design only makes the declared fact observable
after evaluation.

No namespace (`lui`, `egui`, `pl`, `ns-img`) is in scope.

## Documentation Architecture

### The Phase 5 explanation (user requirement, 2026-09-03)

Phase 5 must document the consolidated design as **technical detail**, not as a summary:

1. the **public surface** — what callers may use, what is framework-facing, and why;
2. the **surviving methods**, their purposes and their relationships to one another;
3. the **execution flow, step by step, for each kind of flow**, and *why those kinds exist* —
   payload, initial state, volatility;
4. the **high-level public API at the asset-manager module level** (the `//!` rustdoc of
   `assets.rs`).

**Where it goes: `ASSET_LIFECYCLE.md`, substantially rewritten — no new reference.** The deciding
fact is in that document's own Overview, which lists as its third purpose:

> "Serve as a basis for potential refactoring — identifying code duplication and responsibility
> boundaries between `Context` and `AssetRef`/`assets.rs`"

**This design completes that purpose.** Once the duplication is gone, the document's reason for
existing is gone with it, and most of its body (Paths A–D, §6's asymmetry table, §7's issue list)
becomes false at HEAD — which a `reference/` document may not be. Creating a new file beside it
would leave two documents about evaluation flow, one of them stale. So the file is rewritten into
the reference this requirement describes, and retitled from "Asset Lifecycle — Comprehensive Map"
to reflect that it now explains the flows rather than cataloguing their divergence.

Its audit content is **not** deleted: §6 and §7 are the evidence trail for
`CORE-EVALUATE-PATH-CONSOLIDATION` and are "true on a date" material, so they are promoted to
`specs/archive/<date>-asset-lifecycle-duplication-audit.md` (`DOCS_STRUCTURE_GUIDE.md` §1015: a
review or audit belongs in `archive/`).

**The flow dimensions to explain, and why each exists.** The rewritten reference must present these
as the axes that generate every flow, rather than enumerating paths:

| Dimension | Why it exists | What it changes |
|---|---|---|
| Keyed / query / ad-hoc identity | what the asset *is*, and whether anything can ask for it again | store target, map membership, reuse |
| Initial state supplied | the caller injects input the identity does not describe | non-reproducible: no store target, no reuse |
| Payload present or required | per-call caller context that is deliberately *not* part of identity, and cannot cross a key boundary | unmappable, unreusable, never persisted as loadable |
| Volatility | the result is valid but single-use | stored but not loadable; never reused |
| Delegation | another asset owns the key | hand-off: no write, no second dependency edge |
| Fast-track | a stored value is already valid | evaluation is skipped entirely |
| Queued / inline | manager policy, not a property of the asset | scheduling and the status sequence only |

The first five are properties of the asset; the sixth is a relationship; only the last is policy.
That separation is the explanation the requirement asks for, and it is what the design is *for*.

### Document plan

| Path | Kind | Audience | Change | Links |
|---|---|---|---|---|
| `specs/reference/ASSET_LIFECYCLE.md` | reference | core developers | **Primary, rewritten.** The public surface; the surviving methods and their relationships; the flow dimensions above with a step-by-step execution sequence for each; the persistence-outcome table. §2, §3 Paths A–D, §6 and §7 are replaced, not amended | link from README capability line; points at the `assets.rs` rustdoc as primary |
| `specs/archive/<date>-asset-lifecycle-duplication-audit.md` | archive | — | **New.** The §6 asymmetry table and §7 issue list, preserved as the evidence trail for the issue | referenced from the design folder |
| `liquers-core/src/assets.rs` `//!` | code | core developers | **The high-level public API at module level** — the requirement's item 4. `DOC_03` already designates this rustdoc "the primary reference", so it carries the API overview and the reference documents point at it. Its current entry-point table is replaced | ↔ `ASSET_LIFECYCLE.md` |
| `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` | reference | integrators | §"Public entry-point contract" (three entry points become two), §"Persistence contract" (the `store_target` predicate), §"Public versus infrastructure APIs" — which today says the boundary "is not enforced by Rust visibility consistently" and that "a future API pass should narrow or separate it": **this design is that pass**, so the section records the narrowed surface — and §"Conflicts and unresolved gaps" (remove what this closes) | ↔ `ASSET_LIFECYCLE.md` |
| `specs/reference/ASSETS.md` | reference | core developers | §Overview and §AssetManager entry-point list | ↔ both above |
| `specs/reference/PAYLOAD_GUIDE.md` | reference | command authors | the requirement is now recorded and observable in metadata and `AssetInfo` | ↔ `DOC_03` |
| `specs/README.md` | map | everyone | capability lines for evaluation and assets point at the updated reference | — |
| `specs/issues/*` | issues | — | `CORE-EVALUATE-PATH-CONSOLIDATION`, `ASSET-PAYLOAD-REQUIREMENT-NOT-RECORDED`, `INLINE-PATH-LACKS-EXECUTE-ONCE` closed on Phase 5; `ASSETS-FIX1` and `EXTENDED-FAST-TRACK` updated where this design moves their ground | `design:` fields |
| `liquers-core/src/assets.rs` `//!` | code | — | the module's entry-point table is part of the implementation | — |

Proposed `affects_docs`: `ASSET_LIFECYCLE.md`, `DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`,
`ASSETS.md`, `PAYLOAD_GUIDE.md`. `DEPENDENCIES_STATUS.md` is **not** included: the
`Dependencies` status contract and the delegation hand-off rule are unchanged by this design.

No new reference and no guide. Reconsidered against the Phase 5 requirement above and confirmed:
the requirement is satisfied by rewriting the document that already owns the subject, plus the
module rustdoc that `DOC_03` already designates primary. A new file would split "how evaluation
works" in two. No guide, because none of this answers "how do I achieve X" for a command author —
the audience is developers working inside `liquers-core`.

## Rust Convention Review (rust-best-practices)

Applied to the signatures above.

**Blocking — none.**

**Resolved during drafting:**
- *Two sources of truth.* The Phase 1 `AssetData.payload_required` field would have duplicated
  `MetadataRecord.payload_required`, repeating the `is_volatile` split that already forces
  two-source reads in `try_to_set_ready`. Dropped in favour of an accessor.
- *Trait mutation.* Removing `apply_immediately` outright would break any out-of-tree
  `AssetManager`. Kept as a `#[deprecated]` default forwarding to `apply` — "extend, don't mutate".
- *Error construction.* The payload-precondition error already uses
  `Error::general_error(...).with_query(...)`; no new error type, no `Error::new`.
- *`cfg` symmetry.* `run(payload)` inherits `#[cfg(not(target_arch = "wasm32"))]` from
  `run_with_future`; `run_inline` is unconditional. The wasm build sees exactly one run entry
  point, as it does today.

**Advisory:**
- `Option<E::Payload>` is passed by value through `evaluate`/`run`; `PayloadType` is not required to
  be `Clone`, so the payload must be *moved* into the context, not cloned. `Context::apply` clones
  its own `Option<E::Payload>` — which is why `PayloadType: Clone` matters there and is already
  satisfied. Phase 4 must not introduce a clone in `evaluate`.
- `store_target` is set at construction rather than after, to avoid a window in which an asset
  exists with the wrong target. The volatile branch currently mutates `is_volatile` post-construction;
  do not copy that shape.
- Matches over `Status` in the finalization path stay exhaustive; no `_ =>` arm is introduced.

## Review Outcome

Two independent reviews ran against this document before the approval gate.

**Reviewer A — Phase 1 conformity.** No blocking findings. All eight Phase 1 commitments are
addressed. One concern: Phase 1 scoped the change to `assets.rs` and `context.rs`, while Phase 2
also touches `interpreter.rs` and `recipes.rs`. **Fixed** — Phase 1's Crate Placement section now
records the widening and its justification, and the payload-projection point is framed as a
deliberate revision rather than a silent one. Reviewer A judged both Phase 2 corrections to Phase 1
(the write predicate, the rejected duplicate field) justified by evidence and clearly recorded.

**Reviewer B — codebase alignment.** No blocking findings; every factual claim verified against
HEAD, including the load-bearing one: `get_volatile_resource_asset` (`assets.rs:4292`) creates an
asset it does not insert into the key map, and that asset *does* write today through
`save_to_store`'s `recipe.key()?.or(recipe.store_to_key()?)` (`:2447`). Confirmed too: the four
evaluation methods have no callers outside `assets.rs` and its tests; `liquers-lib/src/ui/runner.rs`
is the only out-of-crate `apply_immediately` caller; `apply_plan`'s gate calls itself authoritative;
`recipes.rs:526` projects `is_volatile` and `expires` but not `payload_required`.

The construction-site enumeration was completed by hand afterwards (the table above), which
surfaced two details neither review had: `make_volatile` serves both a key and a query caller and
therefore needs the target as a parameter, and `set_state` pairs a store target with a supplied
state — the one construction outside the reproducibility closure, now scoped explicitly.

## Open Decisions for Phase 3

1. Whether the payload path's key branch should return the error above or be removed entirely.
   The error is proposed because it names the boundary if the path ever becomes reachable.
2. Whether `RecipeEvaluation.delegated` survives as a field or becomes a local — it is kept here for
   dependency-manager suppression, and Phase 3 pins the persistence equivalence with a test.
3. Whether the inline claim's `Drop` repair resets to `Status::Recipe` or to the status observed
   before the claim. The queued `RunClaim` re-parks; the inline one has nowhere to park.
