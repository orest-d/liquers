# Phase 2: Solution & Architecture — dependency-resolved-once-per-evaluation

## Overview

An `EvaluationSession` maps each dependency identity to one asset. Running `Context`s hold it
strongly, and assets scheduled into it hold it only weakly. Pinning lives in
`Context::schedule_dependency_asset_with_key`, the one point every non-payload dependency request
passes, so it works for any manager. The built-in managers also give a dependency the session
*before* it can start, so its own upstream lands in the same session. Stack depth uses the two
mechanisms of the superseded design: the queued manager spawns each locally queued run as its own
task, and the inline manager primes the static upstream bottom-up into the session.

Rejected:
- **Pinning inside each manager:** every external manager would have to reimplement it. The
  session hand-off (`adopt_session`, `apply_for`) has the same weakness, but it only decides which
  session a dependency's *own* upstream lands in. Pinning itself stays manager-independent.
- **The session passed through `run(payload)` and the queue entries:** changes `pub` `AssetRef::run`
  and both queues to reach the same runs.
- **A strong session reference in `AssetData`:** a pinned asset would point back to its session.
  The cycle leaks whenever a run is dropped before it finishes.
- **Refusing to evict an `Error` from "its own" session in `get_dependency_asset` (option B):**
  cannot reach volatile or uncached assets.

Carried from the superseded design:
- A task per dependency for the inline manager: no executor natively, and wasm's `spawn_local`
  breaks "finished on return".
- Priming for the queued manager too: it serialises fan-out that the queued manager runs in
  parallel.
- `stacker` (a new dependency, no wasm support); more boxing or a smaller `Value` (a constant
  factor only); a depth limit that errors (deep chains still fail).

**Residual limit:** on the inline manager, the part of a chain upstream of a dynamic or volatile
dependency is invisible to priming and still nests. It is filed during implementation as
`INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS`.

## Known-Issue Preflight

| Issue | Status | Pri | Impact | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` (design in implementation) | draft | P2 | Changes `Context` / `AssetRef` lifetimes; the session holds `AssetRef`s and is held by `Context` | No | No | Rebase on it if it lands first; the `Weak` field adds no cycle |
| `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS` | draft | P2 | `run_inline` unchanged; priming calls it once per key | No | No | None |
| `EXTERNAL-MANAGER-REPLACEMENT-SURFACE` (design, PR 118) | — | P3 | Same `AssetManager` trait; `apply_for` is one more provided method | No | No | Merge order only |
| `CANCEL-CAN-OVERTAKE-A-RETURNED-COMMAND` | draft | P3 | The spawned child changes where a cancel lands; the outcome contract is unchanged | No | No | AC-13 |
| `ENVIRONMENT-ORDERLY-SHUTDOWN` | draft | P3 | Spawned children are detached tasks, like `try_to_start_immediately`'s | No | No | None |
| `RESOURCE-STEP-VOLATILITY-IS-NEVER-CHECKED` | draft | P3 | `upstream_graph` skips volatile reads by the plan's flags; a resource step reports non-volatile, so priming could pin a volatile resource early | No | No | Priming also checks `is_volatile(key)`; AC-7 runs inline too |
| `CORE-TOKIO-REMOVAL` | accepted | P3 | New `tokio::spawn` is in native-only `DefaultAssetManager`; the session uses `tokio::sync` only | No | No | None |
| `CORE-ASSET-GC` | accepted | P3 | A session keeps pinned assets alive until its last run ends | No | No | Guide note |
| `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION`, `DEPENDENCY-ANALYSIS-REWALKS-UPSTREAM-PER-EVALUATION` | draft | P3 | Uncached results pinned per session; one more static walk per cold inline evaluation | No | No | None |
| `asset-cancellation-outcome` (design) | in_review | — | Flow D: the child finishes detached instead of being re-parked | No | No | Its tests stay green |

No blocker.

## Interfaces

New `liquers-core/src/evaluation_session.rs`, `pub(crate) mod` in `lib.rs`. It has no I/O and no
public items. The name avoids `Environment::SessionType` / `create_session`, which mean a *user*
session. The `context.rs` module doc that says `Context` holds no session is reworded to say "no
user session".

```rust
/// One top-level evaluation: each dependency identity resolves to one asset (Phase 1 rules 1-6).
pub(crate) struct EvaluationSession<E: Environment> {
    resolved: scc::HashMap<DependencyKey, Arc<tokio::sync::OnceCell<AssetRef<E>>>>,
}
pub(crate) type EvaluationSessionRef<E> = Arc<EvaluationSession<E>>;

