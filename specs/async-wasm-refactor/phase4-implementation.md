# Phase 4: Implementation Plan — async-wasm-refactor

## Overview

**Feature:** make `liquers-core` run in the browser (`wasm32-unknown-unknown`) by (Axis 1 / b1) adding a spawn-free `ImmediateAssetManager` selected via `Environment::AssetManager`, and (Axis 2) target-gated conditional compilation that relaxes the five core async traits to non-`Send` on wasm. Bonus: wasm tokio features reduced to `["sync"]`. **Acceptance:** `ui_spec_demo` runs in Chromium under Playwright.

**Strategy:** implement in six milestones (M-A … M-F), each ending at a **green build checkpoint**. Because Axis 2 relaxes trait signatures that many impls share, individual steps within a milestone may not compile alone; the checkpoint command is the gate, not each step. On native, `MaybeSend == Send`, so M-A is semantically a no-op there and stays green throughout; wasm only goes green after M-B.

| Milestone | Steps | Checkpoint (must be green) |
|---|---|---|
| M-A Axis-2 scaffolding | 1–3 | `cargo check -p liquers-core` (native) |
| M-B Axis-1 managers | 4–9 | `cargo check/test -p liquers-core` (native) **and** `cargo check --target wasm32-unknown-unknown -p liquers-core` |
| M-C downstream + env selection | 10–12b | native workspace check **and** `cargo check --target wasm32 -p liquers-lib` |
| M-D tests | 13 | `cargo test -p liquers-core` (incl. parametric + immediate) |
| M-E acceptance | 14–15 | full build matrix + `npx playwright test` |
| M-F docs/follow-up | 16 | docs updated; ISSUES follow-up recorded |

**Ground rules:** no `unwrap`/`expect` outside tests; typed `Error` constructors; no `_ =>` arms; every `#[async_trait]` trait+impl carries the dual `cfg_attr`. Commit at each milestone checkpoint.

## Implementation Steps

### M-A — Axis-2 scaffolding

**Step 1 — `maybe_send` module.**
- **Files:** `liquers-core/src/maybe_send.rs` (new), `liquers-core/src/lib.rs` (`pub mod maybe_send;`).
- **Change:** `MaybeSend`/`MaybeSync` marker traits (target-gated blanket impls) + `pub type BoxFuture<'a, T>` (Send on native, bare on wasm), exactly as Phase 2 "Data Structures". Doc-comment the "never a cargo feature" rule.
- **Validate:** `cargo check -p liquers-core` (native) and `cargo check --target wasm32-unknown-unknown -p liquers-core --no-default-features --features async_store` (module compiles on both).
- **Agent:** haiku · rust-best-practices · Knowledge: Phase 2 maybe_send section.
- **Rollback:** delete the module + `pub mod` line.

