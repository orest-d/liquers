# Phase 4: Implementation Plan - Environment Builder

## Overview

**Feature:** Environment Builder (resolves `QUEUED-MANAGER-STARTUP-READINESS`, P1)

**Architecture:** One `GenericEnvironment<V, P, K>` replaces four near-duplicate structs, with the
existing names surviving as aliases; `Environment::try_to_ref` owns a single readiness sequence
(refresh metadata versions → wrap in `EnvRef` → `init_with_envref` constructs, installs and starts
the manager), and `EnvironmentBuilder::build` delegates to it. `EnvironmentConfig` in `liquers-core`
configures the environment and its store from one document.

**Estimated complexity:** High. Two trait contracts change (`Environment`, `AssetManager`), one
1 100-line consolidation, and a new configuration layer.

**Estimated time:** 14–20 hours for an experienced Rust developer, spread as: steps 1–4 ≈ 5 h
(the contract change, and the riskiest part), step 5 ≈ 4 h (consolidation), steps 6–8 ≈ 3 h,
step 9 ≈ 3 h (tests), step 10 ≈ 2 h (configuration), steps 11–12 ≈ 2 h (docs and issues).

**Prerequisites:**
- Phases 1, 2, 3 approved (Phase 3 approved 2026-08-31).
- Gate decisions D1 (`to_ref` stays) and D2 (one configuration document) applied to Phases 1–3.
- All four prerequisite designs merged: `store-factories-in-core` (PR 46),
  `recipe-provider-selection` (PR 48), `command-declaration` (PR 50),
  `payload-env-recipe-provider-fallback` (PR 51).
- No new crate dependencies. `scc` 3.4.8, `serde`, `tokio` are already workspace dependencies.

### Open questions resolved during planning

Two of the three questions Phase 3 carried are answerable from the code, and are answered here
rather than left to the implementer.

**Q1 — `refresh_command_versions` cascade application. Resolved: return the changed keys; do not
apply.** `DependencyManager::expire_dependents` reaches `scc` through `get_async` and `iter_async`
(`liquers-core/src/dependencies.rs:380-390`), so a synchronous `register_version_sync` *cannot*
compute an `ExpiredDependents` — Phase 2's proposed signature is unimplementable as written. The
shape that works splits the sync detection from the async cascade:

```rust
// sync: registers, reports whether the stored version changed
pub fn register_version_sync(&self, key: &DependencyKey, version: Version) -> bool;

// AssetManager: sync, returns the keys whose version changed
fn refresh_command_versions(&self) -> Result<Vec<DependencyKey>, Error>;

// AssetManager: provided async companion that applies the cascade
async fn refresh_command_versions_and_expire(&self) -> Result<(), Error> {
    for key in self.refresh_command_versions()? {
        self.cascade_expire_dependents(&key).await;
    }
    Ok(())
}
```

First `start()` is unaffected: every key is `Vacant`, so `register_version_sync` always returns
`false` and the returned vector is empty. The readiness guarantee stays synchronous, and
`POST-INIT-COMMAND-REGISTRATION` gets the async hook it needs without making startup async.

**Q2 — `Queued` present-but-unusable on wasm. Resolved: not needed.** Grep for
`DefaultAssetManager` outside `liquers-core` finds only `liquers-lib/src/environment.rs` (behind
`#[cfg(not(target_arch = "wasm32"))]`), `liquers-py` (a native-only crate) and two native binaries
and tests. No wasm code path names it, so `#[cfg(not(target_arch = "wasm32"))] pub struct Queued`
is safe and `DefaultKind = Inline` on wasm covers every browser build.

**Q3 — remains open, and is a Step 1 spike.** Whether `scc::HashMap` 3.4.8 exposes `entry_sync`.
`read_sync` is used in tree (`liquers-core/src/assets.rs:5170`), so the `_sync` family exists;
`entry_sync` specifically could not be confirmed offline (docs.rs is unreachable from this
environment). Step 1 settles it in five minutes with a fallback that does not change any signature.

## Implementation Steps

Steps 2–5 change two trait contracts and must land as one reviewable unit — the tree does not
compile between 4 and 5's start. Each step below still states its own validation, because the
sequence is what keeps the *diff* reviewable, not what keeps `main` green; the whole run is one PR.

