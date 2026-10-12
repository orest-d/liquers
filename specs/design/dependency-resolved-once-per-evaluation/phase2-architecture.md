# Phase 2: Solution & Architecture — dependency-resolved-once-per-evaluation

## Overview

An `EvaluationSession` maps each dependency identity to one asset. The session is held strongly
by the `Context` of every run in it, and only weakly by the assets scheduled into it. Pinning lives
in `Context::schedule_dependency_asset_with_key`, the one point every non-payload dependency request
goes through, so it works for any manager. The two built-in managers also hand the session to a
dependency *before* it can start, so the dependency's own upstream lands in the same session.
Stack depth (the two mechanisms of the superseded design) rides on this: the queued manager spawns
each locally queued run as its own task, and the inline manager primes the static upstream
bottom-up into the session before the top-level run.

Rejected:
- **Pinning inside each manager** (`get_dependency_asset` reads a session): every external manager
  would have to reimplement it, and `MinimalInlineAssetManager` would silently ignore it.
- **The session passed through `run(payload)` and the job queue entries:** this changes `pub`
  `AssetRef::run` and both queues, while the asset field below reaches the same runs.
- **A strong session reference in `AssetData`:** a pinned asset would point back to the session
  that holds it. That cycle would leak whenever a run is dropped before it finishes.
- **`get_dependency_asset` refusing to evict an `Error` from "its own" session (option B):** it
  cannot reach volatile or uncached assets.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS` | draft | P2 | `run_inline` is unchanged; priming calls it once per key | No | No | None |
| `CORE-TOKIO-REMOVAL` | draft | P3 | The new `tokio::spawn` is in the native-only `DefaultAssetManager`; the session uses only `tokio::sync`, which wasm already builds | No | No | None |
| `CORE-ASSET-GC` | draft | P3 | A session keeps its pinned assets alive until its last run ends: bounded by one request | No | No | Note in guide |
| `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` | draft | P3 | Uncached results become pinned per session (rule 3); the race is in the store copy, not resolution | No | No | None |
| `DEPENDENCY-ANALYSIS-REWALKS-UPSTREAM-PER-EVALUATION` | draft | P3 | Priming adds one more static walk per cold inline top-level evaluation | No | No | None |
| `asset-cancellation-outcome` (design) | in_review | — | Flow D re-checked by AC-13; a spawned child now finishes detached instead of being re-parked | No | No | Keep its tests green |

No blocker.

## Interfaces

**New module** `liquers-core/src/evaluation_session.rs`, declared `pub(crate) mod` in `lib.rs`.
It holds no I/O and has no public items.

```rust
/// One top-level evaluation: each dependency identity resolves to one asset (rules 2-6).
pub(crate) struct EvaluationSession<E: Environment> {
    resolved: scc::HashMap<DependencyKey, Arc<tokio::sync::OnceCell<AssetRef<E>>>>,
}
pub(crate) type SessionRef<E> = Arc<EvaluationSession<E>>;

impl<E: Environment> EvaluationSession<E> {
    pub(crate) fn new() -> SessionRef<E>;
    /// The first call for `id` runs `resolve`, and every other call, concurrent or later, gets its
    /// asset. An `Err` from `resolve` pins nothing, and the next call resolves again.
    pub(crate) async fn resolve<F, Fut>(&self, id: DependencyKey, resolve: F) -> Result<AssetRef<E>, Error>
    where F: FnOnce() -> Fut, Fut: Future<Output = Result<AssetRef<E>, Error>>;
    /// Pins an asset resolved outside `resolve` (inline priming); returns the pinned one if
    /// `id` was already pinned.
    pub(crate) async fn pin(&self, id: DependencyKey, asset: AssetRef<E>) -> AssetRef<E>;
}
```

Async, because `resolve` awaits the manager and concurrent resolvers wait on the cell.
`tokio::sync::OnceCell::get_or_try_init` gives first-wins semantics without holding a lock across
the manager call. `scc::HashMap` is already used for asset maps.

**`AssetData` / `AssetRef`** (`liquers-core/src/assets.rs`): a new field
`session: std::sync::Weak<EvaluationSession<E>>`, initially `Weak::new()`, and three methods:

```rust
impl<E: Environment> AssetRef<E> {
    /// The session this asset was scheduled into, if it is still alive.
    pub(crate) async fn session(&self) -> Option<SessionRef<E>>;
    /// Joins `session` unless the asset already belongs to a live session or has started
    /// running (rule 5). Called by a manager before the asset can start.
    pub(crate) async fn adopt_session(&self, session: &SessionRef<E>);
    /// The live session, or a new one recorded on the asset (rule 1). Used by `create_context`.
    pub(crate) async fn session_or_begin(&self) -> SessionRef<E>;
}
```

**`Context`** (`liquers-core/src/context.rs`): a new field `session: SessionRef<E>`, shared by the
context's clones like `submitted_dependencies`. `Context::new` (signature unchanged) takes it from
`assetref.session_or_begin()`. The test-only constructor near line 1684 gets a fresh session.

**`AssetManager` trait:** one new provided method, so no implementor breaks:

```rust
/// `Context::apply`: evaluate an ad-hoc asset inside `parent`'s session (rule 1).
/// Default: `apply`, which runs it in a session of its own.
async fn apply_for(&self, parent: &AssetRef<E>, recipe: Recipe, to: State<E::Value>,
                   payload: Option<E::Payload>) -> Result<AssetRef<E>, Error>;