**Step 2 — relax the five core async traits + explicit future bounds + macro.**
- **Files:** `context.rs` (Environment supertrait + `apply_recipe`/`evaluate`/`evaluate_immediately` → `BoxFuture`), `assets.rs` (AssetManager: dual `cfg_attr` + `MaybeSend+MaybeSync`), `commands.rs` (CommandExecutor dual `cfg_attr`+markers; `SyncExecutorFn`/`AsyncExecutorFn` cfg aliases; `register_*` where-bounds → markers; `PayloadType` markers), `store.rs` (AsyncStore + its wasm-compiled impls dual `cfg_attr`+markers; `Store` sync bound left as-is here — cfg'd in Step 3), `recipes.rs` (AsyncRecipeProvider trait + **both** provider impls at 381/438), `value.rs` (ValueInterface markers), `interpreter.rs`/`plan.rs` (`BoxFuture` returns), `liquers-macro/src/registration.rs:1118` (emit `liquers_core::maybe_send::BoxFuture`; update test fixtures 1890/2358).
- **Change:** mechanical per the Phase 2 "complete Axis-2 surface" table. Every `#[async_trait]` trait def AND impl gets the dual `cfg_attr` pair.
- **Validate:** `cargo check -p liquers-core` + `cargo check -p liquers-macro` + `cargo test -p liquers-macro` (fixtures). Native only (wasm blocked until M-B).
- **Agent:** sonnet · rust-best-practices · Knowledge: Phase 2 Axis-2 surface table + the enumerated impl sites (recipes 381/438; store impls 488/597/1788; AsyncFileStore 916 stays native-only plain `#[async_trait]`).
- **Rollback:** revert the trait/attr edits (git per-file); macro revert restores `+ Send`.

**Step 3 — cfg-out obsolete sync `Store`/`BinCache`/`Cache` + `SimpleEnvironment*` on wasm.**
- **Files:** `store.rs` (`Store`, `NoStore`), `cache.rs` (`BinCache`, `Cache`), `context.rs` (`SimpleEnvironment`, `SimpleEnvironmentWithPayload` + their impls/sessions as needed).
- **Change:** add `#[cfg(not(target_arch = "wasm32"))]` to these items. Do NOT relax them to markers (Q2). Keep them fully `Send` on native.
- **Validate:** `cargo check -p liquers-core` (native, unchanged) + `cargo check --target wasm32-unknown-unknown -p liquers-core --no-default-features --features async_store` — must not error on these types being absent (nothing wasm-side references them yet; confirms the cut is clean).
- **Agent:** haiku · rust-best-practices · Knowledge: Phase 2 Q2 resolution; grep for wasm-side references first.
- **Rollback:** remove the cfg attributes.
- **Checkpoint M-A:** `cargo check -p liquers-core` (native) green; commit.

### M-B — Axis-1 managers

**Step 4 — `EvalMode`, shared trait methods, shared `load_command_versions` helper.**
- **Files:** `assets.rs`.
- **Change:** add `pub enum EvalMode { Queued, Inline }`; extend `AssetManager` with the required primitives (`eval_mode`, `lookup_key_asset`, `create_temporary_asset`, `start`, `set_envref`, `dependency_manager`, `track_expiration`) and the **default methods** (`cascade_expire_dependents`, `expire_dependencies_result`, `register_plan_dependencies`) per Phase 2. Extract `load_command_versions<E>(dm, cmr)` as a shared free helper (from the current `DefaultAssetManager` method body).
- **Validate:** compiles only after Step 8 (DefaultAssetManager impls the new required methods). Interim: `cargo check -p liquers-core` deferred to M-B checkpoint.
- **Agent:** sonnet · rust-best-practices · Knowledge: Phase 2 Trait section (default-method bodies), Q1/Q4 resolutions.
- **Rollback:** revert `assets.rs` trait edits.

**Step 5 — `Environment::AssetManager` associated type; drop `Arc<Box<…>>`.**
- **Files:** `context.rs` (Environment trait + `EnvRef` mirror + `SimpleEnvironment*` conformance), and every `get_asset_manager` call site that double-derefs (`(**am)` → `am`).
- **Change:** `type AssetManager: AssetManager<Self>`; `get_asset_manager(&self) -> Arc<Self::AssetManager>`; `SimpleEnvironment*`: `type AssetManager = DefaultAssetManager<Self>` (native), field `Arc<DefaultAssetManager<Self>>`.
- **Validate:** part of M-B checkpoint.
- **Agent:** sonnet · rust-best-practices · Knowledge: Phase 2 Environment section; call-site audit (assets.rs, interpreter.rs, context.rs).
- **Rollback:** revert to `Arc<Box<DefaultAssetManager<Self>>>`.

**Step 6 — `run_inline`/`run_immediately_inline`; cfg-out Queued-path spawn/timer carriers.**
- **Files:** `assets.rs`.
- **Change:** add `run_with_future_inline` (`futures::join!` + `futures::select!`), `run_inline`, `run_immediately_inline`; refactor `finish_run_with_result` to take `Result<(), Error>` (Queued caller maps `JoinError`). **[opus A1]** `futures::select!` is not a token-for-token swap for `tokio::select!`: its operands must be `Unpin + FusedFuture`, so budget for `.fuse()` + `futures::pin_mut!` on `wait_to_finish()`/`evaluate_future` (or use the function-form `futures::future::select`, which needs no macro). `futures`' default `async-await` feature supplies the macros. `#[cfg(not(wasm32))]` on `run`/`run_with_future`/`run_immediately`/`new_temporary` (the `tokio::spawn` carriers). `MetadataSaver::save_immediately` and `AssetRef::cancel`: the mode (from `eval_mode()` via envref) selects `Inline` (direct) vs `Queued` (spawn/timeout, `#[cfg(not(wasm32))]` body, wasm arm `unreachable!()`).
- **Validate:** part of M-B checkpoint (native + wasm).
- **Agent:** sonnet · rust-best-practices · Knowledge: Phase 2 Sync-vs-Async (`join!`/`JobFinishing` invariant), Tokio Reduction.
- **Rollback:** revert `assets.rs`; keep `run_with_future` as sole path.

**Step 7 — `ImmediateAssetManager`.**
- **Files:** `assets_immediate.rs` (new), `assets.rs`/`lib.rs` re-export.
- **Change:** struct with `std::sync::Mutex<HashMap>` maps (brief locking, never across `.await`), `OnceCell` `started`; full `AssetManager` impl — inline `get_asset`/`get`/`apply`/`apply_immediately`, lazy `start()` (calls shared helper), lazy expiration-on-access, `eval_mode()==Inline`, `lookup_key_asset`, `create_temporary_asset`. Dual `cfg_attr` on the impl.
- **Validate:** part of M-B checkpoint.
- **Agent:** sonnet · rust-best-practices · Knowledge: Phase 2 ImmediateAssetManager semantics; reuse `AssetRef`/`AssetData`/`DependencyManager` unchanged.
- **Rollback:** delete `assets_immediate.rs` + re-export.

**Step 6b — [opus B2] gate the `persist_with_status_tracking` background spawn.**
- **Files:** `assets.rs` (`persist_with_status_tracking`, assets.rs:1139-1153; `AssetData` `save_in_background` field/default at 264/349).
- **Change:** the background-persist `tokio::spawn` at assets.rs:1147 is on the **always-compiled inline path** (`evaluate_and_store`→`persist_with_status_tracking`, `save_in_background` defaults `true`) — it was missed by the Phase 2 carriers table. Fix: in inline mode persist **synchronously** via the existing `else` branch (assets.rs:1151, already `self.save_to_store().await`); gate the spawn branch `#[cfg(not(target_arch = "wasm32"))]`, and force `save_in_background = false` for `Inline`-mode assets (read `eval_mode()` at the `persist_with_status_tracking` call sites 1580/2141, or set the flag when the immediate manager creates the asset). Without this, wasm panics at runtime and fails the Step 9 `["sync"]` checkpoint (no `rt`).
- **Validate:** part of M-B checkpoint (native + wasm).
- **Agent:** sonnet · rust-best-practices · Knowledge: this finding + Phase 2 Tokio Reduction.
- **Rollback:** ungate the spawn.

**Step 8 — `DefaultAssetManager` native-only + conform to extended trait.**
- **Files:** `assets.rs`.
- **Change:** `#[cfg(not(target_arch = "wasm32"))]` on `DefaultAssetManager`, `JobQueue`, expiration monitor, `RunClaim`. Implement the new required trait methods (`eval_mode()==Queued`, `lookup_key_asset` over its `scc` map, `create_temporary_asset`, `start`→shared helper, `track_expiration` as today). Default methods inherited.
- **[opus B1] `remove_expired_from_maps` must be lifted onto the `AssetManager` trait.** `run_expiration_monitor` (assets.rs:2499, a static fn with no `self`) reaches the manager only via `envref.get_asset_manager()` and calls `manager.remove_expired_from_maps(...)` at assets.rs:2608 — a site the Phase 2 audit missed. After Step 5 the return type is `Arc<E::AssetManager>`, so this is an `E0599` on the **native** M-B checkpoint unless the method is on the trait. Add `async fn remove_expired_from_maps(&self, asset_id: u64, query: Option<&Query>, key: Option<&Key>) -> bool` to the trait (`DefaultAssetManager` keeps its body at 2777; `ImmediateAssetManager` implements it over its `Mutex<HashMap>` maps — harmless, and it has no monitor). Add this to Step 4's trait-method list.
- **Validate:** **M-B checkpoint** — `cargo test -p liquers-core` (native) + `cargo check --target wasm32-unknown-unknown -p liquers-core --no-default-features --features async_store`.
- **Agent:** sonnet · rust-best-practices · Knowledge: Phase 2; existing DefaultAssetManager method bodies.
- **Rollback:** remove cfg gates; revert trait-method wiring.

**Step 9 — wasm tokio features → `["sync"]`.**
- **Files:** `liquers-core/Cargo.toml` `[target.'cfg(target_arch = "wasm32")'.dependencies].tokio`.
- **Change:** `features = ["sync"]` (drop `rt`/`macros`/`time`).
- **Validate:** `cargo check --target wasm32-unknown-unknown -p liquers-core --no-default-features --features async_store` — proves no residual `spawn`/`time`/tokio-macro on the wasm path. **Commit M-B.**
- **Agent:** haiku · rust-best-practices · Knowledge: Phase 2 Tokio Reduction.
- **Rollback:** restore `["sync","rt","macros","time"]`.

### M-C — downstream + environment selection

**Step 10 — `DefaultEnvironment` (liquers-lib): cfg-selected manager + init split + test-support `ImmediateEnvironment`.**
- **Files:** `liquers-lib/src/environment.rs`; `liquers-core/src/context.rs` (test-support `ImmediateEnvironment`).
- **Change:** `type AssetManager` cfg-selected (`DefaultAssetManager` native / `ImmediateAssetManager` wasm); field/constructor cfg-selected; `init_with_envref` cfg-split (native spawn `start()`, wasm `set_envref` only); dual `cfg_attr` on impls; drop `Box`. Add minimal `ImmediateEnvironment<V,P>` (mirrors `SimpleEnvironment` with the immediate manager) for the parametric tests — placed in core so core tests use it; keep it lightweight.
- **Validate:** native workspace `cargo check`; `cargo check --target wasm32 -p liquers-lib`.
- **Agent:** sonnet · rust-best-practices · Knowledge: Phase 2 liquers-lib integration + Q3 (test-support env only, future BrowserEnvironment out of scope).
- **Rollback:** revert env changes.

**Step 11 — `liquers-py` + `liquers-axum` conformance.**
- **Files:** `liquers-py/src/context.rs:102`, `liquers-axum/src/**` env-consuming impls.
- **Change:** py `type AssetManager = DefaultAssetManager<Self>` + return type + dual `cfg_attr`; axum needs recompile only (no env impl there — Phase 2 finding). liquers-store `opendal_store.rs:283` dual `cfg_attr` (native-only, uniformity).
- **Validate:** `cargo check -p liquers-py -p liquers-axum -p liquers-store` (native).
- **Agent:** haiku · rust-best-practices · Knowledge: Phase 2 py/axum/store integration notes.
- **Rollback:** revert conformance edits.

**Step 12 — interpreter/plan temp-asset + BoxFuture wiring.**
- **Files:** `interpreter.rs` (line 355 `create_temporary_asset`; BoxFuture returns), `plan.rs` (BoxFuture).
- **Change:** temp asset via `envref.get_asset_manager().create_temporary_asset()`; boxed-future returns → `BoxFuture`. (May already be done incidentally in Step 2/5; this step is the sweep to confirm.)
- **Validate:** **M-C checkpoint** — native workspace `cargo check` + `cargo check --target wasm32 -p liquers-lib`. Commit.
- **Agent:** haiku · rust-best-practices · Knowledge: Phase 2 interpreter section.
- **Rollback:** revert.

**Step 12b — [opus A2] audit the liquers-lib wasm render path for wasm-illegal time calls.**
- **Files:** `liquers-lib/src/ui/widgets/query_console_element.rs:139` (`std::time::Instant::now()`, field at :68), and a grep sweep for `Instant::now`/`SystemTime::now` across the wasm-compiled `liquers-lib/src/ui` tree.
- **Change:** `Instant::now()` **panics at runtime on `wasm32-unknown-unknown`** and is compiled (not cfg'd). Since the acceptance target is the demo *running*, replace/gate it on wasm (e.g. `web_sys::window().performance().now()` behind `#[cfg(target_arch="wasm32")]`, or feature-gate the timing entirely). Even if `ui_spec_demo`'s dashboard doesn't instantiate the query console today, this removes a latent E4 `pageerror` and future footgun.
- **Validate:** `cargo check --target wasm32 -p liquers-lib`; confirmed green at E4 (no `pageerror`).
- **Agent:** haiku · rust-best-practices · Knowledge: this finding; wasm has no `std::time::Instant`.
- **Rollback:** revert the gate.
- **Checkpoint M-C:** native workspace + `cargo check --target wasm32 -p liquers-lib`. Commit.

### M-D — tests

**Step 13 — parametric harness + immediate-only tests.**
- **Files:** `liquers-core/tests/manager_parametric.rs` (new); lifted scenarios referencing existing tests; `liquers-core/Cargo.toml` `[dev-dependencies]` add `futures = { version = "0.3", features = ["executor"] }`; a tiny `combine` test command for E2.
- **Change:** implement T1–T8, T12, T13 per Phase 3 (generic `scenario_*<E>` + `_default`/`_immediate` wrappers; deterministic T5; no-runtime T7 via `futures::executor::block_on`).
- **Validate:** **M-D checkpoint** — `cargo test -p liquers-core` all green (new + existing). Commit.
- **Agent:** sonnet · liquers-unittest + rust-best-practices · Knowledge: Phase 3 test plan; existing test style in `tests/asset_failure_contract.rs`.
- **Rollback:** delete the new test file + dev-dep.

### M-E — acceptance

**Step 14 — build matrix.**
- **Files:** none (CI/commands); optionally a `just`/script note.
- **Change:** run T10/T11/T14: wasm `-p liquers-core -p liquers-lib`; native `-p liquers-core/store/axum/py`; egui/webui/polars combos; assert `Cargo.toml` wasm tokio features `== ["sync"]`; wasm-target `DependencyManager<!Send>` compile check.
- **Validate:** all green.
- **Agent:** haiku · rust-best-practices · Knowledge: Phase 3 build-matrix.
- **Rollback:** n/a (diagnostic).

**Step 15 — `ui_spec_demo` Playwright e2e.**
- **Files:** `liquers-lib/examples-web/ui_spec_demo/tests/webui.spec.ts` (new), `playwright.config.ts` (new); `package.json` if needed for `@playwright/test`.
- **Change:** implement E4 — `trunk serve` webServer, navigate, assert render + click-reaction + zero console/`pageerror`; pin selectors against the running page (using webui M3 SSR structure).
- **Validate:** **M-E checkpoint** — `trunk build` then `npx playwright test` green (headless Chromium at `PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers`, no `playwright install`). This is the acceptance gate.
- **Agent:** sonnet · Knowledge: Phase 3 E4; `specs/webui/phase4` Playwright plan; `ui_spec_demo/src/lib.rs` + `ui::web` markup.
- **Rollback:** stop `trunk serve`; `rm` the test files.

### M-F — docs & follow-up

**Step 16 — docs + tracked follow-up.**
- **Files:** `specs/ISSUES.md` (full-tokio-removal / executor-agnostic follow-up — explicitly out of scope), `specs/webui/DESIGN.md` (mark the async-on-wasm follow-up resolved, link here), `specs/PROJECT_OVERVIEW.md` (note `Environment::AssetManager` + wasm support), `CLAUDE.md` (async-on-wasm note if warranted), `specs/async-wasm-refactor/DESIGN.md` (mark phases complete).
- **Validate:** docs consistent; no code impact.
- **Agent:** haiku · Knowledge: all four phase docs.
- **Rollback:** revert doc edits.

## Testing Plan

- **Per step:** the step's `Validate` command (fast `cargo check` inside a milestone).
- **Milestone gates (must be green before commit):** M-A native check; M-B native `test` + wasm `check`; M-C native workspace + wasm-lib; M-D native `test`; M-E full matrix + Playwright.
- **Unit/integration (M-D):** T1–T8, T12, T13 (Phase 3). Existing suites (`asset_failure_contract`, volatility, dependency, scheduling, expiration) must still pass unchanged = native-behavior-preserved proof.
- **Build matrix (M-E):** T10/T11/T14 — the wasm build is the authoritative `E0053`/residual-spawn/cfg-out check.
- **E2E (M-E):** E4 Playwright — the acceptance gate.
- **Regression watch:** `join!`/`JobFinishing` (T12 hangs on regress); no-runtime proof (T7 panics on a reintroduced spawn).

## Agent Assignment

| Step | Model | Skills | Key knowledge |
|---|---|---|---|
| 1 | haiku | rust-best-practices | Phase 2 maybe_send |
| 2 | sonnet | rust-best-practices | Axis-2 surface table + impl sites |
| 3 | haiku | rust-best-practices | Q2; wasm-ref grep |
| 4 | sonnet | rust-best-practices | Trait defaults, Q1/Q4 |
| 5 | sonnet | rust-best-practices | Environment assoc type + call sites |
| 6 | sonnet | rust-best-practices | join!/JobFinishing, tokio reduction |
| 7 | sonnet | rust-best-practices | ImmediateAssetManager semantics |
| 8 | sonnet | rust-best-practices | DefaultAssetManager bodies |
| 9 | haiku | rust-best-practices | tokio reduction |
| 10 | sonnet | rust-best-practices | liquers-lib env, Q3 |
| 11 | haiku | rust-best-practices | py/axum/store notes |
| 12 | haiku | rust-best-practices | interpreter section |
| 13 | sonnet | liquers-unittest, rust-best-practices | Phase 3 test plan |
| 14 | haiku | rust-best-practices | Phase 3 build matrix |
| 15 | sonnet | — | Phase 3 E4, webui Playwright plan |
| 16 | haiku | — | all phase docs |

**Orchestration note:** M-A→M-B→M-C are sequential (later steps depend on earlier signatures). Within M-B, Steps 4–8 are tightly coupled and best done by **one sonnet agent in a single pass** (the trait, both managers, and the env assoc type must land together to compile); Step 9 follows. M-D/M-E after M-C is green. Steps are not parallelizable across milestones.

## Rollback Plan

- **Per step:** listed above (each is a localized git revert or file delete).
- **Per milestone:** each milestone is a single commit; `git revert <milestone-commit>` backs it out cleanly. M-A is a native no-op (safe to keep even if later milestones are deferred). M-B is the large one — if wasm doesn't go green, the native path is unaffected (all wasm-specific behavior is behind `cfg(target_arch="wasm32")` and `DefaultAssetManager` is unchanged on native).
- **Whole feature:** the branch is `claude/liquers-webui-design-hvy9c2`; the design docs (M-F) and code are separable — reverting code milestones leaves the specs intact.
- **Acceptance contingency:** if E4 (Playwright) reveals a runtime gap not caught by compilation (e.g. an unexpected `!Send` in `ui::web` held across an await through a core `Send`-bounded seam), the fix is localized to that seam (relax the specific bound or adjust the `ui::web` call) — it does not invalidate M-A…M-D. Documented as the one integration risk.

## Documentation Updates

- `specs/PROJECT_OVERVIEW.md`: `Environment::AssetManager` associated type; wasm/browser support; `ImmediateAssetManager`.
- `specs/ISSUES.md`: full-tokio-removal + executor-agnostic core as tracked, out-of-scope follow-up.
- `specs/webui/DESIGN.md`: async-on-wasm follow-up resolved → link here.
- `specs/async-wasm-refactor/DESIGN.md`: phases complete.
- `CLAUDE.md`: brief async-on-wasm / manager-selection note if warranted.

## Review outcome (4-reviewer + opus, applied)
- **Sequencing reviewer:** all milestone checkpoints build-order sound; Steps 4–8 coupling correctly flagged as a single pass.
- **Opus cross-phase reviewer:** two BLOCKING misses now folded in as explicit steps — **B1** (`remove_expired_from_maps` lifted onto the trait, Step 8) and **B2** (`persist_with_status_tracking` background spawn gated, Step 6b); plus **A2** (liquers-lib `Instant::now()` wasm audit, Step 12b) and **A1** (`futures::select!` fuse/pin, Step 6). A3/A4 are wording/effort calibrations (recorded in Phase 2).

## Open Questions
None blocking. The single integration risk (a residual `!Send` in the `ui::web`→core seam surfacing only at E4 runtime) is documented in Rollback; it is localized and does not affect earlier milestones.
