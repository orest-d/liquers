# Phase 2: Solution & Architecture — async-wasm-refactor (ImmediateAssetManager)

## Overview

Introduce `ImmediateAssetManager<E>` — a parallel, spawn-free, timer-free implementation of the `AssetManager<E>` trait in which every evaluation happens **inline** (awaited to completion inside `get_asset`/`get`/`apply` before they return), with no `JobQueue`, no expiration-monitor task, and no per-asset background loops. To make the manager selectable, `Environment` gains an associated type `type AssetManager: AssetManager<Self>`; to make `AssetRef`'s shared evaluation machinery usable without `tokio::spawn`, `AssetData` gains an explicit runtime `EvalMode { Queued, Inline }` that drives the three shared-path forks (service-message loop, metadata persistence, cancel timeout) via exhaustive matches — a first-class runtime mode compiled and testable on **all** targets, never behind `cfg`.

Research grounding (Phase 1 + code audit): all `tokio::spawn`/`tokio::time` sites in `liquers-core` fall into two groups. **JobQueue-owned** (avoided entirely by this design): manager-constructor spawns (`assets.rs:2480,2484`), job-start spawn (`assets.rs:3985`), claim-repair spawn (`assets.rs:3820`), monitor timer (`assets.rs:2546`). **Shared-path** (handled by `EvalMode`): psm spawn in `run_with_future` (`assets.rs:1440`), psm spawn in `new_temporary` (`assets.rs:943`, called from `interpreter.rs:355`), `MetadataSaver` debounce spawn + `sleep` + `std::time::Instant` (`assets.rs:187,206` — `Instant::now()` **panics** on wasm32-unknown-unknown), cancel timeout (`assets.rs:1790`). Environment-init spawns (`context.rs:605,732`, `liquers-lib/src/environment.rs:150`) are avoided by lazy start in the immediate manager.

**No `cfg` is used for behavior anywhere in this design.** The only wasm-conditional compilation remains the existing `Cargo.toml` target-gated tokio features. The Tier-2 `MaybeSend` work stays deferred; nothing here blocks it.

## Data Structures

### New Structs

#### `ImmediateAssetManager<E: Environment>` (new file `liquers-core/src/assets_immediate.rs`, re-exported from `assets`)

```rust
pub struct ImmediateAssetManager<E: Environment> {
    /// Monotonic asset id source (same convention as DefaultAssetManager, starts at 1000)
    id: std::sync::atomic::AtomicU64,
    /// Back-reference to the environment; set once by set_envref
    envref: std::sync::OnceLock<EnvRef<E>>,
    /// Key-addressed assets (same concurrent map as DefaultAssetManager; scc compiles on wasm)
    assets: scc::HashMap<Key, AssetRef<E>>,
    /// Query-addressed assets
    query_assets: scc::HashMap<Query, AssetRef<E>>,
    /// Reused as-is from dependencies.rs — it contains no spawns or timers
    dependency_manager: crate::dependencies::DependencyManager<E>,
    /// Lazy one-shot command-version loading (replaces the init-time tokio::spawn)
    started: tokio::sync::OnceCell<()>,
    /// Same retry policy as DefaultAssetManager
    max_dependency_retries: u32,
}
```

- **Ownership:** held as `Arc<ImmediateAssetManager<E>>` by the environment (no `Box` — the existing `Arc<Box<…>>` double indirection is dropped for both managers, see Integration Points).
- **No `JoinHandle`s, no channels, no monitor** — nothing to shut down; no `Drop` impl needed.
- **Serialization:** none (runtime object), same as `DefaultAssetManager`.
- **Send/Sync:** satisfied naturally on all targets (Tier 1 keeps all `Send` bounds).

### New Enums