impl<E: Environment> EvaluationSession<E> {
    pub(crate) fn new() -> EvaluationSessionRef<E>;
    /// The first call for `id` runs `resolve`; every other call, concurrent or later, gets its asset.
    /// An `Err` pins nothing. Looks up or inserts the cell, drops the map entry, then awaits
    /// `cell.get_or_try_init(resolve)`: no map lock is held across the await.
    pub(crate) async fn resolve<F, Fut>(&self, id: DependencyKey, resolve: F) -> Result<AssetRef<E>, Error>
    where F: FnOnce() -> Fut, Fut: Future<Output = Result<AssetRef<E>, Error>>;
    /// Pins an asset resolved outside `resolve` (inline priming); returns the existing pin if any.
    pub(crate) async fn pin(&self, id: DependencyKey, asset: AssetRef<E>) -> AssetRef<E>;
}
```

Both methods are async because `resolve` awaits the manager and concurrent resolvers await the
cell. No `MaybeSend` bounds: `Send` is inferred, and native callers get it. scc 3.8.8 `entry_async`
/ `or_insert_with` and tokio's `OnceCell` (feature `sync`, so wasm builds it) exist.
**Identity:** `DependencyKey::from(&query)` of the CWD-resolved query, as `context.rs` computes it
today. Priming uses `DependencyKey::from(&Query::from(key))`, the same value for a key.

`AssetData` gets `evaluation_session: std::sync::Weak<EvaluationSession<E>>` (`Weak::new()` at
construction). `AssetData` derives nothing that a `Weak` breaks. `AssetRef` gets:

```rust
pub(crate) async fn evaluation_session(&self) -> Option<EvaluationSessionRef<E>>;
/// No-op if the asset has a live session or has left None/Recipe/Submitted (rule 5).
pub(crate) async fn adopt_evaluation_session(&self, session: &EvaluationSessionRef<E>);
/// The live session, or a new one recorded on the asset (rule 1).
pub(crate) async fn evaluation_session_or_begin(&self) -> EvaluationSessionRef<E>;
```

`Context` gets `evaluation_session: EvaluationSessionRef<E>`. `Context::new` (signature unchanged)
takes it from `evaluation_session_or_begin()`; `create_context` is unchanged. The struct literals
in `with_volatile`, `clone_context`, `Clone::clone` (sharing it) and the test constructor near line
1684 (a fresh session) copy it.

`AssetManager` gets one provided method, which breaks no implementor:

```rust
/// `Context::apply`: an ad-hoc asset evaluated in `parent`'s session (rule 1). Default: `apply`.
async fn apply_for(&self, parent: &AssetRef<E>, recipe: Recipe, to: State<E::Value>,
                   payload: Option<E::Payload>) -> Result<AssetRef<E>, Error>;