---

### Step 1: Confirm `scc`'s synchronous entry API

**File:** none (spike).

**Action:**
- Add a temporary `#[test]` in `liquers-core/src/dependencies.rs` that calls
  `self.versions.entry_sync(key.clone())` and matches on
  `scc::hash_map::Entry::{Occupied, Vacant}`.
- If it compiles, delete the spike and proceed. If it does not, use the fallback: `insert_sync`
  returns `Err((key, value))` when the entry is occupied, and `update_sync` compares and replaces —
  two calls instead of one, same semantics, no signature change anywhere.

**Validation:**
```bash
cargo check -p liquers-core --lib
```

**Rollback:** delete the spike test.

**Agent Specification:**
- **Model:** haiku
- **Skills:** none
- **Knowledge:** `liquers-core/src/dependencies.rs:159-181` (the async `register_version`),
  `liquers-core/src/assets.rs:5170` (the in-tree `read_sync` precedent)
- **Rationale:** a single mechanical compile check with a written fallback.

---

### Step 2: Synchronous dependency-version registration

**File:** `liquers-core/src/dependencies.rs`

**Action:**
- Add `register_version_sync`, the synchronous counterpart of `register_version`, returning whether
  the stored version changed rather than the dependents to expire (see Q1).
- Leave `register_version` untouched; every other caller keeps using it.

**Code changes:**
```rust
// NEW
impl<E: Environment> DependencyManager<E> {
    /// Synchronous counterpart of [`Self::register_version`], for the uncontended startup path.
    ///
    /// Returns `true` when the stored version differed from `version`. It deliberately does **not**
    /// return [`ExpiredDependents`]: computing them requires `expire_dependents`, which is async.
    /// The caller decides what to do with a changed key — at first startup nothing can have
    /// changed, and [`AssetManager::refresh_command_versions_and_expire`] applies the cascade.
    pub fn register_version_sync(&self, key: &DependencyKey, version: Version) -> bool {
        match self.versions.entry_sync(key.clone()) {
            scc::hash_map::Entry::Occupied(mut entry) => {
                let changed = *entry.get() != version;
                *entry.get_mut() = version;
                changed
            }
            scc::hash_map::Entry::Vacant(entry) => {
                entry.insert_entry(version);
                false
            }
        }
    }
}
```

Explicit match on both `scc::hash_map::Entry` variants, no default arm.

**Validation:**
```bash
cargo check -p liquers-core --lib
cargo test -p liquers-core --lib dependencies
```

**Rollback:** `git checkout liquers-core/src/dependencies.rs`

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `dependencies.rs:159-181`, Phase 2 §`DependencyManager` — a sync registration path,
  Q1 above
- **Rationale:** small but semantically load-bearing; the return type differs from Phase 2's draft
  and the reason must be preserved in the doc comment.

---

### Step 3: Environment-side manager slot

**File:** `liquers-core/src/context.rs`

**Action:**
- Change each of the four environments' `asset_store` field from `Arc<Manager<Self>>` to
  `OnceLock<Arc<Manager<Self>>>`, initialized empty in `new_with_type_registry`.
- Move manager construction into `init_with_envref` — still using today's `set_envref` and detached
  `start`, which is what makes this step compile on its own.
- `get_asset_manager` reads the slot.

**Why separately:** it moves the deferred slot from the manager to the environment (Phase 1,
question 2) *without* touching `assets.rs`, so the tree compiles and the whole existing test suite
runs against the new ownership before the trait contract changes. If a test breaks here, the cause
is the slot, not the contract.

**Code changes:**
```rust
// MODIFY (×4: SimpleEnvironment, SimpleEnvironmentWithPayload,
//              ImmediateEnvironment, ImmediateEnvironmentWithPayload)
-   asset_store: Arc<DefaultAssetManager<Self>>,
+   asset_store: std::sync::OnceLock<Arc<DefaultAssetManager<Self>>>,

// MODIFY: construction no longer happens here
-   asset_store: Arc::new(crate::assets::DefaultAssetManager::new()),
+   asset_store: std::sync::OnceLock::new(),

// MODIFY: the hook constructs, installs and (still, for now) spawns
    fn init_with_envref(&self, envref: EnvRef<Self>) {
        let manager = Arc::new(crate::assets::DefaultAssetManager::new());
        manager.set_envref(envref);
        let _ = self.asset_store.set(manager.clone());
        tokio::spawn(async move { manager.start().await; });
    }

// MODIFY: read the slot. Unset is unreachable — init_with_envref writes it before any EnvRef
//         is observable — so this is debug_assert plus the value, never expect().
    fn get_asset_manager(&self) -> Arc<DefaultAssetManager<Self>> { … }
```

