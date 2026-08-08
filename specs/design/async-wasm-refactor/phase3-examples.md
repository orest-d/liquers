# Phase 3: Examples & Use-cases — async-wasm-refactor

## Example Type

**Runnable prototypes.** Everything below is real code: `#[tokio::test]` integration tests, a native no-runtime proof, a wasm build-matrix, and the `ui_spec_demo` Playwright e2e (the acceptance gate). No conceptual-only snippets except the one clearly-labeled forward-looking JS-closure compile check.

## Overview Table

| # | Kind | Name | Demonstrates / checks | Manager(s) |
|---|---|---|---|---|
| E1 | Example (primary) | Manager-parametric evaluation | Same query → same terminal `State` on both managers; the shared trait contract holds regardless of scheduling | Default + Immediate |
| E2 | Example (advanced) | Inline recursive dependencies | `ImmediateAssetManager` evaluates a dependency chain inline to completion, no `tokio::spawn`, dependencies resolved before return | Immediate |
| E3 | Example (edge, forward-looking) | `!Send` closure registers as a command | Compile-level proof that the Axis-2 relaxation lets a `!Send` async closure be registered on wasm (the JS-command enabler) | wasm build |
| E4 | Example (acceptance) | `ui_spec_demo` in the browser | `mount_web → evaluate_immediately → inline eval → re-render` runs in real Chromium via Playwright; the terminal success gate | Immediate (wasm) |
| T1 | Unit/integration | Parametric harness | `run_<scenario>::<E>()` bodies run against both env types via thin wrappers | both |
| T2 | Integration | Failure contract (parametric) | `asset_failure_contract` scenarios pass on both managers | both |
| T3 | Integration | Volatility + dependency (parametric) | existing volatility / dependency-manager integration pass on both | both |
| T4 | Integration (immediate) | Inline cycle detection | a dependency cycle errors (not hangs/stack-overflows) under inline recursion | Immediate |
| T5 | Integration (immediate) | Lazy expiration on access | an expired `At(..)` asset is re-evaluated on next `get_asset`, with no monitor task | Immediate |
| T6 | Integration (immediate) | Concurrent same-query | two concurrent `get_asset` calls share one `AssetRef` (no double-run) | Immediate |
| T7 | Unit (immediate) | **No-runtime proof** | the inline path runs under `futures::executor::block_on` with **no tokio runtime**; a `tokio::spawn` regression would panic here | Immediate |
| T8 | Integration (immediate) | Inline metadata + cancel | metadata persists inline (no `Instant`, no spawn); `cancel` on a finished inline asset is a direct no-op | Immediate |
| T9 | Default-only | Threaded-path regression | queue capacity, parking/drain, in-flight cancellation, monitor-driven expiration still pass (native, unchanged) | Default |
| T10 | Build matrix | Axis-2 / wasm compile | `cargo check --target wasm32-unknown-unknown -p liquers-core -p liquers-lib` green; catches an `E0053` attribute-parity miss and any residual `tokio::spawn`/`time` | — |
| T11 | Build matrix | Native feature combos | native `-p liquers-core/store/axum/py` + egui/webui/polars combos still green | — |
| T12 | Regression | `join!`/`JobFinishing` invariant | inline `run` completes (would hang → caught by harness timeout) — verifies the psm loop terminates | Immediate |
| T13 | Unit (immediate) | Manager primitives | `eval_mode()==Inline`; `create_temporary_asset()` yields a runnable temp asset; `start()`/`load_command_versions` helper registers versions on both managers | both |
| T14 | Build check | cfg-selected manager + wasm feature set | on wasm, `DefaultEnvironment::AssetManager == ImmediateAssetManager`; `liquers-core/Cargo.toml` `[target.wasm32]` tokio features are exactly `["sync"]` | — |

## Example

### E1 — Manager-parametric evaluation (primary use case)

The single most important example: the same query produces the same terminal `State` whether scheduled by `DefaultAssetManager` (queued) or `ImmediateAssetManager` (inline). This is the whole point of b1 — the manager is swappable behind the `AssetManager` trait.