#### `EvalMode` (in `assets.rs`, on `AssetData`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalMode {
    /// Background evaluation via JobQueue; psm loop runs in a spawned task;
    /// metadata persistence is debounced; cancel uses a 5s timeout. (Current behavior.)
    Queued,
    /// Inline evaluation in the caller's task; psm loop joined in the same task;
    /// metadata persisted directly (no debounce task, no Instant); cancel completes directly.
    Inline,
}
```

- Stored as a new field `eval_mode: EvalMode` on `AssetData<E>` (`assets.rs:229`), set at asset creation by whichever manager creates the asset. Default `Queued` in existing constructors so `DefaultAssetManager` behavior is byte-for-byte unchanged.
- Every consumer **matches exhaustively** (no `_ =>` arm) per the project convention, so adding a mode later is a compile error.
- Rationale vs. duplicating `AssetRef`: `AssetRef`/`AssetData` (~2000 lines of status/notification/persistence logic) is shared infrastructure both managers reuse; the b1 isolation boundary is the **manager**, and `EvalMode` confines the three unavoidable shared-path forks to explicit, natively-testable matches.

### ExtValue Extensions

None. No value-type changes.

## Trait Implementations

### Trait: `AssetManager<E>` — extended (assets.rs:2248)

The trait grows the methods that `AssetRef`, `Context`, and the interpreter currently reach via the **concrete** `DefaultAssetManager` (audit: `assets.rs:513,853,1586,1904,1976`; `interpreter.rs:51,355`; `context.rs:603,730`). Existing method set is unchanged. New required/default methods:

```rust
#[async_trait]
pub trait AssetManager<E: Environment>: Send + Sync {
    // ... existing methods unchanged (get_asset, apply, apply_immediately, get,
    //     recipe_opt, is_volatile, get_dependency_asset, drain_dependencies,
    //     wait_for_dependency, remove, remove_asset, set_binary, set_state,
    //     get_asset_info, contains, keys, listdir*, makedir) ...

    /// Set the environment back-reference. Called once from Environment::init_with_envref.
    fn set_envref(&self, envref: EnvRef<E>);

    /// Access the dependency manager (shared component; both impls own one).
    fn dependency_manager(&self) -> &crate::dependencies::DependencyManager<E>;

    /// One-time async startup work (command-version registration). Idempotent.
    /// DefaultAssetManager: called eagerly via spawn from init_with_envref (as today).
    /// ImmediateAssetManager: called lazily from get_asset/get/apply (OnceCell).
    async fn start(&self);

    /// Schedule expiration tracking for a Ready asset.
    /// DefaultAssetManager: sends to the monitor task (current track_expiration).
    /// ImmediateAssetManager: no-op — expiration is checked lazily on access.
    fn track_expiration(&self, asset_ref: &AssetRef<E>, expiration_time: &ExpirationTime);

    /// Cascade-expire all dependents of a changed dependency key.
    async fn cascade_expire_dependents(&self, dep_key: &crate::metadata::DependencyKey);

    /// Apply an ExpiredDependents result: expire keyed and untracked assets.
    async fn expire_dependencies_result(&self, expired: crate::dependencies::ExpiredDependents<E>);

    /// Register plan dependencies into the dependency manager.
    async fn register_plan_dependencies(
        &self,
        dependent_key: &Key,
        plan_deps: &[crate::dependencies::PlanDependency],
    ) -> Result<(), Error>;