**Validation:**
```bash
cargo test -p liquers-core --lib --tests
```

**Rollback:** `git checkout liquers-core/src/context.rs`

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `context.rs:965-1130` and the three sibling environments,
  Phase 1 question 2, Phase 2 §`GenericEnvironment`
- **Rationale:** four parallel edits with one subtlety — `get_asset_manager` must not introduce an
  `expect`, which the constraints forbid in library code.

---

### Step 4: `AssetManager` lifecycle — constructors take the `EnvRef`, startup is sync and fallible

**File:** `liquers-core/src/assets.rs` (and the four `init_with_envref` bodies from step 3)

**Action:**
- `DefaultAssetManager::new(envref)` / `with_capacity(envref, capacity)` and
  `ImmediateAssetManager::new(envref)` take the `EnvRef` and store it as a plain strong field.
- Delete both `envref: OnceLock<EnvRef<E>>` fields, `set_envref` (inherent and trait), and the two
  `expect("Environment not set …")` calls they existed to guard.
- `AssetManager::start` becomes `fn start(&self) -> Result<(), Error>`, built on a new
  `load_command_versions_sync`.
- Add `refresh_command_versions`, its async companion, and `is_started`.
- Replace `ImmediateAssetManager`'s `tokio::sync::OnceCell` with `AtomicBool` and remove the five
  lazy `ensure_started()` calls at the inline entry points — `try_to_ref` has already started the
  manager.
- Delete the stray `eprintln!("Spawned job queue")` (`assets.rs:3902`).
- Update the four `init_with_envref` bodies from step 3 to construct with the `EnvRef` and call the
  now-sync `start()`, dropping the `tokio::spawn`.

**Code changes:**
```rust
// NEW (private helper beside the async load_command_versions at assets.rs:3236)
pub(crate) fn load_command_versions_sync<E: Environment>(
    dm: &DependencyManager<E>,
    cmr: &CommandMetadataRegistry,
) -> Vec<DependencyKey>;   // the keys whose version changed; empty at first startup

// MODIFY: AssetManager<E>
-   fn set_envref(&self, envref: EnvRef<E>);
-   async fn start(&self);
+   fn start(&self) -> Result<(), Error>;
+   fn refresh_command_versions(&self) -> Result<Vec<DependencyKey>, Error>;
+   async fn refresh_command_versions_and_expire(&self) -> Result<(), Error> { /* provided */ }
+   fn is_started(&self) -> bool;

// MODIFY: constructors
-   impl<E: Environment> DefaultAssetManager<E> { pub fn new() -> Self; }
+   impl<E: Environment> DefaultAssetManager<E> {
+       pub fn new(envref: EnvRef<E>) -> Self;
+       pub fn with_capacity(envref: EnvRef<E>, capacity: usize) -> Self;
+   }

// DELETE
-   envref: std::sync::OnceLock<EnvRef<E>>,        // assets.rs:3853 and 5613
-   eprintln!("Spawned job queue");                // assets.rs:3902
```

`start` and `refresh_command_versions` return `Result` although neither can fail today. That is
deliberate and Phase 4 must not "simplify" it: a manager restoring a persisted dependency graph from
a store is the case the signature is reserved for, and adding the `Result` later is breaking.

**Validation:**
```bash
cargo test -p liquers-core --lib --tests
cargo test -p liquers-core --test manager_parametric
```

**Rollback:** `git checkout liquers-core/src/assets.rs liquers-core/src/context.rs`