```rust
// liquers-core/tests/manager_parametric.rs  (new)
use liquers_core::{
    assets::AssetManager,
    context::{Environment, EnvRef, SimpleEnvironment},
    error::Error,
    metadata::Status,
    query::{Query, TryToQuery},
    value::Value,
};
// test-support immediate env (Q3): native-only, mirrors SimpleEnvironment with ImmediateAssetManager
use liquers_core::context::ImmediateEnvironment;

fn q(s: &str) -> Query { s.try_to_query().expect("query") }

/// Register a `greet` command on any env whose command registry is reachable.
/// Both SimpleEnvironment and ImmediateEnvironment expose `command_registry`.
macro_rules! greet_env {
    ($env:expr) => {{
        let mut env = $env;
        env.command_registry
            .register_command(
                liquers_core::command_metadata::CommandKey::new_name("greet"),
                |_state, _args, _ctx| -> Result<Value, Error> { Ok(Value::from("hello")) },
            )
            .expect("register greet");
        env
    }};
}

/// Generic scenario body — written ONCE, run against both managers.
async fn scenario_basic_eval<E>(envref: EnvRef<E>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let asset = envref.get_asset_manager().get_asset(&q("greet")).await?;
    // Trait contract: a returned asset resolves to a terminal state.
    let state = asset.get().await?;
    assert_eq!(state.status(), Status::Ready);
    assert_eq!(state.try_into_string()?, "hello");
    Ok(())
}

#[tokio::test]
async fn basic_eval_default_manager() -> Result<(), Error> {
    let env = greet_env!(SimpleEnvironment::<Value>::new());
    scenario_basic_eval(env.to_ref()).await
}

#[tokio::test]
async fn basic_eval_immediate_manager() -> Result<(), Error> {
    let env = greet_env!(ImmediateEnvironment::<Value>::new());
    scenario_basic_eval(env.to_ref()).await
}
```

**What it proves:** the `AssetManager` trait contract (`get_asset` → obtainable terminal `State`) is identical across the two implementations; existing behavioral tests can be lifted into `scenario_*` bodies and run on both.

### E2 — Inline recursive dependency evaluation (advanced)

A command whose evaluation depends on another query. On `ImmediateAssetManager` the dependency is evaluated **inline** (recursively) and is already finished before `get_asset` returns — no queue, no spawn.

```rust
// same file; `upper` depends on `greet` via the context dependency API
async fn scenario_inline_dependency<E>(envref: EnvRef<E>) -> Result<(), Error>
where E: Environment<Value = Value> {
    // `combine` requests dependency `greet` inline, then transforms it.
    let asset = envref.get_asset_manager().get_asset(&q("greet/combine")).await?;
    let state = asset.get().await?;
    assert_eq!(state.status(), Status::Ready);
    assert_eq!(state.try_into_string()?, "hello!");   // combine appends "!"
    Ok(())
}
```

**What it proves:** recursive inline evaluation terminates with dependencies resolved; the `Box::pin`ned `get_asset` recursion and the existing cycle-check work under inline scheduling. Runs on Immediate (and, unchanged, on Default).

### E3 — `!Send` closure registers as a command (forward-looking, compile-level)

Certifies Axis-2 requirement 1 (JS closures) at the type level. This is a **wasm-target compile check**, not a runtime test — it holds a `Rc` (a `!Send` stand-in for a captured `JsValue`) across the command's `.await`.

```rust
// liquers-lib/examples-web/ui_spec_demo/src/lib.rs (or a wasm-only test): compile-only
#[cfg(target_arch = "wasm32")]
fn register_non_send_command(cr: &mut CommandRegistry<CommandEnvironment>) -> Result<(), Error> {
    use std::rc::Rc;
    let shared = Rc::new(41);                         // !Send capture (JsValue stand-in)
    cr.register_async_command(
        liquers_core::command_metadata::CommandKey::new_name("rc_cmd"),
        move |_state, _args, _ctx| {
            let shared = shared.clone();              // moved into a !Send future
            Box::pin(async move { Ok(Value::from((*shared + 1) as i64)) })
        },
    )?;
    Ok(())
}
```