    /// Create a temporary (non-addressable) asset with this manager's EvalMode.
    /// Replaces the direct AssetRef::new_temporary call in interpreter.rs:355 —
    /// the manager owns the eval-mode policy.
    fn create_temporary_asset(&self) -> AssetRef<E>;
}
```

Notes:
- `cascade_expire_dependents`, `expire_dependencies_result`, `register_plan_dependencies` have **identical bodies** for both managers (they only touch `dependency_manager()` + the asset maps). They are implemented per-manager in Tier 1 (the maps are private fields); extracting a shared helper is a Phase-4 refactor option, not an architectural requirement.
- `dependency_manager()` and several of these were `pub(crate)`; lifting them into the public trait widens the API surface. Accepted: they are the minimal surface `AssetRef`/interpreter actually need, and both managers must provide them.
- Object safety is preserved (no generic methods added); the associated-type route below doesn't require `dyn`, but keeping the trait object-safe leaves `Arc<dyn AssetManager<E>>` usable for tests/tools.

### Trait: `Environment` — generalized manager (context.rs:43)

```rust
pub trait Environment: Sized + Sync + Send + 'static {
    type Value: ValueInterface;
    type CommandExecutor: CommandExecutor<Self>;
    type SessionType: Session;
    type Payload: crate::commands::PayloadType;
    /// NEW: the asset manager implementation this environment uses.
    type AssetManager: AssetManager<Self>;

    /// CHANGED: was `Arc<Box<DefaultAssetManager<Self>>>`.
    fn get_asset_manager(&self) -> Arc<Self::AssetManager>;
    // ... everything else unchanged ...
}
```

- Associated type (zero-cost, monomorphized) over `Arc<dyn …>`: matches the house style (`type Value`, `type CommandExecutor`), avoids dynamic dispatch on the hot asset path, and sidesteps `dyn`-compatibility risk entirely.
- The `Arc<Box<…>>` wart is removed in the same change (pure win; `Arc<T>` suffices).
- `EnvRef::get_asset_manager` mirror changes to `Arc<E::AssetManager>` (context.rs:97).

### Trait: `AssetManager<E> for ImmediateAssetManager<E>` — semantics

- **`get_asset(query)`**: check `query_assets` for a cached asset → if `Ready` and **not lazily-expired** (see below), return it. Otherwise create an `AssetRef` with `EvalMode::Inline`, insert into the map, **await `run_inline()` to completion**, then return. The returned asset is always in a finished state (`Ready` or `Error`); callers' subsequent `.get().await` resolves immediately. This satisfies the existing trait contract (which never promised *when* evaluation happens).
- **`get(key)`**: same pattern keyed by `Key` (recipe lookup / store load as in `DefaultAssetManager`, minus submission).
- **`apply` / `apply_immediately`**: build the temporary/recipe asset with `EvalMode::Inline`, await `run_inline()` / `run_immediately_inline(payload)`, return.
- **Recursive dependencies**: a command evaluating inline requests dependencies via `Context` → `get_dependency_asset` (trait default → `get_asset`) → recursive inline evaluation. The recursion is async-recursive → `get_asset`'s body is `Box::pin`ned. Cycle protection: the existing `Context` dependency cycle check applies unchanged (Phase 3 must include a cycle test in inline mode).
- **`wait_for_dependency`** (trait default): dependency is already finished when it returns — the default implementation is correct as-is; no override needed. **`drain_dependencies`**: trait default no-op is correct (no local queues). **`get_dependency_asset`**: trait default (plain `get_asset`) is correct.
- **Lazy expiration (replaces the monitor):** on each cache hit in `get_asset`/`get`, if the cached asset is `Ready`, compute `expiration_time()` from its metadata; if expired → `expire_without_cascade()` + `remove_expired_from_maps`-equivalent + re-evaluate inline. `track_expiration` is a **no-op**; correctness relies on check-on-access. Consequence (documented behavior difference): an expired asset is observed as expired only at next access, and cascade expiration triggered *by the clock* does not happen — cascades still run when a dependency is re-evaluated (via `cascade_expire_dependents` on version change, same as today).
- **`start()`**: `self.started.get_or_init(|| load_command_versions_body)` — same registration loop as `DefaultAssetManager::load_command_versions` (assets.rs:2688), executed lazily on first use; no environment-init spawn required.
- **Concurrency stance:** the manager is safe under concurrent native use (scc maps + inline awaits), but its *scheduling* is FIFO-per-caller: two concurrent callers evaluating the same query may both find no cached Ready asset. Mitigation: same-`AssetRef` reuse — second caller finds the (unfinished, `EvalMode::Inline`) asset in the map and awaits `asset.get()` instead of starting a second run. `run_inline` must therefore claim via the existing status transition (`try_claim_for_run`-equivalent check: only run if not already running/finished).

## Generic Parameters & Bounds

- `ImmediateAssetManager<E: Environment>` — single parameter, same as `DefaultAssetManager<E>`. No extra bounds: `Send + Sync` for the trait come from field types (`scc::HashMap`, `OnceLock`, `OnceCell` are all `Send + Sync` given `E: Environment`).
- `Environment::AssetManager: AssetManager<Self>` — F-bounded like the existing `CommandExecutor<Self>`; proven pattern in this codebase.
- No new lifetimes; `dependency_manager()` returns `&DependencyManager<E>` tied to `&self` (same as today's `pub(crate)` accessor).

## Sync vs Async Decisions

- **All manager methods stay async** (trait unchanged in this respect) — inline evaluation *awaits* command futures; async is mandatory, there is no blocking.
- `set_envref`, `track_expiration`, `create_temporary_asset` are **sync** (match current concrete signatures; they only touch sync state / send on unbounded channels).
- **`run_inline` replaces `tokio::spawn` with `futures::join!`** (single-task structured concurrency):

```rust
/// AssetRef: single-task variant of run_with_future (assets.rs:1430).
/// psm runs as a joined future, not a spawned task. JobFinishing terminates it.
async fn run_with_future_inline<Fut>(&self, evaluate_future: Fut) -> Result<(), Error>
where Fut: Future<Output = Result<(), Error>>;
pub(crate) async fn run_inline(&self) -> Result<(), Error>;
pub(crate) async fn run_immediately_inline(&self, payload: Option<E::Payload>) -> Result<(), Error>;
```

  Shape: `join!(process_service_messages(), async { let r = select!(wait_to_finish, evaluate); finalize_primary_progress(); send(JobFinishing); r })`, then `finish_run_with_result`. `join!` only completes when *both* futures finish, so this is sound **iff** the psm loop terminates on `JobFinishing` — which the existing code guarantees (see the comment at assets.rs:1413: the psm loop "has already terminated via the JobFinishing message sent in run_with_future"). Phase 4 must re-verify this invariant with a test that would hang on regression (bounded by the test harness timeout). `tokio::select!`/`futures::join!` are executor-agnostic (no reactor needed) — they work on wasm. `finish_run_with_result` currently takes `Result<Result<(),Error>, JoinError>` (assets.rs:1328); it is refactored to take `Result<(), Error>` for the psm outcome, with the `Queued` caller mapping `JoinError` → `Error` at the call site (small seam, Phase 4).
- **`MetadataSaver`** (assets.rs:145): `MetadataSaver` itself has no view of the asset — the mode is **passed as a parameter**: `save_immediately(metadata, key, envref, eval_mode: EvalMode)`. Its only caller is `AssetData::save_metadata` (assets.rs:373), and `AssetData` owns the new `eval_mode` field, so the plumbing is one argument. `Queued` → current debounced spawn path; `Inline` → direct `store.set_metadata(...).await` (no spawn, no `sleep`, **no `std::time::Instant`** — removing the wasm panic hazard from the inline path).
- **Cancel** (assets.rs:1790): the 5s `tokio::time::timeout` applies only in `Queued` mode; `Inline` matches to a direct await (in single-threaded inline execution, `cancel` cannot race a running evaluation — by the time a caller can invoke it, the run has completed).
- Sync wrappers: none needed; Python bindings interact through the unchanged async trait surface.

## Function Signatures

### Module: `liquers_core::assets_immediate` (new)

```rust
impl<E: Environment> ImmediateAssetManager<E> {
    pub fn new() -> Self;                          // no spawns — safe in any context
    // Default impl mirrors new()
}
impl<E: Environment> Default for ImmediateAssetManager<E> { fn default() -> Self; }
#[async_trait]
impl<E: Environment> AssetManager<E> for ImmediateAssetManager<E> { /* full trait */ }
```

### Module: `liquers_core::assets` (changes)

```rust
pub enum EvalMode { Queued, Inline }                         // new
impl<E: Environment> AssetData<E> {
    // existing constructors default to EvalMode::Queued;
    pub(crate) fn with_eval_mode(self, mode: EvalMode) -> Self;   // builder-style setter
}
impl<E: Environment + 'static> AssetRef<E> {
    pub(crate) async fn run_inline(&self) -> Result<(), Error>;
    pub(crate) async fn run_immediately_inline(&self, payload: Option<E::Payload>) -> Result<(), Error>;
    pub fn new_temporary_with_mode(envref: EnvRef<E>, mode: EvalMode) -> Self;  // Inline: no psm spawn
}
// DefaultAssetManager: existing inherent methods become trait-impl methods
// (set_envref, dependency_manager, start [= load_command_versions], track_expiration,
//  cascade_expire_dependents, expire_dependencies_result, register_plan_dependencies,
//  create_temporary_asset). Bodies unchanged; `load_command_versions` retained as a
//  deprecated alias delegating to start() if external callers exist.
```

### Module: `liquers_core::context` (changes)

```rust
pub trait Environment {                       // context.rs:43
    type AssetManager: AssetManager<Self>;    // NEW
    fn get_asset_manager(&self) -> Arc<Self::AssetManager>;   // CHANGED return type
}
impl<E: Environment> EnvRef<E> {
    pub fn get_asset_manager(&self) -> Arc<E::AssetManager>;  // CHANGED (context.rs:97)
}
// SimpleEnvironment / SimpleEnvironmentWithPayload: type AssetManager = DefaultAssetManager<Self>;
// field becomes Arc<DefaultAssetManager<Self>> (Box removed).