**Agent Specification:**
- **Model:** opus
- **Skills:** rust-best-practices
- **Knowledge:** `assets.rs:3236-3257` (`load_command_versions`), `3540-3660` (the trait),
  `3853`, `3902`, `5157-5210`, `5613-5680`, `6050-6095`; Phase 2 §`AssetManager`, Q1 above
- **Rationale:** the riskiest step. It edits a 9 000-line file, changes a trait every manager and
  environment implements, and deletes two panics whose absence must be *provably* unreachable
  rather than merely likely.

---

### Step 5: Consolidate the four environments into `GenericEnvironment`

**File:** `liquers-core/src/context.rs`

**Action:**
- Add `AssetManagerKind`, `Queued`, `Inline`, `DefaultKind`, `AssetManagerOptions` (new module
  `liquers-core/src/environment_builder.rs`, re-exported).
- Replace the four structs with `GenericEnvironment<V, P = (), K = DefaultKind>` and the four
  compatibility aliases.
- Drop the dead `store: Arc<dyn Store>` field, `with_store` and the always-panicking `with_cache`.
- `recipe_provider` becomes a non-optional `Arc`, resolved at construction; the per-call
  `eprintln!("No recipe provider configured …")` goes with it.
- `Environment`: `init_with_envref` becomes `Result<(), Error>`; add the provided `try_to_ref`;
  `to_ref` keeps its signature and delegates.
- Deprecate `EnvRef::new`.

**Code changes:** as specified in Phase 2 §`GenericEnvironment`, §Compatibility aliases,
§`Environment`. The four structs are already structurally identical after steps 3–4, and
`liquers-lib`'s `DefaultEnvironment` (`liquers-lib/src/environment.rs:30-37`) is *already* the
generic shape — same fields, non-optional `recipe_provider`, `PhantomData<P>` — so it doubles as a
worked reference for what `GenericEnvironment` should look like.

**Validation:**
```bash
cargo test -p liquers-core --lib --tests
bash scripts/check-build-matrix.sh
```

**Rollback:** `git checkout liquers-core/src/context.rs liquers-core/src/environment_builder.rs`

**Agent Specification:**
- **Model:** opus
- **Skills:** rust-best-practices
- **Knowledge:** `context.rs:150-232` (the trait), `965-1130`, `1134-1268`, `1269-1400`,
  `1955-2113`; `liquers-lib/src/environment.rs:30-100`; Phase 2 §Data Structures and §`Environment`
- **Rationale:** ~1 100 lines collapse to ~250, and a GAT-carrying trait is introduced. Type
  identity must be preserved exactly or every downstream crate breaks at once.

---

### Step 6: `EnvironmentBuilder`

**File:** `liquers-core/src/environment_builder.rs`

**Action:**
- `EnvironmentBuilder<V, P = (), K = DefaultKind>` with the by-value setters from Phase 2, a public
  `command_registry` field, and `build(self) -> Result<EnvRef<…>, Error>` that resolves services and
  calls `env.try_to_ref()`.
- Defaults: `RecipeProviderChoice::Trivial`, `NoAsyncStore`, `TypeRegistry::from_value_type::<V>()`.

**Validation:**
```bash
cargo test -p liquers-core --lib --tests
```

**Rollback:** `git checkout liquers-core/src/environment_builder.rs`

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 §`EnvironmentBuilder` inherent API, Phase 3 Scenarios 1a/1b/2a
- **Rationale:** mechanical once step 5 lands; `build()` is now a short function because the
  sequence lives in `try_to_ref`.

---

### Step 7: `liquers-lib` — alias, extension trait, library builder

**File:** `liquers-lib/src/environment.rs`

**Action:**
- Delete the `SelectedAssetManager` cfg-import pair (lines 16–22); `DefaultKind` replaces it.
- `pub type DefaultEnvironment<V, P = ()> = GenericEnvironment<V, P>;`
- `register_polars_commands` moves from an inherent method on `DefaultEnvironment<Value>` to a local
  `PolarsCommandRegistration` trait — an inherent `impl` is permitted only in the defining crate,
  and a type alias creates no new type (Phase 2, finding B1).
- Add `default_environment_builder`, carrying `RecipeProviderChoice::Default` — the library default,
  which the core builder deliberately does not share.
- `CommandRegistryAccess` is unaffected: a local trait may be implemented for a foreign type.