**What it proves:** on wasm the `register_async_command` bound is `MaybeSend + MaybeSync` (vacuous), the stored `AsyncExecutorFn` drops `+ Send`, and `BoxFuture` is non-`Send` — so a `!Send` closure/future compiles. On **native** the identical code would be rejected (that's correct — native keeps `Send`), so this check is `#[cfg(target_arch = "wasm32")]` and validated by the wasm build (T10). It is the minimal proof that the core does not preclude a future JS-command backend; the backend itself is out of scope.

### E4 — `ui_spec_demo` in the browser (acceptance gate)

The deferred `specs/webui` M4 test, now unblocked. `DefaultEnvironment` on wasm cfg-selects `ImmediateAssetManager`, so the example source is **unchanged**; the harness is new.

```ts
// liquers-lib/examples-web/ui_spec_demo/tests/webui.spec.ts (new)
import { test, expect } from '@playwright/test';

test('dashboard renders and reacts to a menu action', async ({ page }) => {
  await page.goto('/');                                  // trunk serve
  // 1. Initial render (SSR-equivalent DOM produced by mount_web after inline eval)
  await expect(page.locator('#app')).toContainText('Add Dashboard');
  // 2. Drive a menu action → evaluate_immediately → inline eval → re-render
  await page.getByText('Add Dashboard').click();
  // 3. The dashboard child appears (proves the eval loop ran with no spawn panic)
  await expect(page.locator('#app')).toContainText('Dashboard', { timeout: 5000 });
  // 4. No uncaught panic/console error (the tokio::spawn panic would surface here)
  //    — collected via page.on('console'/'pageerror') asserted empty.
});
```

```toml
# playwright.config.ts equivalent: webServer runs `trunk serve` on 127.0.0.1:8080,
# baseURL http://127.0.0.1:8080, headless Chromium at PLAYWRIGHT_BROWSERS_PATH.
```

**What it proves:** the entire chain runs in a real browser with no `tokio::spawn` panic and no `Send` error — the terminal success criterion for the refactor.

## Corner Cases

| Corner case | Handling | Test |
|---|---|---|
| **Two concurrent `get_asset` for the same new query** | first inserts the `AssetRef` under the brief map lock and starts `run_inline`; second finds it and awaits `asset.get()` — no double evaluation | T6 |
| **Dependency cycle under inline recursion** | existing `Context` cycle check fires; returns `Err`, does not stack-overflow or hang | T4 |
| **Expired `At(instant)` asset, no monitor task** | lazy check-on-access in `get_asset`: expired → `expire_without_cascade` + re-evaluate inline | T5 |
| **Metadata persistence with no runtime** | `Inline` mode persists directly (`store.set_metadata().await`) — no `tokio::spawn`, no `std::time::Instant` (which panics on wasm) | T7, T8 |
| **`cancel` on an inline asset** | inline eval already finished before a caller could cancel; `Inline` arm awaits directly (no 5 s timeout) | T8 |
| **`join!`/`JobFinishing` termination** | the psm loop must end on `JobFinishing`, else `run_inline`'s `join!` hangs; T12 would time out on regression | T12 |
| **`!Send` value/payload held across `.await`** | permitted on wasm (`MaybeSend` vacuous); rejected on native (correct) | E3 / T10 |
| **Attribute-parity miss (`async_trait` vs `?Send`)** | hard `E0053` — surfaces only on the wasm target build | T10 |
| **Residual `tokio::spawn`/`time` on the wasm path** | would fail to compile once wasm tokio features are `["sync"]` only | T10 |
| **`DependencyManager<E>` with a `!Send` `AssetRef`** | scc value type is `!Send` on wasm; compile-check confirms scc internals accept it | T10 (targeted) |
| **Native immediate manager still `Send`** | `MaybeSend = Send` natively; `ImmediateEnvironment` is `Send` so the parametric tests compile natively | T1–T8 |

## Test Plan

### Harness (T1) — the parametric mechanism
- New `liquers-core/tests/manager_parametric.rs`. Each scenario is a generic `async fn scenario_*<E: Environment<Value = Value>>(envref: EnvRef<E>)`; two `#[tokio::test]` wrappers construct `SimpleEnvironment<Value>` and the test-support `ImmediateEnvironment<Value>` and call it. Command registration via a small macro (both envs expose `command_registry`).
- **Concretely lifted scenarios (T2/T3)** — the manager-agnostic assertions moved into `scenario_*` bodies, each run by a `*_default` and `*_immediate` wrapper:
  - from `tests/asset_failure_contract.rs`: `test_failed_asset_get_returns_ok_error_state`, and the re-evaluation-on-fresh-request case → `scenario_failure_contract` (T2).
  - from `tests/volatility_integration.rs`: the volatile-recompute assertions → `scenario_volatility` (T3).
  - from `tests/dependency_manager_integration.rs`: the dependency-registration + version pipeline assertions → `scenario_dependency_pipeline` (T3).
  - The **original files remain** as `SimpleEnvironment`-only tests (native regression, T9); the parametric file adds the second (immediate) run. This avoids destabilizing the existing suite while adding immediate coverage.
- Tests that assert queue/parking/monitor specifics (`dependency_scheduling.rs`, `expiration_integration.rs`) are **not** lifted — they are `DefaultAssetManager`-only by nature (T9).

### Immediate-only (T4–T8, T12)
- **T4 cycle:** register `a→b→a`; assert `get_asset` returns `Err` with a cycle error; assert it returns (bounded by `#[tokio::test]` default timeout).
- **T5 lazy expiration (deterministic, not sleep-flaky):** use `ExpirationTime::At(t)` with `t` in the **past** (already expired) rather than racing a real timer — a `recompute` `AtomicUsize` counter in the command body. First `get_asset` → counter 1; a second `get_asset` on an asset whose expiry is already in the past → lazy check-on-access recomputes → counter 2. No sleeps, no timing race. The "no monitor" property is asserted **structurally** (the immediate manager type has no `monitor_tx`/`JoinHandle` field — a compile-time fact, plus T7 proves no background task runs), not via a runtime probe.
- **T6 concurrency:** `join!` two `get_asset(same_q)`; assert the command body ran once (an `AtomicUsize` counter == 1).
- **T7 no-runtime proof:** `futures::executor::block_on(async { immediate_env.to_ref().get_asset_manager().get_asset(&q("greet")).await })` — **no `#[tokio::test]`, no tokio runtime**. A reintroduced `tokio::spawn` on the inline path panics ("no reactor"); green means browser-ready. Requires `futures` with the `executor` feature — Phase 4 adds `futures = { version = "0.3", features = ["executor"] }` to `liquers-core [dev-dependencies]` (the main `futures` dep is optional and `executor` is not a default feature). *Caveat:* `ImmediateEnvironment::new()` must not spawn (lazy `start()`), and `to_ref()` on wasm/immediate must not spawn — verified here. **This test also transitively guards the `std::time::Instant` hazard:** `Instant::now()` on the inline metadata path would not panic on native, but any spawn/timer on the inline path fails here, and the wasm build (T10) rejects `Instant`-on-wasm outright.
- **T8 metadata+cancel:** with a `MemoryStore`, evaluate a keyed asset; assert metadata is persisted after `get` returns (no debounce delay, no `Instant`); call `cancel` on the finished asset → immediate `Ok`.
- **T13 manager primitives:** on both envs assert `get_asset_manager().eval_mode()` (`Queued` for Default, `Inline` for Immediate); `create_temporary_asset()` returns an asset that `run_inline`/`run` can drive to `Ready`; after `to_ref()`, the dependency manager has command versions registered (proves `start()`→`load_command_versions` helper ran on both paths).
- **T12 invariant:** `#[tokio::test]` (immediate env) that evaluates a normal command via `get_asset` and asserts `Status::Ready` — concretely just `scenario_basic_eval::<ImmediateEnvironment<Value>>`, but named/kept as the guard for the `join!`/`JobFinishing` contract: because `run_inline` uses `join!(psm, eval)`, it returns only if the psm loop ends on `JobFinishing`; a regression makes this test hang and fail via the harness timeout. (No new command needed — reuses `greet`.)

### Default-only regression (T9)
- Re-run existing `dependency_scheduling`, capacity/parking, in-flight cancellation, and `expiration_integration` (monitor path) unchanged on `SimpleEnvironment` — proves native behavior is byte-for-byte preserved.

### Build matrix (T10, T11)
```bash
# Axis-2 / wasm — the E0053 + residual-spawn/timer gate
cargo check --target wasm32-unknown-unknown -p liquers-core
cargo check --target wasm32-unknown-unknown -p liquers-lib
# Native — nothing regressed
cargo check -p liquers-core -p liquers-store -p liquers-axum -p liquers-py
cargo test  -p liquers-core                      # all native tests incl. T1–T9,T12
cargo check -p liquers-lib --no-default-features --features webui
cargo check -p liquers-lib --features egui,polars,webui
```
- T10 also includes the targeted `DependencyManager<E>`-with-`!Send`-`AssetRef` compile check (a tiny wasm-target test module instantiating it with a `!Send` env stand-in).
- **T14 explicit feature-set + cfg-selection check:** a small check (CI grep or a `build.rs`-free assertion) that `liquers-core/Cargo.toml` `[target.'cfg(target_arch="wasm32")'.dependencies].tokio` `features == ["sync"]` — so dropping the residual `"rt"`/`"time"`/`"macros"` can't silently regress; and a wasm-target `const _: fn() = || { let _: <DefaultEnvironment<Value> as Environment>::AssetManager; };` style assertion that the selected manager is `ImmediateAssetManager` on wasm. **The wasm build (T10) is the authoritative check** for attribute-parity (`E0053`), residual `tokio::spawn`/`time`, and accidental references to cfg'd-out sync `Store`/`SimpleEnvironment` — all three are compile failures on the wasm target, so no separate active audit is needed; T14 adds only the two things a successful compile would *not* catch (feature-list creep and wrong-manager selection).

### E2E (E4)
```bash
cd liquers-lib/examples-web/ui_spec_demo
trunk build                                        # wasm builds
trunk serve &                                       # 127.0.0.1:8080
npx playwright test                                 # headless Chromium (pre-installed)
```
- New files: `tests/webui.spec.ts`, `playwright.config.ts`. Chromium at `PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers` (no `playwright install`). Assert render, click-reaction, and **zero console/page errors** (the spawn panic would appear as a `pageerror`).

### Coverage summary
- **Trait contract:** T1–T3 (both managers), the core correctness guarantee.
- **Immediate semantics:** T4–T8, T12 — recursion, cycles, lazy expiry, concurrency, no-runtime, metadata/cancel, join-invariant.
- **Native preserved:** T9.
- **Axis-2 compiles everywhere it must:** T10, T11, E3.
- **Acceptance:** E4 (browser + Playwright).

## Open Questions (for Phase 4)
1. `greet/combine` (E2) needs a real dependency-invoking command — Phase 4 defines a tiny `combine` test command that requests dependency `greet` via the `Context` dependency API (the query syntax `greet/combine` is confirmed valid; `combine` must be registered).
2. ✅ **Resolved:** T7 needs `futures = { version = "0.3", features = ["executor"] }` in `liquers-core [dev-dependencies]` — the crate's `futures` dep is optional and `executor` is not a default feature. Phase 4 adds it.
3. **Legitimately Phase 4:** exact Playwright selectors/assertions for the dashboard's post-click DOM depend on `ui::web`'s live markup and are pinned in Phase 4 against the running page (the SSR tests from webui M3 give the expected structure). E4's pass conditions (renders "Add Dashboard"; click yields "Dashboard"; zero console/`pageerror`) are fixed here; only the CSS selectors are deferred.