/// NEW: single-threaded environment preconfigured with ImmediateAssetManager.
/// Mirrors SimpleEnvironmentWithPayload's configuration surface.
pub struct ImmediateEnvironment<V: ValueInterface, P: PayloadType = ()> { /* same fields, ImmediateAssetManager */ }
impl Environment for ImmediateEnvironment<V, P> {
    type AssetManager = ImmediateAssetManager<Self>;
    fn init_with_envref(&self, envref: EnvRef<Self>) {
        self.get_asset_manager().set_envref(envref);   // NO spawn; start() is lazy
    }
    // rest mirrors SimpleEnvironmentWithPayload
}
```

(If `PayloadType` has no existing `()` impl, `ImmediateEnvironment` takes both parameters explicitly like `SimpleEnvironmentWithPayload` — cosmetic, resolved in Phase 4.)

### Module: `liquers_core::interpreter` (changes)

```rust
// interpreter.rs:355 — was: AssetRef::new_temporary(envref.clone())
let assetref = envref.get_asset_manager().create_temporary_asset();
// interpreter.rs:51 — unchanged call, now resolves via trait method
```

## Integration Points

### Crate: liquers-core
- `assets.rs`: trait extension; `EvalMode` + exhaustive matches at the three forks (`run_with_future`→ split, `MetadataSaver::save_immediately`, `cancel`); `DefaultAssetManager` inherent → trait methods; `new_temporary_with_mode`.
- `assets_immediate.rs` (new): `ImmediateAssetManager`.
- `context.rs`: `Environment::AssetManager` associated type; `EnvRef` mirror; `SimpleEnvironment`/`SimpleEnvironmentWithPayload` conformance (one line + field type); new `ImmediateEnvironment`.
- `interpreter.rs`: temp-asset creation via manager (line 355); `register_plan_dependencies` via trait (line 51 — call site unchanged).
- `dependencies.rs`: **no changes** (audited: no spawns, no timers).

### Crate: liquers-lib
- `environment.rs:25,47,106,150`: add `type AssetManager = DefaultAssetManager<Self>;`, drop `Box` from the field/return, keep the init spawn. Mechanical.

### Crate: liquers-axum
- `assets/handlers.rs`, `assets/websocket.rs`: call only trait methods on the result of `get_asset_manager()` — **no changes** beyond recompilation.

### Crate: liquers-py
- `context.rs:102`: same one-line conformance as liquers-lib (`type AssetManager = DefaultAssetManager<Self>;` + return type). Python-visible API unchanged (managers are not exposed to Python directly).

### Dependencies
- **No new dependencies.** `futures` (already a core dep, `async_store` feature) provides `join!`. No `wasm-bindgen-futures`/`gloo-timers` needed for Tier 1 — the rt shim from Phase 1 is **not required** because the immediate path contains no spawns and no timers at all (a stronger result than shimming them).

## Relevant Commands

### New Commands
None. This is core infrastructure; no command signatures are added or changed, and `register_command!` output is unaffected.

### Relevant Existing Namespaces
None affected. All existing command namespaces (`lui`, `pl`, root) run unchanged on either manager — commands never see the manager type. (Flagged to the user at the Phase 2 gate for confirmation.)

## Error Handling

- All fallible paths return `liquers_core::error::Error` via typed constructors (`Error::general_error`, `Error::key_not_found`, …); no new error types; no `Error::new`.
- `run_inline` propagates evaluation errors identically to `run` (`finish_run_with_result` unchanged semantics; asset ends in `Status::Error` with the error recorded in metadata).
- `ImmediateAssetManager::get_asset` returns `Err` only for pre-evaluation failures (parse/recipe resolution), matching `DefaultAssetManager`; evaluation failures surface through the returned asset's status/`.get()`, preserving the existing failure contract (`liquers-core/tests/asset_failure_contract.rs` must pass parameterized over both managers).
- The existing `set_envref` double-set `panic!` (assets.rs:2674) is retained on both impls for now (pre-existing behavior; changing it to `Result` would ripple through `Environment::init_with_envref` — noted as out of scope, tracked in ISSUES if desired).
- No `unwrap()`/`expect()` outside tests.

## Testing Strategy (preview for Phase 3)

- **Manager-parametric suite:** existing trait-surface tests (`asset_failure_contract.rs`, volatility, dependency-manager integration, the `assets.rs` mod-tests exercising `get_asset`/`apply`/`apply_immediately`) parameterized over `SimpleEnvironment` (Default) and `ImmediateEnvironment` (Immediate) via a small generic harness.
- **DefaultAssetManager-only:** queue capacity, parking/local-dependency drain, in-flight cancellation, monitor-driven expiration.
- **Immediate-only:** inline recursion depth, cycle detection under inline recursion, lazy-expiration-on-access, no-spawn invariant (runs under a plain `futures::executor::block_on` with **no tokio runtime** — the strongest proof of browser-readiness on native CI).

## Open Questions (carried to Phase 3/4)

1. Shared helper extraction for the three identical trait-method bodies (`cascade_expire_dependents` etc.) — Phase 4 refactor decision.
2. `load_command_versions` external callers: keep as deprecated alias or rename to `start()` outright (grep at Phase 4 time).
3. `ImmediateEnvironment` payload-parameter ergonomics (default `()` vs explicit) — Phase 4 cosmetic.
