# Phase 2: Solution & Architecture — async-wasm-refactor (complete: both blockers)

## Overview

Complete Tier-1 **and** Tier-2 solution, hybrid per the approved strategy: **specialization (b1) for Axis 1 (runtime behavior)** and **target-gated conditional compilation for Axis 2 (the `Send` bound)**. Axis 1 adds `ImmediateAssetManager<E>` — a spawn-free, timer-free `AssetManager` whose evaluations run inline — and makes the manager selectable via `Environment::AssetManager`. Axis 2 adds a `maybe_send` module (`MaybeSend`/`MaybeSync` marker aliases + a cfg'd `BoxFuture` type alias) and switches the five core async traits to `#[async_trait(?Send)]` on `wasm32`, relaxing their supertrait bounds and every explicit `+ Send` future/closure bound in core and in the `register_command!` macro. On `wasm32` the threaded machinery (`DefaultAssetManager`, `JobQueue`) is `#[cfg(not(target_arch = "wasm32"))]` and simply absent; `ImmediateAssetManager` is the only manager compiled.

**Acceptance criterion (from the user):** `liquers-lib/examples-web/ui_spec_demo` runs in a real browser and passes a Playwright e2e test (the deferred M4 goal in `specs/webui/DESIGN.md`). The design is organized so that this specific chain — `mount_web` → `DefaultEnvironment` (wasm) → `ImmediateAssetManager` → inline `evaluate`/`evaluate_immediately` → `ui::web` render — compiles and runs with no `tokio::spawn` and no `Send` violation.

**Why both axes are needed together.** Axis 1 alone makes core *run* on wasm for `Send`-satisfiable data, but the moment Axis 2 relaxes `E`'s `Send` bound, `tokio::spawn` in `DefaultAssetManager` no longer *compiles* on wasm (its signature requires `F: Send`, and `AssetRef<E>` is not `Send` when `E: MaybeSend`). So the two axes are co-dependent on wasm: Axis 2 forces the threaded manager out of the wasm build, and Axis 1 supplies the replacement. b1's clean type-level split is what makes this a single `#[cfg]` boundary rather than cfg scattered through a type.

**Conditional compilation is minimized and purposeful.** `EvalMode` (Axis 1) stays a runtime enum, testable on all targets. The `cfg` used is: (i) target-gating the threaded manager (`DefaultAssetManager`/`JobQueue`) out of wasm; (ii) the `maybe_send` aliases (attribute/type level); and (iii) method/arm-level `#[cfg(not(wasm32))]` on the Queued-path spawn/timer carriers so the wasm build drops the tokio runtime/timer/macros (see Tokio Dependency Reduction). (iii) is the one place a `cfg` touches a function body — a deliberate trade to shrink the wasm tokio surface to `["sync"]`, safe because those arms are provably dead on wasm.

**Bonus outcome (user requirement 2):** because all tokio `spawn`/`time`/macro use is on the now-native-only Queued path, the wasm build needs **only `tokio::sync`** — the feature set drops `["sync","rt","macros","time"]` → `["sync"]`, a real reduction of the tokio dependency surface, with a documented path to removing tokio entirely later.

## Data Structures

### New module `liquers-core/src/maybe_send.rs` (Axis 2 foundation)

```rust
//! Target-conditional Send/Sync. On native these are Send/Sync; on wasm32 they are
//! universally implemented, so `?Send` async traits and !Send data compile.
//! MUST be target-gated, NEVER a cargo feature (feature unification would strip Send
//! from the native multi-threaded build workspace-wide).

use core::future::Future;
use core::pin::Pin;

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    pub trait MaybeSend: Send {}
    impl<T: Send + ?Sized> MaybeSend for T {}
    pub trait MaybeSync: Sync {}
    impl<T: Sync + ?Sized> MaybeSync for T {}
}
#[cfg(target_arch = "wasm32")]
mod imp {
    pub trait MaybeSend {}
    impl<T: ?Sized> MaybeSend for T {}
    pub trait MaybeSync {}
    impl<T: ?Sized> MaybeSync for T {}
}
pub use imp::{MaybeSend, MaybeSync};

/// Boxed future used in return positions. `dyn` trait-object bounds cannot use the
/// marker traits above (only auto-traits may follow the principal trait), so the whole
/// type is aliased per target.
#[cfg(not(target_arch = "wasm32"))]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Dual async_trait attribute is applied at each site as:
///   #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
///   #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
/// (async_trait itself emits the correct `dyn Future (+ Send)` boxing per variant.)
```

**Two tools, applied where each is legal** (a correctness point that a naive "MaybeSend everywhere" gets wrong):
- **Supertrait bounds and generic `where` bounds** → `MaybeSend` / `MaybeSync` marker traits (`trait Foo: MaybeSend`, `F: MaybeSend`). Legal because these are ordinary trait bounds.
- **Trait-object additional bounds** (`dyn Fn(..) -> Pin<Box<dyn Future + Send>>`, stored closure/future types) → **cfg'd whole-type aliases** (`BoxFuture`, `AsyncExecutorFn`). `dyn Trait + MaybeSend` is illegal — only auto-traits may follow the principal trait.
- **`#[async_trait]` method futures** → `#[cfg_attr(.., async_trait(?Send))]`; async_trait does the trait-object boxing itself.

### New struct `ImmediateAssetManager<E: Environment>` (Axis 1; new file `liquers-core/src/assets_immediate.rs`)

```rust
pub struct ImmediateAssetManager<E: Environment> {
    id: std::sync::atomic::AtomicU64,               // monotonic ids, starts at 1000
    envref: std::sync::OnceLock<EnvRef<E>>,
    /// Registry maps. NOT scc — a plain map behind std::sync::Mutex, locked only for
    /// brief SYNC get/insert (never held across .await). Rationale: (1) inline mode has
    /// no lock-contention need; (2) scc's async API and its Send/Sync bounds on values
    /// are avoided, so the maps compile with a !Send `AssetRef<E>` on wasm; (3) the
    /// std Mutex guard is !Send and cannot cross an .await, which statically enforces
    /// the "no lock across await" discipline that prevents re-entrant deadlock during
    /// recursive inline dependency evaluation.
    assets: std::sync::Mutex<std::collections::HashMap<Key, AssetRef<E>>>,
    query_assets: std::sync::Mutex<std::collections::HashMap<Query, AssetRef<E>>>,
    dependency_manager: crate::dependencies::DependencyManager<E>,  // reused as-is (spawn/timer-free)
    // NOTE: DependencyManager internally holds scc::HashMap<DependencyKey, Vec<WeakAssetRef<E>>>
    // (dependencies.rs:130) — the same E-carrying-scc shape avoided above for this manager's own
    // maps. That avoidance is a belt-and-suspenders simplification, not a hard requirement: scc's
    // async methods carry no K/V: Send bound, and the current (fully Send-bound) core already
    // compiles for wasm32 with scc. Phase 4 adds a compile-check that DependencyManager<E> builds
    // under wasm32 with a genuinely !Send AssetRef<E>, to confirm scc's epoch-reclamation internals
    // are fine with a !Send value type before relying on it.
    started: tokio::sync::OnceCell<()>,             // lazy start(): command-version load
    max_dependency_retries: u32,
}
```

- **Ownership:** `Arc<ImmediateAssetManager<E>>` (no `Box`; the `Arc<Box<…>>` wart is removed for both managers).
- **No `JoinHandle`s, channels, monitor, or `Drop`.**
- **`Send`/`Sync`:** native — all fields `Send + Sync`; wasm — the relaxed trait bounds accept the (possibly `!Send`) `AssetRef<E>`.

### New enum `EvalMode` (Axis 1; on `AssetData`, in `assets.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalMode { Queued, Inline }
```

- New field `eval_mode: EvalMode` on `AssetData<E>` (assets.rs:229), defaulting to `Queued` in existing constructors so `DefaultAssetManager` behavior is byte-for-byte unchanged. `ImmediateAssetManager` creates assets with `Inline`.
- Every consumer matches exhaustively (no `_ =>`), so a future mode is a compile error.
- Drives the three shared-path forks (service-message loop, metadata persistence, cancel) — see Sync vs Async.

### ExtValue Extensions

None.

## Trait Implementations

### The five core async traits — Axis-2 attribute + bound change (uniform pattern)

Applied to `Environment` (context.rs:43, not `#[async_trait]` itself but its supertrait bound), `AssetManager` (assets.rs:2248), `CommandExecutor` (commands.rs:407), `AsyncStore` (store.rs:267), `AsyncRecipeProvider` (recipes.rs:305), and their `impl` blocks:

```rust
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait AssetManager<E: Environment>: MaybeSend + MaybeSync { /* ... */ }
```

- Supertrait `Send + Sync` → `MaybeSend + MaybeSync` (via `use crate::maybe_send::{MaybeSend, MaybeSync}`).
- `Environment: Sized + Sync + Send + 'static` → `Sized + MaybeSync + MaybeSend + 'static`.
- **Data-carrying supertrait bounds** `ValueInterface: … + Send + Sync + 'static` (value.rs:45) and `PayloadType: Clone + Send + Sync + 'static` (commands.rs:343) → `… + MaybeSend + MaybeSync + 'static`. Relaxed for completeness (lets a wasm value/payload hold `Rc`/JS handles later); native builds keep full `Send + Sync`. `liquers_lib::value::Value` and `SimpleUIPayload` satisfy both variants (blanket impl on wasm; they are `Send` on native).
- Every `impl` of these traits gets the same `cfg_attr` pair. Downstream impls (`liquers-store`, `liquers-lib`, `liquers-axum`, `liquers-py`) must add the pair — mechanical, enumerated in Integration Points.

### Explicit boxed-future return types → `BoxFuture` alias (Axis 2)

Replace every `Pin<Box<dyn Future<Output = …> + Send + 'x>>` return with `BoxFuture<'x, …>` (which is `+ Send` on native, bare on wasm). Sites (audited): context.rs:66,111,120,137,572,700 (`apply_recipe`, `evaluate`, `evaluate_immediately`); interpreter.rs:122,155,350,385; plan.rs:1673,2005; commands.rs:449,498,516. This preserves native semantics exactly and drops the bound on wasm.

### Command registry stored types → cfg'd type aliases (Axis 2, commands.rs:427-458)

The `dyn Fn(..) -> Pin<Box<dyn Future + Send>> + Send + Sync` map values cannot use markers (trait-object bounds). Introduce, in `commands.rs`:

```rust
#[cfg(not(target_arch = "wasm32"))]
type SyncExecutorFn<E>  = dyn Fn(&State<<E as Environment>::Value>, CommandArguments<E>, Context<E>)
                              -> Result<<E as Environment>::Value, Error> + Send + Sync + 'static;
#[cfg(target_arch = "wasm32")]
type SyncExecutorFn<E>  = dyn Fn(&State<<E as Environment>::Value>, CommandArguments<E>, Context<E>)
                              -> Result<<E as Environment>::Value, Error> + 'static;
// AsyncExecutorFn<E> analogous, returning BoxFuture<'static, Result<E::Value, Error>>.
```

`CommandRegistry`'s `executors`/`async_executors` fields use these aliases. The `where F: … + Send` bounds on `register_command`/`register_async_command` (commands.rs:474-477,499-501) become `F: … + MaybeSend + MaybeSync` (generic bounds — markers are legal here).

**Forward-looking validation — `!Send` JavaScript closures as commands (user requirement 1).** This treatment is deliberately the enabler for a future browser command backend that registers JS closures (a `Closure<dyn FnMut>` / a Rust closure capturing `JsValue` is `!Send` and usually `!Sync`). The full type chain permits it on wasm: `register_async_command`'s bound is `MaybeSend + MaybeSync` (vacuous on wasm, so a `!Send` `F` is accepted); the stored `AsyncExecutorFn<E>` alias drops `+ Send + Sync` on wasm, so `Arc<Box<AsyncExecutorFn<E>>>` holds a `!Send` closure; `CommandExecutor::execute_async` returns `BoxFuture` (no `+ Send` on wasm), so a future that captures a JS handle is legal; and `CommandRegistry: CommandExecutor` requires only `MaybeSend + MaybeSync` (vacuous on wasm), so the registry — a field of the `Environment` — can hold `!Send` closures while the environment itself stays `MaybeSend`. **No part of the design forces a command closure or its future to be `Send` on wasm.** (The JS-command backend itself — wrapping a `Closure` as `dyn Fn(..) -> BoxFuture` via `Rc<RefCell<..>>` — is future work in `liquers-lib`/a wasm crate, not this refactor; this section only certifies the core does not preclude it.)

### Trait: `Environment` — generalized manager (Axis 1, context.rs:43)

```rust
#[cfg_attr(…)] // note: Environment is not itself #[async_trait]; only the bound changes
pub trait Environment: Sized + MaybeSync + MaybeSend + 'static {
    type Value: ValueInterface;
    type CommandExecutor: CommandExecutor<Self>;
    type SessionType: Session;
    type Payload: crate::commands::PayloadType;
    type AssetManager: AssetManager<Self>;                     // NEW

    fn get_asset_manager(&self) -> Arc<Self::AssetManager>;    // was Arc<Box<DefaultAssetManager<Self>>>
    fn apply_recipe(/*…*/) -> BoxFuture<'static, Result<Arc<Self::Value>, Error>>;  // BoxFuture alias
    // … rest unchanged …
}
```

- Associated type (zero-cost, house style) over `Arc<dyn …>`; avoids dynamic dispatch on the hot path and any `dyn`-safety risk.
- `EnvRef::get_asset_manager` mirrors to `Arc<E::AssetManager>` (context.rs:97); `evaluate`/`evaluate_immediately` returns become `BoxFuture` (context.rs:120,137).

### Trait: `AssetManager<E>` — extended surface (Axis 1)

The trait absorbs the methods that `AssetRef`/`Context`/interpreter currently reach on the **concrete** `DefaultAssetManager` (audit: assets.rs:513,853,1586,1904,1976; interpreter.rs:51,355; context.rs:603,730). Existing methods unchanged; added:

```rust
fn set_envref(&self, envref: EnvRef<E>);
fn dependency_manager(&self) -> &crate::dependencies::DependencyManager<E>;
async fn start(&self);                       // idempotent command-version load (replaces init spawn)
fn track_expiration(&self, asset_ref: &AssetRef<E>, expiration_time: &ExpirationTime);
async fn cascade_expire_dependents(&self, dep_key: &crate::metadata::DependencyKey);
async fn expire_dependencies_result(&self, expired: crate::dependencies::ExpiredDependents<E>);
async fn register_plan_dependencies(&self, dependent_key: &Key,
                                    plan_deps: &[crate::dependencies::PlanDependency]) -> Result<(), Error>;
fn create_temporary_asset(&self) -> AssetRef<E>;   // manager owns the EvalMode of temp assets
```

- `cascade_expire_dependents`/`expire_dependencies_result`/`register_plan_dependencies` have **identical bodies** for both managers (touch only `dependency_manager()` + maps); implemented per-manager in Tier 1 (maps private), shared-helper extraction deferred to Phase 4.
- Object-safe (no generic methods) — `Arc<dyn AssetManager<E>>` remains usable for tests even though the associated-type path is `dyn`-free.

### Trait: `AssetManager<E> for ImmediateAssetManager<E>` — semantics

- **`get_asset(query)`**: lock `query_assets` (brief, sync) → if present return it; else create `AssetRef` with `EvalMode::Inline`, insert, unlock; then `self.start().await` (once) and **`asset.run_inline().await`** to completion; return the (finished) asset. Concurrent second caller finds the inserted (possibly running) asset and awaits `asset.get()` instead of starting a second run (`run_inline` guards via the existing claim/status check).
- **`get(key)`**: same, keyed; recipe/store resolution as `DefaultAssetManager` minus submission.
- **`apply` / `apply_immediately`**: build temp/recipe asset `Inline`, `run_inline().await` / `run_immediately_inline(payload).await`, return.
- **Recursive dependencies**: inline command → `Context` → `get_dependency_asset` (default → `get_asset`) → recursive inline eval; `get_asset`'s body is `Box::pin`ned for async recursion; existing `Context` cycle check applies unchanged. No map lock is held across these awaits (std Mutex guard is `!Send`/scope-local), so recursion cannot self-deadlock.
- **`wait_for_dependency`** / **`drain_dependencies`** / **`get_dependency_asset`**: trait defaults are correct (dependency already finished; no local queues). No overrides.
- **Lazy expiration (replaces the monitor task)**: on each cache hit, if `Ready`, compute `expiration_time()`; if expired → `expire_without_cascade()` + map removal + re-eval inline. `track_expiration` is a **no-op**. Documented behavior difference: expiry is observed at next access; clock-driven cascade does not fire, but dependency-version-change cascade still runs (`cascade_expire_dependents`).
- **`start()`**: `self.started.get_or_init(async { /* load_command_versions body */ }).await` — no init-time spawn.

## Generic Parameters & Bounds

- `ImmediateAssetManager<E: Environment>` / `DefaultAssetManager<E: Environment>` — single param; `MaybeSend + MaybeSync` for the trait come from field types.
- `Environment::AssetManager: AssetManager<Self>` — F-bounded like the existing `CommandExecutor<Self>`.
- **Minimal-bounds review:** no bound is added that a call site doesn't need. `MaybeSend`/`MaybeSync` on wasm are vacuous (blanket), so they never over-constrain; on native they are exactly today's `Send`/`Sync`.
- No new lifetimes.

## Sync vs Async Decisions

- All manager trait methods stay async (inline mode still *awaits* command futures — async is mandatory, never blocking).
- `set_envref`, `track_expiration`, `create_temporary_asset`, `dependency_manager` are sync (match current concrete signatures).
- **`run_inline` replaces `tokio::spawn` with `futures::join!`** (executor-agnostic; works on wasm):

```rust
// AssetRef, single-task variant of run_with_future (assets.rs:1430)
async fn run_with_future_inline<Fut>(&self, evaluate_future: Fut) -> Result<(), Error>
where Fut: core::future::Future<Output = Result<(), Error>>;
pub(crate) async fn run_inline(&self) -> Result<(), Error>;
pub(crate) async fn run_immediately_inline(&self, payload: Option<E::Payload>) -> Result<(), Error>;
```

  Shape: `let (psm_res, run_res) = join!(process_service_messages(), async { let r = select!(wait_to_finish, evaluate); finalize_primary_progress(); send(JobFinishing); r }); finish_run_with_result(run_res, psm_res)`. `join!` completes only when **both** finish, sound **iff** the psm loop terminates on `JobFinishing` — which the existing code guarantees (assets.rs:1413 comment). `finish_run_with_result` (assets.rs:1328) is refactored to take `Result<(), Error>` for the psm outcome; the `Queued` caller maps `JoinError → Error` at its call site. Phase 4 adds a regression test that would hang (bounded by harness timeout) if the invariant breaks.
- **`MetadataSaver`** (assets.rs:145): mode passed as a parameter — `save_immediately(metadata, key, envref, eval_mode)` — from its sole caller `AssetData::save_metadata` (assets.rs:373), which owns the field. `Queued` → current debounced spawn path; `Inline` → direct `store.set_metadata(..).await` (no spawn, no `sleep`, **no `std::time::Instant`**, which panics on wasm).
- **Cancel** (assets.rs:1790): 5 s `tokio::time::timeout` in `Queued`; `Inline` → direct await (single-threaded inline eval cannot race a running evaluation).
- No sync wrappers introduced; Python bindings use the unchanged async surface.

## Function Signatures

### Module `liquers_core::maybe_send` (new)
`pub trait MaybeSend`, `pub trait MaybeSync`, `pub type BoxFuture<'a, T>` (as above).

### Module `liquers_core::assets_immediate` (new)
```rust
impl<E: Environment> ImmediateAssetManager<E> { pub fn new() -> Self; }
impl<E: Environment> Default for ImmediateAssetManager<E> { fn default() -> Self; }
#[cfg_attr(not(wasm32), async_trait)] #[cfg_attr(wasm32, async_trait(?Send))]
impl<E: Environment> AssetManager<E> for ImmediateAssetManager<E> { /* full trait */ }
```

### Module `liquers_core::assets` (changes)
```rust
pub enum EvalMode { Queued, Inline }
impl<E: Environment> AssetData<E> { pub(crate) fn with_eval_mode(self, m: EvalMode) -> Self; }
impl<E: Environment + 'static> AssetRef<E> {
    pub(crate) async fn run_inline(&self) -> Result<(), Error>;
    pub(crate) async fn run_immediately_inline(&self, payload: Option<E::Payload>) -> Result<(), Error>;
    pub fn new_temporary_with_mode(envref: EnvRef<E>, m: EvalMode) -> Self;   // Inline: no psm spawn
}
// DefaultAssetManager and JobQueue: entire type/impls gated
#[cfg(not(target_arch = "wasm32"))] pub struct DefaultAssetManager<E: Environment> { /* … */ }
// Inherent methods (set_envref, dependency_manager, load_command_versions→start,
// track_expiration, cascade_*, register_plan_dependencies, create_temporary_asset)
// move into the AssetManager impl; bodies unchanged.
```

### Module `liquers_core::context` (changes)
```rust
pub trait Environment: Sized + MaybeSync + MaybeSend + 'static {
    type AssetManager: AssetManager<Self>;
    fn get_asset_manager(&self) -> Arc<Self::AssetManager>;
    fn apply_recipe(/*…*/) -> BoxFuture<'static, Result<Arc<Self::Value>, Error>>;
}
impl<E: Environment> EnvRef<E> {
    pub fn get_asset_manager(&self) -> Arc<E::AssetManager>;
    pub fn evaluate<Q: TryToQuery>(&self, q: Q) -> BoxFuture<'static, Result<AssetRef<E>, Error>>;
    pub fn evaluate_immediately(/*…*/) -> BoxFuture<'static, Result<AssetRef<E>, Error>>;
}
// SimpleEnvironment / SimpleEnvironmentWithPayload:
//   type AssetManager = DefaultAssetManager<Self>;  (native)
//   field Arc<DefaultAssetManager<Self>> (Box removed)
```

### Module `liquers_core::interpreter` (changes)
```rust
// line 355: AssetRef::new_temporary(envref) -> envref.get_asset_manager().create_temporary_asset()
// finalize_plan / apply_plan boxed-future returns -> BoxFuture alias (lines 122,155,350,385)
```

### Crate `liquers-macro` (Axis 2)
There is exactly **one** production codegen site: `registration.rs:1118` inside `wrapper_fn_signature()` (reused by its only caller `command_wrapper()`). It emits the core alias instead of the inline Send-bounded future:
```rust
// was: -> Pin<Box<dyn Future<Output = Result<V, Error>> + core::marker::Send + 'static>>
// now: -> liquers_core::maybe_send::BoxFuture<'static, Result<V, Error>>
```
Splicing a `liquers_core::…` path into caller-crate output is the existing pattern (registration.rs already emits `liquers_core::state::State`, `liquers_core::context::Environment`, …; resolved at the invocation site's prelude, so `liquers-macro` needs no dep on `liquers-core`). `registration.rs:1890,2358` are **`#[cfg(test)]` expected-token fixtures**, not codegen — they must be updated to match (else the macro's own unit tests fail loudly), but are not independent edits. No DSL/user-facing change; `register_command!` call sites untouched.

## Integration Points

### Crate: liquers-core
- New `maybe_send.rs` (+ `mod maybe_send;` in `lib.rs`).
- `assets.rs`: `EvalMode` + three exhaustive-match forks; `AssetManager` trait extension + dual `cfg_attr`; `#[cfg(not(wasm32))]` on `DefaultAssetManager`/`JobQueue`/expiration-monitor; `run_inline`/`run_immediately_inline`/`new_temporary_with_mode`.
- `assets_immediate.rs`: `ImmediateAssetManager`.
- `context.rs`: `Environment::AssetManager` + `MaybeSend`/`MaybeSync` bounds + `BoxFuture` returns; `EnvRef` mirror; `SimpleEnvironment*` conformance; existing `init_with_envref` keeps its native spawn (gated implicitly — those envs are used natively; a wasm-first env selects Immediate, below).
- `commands.rs`: `CommandExecutor` `cfg_attr` + `MaybeSend`/`MaybeSync`; `SyncExecutorFn`/`AsyncExecutorFn` aliases; `register_*` `where` bounds → markers; `PayloadType` bound.
- `store.rs`, `recipes.rs`: trait `cfg_attr` + `MaybeSend`/`MaybeSync` **on the traits AND every wasm-compiled impl**. Impls that compile on wasm and were missed by the first draft: `recipes.rs:381` (`impl AsyncRecipeProvider for TrivialRecipeProvider`), `recipes.rs:438` (`impl … for DefaultRecipeProvider`), and core's wasm-compiled `AsyncStore` impls (`NoAsyncStore` and the in-memory/wrapper stores at `store.rs:488,597,1788`). **Native-only impls keep plain `#[async_trait]` under their existing `#[cfg(not(wasm32))]` gate** — `AsyncFileStore` (`store.rs:916`), `DefaultAssetManager` (`assets.rs:2974`) — because on native the trait is plain `#[async_trait]` and the attributes still match. Rule: attribute parity is per-target; a mismatched pair is a hard `E0053`, but only on a `wasm32` build (see build-matrix note below).
- `value.rs`: `ValueInterface` supertrait bound → markers.
- `interpreter.rs`, `plan.rs`: `BoxFuture` returns; temp-asset via manager.
- `Cargo.toml`: `async-trait` must be available on wasm (already pulled via default `async_store`); **no new deps**. **wasm tokio features reduced `["sync","rt","macros","time"]` → `["sync"]`** (see Tokio Dependency Reduction below). `run_inline` uses `futures::join!`/`futures::select!` (already-present `futures` dep), not `tokio::{join,select}!`, so `"macros"` is not needed on wasm.

### Crate: liquers-macro
- registration.rs: 3 future-type sites emit `liquers_core::maybe_send::BoxFuture`; drop generated `+ Send` on the async closure. Recompiles all `register_command!` users unchanged.

### Crate: liquers-lib
- `environment.rs`: `DefaultEnvironment` gains `type AssetManager` **selected by target**:
  ```rust
  #[cfg(not(target_arch = "wasm32"))] type AssetManager = DefaultAssetManager<Self>;
  #[cfg(target_arch = "wasm32")]      type AssetManager = ImmediateAssetManager<Self>;
  ```
  field/constructor cfg-selected the same way. **`init_with_envref` must be cfg-split explicitly** — today (environment.rs:148-154) it unconditionally `tokio::spawn`s `load_command_versions`, which panics at `env.to_ref()` on wasm:
  ```rust
  fn init_with_envref(&self, envref: EnvRef<Self>) {
      self.get_asset_manager().set_envref(envref.clone());
      #[cfg(not(target_arch = "wasm32"))]
      { let am = self.get_asset_manager(); tokio::spawn(async move { am.load_command_versions().await; }); }
      // wasm: no spawn — ImmediateAssetManager::start() runs lazily on first get_asset/get/apply.
  }
  ```
  **This is what lets `ui_spec_demo` keep using `DefaultEnvironment` unchanged and transparently get the immediate manager in the browser.** Add the dual `async_trait` `cfg_attr` to its trait impls; `Box` removed.
- `ui/*`: no API change; `spawn_ui_task` already target-splits (mod.rs:66-79). The runner's `evaluate`/`evaluate_immediately`/`get_asset_manager` calls now resolve inline on wasm. **`AppState` (app_state.rs:55) is a *sync* trait (`Send + Sync + Debug`, no `#[async_trait]`)** — no Axis-2 attribute change applies; its `Send + Sync` bound is satisfied by the demo's data-only element impls, so `Arc<tokio::sync::Mutex<dyn AppState>>` compiles on wasm as-is. (A future `!Send` UI element holding `web-sys` would need a separate liquers-lib relaxation — out of scope for the acceptance criterion.)

### Crate: liquers-axum
- **No `Environment` impl exists here** — `liquers-axum` only uses `Environment` as a generic bound (`E: Environment`) in handlers/builders and consumes `liquers-lib`'s `DefaultEnvironment`. There is no axum-side `type AssetManager` to set. Handlers/websocket call only trait methods → **recompile only, no source change**. Native-only crate; not part of the wasm build matrix.

### Crate: liquers-py
- `context.rs:102` (`get_asset_manager`, currently `todo!()`): add `type AssetManager = DefaultAssetManager<Self>` + return type. Native-only crate (the `not(wasm32)` branch is what compiles); dual `cfg_attr` is uniformity-only there. Python API unchanged.

### Crate: liquers-store (native-only)
- `opendal_store.rs:283` (`impl AsyncStore for AsyncOpenDALStore`): dual `cfg_attr` for uniformity, but **OpenDAL is not wasm-viable and `liquers-store` is native-only — this site is untested by the wasm build matrix**. Because an attribute mismatch is an `E0053` that surfaces *only* on a `wasm32` target build, native-only crates carry the dual attribute purely for uniformity; their `not(wasm32)` branch is the one that ever compiles. Phase 4 treats these as low-risk uniformity edits, distinct from the verified core/lib wasm path.

### Dependencies
No new dependencies in any crate. `async-trait`'s `?Send` mode is a call-site attribute, not a feature. **Note:** `async-trait` is already a *de facto mandatory* dep of `liquers-core` (`commands.rs:406` applies `#[async_trait]` unconditionally, so a `--no-default-features` build without `async_store` already fails to compile today) — this design does not change that, and does not make it optional.

## The complete Axis-2 surface (checklist for Phase 4)

| Kind | Sites | Transformation |
|---|---|---|
| Trait `#[async_trait]` attr | `AssetManager`, `CommandExecutor`, `AsyncStore`, `AsyncRecipeProvider` traits + **all impls** (core + 4 downstream crates) | dual `cfg_attr(async_trait / async_trait(?Send))` |
| Supertrait `Send+Sync` | those 4 + `Environment`, `ValueInterface`, `PayloadType`, `Store`(store.rs:16), `BinCache`(cache.rs:23) | `MaybeSend + MaybeSync` |
| Explicit `Pin<Box<dyn Future+Send>>` | context.rs 66,111,120,137,572,700; interpreter.rs 122,155,350,385; plan.rs 1673,2005; commands.rs 449,498,516 | `BoxFuture<'x,T>` alias |
| `dyn Fn + Send + Sync` stored | commands.rs 428-458 | `SyncExecutorFn`/`AsyncExecutorFn` cfg aliases |
| Generic `where F: …+Send` | commands.rs 474-477,499-501 | `+ MaybeSend + MaybeSync` |
| Macro `+ Send` | liquers-macro registration.rs 1118,1890,2358 | emit `BoxFuture` alias |
| Threaded manager compile-out | `DefaultAssetManager`, `JobQueue`, expiration monitor (assets.rs) | `#[cfg(not(target_arch="wasm32"))]` |
| Queued-path spawn/timer carriers | `run`/`run_with_future`/`run_immediately`/`new_temporary`; `Queued` arms of `MetadataSaver::save_immediately`, `AssetRef::cancel` | method/arm `#[cfg(not(wasm32))]` (wasm arm `unreachable!()`) → drops tokio `rt`/`time` on wasm |
| Env manager selection | `DefaultEnvironment` (liquers-lib) | cfg-selected `type AssetManager` |
| wasm tokio features | `liquers-core/Cargo.toml` `[target.'cfg(wasm32)'.dependencies]` | `["sync","rt","macros","time"]` → `["sync"]` |

## Tokio Dependency Reduction (investigation — user requirement 2)

This refactor is naturally also a **reduction of the tokio surface on wasm**. Audit of `liquers-core` tokio usage (non-test):

| Category | Symbols (counts) | On wasm path? |
|---|---|---|
| Runtime/scheduler | `tokio::spawn` ×13, `tokio::task::{JoinHandle, JoinError, yield_now}` | **No** — all on the Queued/threaded path (JobQueue, monitor, `run_with_future`, `MetadataSaver` debounce, `RunClaim` repair, `new_temporary`) |
| Timers | `tokio::time::{sleep ×5, timeout ×3, Duration}` | **No** — Queued path only (monitor, cancel timeout, metadata debounce) |
| Macros | `tokio::select!` ×3, `tokio::join!` ×1 | **No** — replaced by `futures::select!`/`join!` on the inline path |
| Sync primitives | `tokio::sync::{mpsc ×11, watch ×9, RwLock ×6, Notify ×4, Mutex, OnceCell}` (assets.rs, dependencies.rs) | **Yes** — the shared `AssetData`/`DependencyManager` infrastructure both managers reuse |

### Easy win (in scope for this change): wasm tokio → `["sync"]` only

Because every runtime/timer/macro use is on the Queued path — which b1 makes native-only — the wasm build needs **no tokio runtime, no timer driver, no tokio macros**. Requirements (all already implied by the b1 design, made explicit here):
- `run`/`run_with_future`/`run_immediately`/`new_temporary` (the `tokio::spawn` carriers) are **method-level `#[cfg(not(target_arch = "wasm32"))]`**; the `*_inline` variants are the always-compiled counterparts.
- The `EvalMode::Queued` arms in `MetadataSaver::save_immediately` and `AssetRef::cancel` (the `tokio::spawn`/`tokio::time` branches) are `#[cfg(not(target_arch = "wasm32"))]`, with the wasm arm `unreachable!()` (kept exhaustive, no `_ =>`). Safe because on wasm only `ImmediateAssetManager` exists and it constructs **only** `Inline` assets, so `Queued` is provably dead there.
- `DefaultAssetManager`/`JobQueue`/expiration-monitor already `#[cfg(not(wasm32))]`.
- `run_inline` uses `futures::{join, select}!`.

**Net:** on wasm, tokio contributes only its standalone `sync` primitives (which need no runtime). This is a concrete dependency-surface reduction shipped by this change. *(Trade-off vs. the "no cfg in bodies" ideal: two of the three shared forks gain a cfg'd-out `Queued` arm. Justified — the arm is dead code on wasm and the payoff is dropping the tokio runtime/timer/macros entirely.)*

### Full removal (future work — concrete path, not this change)

Removing tokio **entirely** on wasm means replacing `tokio::sync` in the shared `AssetData`/`DependencyManager` with executor-agnostic primitives:

| tokio::sync | executor-agnostic replacement |
|---|---|
| `Mutex`, `RwLock` | `async-lock` (or `futures::lock::Mutex`) |
| `mpsc` | `async-channel` (or `futures::channel::mpsc`) |
| `watch` | `async-watch`, or a small latest-value + `event-listener` wrapper |
| `Notify` | `event-listener` |
| `OnceCell` | `async-once-cell` |

This is a **substantial refactor of `assets.rs`** (the whole asset runtime is built on `tokio::sync`) and touches the native path too, so it is deliberately out of scope here. But it is the endgame the user's embedded/single-threaded angle points at:

### "Replace tokio with another async framework" — reframed

The inline core is **already executor-agnostic in its scheduling** (no `spawn`, `futures` macros). The only remaining framework coupling on wasm is `tokio::sync`. So the goal is not "swap tokio for framework X" but **"make the core executor-agnostic so the embedder chooses the executor"** — `async-lock`/`async-channel`/`event-listener` are framework-*neutral*, not a competing framework. Once `tokio::sync` is abstracted (future work above), `liquers-core`'s inline path runs unchanged under `futures::executor::block_on`, `wasm_bindgen_futures`, `embassy` (embedded, no-std-ish), `smol`, etc. The easy win in this change (`["sync"]` only, no runtime/timer/macros) is the first and largest step down that path; a follow-up feature (e.g. `sync-backend = "tokio" | "async-lock"`) could complete it. Recorded as a tracked follow-up in `specs/ISSUES.md` at Phase 4.

## Relevant Commands

### New Commands
None. Core infrastructure only; no command signatures added/changed; `register_command!` DSL and all call sites unchanged.

### Relevant Existing Namespaces
None affected in behavior. `ui_spec_demo` uses `dashboard` (local) + the `lui` namespace (`register_lui_commands!`); both compile under the relaxed bounds and run on the immediate manager. Flagged for user confirmation at the gate.

## Error Handling

- All fallible paths use `liquers_core::error::Error` typed constructors; no new error types; no `Error::new`; no `unwrap`/`expect` outside tests.
- `run_inline` propagates evaluation errors identically to `run` (asset ends `Status::Error`, error in metadata). `ImmediateAssetManager::get_asset` returns `Err` only for pre-eval failures (parse/recipe), matching `DefaultAssetManager`; the existing failure contract (`tests/asset_failure_contract.rs`) must pass parameterized over both managers.
- Existing `set_envref` double-set `panic!` retained on both impls (pre-existing; converting to `Result` would ripple `init_with_envref` — out of scope, note in ISSUES).

## Acceptance & Testing Strategy (preview for Phase 3/4)

**Primary acceptance (user-specified):** `ui_spec_demo` builds to wasm, `trunk build` succeeds, and a **Playwright e2e** loads the page and drives the dashboard (the deferred `specs/webui` M4 test) — the click path exercises `mount_web → evaluate_immediately → inline eval → ui::web re-render` with no `tokio::spawn` panic and no `Send` error. **The harness is net-new** (no `playwright.config.*` / `.spec.ts` exists in the repo today — M4 deferred it), but its plan is already written in `specs/webui/phase4-implementation.md` (Step 15): new `liquers-lib/examples-web/ui_spec_demo/tests/webui.spec.ts` + `playwright.config.ts`, driven by `trunk serve` + `npx playwright test` against pre-installed headless Chromium. Phase 3 specifies the test scenarios; Phase 4 creates and runs the harness. This is the terminal success gate for the whole refactor.

- **Manager-parametric suite:** existing trait-surface tests (`asset_failure_contract`, volatility, dependency-manager integration, `assets.rs` mod-tests over `get_asset`/`apply`/`apply_immediately`) run over both `SimpleEnvironment` (Default) and an immediate environment, via a small generic harness.
- **Default-only:** queue capacity, parking/drain, in-flight cancellation, monitor-driven expiration.
- **Immediate-only:** inline recursion, cycle detection under inline recursion, lazy expiration-on-access, and a **no-runtime proof** — the immediate path runs under `futures::executor::block_on` with no tokio runtime present (the native stand-in for browser-readiness).
- **Axis-2 build matrix:** `cargo check -p liquers-core` (native), `--target wasm32-unknown-unknown -p liquers-core -p liquers-lib` (the wasm-relevant crates — this is what catches an `E0053` attribute-parity miss on `recipes.rs`/core stores), plus native `cargo check -p liquers-store -p liquers-axum -p liquers-py` and the existing egui/webui/polars feature combos, all green. `liquers-store`/`axum`/`py` are native-only, so their dual-`cfg_attr` sites are *not* exercised by the wasm target — a mismatch there is caught only by review, not CI; keep those edits minimal and uniform.

## Open Questions (carried to Phase 3/4)

1. Extract the three identical trait-method bodies (`cascade_expire_dependents` etc.) into a shared helper vs. duplicate per-manager — Phase 4 refactor call.
2. Should `Store` (sync, store.rs:16) and `BinCache` (cache.rs:23) be relaxed too, or left `Send` (native-only paths)? Leaning relax-for-uniformity; confirm no wasm impl is forced.
3. `ImmediateEnvironment` as a *named* env vs. relying solely on `DefaultEnvironment`'s cfg-selected manager — the latter satisfies the acceptance criterion with zero example changes; a named env is optional sugar for native embedded use. Phase 4.
4. `load_command_versions` external callers — keep as deprecated alias vs. rename to `start()` (grep at Phase 4).