**Validation:**
```bash
cargo test -p liquers-lib --lib --tests
cargo test -p liquers-lib --test polars_commands --features polars
cargo test -p liquers-lib --test registry_export
```

**Rollback:** `git checkout liquers-lib/src/environment.rs`

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-lib/src/environment.rs` (whole file), `liquers-lib/tests/polars_commands.rs:17-22`,
  Phase 2 §Compatibility aliases and §The recipe-provider default is per-crate
- **Rationale:** the coherence constraint is the one place a plausible-looking shortcut does not
  compile, and the recipe-provider default is the one place a plausible-looking simplification is a
  silent behavior regression.

---

### Step 8: Migrate `liquers-web`, `liquers-axum`, `liquers-py`

**Files:** `liquers-web/src/environment.rs`, `liquers-axum/examples/*.rs`, `liquers-py/src/context.rs`

**Action:**
- `liquers-web`: `new_environment()` returns an `EnvironmentBuilder`; `build_environment()` calls
  `.build()`. The `REGISTERED_SPECS` / `STORE_CONFIG` / `STORE_OBJECTS` replay is preserved
  unchanged — it must keep working on every rebuild path. Correct the stale comment above
  `with_default_recipe_provider` while editing (it cites a panic that `LIB-RECIPE-PROVIDER-PANIC`
  closed). **Do not** migrate `apply_store` onto `EnvironmentConfig` — that is
  `WEB-STORE-CONFIG-NOT-APPLIED-THROUGH-ENVIRONMENT-CONFIG`, filed and deliberately out of scope.
- `liquers-axum`: three examples move `env.with_async_store(Box::new(s)); … env.to_ref()` to the
  builder chain with `Arc`.
- `liquers-py`: `init_with_envref` is a `todo!()`; it must become a real implementation or an
  explicit `Err`, because the method is now fallible and its contract carries the readiness
  guarantee. A `todo!()` left in place is a panic on a supported path.

**Validation:**
```bash
cargo check -p liquers-axum --examples
cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

**Rollback:** `git checkout liquers-web liquers-axum liquers-py`

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-web/src/environment.rs` (whole file — the borrow rule in its module doc is
  binding), `liquers-axum/examples/basic_server.rs:61-69`, `liquers-py/src/context.rs:85-120`,
  Phase 3 Scenarios 1b and 2a
- **Rationale:** `liquers-web`'s `RefCell` borrow rule ("no borrow held across an `await` or a call
  into JavaScript") is the crate's stated most important invariant, and the rebuild path is its most
  delicate code.

---

### Step 9: Tests

**Files:** `liquers-core/tests/environment_builder.rs` *(new)*,
`liquers-core/tests/manager_parametric.rs`, `liquers-core/tests/dependency_manager_integration.rs`,
plus unit tests in `liquers-core/src/environment_builder.rs`

**Action:** implement T1–T14 from Phase 3, and delete the timing dependency.

| Test | Where | Note |
|---|---|---|
| T1 `build_returns_a_started_manager` | unit | `is_started()` true on return |
| T2 `plan_dependencies_registered_on_first_evaluation` | integration | **the original bug**; returned 0 edges before |
| T3 `command_version_present_immediately_after_build` | integration | the Phase 1 reproduction, inverted, no sleep |
| T4 `concurrent_first_evaluations_share_one_startup` | integration | issue verification item 3 |
| T5 `startup_failure_propagates_from_build` | unit | test kind whose `start` returns `Err`; no `EnvRef` produced |
| T6 `readiness_equivalent_across_kinds` | `manager_parametric.rs` | issue verification item 5; the file is already parametric |
| T7 `refresh_command_versions_expires_dependents` | integration | via `refresh_command_versions_and_expire` |
| T8 `refresh_is_idempotent_when_nothing_changed` | unit | second call returns an empty key list |
| T9 `recipe_provider_defaults_across_all_aliases` | unit | both per-crate defaults asserted |
| T10 `to_ref_produces_a_ready_envref` | integration | no `#[allow(deprecated)]`; `try_to_ref` agrees |
| T11 `aliases_are_the_generic_type` | compile-time | type identity preserved |
| T12 `inline_builds_without_a_tokio_runtime` | unit | plain `#[test]`; extend the existing no-runtime proof |
| T13 `build_refreshes_command_metadata_versions` | unit | the refreshed version reaches the dependency graph |
| T14 `custom_environment_gets_the_readiness_guarantee` | integration | a test-local `Environment` implementing only `init_with_envref` |

- **Delete** `dependency_manager_integration.rs:89-90` — the `yield_now()` plus
  `sleep(Duration::from_millis(50))`. The assertion becomes deterministic; that deletion is the
  clearest evidence the issue is closed.

**Validation:**
```bash
cargo test -p liquers-core --lib --tests
cargo test -p liquers-lib --lib --tests
```

**Rollback:** `git checkout liquers-core/tests`

**Agent Specification:**
- **Model:** sonnet
- **Skills:** liquers-unittest, rust-best-practices
- **Knowledge:** Phase 3 §Test Plan, `manager_parametric.rs:1-45` (the parametric contract and its
  module doc), `dependency_manager_integration.rs:80-95`
- **Rationale:** T2, T4 and T14 are the tests that would catch a regression of the actual defect;
  they need care rather than volume.

---

### Step 10: `EnvironmentConfig` — final, separable

**File:** `liquers-core/src/environment_config.rs` *(new)*

**Action:**
- The three-field serde struct from Phase 2 §`EnvironmentConfig`, with
  `from_yaml`/`from_json`/`from_toml`/`to_yaml`/`to_json`/`expand_env_vars` delegating the store
  half to `StoreRouterConfig`.
- Builder: `with_store_config`, `with_store_config_unexpanded`, `with_config`. Construction and
  `${VAR}` expansion are deferred to `build()`, so no setter is fallible.
- Tests T15 (`config_roundtrips_and_applies`, including the documented `recipes:`-absent asymmetry)
  and T16 (`config_errors_surface_at_build`).

**Separable by construction:** nothing in steps 1–9 depends on this module. If it turns up a
surprise, drop the step, file the remainder as an issue, and the P1 readiness fix still ships.

**Validation:**
```bash
cargo test -p liquers-core --lib --tests
bash scripts/check-build-matrix.sh
```

**Rollback:** `git rm liquers-core/src/environment_config.rs` and revert the builder setters — no
other code references them.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/store_config.rs` (the surface to mirror),
  `liquers-core/src/store_factory.rs:534-585` (`StoreRouterBuilder`, and the fact that its `build`
  is synchronous), `liquers-core/src/recipes.rs` (`RecipeProviderChoice` and its `#[default]`),
  Phase 3 Scenario 4
- **Rationale:** ordinary serde work whose only trap is the `recipes:`-absent default, which is
  documented and pinned by T15.

---

### Step 11: Documentation

**Files:** `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` *(new)*,
`specs/reference/ENVIRONMENT_CONFIG.md` *(new)*,
`specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`,
`specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`,
`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`, `specs/reference/PAYLOAD_GUIDE.md`,
`specs/reference/STORE_CONFIG_FSD.md`, `CLAUDE.md`, `specs/README.md`

**Action:** as specified in Phase 2 §Documentation Architecture. Every `reference/` or `guides/`
edit adds a `## History` row and bumps `reviewed:` in the same commit (§9.2). `DOC_04`'s P0 and P1
gap rows are retired. `CLAUDE.md` §Adding a Value Type stops pointing at `new_with_type_registry`.

**Validation:**
```bash
python3 scripts/docs_index.py --check
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** none
- **Knowledge:** `specs/DOCS_STRUCTURE_GUIDE.md` §9, Phase 2 §Documentation Architecture, the
  implemented code
- **Rationale:** written against the implementation, not the design — Phase 5's requirement.

---

### Step 12: Issue and index status

**Files:** `specs/issues/QUEUED-MANAGER-STARTUP-READINESS.md`, `specs/index.csv`,
`specs/design/environment-builder/DESIGN.md`

**Action:** close `QUEUED-MANAGER-STARTUP-READINESS` with a resolution note (§4.3); file an issue
for anything deferred; regenerate the index. `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` stays open by
decision, and its file gains a line recording that this design kept the back-reference strong.

**Validation:**
```bash
python3 scripts/docs_index.py --check
```

**Agent Specification:** haiku; knowledge: `DOCS_STRUCTURE_GUIDE.md` §4.3, §4.8.

---

## Testing Plan

### Unit tests
Run after every step: `cargo test -p liquers-core --lib`. T1, T5, T8, T9, T12, T13, T15, T16 live
beside the code they test, in `environment_builder.rs` and `environment_config.rs`.

### Integration tests
Run after steps 3, 4, 5, 7 and 9: `cargo test -p liquers-core --lib --tests` and
`cargo test -p liquers-lib --lib --tests`. The existing suites are the regression net: if
consolidation broke a type identity, `payload_inheritance.rs`, `plan_cwd_freeze.rs`, `injection.rs`,
`expiration_integration.rs`, `volatility_integration.rs`, `type_consistency.rs`,
`recipe_cwd_resolution.rs`, `asset_failure_contract.rs` and `manager_parametric.rs` stop compiling
before any assertion runs.

### Feature matrix and wasm
After steps 5, 7 and 10:
```bash
bash scripts/check-build-matrix.sh
cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```
`cargo clean` first — the web loop builds a different target, and the workspace disk allowance is
30 GB (see `CLAUDE.md` §Building and testing).

### Manual validation
```bash
cargo run --example basic_server -p liquers-axum      # step 8: a real server still starts
cargo run -p liquers-lib --features cli --bin export-command-registry -- --format yaml -o /dev/null
```
The second is the check that command registration still works end to end through a binary that
constructs an environment outside the test harness.

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|---|---|---|---|
| 1 | haiku | — | Mechanical compile spike with a written fallback |
| 2 | sonnet | rust-best-practices | Small, but the return type departs from Phase 2 for a reason that must survive |
| 3 | sonnet | rust-best-practices | Four parallel edits; must not introduce an `expect` |
| 4 | **opus** | rust-best-practices | Trait contract change across a 9 000-line file; deletes two panics |
| 5 | **opus** | rust-best-practices | 1 100 → 250 lines; GAT; type identity must be preserved exactly |
| 6 | sonnet | rust-best-practices | Mechanical once step 5 lands |
| 7 | sonnet | rust-best-practices | Coherence constraint and a silent-regression risk |
| 8 | sonnet | rust-best-practices | `liquers-web`'s borrow rule and rebuild path |
| 9 | sonnet | liquers-unittest, rust-best-practices | The tests that prove the defect is closed |
| 10 | sonnet | rust-best-practices | Ordinary serde; one documented trap |
| 11 | sonnet | — | Written against the implementation |
| 12 | haiku | — | Front-matter and index bookkeeping |

## Rollback Plan

**Per step:** each step above names the files to `git checkout`. Steps 1–2, 6, 10, 11 and 12 are
individually revertible with no other change.

**Steps 3–5 are one unit.** They change two trait contracts, and the tree does not compile with a
partial revert. Rolling any of them back means rolling back all three. Commit them separately for
review, but treat them as one revert boundary.

**Whole feature:** the branch is `claude/queued-manager-startup-readiness-6ucxnj`; `git revert` the
merge, or reset the branch to the merge base. Nothing outside the repository is mutated at any
point — no migrations, no persisted state, no published artifact.

**Partial delivery:** if step 10 fails, drop it. Steps 1–9 and 11–12 are a complete, shippable
resolution of `QUEUED-MANAGER-STARTUP-READINESS`, and the configuration layer becomes an issue with
this design folder as its reference.

## Documentation Updates

Step 11 covers them, with the authoritative `affects_docs` set from Phase 2. Two new documents
(`ENVIRONMENT_CONSTRUCTION_GUIDE.md`, `ENVIRONMENT_CONFIG.md`), six updated, `## History` rows and
`reviewed:` bumps in the same commit as each edit, and `specs/README.md` relinked.

**Learning to collect for Phase 5, as the work proceeds:** whether the consolidation actually landed
near 250 lines; what `liquers-web`'s migration cost in practice; whether any custom-environment
obligation turned out to be undocumentable in the guide; and whether the deleted `sleep` had been
masking anything besides the startup race.

## Phase 5 Entry Criteria

- [ ] Steps 1–9 complete, `cargo test -p liquers-core --lib --tests` and
      `cargo test -p liquers-lib --lib --tests` green
- [ ] `bash scripts/check-build-matrix.sh` green
- [ ] The `liquers-web` wasm suite green
- [ ] Step 10 complete, or dropped and filed as an issue
- [ ] All user comments and review comments answered
- [ ] Documentation verifiable against implemented and tested behavior
- [ ] Phase 5 documentation included in the implementation PR

## Execution Options

1. **Execute now** — run steps 1–12 in order on this branch.
2. **Execute steps 1–9 only** — ship the P1 readiness fix, file `EnvironmentConfig` as its own issue.
3. **Create a task list** — defer execution.
4. **Revise this plan.**
5. **Exit** — implement manually.

## Review Record

Five review passes were run sequentially before the approval gate (skill host-compatibility
fallback; no agents spawned): Phase 1, Phase 2 and Phase 3 conformity, codebase compatibility, and a
final critical read of all four phase documents together.

**Reviewer 1 — Phase 1 conformity.** Every Phase 1 decision has a step: sync fallible construction
(6), factory not `Arc::new_cyclic` with the slot on the environment (3, 5), the cycle left alone
(no step touches it), the re-runnable barrier (4), `to_ref` kept and `EnvRef::new` deprecated (5),
complexity L (the step count). The one Phase 1 item Phase 4 *adds* to is documentation — Phase 1
planned one new guide, and the D2 decision made a second reference necessary.

**Reviewer 2 — Phase 2 conformity.** Signatures match §Function Signatures, with **one deliberate
departure**: `register_version_sync` returns `bool` and `refresh_command_versions` returns
`Vec<DependencyKey>`, rather than `ExpiredDependents`. Phase 2's signature is unimplementable —
`expire_dependents` is async — and this is recorded as Q1 above rather than silently changed. Phase 2
should be read as amended by it.

**Reviewer 3 — Phase 3 conformity.** All fourteen tests are assigned to a step and a file, plus
T15/T16 for the configuration layer. Every scenario has an implementing step: 1a/1b → 6, 2a → 8,
2b/2c → 5, 2d → 7, 3a/3b → 9, 3c → documented in 11, 3d → 4, 3e → 9 (T14), 4 → 10.

**Reviewer 4 — codebase compatibility.** Verified at `HEAD`: `StoreRouterBuilder::build` is
synchronous, so step 10 does not make `build()` async; `expire_dependents` is async, which forces
Q1; no wasm path names `DefaultAssetManager`, which settles Q2; `liquers-lib`'s `DefaultEnvironment`
is already the generic shape, which is why step 5 is a consolidation rather than a redesign; and
`scc`'s `read_sync` is in tree while `entry_sync` could not be confirmed offline, which is why step 1
exists. Counts re-measured: 348 `.to_ref()` sites, 120 `&mut …command_registry` sites.

**Final critical read (all phases).** Three things this plan asserts that the design documents
should be read as amended by: Q1's signature change; the step 3/4 split, which Phase 2 did not
anticipate and which exists purely so the ownership change can be tested before the contract change;
and `liquers-py`'s `init_with_envref` becoming real work rather than staying out of scope, because a
`todo!()` behind a now-fallible method is a panic on a supported path. Nothing else in Phases 1–3
conflicts with this plan.

**rust-best-practices pass.** No `unwrap`/`expect` introduced; two existing `expect`s deleted
(step 4) and `get_asset_manager` uses `debug_assert!` plus the installed value rather than replacing
one panic with another. All errors are `liquers_core::error::Error` via typed constructors; no
`Error::new`. The one new `match` (step 2, over `scc::hash_map::Entry`) is exhaustive with no default
arm. No `println!` anywhere; one `eprintln!` deleted. Crate dependency flow respected — every new
module is in `liquers-core`. Fallible signatures on `start` and `refresh_command_versions` are called
out as deliberate so a later "simplification" does not remove them.

## Open Questions

None blocking. Q1 and Q2 are resolved above; Q3 is a five-minute spike that is step 1 and cannot
change any signature. `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` and `POST-INIT-COMMAND-REGISTRATION`
remain open by decision, and no step regresses either.