```

The contract of `get_dependency_asset` gains two sentences in its doc comment:
- Return the asset even when evaluating it failed, because the failure is recorded in the asset.
  Return `Err` only when no asset could be produced.
- Adopt the parent's session before the asset can start.

Neither is a signature change. A manager that ignores them keeps today's behaviour for its own
dependencies.

**`plan.rs`:** taken over from the superseded design unchanged. An optional recorder on
`DependencyWalk`, filled at frame pop, and
`pub(crate) async fn upstream_graph<E: Environment>(envref: EnvRef<E>, plan: &Plan, initial_cwd: Option<Key>) -> Result<Vec<(Key, Vec<Key>)>, Error>`
(post-order, non-volatile `State`/`Link` reads served by a recipe).

No new `pub` type, no command, no value type, no format.

## Integration Points

- **`context.rs` `schedule_dependency_asset_with_key`:** replace the direct
  `manager.get_dependency_asset(...)` with
  `self.session.resolve(query_dep_key.clone(), || manager.get_dependency_asset(&self.assetref, &query))`.
  The edge registration (`register_scheduled_dependency`) stays *before* it, so a cycle is reported
  before a resolver can wait on its own cell (AC-12). `add_dependency` and `add_dependent_asset`
  stay after it and are idempotent. The payload path (`schedule_payload_dependency_asset`) is not
  touched (non-goal).
- **`context.rs` `apply`:** call `apply_for(&self.assetref, …)`.
- **`interpreter.rs` `schedule_plan_dependencies_from`:** after each `schedule_dependency_asset`,
  if the returned asset is already `Error` or `Cancelled`, return `context.wait_for_dependency(&asset)`'s
  error. This stops the pre-pass at the first failed dependency: the inline manager stops failing
  fast with `Err` (below), and the queued manager gains the same early exit.
- **`assets.rs` `AssetRef::create_context`:** uses `session_or_begin`. `run_with_future` and
  `run_with_future_inline` are unchanged: the session dies with the last `Context` that holds it.
- **`DefaultAssetManager::get_dependency_asset`:** `asset.adopt_session(&s)` for the parent's live
  session, just before `try_to_start_immediately` / `push_local_dependency`. The stale-terminal
  eviction stays; it now runs only on a session's first resolution.
- **`DefaultAssetManager::drain_dependencies`, `wait_for_dependency` (direct claim):** replace
  `Box::pin(dep.run(None)).await; claim.complete()` with a `tokio::spawn` of both and await the
  handle. Slot accounting and the sequential drain are unchanged. A dropped dependent detaches the
  child, which then finishes on its own (Flow D). A `JoinError` is ignored as today's `let _` is.
- **`DefaultAssetManager::apply_for`, `ImmediateAssetManager::apply_for`:** as `apply`, plus
  `adopt_session` before the run.
- **`ImmediateAssetManager`:**
  - Split `get` into a private `settle(&self, key) -> Result<Option<AssetRef<E>>, Error>` (eviction,
    lazy expiry, finished, `fast_track`) and a private `get_unprimed(key)`.
  - `get_dependency_asset` uses settle → `adopt_session` → `run_inline`, and returns the asset if
    the run finished in `Error`/`Cancelled`. It no longer calls `self.get`.
  - A private `prime_upstream(&self, session, plan, cwd) -> Result<(), Error>`: walk
    `upstream_graph` from the root's children. A key that is finished or fast-trackable is available
    and not entered. Every other key is settled, adopted, pinned (`session.pin`) and run in
    post-order. Return the first failure's error (decision 4).
  - The top-level entries (`get`, `get_query_asset_run` with `TopLevel`, `apply`) create the session,
    prime, adopt it on the root asset and run it. A walk error skips priming, and the normal run
    reports it (AC-12).

## Error Handling

No new error constructors. A pinned failed dependency yields its own stored error through
`wait_for_dependency` (unchanged), so callers see the same message as at HEAD. That is
`leaf failed` in AC-1, AC-2 and AC-14. Cycle errors stay `register_scheduled_dependency`'s. A
`resolve` closure error (plan, recipe or store lookup) propagates unchanged and pins nothing.
`JoinError` from a spawned run is ignored; the claim's drop re-parks the dependency, as today.

## Relevant Commands

No new commands, and no namespace is involved (all of this is below the command layer). Commands
that call `Context::get_dependency_state` (`liquers-lib` `records/commands.rs`, `convert.rs`,
`search/mod.rs`) see AC-6's behaviour: a second request in the same evaluation returns the first
asset. None of them re-requests after a failure.

## Concurrency

- **Rule 5 (joining):** `adopt_session` is a no-op on an asset that has a live session or has left
  the pre-run states (`None`, `Recipe`, `Submitted`).
- **Two first resolutions at once:** these serialize on the identity's `OnceCell`. Resolvers of
  *other* identities are not blocked.
- **The session's lifetime:** strong references are held only by running `Context`s and by the
  inline top-level frame while it primes. An asset parked in a queue holds a `Weak`. If its
  session dies before the asset starts (its parent was cancelled), the run begins a new session.
- **The queued spawn:** it needs `Send + 'static` on the claimed run. That holds, because
  `DefaultAssetManager` already spawns the same future in `try_to_start_immediately`.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/DEPENDENCIES_STATUS.md` | reference | New section "Evaluation session" (rules 1-6); fix step 5 and Flow D item 2 (spawned, not inline) |
| `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` | reference | `submit` / `get_dependency_state`: a repeat in one evaluation returns the first asset; drop "may inline-run locally queued work"; `Context::apply` stays in the session |
| `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` | guide | §11: the `get_dependency_asset` contract (return a failed asset, adopt the session), `apply_for`, no per-link nesting, the residual limit |

`affects_docs`: the three above (unchanged from Phase 1). `specs/README.md`: the map line moves to
"built" at Phase 5. Each document gets a `## History` row and a `reviewed:` bump.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/{evaluation_session.rs (new), lib.rs, context.rs, assets.rs, interpreter.rs, plan.rs}` |
| Crates and workflows affected | `liquers-core`; `liquers-web` (inline manager, wasm loop), `liquers-lib`, `liquers-axum`, `liquers-py` by inheritance only |
| Existing tests likely to change | `dependency_audit_integration.rs` `restart_upstream_command_change_on_load_refuses` and `restart_deep_expired_status_on_load_refuses` (tighten "≥ 2"); `test_get_dependency_skips_stale_error_cached_asset` / `…_cancelled_…` stay valid (no session: first resolution); inline tests that expect a failed dependency's `Err` from the pre-pass now get it through the early exit, with the same message |
| New validation | Phase 3 tests for AC-1 to AC-14; a unit test that a session is dropped when its last run ends (`Weak::upgrade` is `None`) |
| Compatibility | No `pub` signature changes; one provided trait method. Behaviour changes are AC-6/AC-7 (one asset per dependency per evaluation) |
| Concurrency | `OnceCell` per identity; cycle check before the cell; spawned runs keep the existing slot accounting (`dependency_scheduling`, `asset_cancellation` stay green) |
| Performance | One map lookup per dependency request; the queued spawn replaces an inline await; one extra static walk per cold inline top-level evaluation |
| Memory | Pinned assets live until the session's last run ends; there are no reference cycles (the field is `Weak`) |
| Recovery | Three separable groups: session + pinning; queued spawn; inline priming. Each reverts alone |
| Certainty | High for pinning (probe-verified at the per-asset scope), high for the queued spawn, medium-high for inline priming and the `Weak` lifetime |