```

Two existing contracts change without a signature change:
- **`get_dependency_asset` doc:** return the asset even when its evaluation failed (`Err` only when
  no asset was produced), and adopt the parent's session before the asset can start.
- **The trait-default `wait_for_dependency`:** add an `Error` arm like `DefaultAssetManager`'s
  (assets.rs ~8296). It returns `stored_error()` as `Err` instead of handing on `get()`'s `Ok`
  error-state, so a failed dependency is an `Err` to its dependent on every manager.

`plan.rs`, carried over: an optional recorder on `DependencyWalk`, filled at frame pop, and
`pub(crate) async fn upstream_graph<E: Environment>(envref: EnvRef<E>, plan: &Plan, initial_cwd: Option<Key>) -> Result<Vec<(Key, Vec<Key>)>, Error>`.
It returns, in post-order, the non-volatile `State`/`Link` reads that a recipe serves.

## Integration Points

- **`context.rs` `schedule_dependency_asset_with_key`:**
  - Wrap the `get_dependency_asset` call in `self.evaluation_session.resolve(query_dep_key.clone(), || …)`.
  - `register_scheduled_dependency` stays *before* the cell, so a cycle is reported before a
    resolver can wait on its own cell (AC-16). A recipe cycle never gets this far: the plan walk
    rejects it (AC-12).
  - The recording after the cell is idempotent.
  - The payload path is untouched.
  - `Context::apply` calls `apply_for`.
- **`interpreter.rs` `schedule_plan_dependencies_from`** (AC-15, decision 1: dependents fail as
  quickly as possible): if a scheduled dependency is already `Error`/`Cancelled`, return
  `context.wait_for_dependency(&asset)`'s `Err`. This keeps the inline manager's HEAD fail-fast,
  now that it returns failed assets. The queued manager exits only for a dependency that is already
  failed at scheduling time. Siblings already started are unaffected.
- **`DefaultAssetManager`:**
  - `get_dependency_asset`: adopt the parent's session just before `try_to_start_immediately` /
    `push_local_dependency`. Its stale eviction now runs only at a session's first resolution.
  - `drain_dependencies`, `wait_for_dependency` (direct claim): spawn the claimed run plus
    `claim.complete()` and the re-park of the child's undrained local queue (as in
    `try_to_start_immediately`), then await the handle. Slot accounting and the sequential drain
    are unchanged; `JoinError` is ignored as today.
  - `apply_for`: `apply` plus adoption.
- **`ImmediateAssetManager`:**
  - Split `get` into a private `settle(key) -> Result<Option<AssetRef<E>>, Error>` (eviction, lazy
    expiry, finished, `fast_track`), and split `get_query_asset_run` likewise into
    `settle_query(query, origin)`.
  - `get_dependency_asset`, both branches: settle, adopt the parent's session, then `run_inline`.
    On `Err`, return `Ok(asset)` if the asset's status is `Error` or `Cancelled`, and propagate the
    error otherwise (re-entry backstop, infrastructure).
  - `prime_upstream(session, plan, cwd)`: walk `upstream_graph` **top-down** from the root's
    children. Settle each key:
    - one that settles to a finished or fast-tracked asset is available, and its upstream is not
      entered (AC-11);
    - a key with `is_volatile(key)` is skipped;
    - every other key is collected.

    Then run the collected keys in post-order, each adopted and pinned. Return the first failure's
    stored error (decision 4). Settling creates the asset of a collected key without running it.
  - Top-level entries (`get`, `get_query_asset_run` with `TopLevel`, `apply`, `apply_queued`;
    `apply_immediately` forwards to `apply`) settle first. Only a root that is neither finished
    nor fast-trackable gets a session, priming, adoption and `run_inline`. A walk error skips
    priming, and the run reports it (AC-12).
  - `apply_for` = `apply` plus adoption.

## Error Handling

No new constructors. A pinned failed dependency gives its own stored error through
`wait_for_dependency`, so callers see HEAD's message (`leaf failed` in AC-1, AC-2, AC-14). A recipe
cycle is still rejected by the plan walk (`Circular dependency detected`, AC-12); a dynamic cycle
still gets `register_scheduled_dependency`'s `DependencyCycle` error, raised before the cell (AC-16). A `resolve` closure error (plan, recipe or store
lookup) propagates and pins nothing.

## Relevant Commands

No new commands, no namespace involved. Commands calling `Context::get_dependency_state`
(`liquers-lib` `records/commands.rs`, `convert.rs`, `search/mod.rs`) get AC-6's behaviour. None
re-requests after a failure.

## Concurrency

- **Joining (rule 5):** `adopt_evaluation_session` is a no-op on an asset that has a live session or
  has started, which produces rule 5's accepted limit.
- **First resolutions of one identity** serialize on its `OnceCell`; other identities are not
  blocked. On the inline manager the permit covers the dependency's whole run. A same-session
  re-request invisible to the graph waits on it, as HEAD waits in `try_claim_for_run_inline`.
- **Lifetime:** strong references are held by running `Context`s and the inline top-level frame.
  An asset still in a queue holds a `Weak`. Its session can die before it starts in three cases,
  and it then begins its own session:
  - a dependency started before its parent's `Context` exists (recipe lookup, delegation);
  - a `submit` never waited for, after its parent ends;
  - a child of a cancelled parent.

  A `Context` clone that a command keeps extends its session.
- **Queued spawn:** `Send + 'static`, the same shape as `try_to_start_immediately`. A detached
  child of a dropped parent runs without a slot, so capacity can be briefly exceeded. Today's
  re-park has the same effect.

## Documentation Architecture

| Path | Audience, area | Change |
|---|---|---|
| `reference/DEPENDENCIES_STATUS.md` | maintainers, core/assets | New "Evaluation session" section (rules 1-6); step 5 and Flow D item 2 say spawned, not inline |
| `reference/ASSETS.md` | maintainers, core/assets | §Re-evaluation: a stale dependency re-evaluates at a session's first resolution only |
| `reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` | API users, core/assets | Cache-miss text qualified by the session |
| `reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` | API users, core/context | A repeat in one evaluation returns the first asset; drop "may inline-run locally queued work"; `Context::apply` stays in the session |
| `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` | manager authors, core/assets | §11: `get_dependency_asset` contract, `apply_for`, no per-link nesting, residual limit, pinned assets live per session (`CORE-ASSET-GC`) |

`affects_docs` = these five (Phase 1 amended). Each gets a `## History` row and a `reviewed:`
bump; `specs/README.md` moves to "built" at Phase 5.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/{evaluation_session.rs (new), lib.rs, context.rs, assets.rs, interpreter.rs, plan.rs}` |
| Crates and workflows | `liquers-core`; `liquers-web` (wasm loop), `liquers-lib`, `liquers-axum` (its direct `manager.get` calls begin sessions, per rule 1), `liquers-py` by inheritance |
| Existing tests likely to change | `dependency_audit_integration.rs`: two "≥ 2" assertions tightened; inline tests that expect a failed dependency's `Err` keep the same message through the early exit; `test_get_dependency_skips_stale_error_cached_asset` / `…cancelled…` stay valid (first resolution) |
| New validation | Phase 3 tests for AC-1 to AC-15; a unit test that a session drops when its last run ends |
| Compatibility | No `pub` signature change; one provided trait method; the default `wait_for_dependency` turns an `Error` dependency into `Err` (it already did so for `Cancelled`) |
| Performance | One map lookup per dependency request; spawn instead of inline await; one static walk per cold inline evaluation |
| Data / security | None: nothing persisted changes, and no input reaches new code paths beyond existing queries |
| Recovery | Three separable groups: session + pinning; queued spawn; inline priming |
| Certainty | High for pinning and the queued spawn; medium-high for inline priming and the lifetime edge cases |
