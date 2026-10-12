# Phase 4: Implementation Plan — Argument overrides over HTTP and WebSocket

## Overview

Core first, because every axum step depends on it: the alias-aware `Plan::last_action_command`,
then `Recipe::to_plan` conversion and errors, then `AssetManager::apply_queued` (Steps 2 and 3 are
independent of each other). Then axum bottom-up: the parser and test helpers, the `_entry_format`
rename (self-contained, touching four API families), the Assets API routes, the Query API, the
WebSocket, and finally the README and examples. Each step is one commit and leaves the workspace
compiling with all tests passing. The `specs/` reference and guide updates are Phase 5 (they must
describe the behaviour as built and tested). Prerequisites: none — the preflight found no blocker.

## Progress

- [ ] Step 1: `Plan::last_action_command` (core)
- [ ] Step 2: `Recipe::to_plan` conversion, warnings, sorted order, typed errors (core)
- [ ] Step 3: `AssetManager::apply_queued` on both managers (core)
- [ ] Step 4: override parser and shared test helpers (axum)
- [ ] Step 5: `_entry_format` on every entry route (axum)
- [ ] Step 6: Assets API `q/data`, `q/entry`, `q/submit` with overrides; managed reporting (axum)
- [ ] Step 7: Query API with overrides (axum)
- [ ] Step 8: WebSocket `arguments`, subscription ids, `cancel` (axum)
- [ ] Step 9: README and examples (axum)
- [ ] Step 10: final validation

## Implementation Steps

### Step 1: `Plan::last_action_command` (core)
- Files / symbols: `liquers-core/src/plan.rs` `impl Plan` beside `last_action_index` (`:2602`).
- Change: `pub fn last_action_command(&self, cmr: &CommandMetadataRegistry) -> Option<CommandMetadata>` —
  for the last `Step::Action`, `ActionOrigin::Direct` → `cmr.find_command(realm, ns, action_name)`;
  `ActionOrigin::Alias { command }` → `cmr.get(command.clone()).cloned()` (the alias). Exhaustive.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib plan::tests::last_action_command` —
  `last_action_command_returns_alias_metadata` (with a direct-command case).
- Rollback: revert the commit; nothing calls it yet.

### Step 2: `Recipe::to_plan` conversion, warnings, sorted order, typed errors (core)
- Files / symbols: `liquers-core/src/recipes.rs` `Recipe::to_plan` (`:294`); new private
  `fn convert_override(arg: &ArgumentInfo, value: &serde_json::Value) -> Result<(serde_json::Value, Option<String>), Error>`,
  used element-wise for `arg.multiple`; Enum aliases through `EnumArgument::expand_alias`
  (`command_metadata.rs:157`), a `CommandParameterValue::Query` result becoming `plan.override_link`.
- Change: iterate `arguments` / `links` by sorted name; resolve each name in
  `plan.last_action_command(cmr)`'s `arguments` (missing, `injected`, head slot of an alias, or no
  action → `Error::from_error(ErrorType::ParameterError, ..).with_query(&query)`); convert per the
  Phase 2 table; `plan.init_warning(note)` per note; then `override_value` / `override_link`. No
  existing test asserts the old `general_error` text (only a comment, `recipes.rs:1264`); the risk
  is plans that gain `init_steps` or refuse a name, so the proof runs everything that builds recipes.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-core --test recipe_override_conversion` (ROC01–ROC06), then
  `cargo test -p liquers-core --lib --tests` (incl. `plan_cwd_freeze`, `recipe_cwd_resolution`) and
  `cargo test -p liquers-lib --lib --tests` (recipe users such as `record_manifest_resource_key`).
- Rollback: revert; later steps need it only for AC-4/6/12 assertions.

### Step 3: `AssetManager::apply_queued` on both managers (core) — independent of Steps 1–2
- Files / symbols: `liquers-core/src/assets.rs` trait `AssetManager` (provided method after `apply`,
  `:5674`); `DefaultAssetManager` (beside `:8305`); `ImmediateAssetManager` (beside `:9590`). The one
  external implementor, `liquers-core/tests/common/minimal_manager.rs:248`, inherits the default.
- Change: Phase 2 §Interfaces: default → `self.apply(recipe, to, None)`; `DefaultAssetManager` →
  `AssetData::new_ext(self.next_id(), recipe, to, None, self.get_envref()).to_ref()`,
  `self.job_queue.submit(asset.clone()).await?`, `Ok(asset)`; `ImmediateAssetManager` → `new_ext(..)`,
  `let _ = asset.run_inline(None).await;`, `Ok(asset)`. Doc: not to be called from inside an
  evaluation (`ASSETS-FIX1` #17).
- Depends on: none.
- Proof: `cargo test -p liquers-core --test apply_queued` (AQ01–AQ03); best-effort
  `rustup target add wasm32-unknown-unknown && cargo check -p liquers-core --target wasm32-unknown-unknown`
  (a failure to install the target is environmental and is reported, not hidden).
- Rollback: revert; the method is additive.

### Step 4: override parser and shared test helpers (axum)
- Files / symbols: new `liquers-axum/src/api_core/overrides.rs` (`OverrideInput`, `RouteOptions`,
  `OverrideRequest::{parse, is_empty, to_recipe}`), re-exported from `api_core/mod.rs`; new
  `liquers-axum/tests/common/mod.rs` with `#![allow(dead_code)]` (`env_with` generic over the two
  environments, with recipe `arguments`; the Phase 3 fixtures; `start_server`, `connect`,
  `send_json`, `recv_matching`); `liquers-axum/Cargo.toml` `uuid = { version = "1", features = ["v4"] }`.
- Change: Phase 2 §Interfaces; `to_recipe` builds the bare query's plan, converts URL text with
  `ParameterValue::from_string(arg, s, &Position::unknown())` (`EnumLink` → link, defaulting `""` →
  the default), passes JSON through, validates with `recipe.to_plan(cmr)`.
- Depends on: Steps 1–2.
- Proof: `cargo test -p liquers-axum --lib api_core::overrides` (the nine `parse_*` tests).
- Rollback: revert; nothing routes through it yet.

### Step 5: `_entry_format` on every entry route (axum)
- Files / symbols: `api_core/format.rs` — new `entry_format_param(&HashMap<String, String>) ->
  Result<Option<SerializationFormat>, Error>` and a private `negotiate(Option<SerializationFormat>,
  &HeaderMap)` behind the public `select_format`, whose signature and unit tests (`:91-150`) stay;
  `assets/common.rs` `entry_response` (`:208`) and `asset_entry` (`:233`) take the resolved
  `Option<SerializationFormat>`, each caller resolving the option (`asset_entry` for `q/entry`,
  `key_handlers.rs:175`, `recipes/handlers.rs:204`); `key_handlers.rs:293`, `store/handlers.rs:471,526`
  (and doc comments `:441,506`, `recipes/handlers.rs:166`); every `?format=` in
  `store_api_routes.rs`, `recipes_api_routes.rs`, `assets_api_endpoints.rs`, `assets_recipes_integration.rs`.
- Change: read `_entry_format`; `format` and unknown values are `ParameterError`; each caller keeps
  its fallback (`Accept` for reads, `Content-Type` for bodies). `q/entry` resolves the option itself
  for now; Step 6 moves that to `OverrideRequest.entry_format`.
- Depends on: Step 4 (shared helpers).
- Proof: `cargo test -p liquers-axum --test entry_format_option` (EFO01–EFO05; EFO06 is added in
  Step 6) `--test store_api_routes --test recipes_api_routes --test assets_api_endpoints
  --test assets_recipes_integration`.
- Rollback: revert; one helper and its call sites.

### Step 6: Assets API `q/data`, `q/entry`, `q/submit` with overrides; managed reporting (axum)
- Files / symbols: `assets/common.rs` (`Requested`, `request_with_overrides`, `is_managed`,
  `MANAGED_HEADER`, `SubmitResult`, `asset_entry` error metadata with the `lookup_query_asset`
  fallback); `assets/query_handlers.rs` `q_get_data_handler`, `q_get_entry_handler`,
  `q_submit_handler` and POST handlers taking `Query<Vec<(String, String)>>` and `Bytes`;
  `assets/builder.rs` `POST q/data`, `POST q/entry` outside the `read_only()` switch.
- Change: parse with `RouteOptions::Entry` on `q/entry`, `None` elsewhere; empty overrides keep
  today's request; otherwise `request_with_overrides`; `x-liquers-managed` and `SubmitResult` as
  Phase 2 §Web Endpoints; `q/entry` takes `entry_format` from the `OverrideRequest`, so `format`
  is an override there.
- Depends on: Steps 3–5.
- Proof: `cargo test -p liquers-axum --test argument_overrides_http` (created here with the Assets
  API cases: AOH02, 04–06, 08, 09 and 12 on `$A` routes, 11, 13–21) `--test entry_format_option`
  (now with EFO06) `--test assets_api_endpoints`.
- Rollback: revert; routes return to ignoring query strings.

### Step 7: Query API with overrides (axum)
- Files / symbols: `query/handlers.rs` `serve_query`, `get_query_handler`, `post_query_handler`
  (the TODO and the doc comment claiming the body is unused go).
- Change: `OverrideRequest::parse(.., RouteOptions::None)`; empty → `env.evaluate` as today; else
  `request_with_overrides`; `x-liquers-managed`; the timeout wraps both paths.
- Depends on: Step 6.
- Proof: `cargo test -p liquers-axum --test argument_overrides_http` (the Query API cases added
  here: AOH01, 03, 07, 10, and the `$B` forms of AOH09 and AOH12) `--test query_api_routes
  --test query_api_integration` (unchanged, still green).
- Rollback: revert.

### Step 8: WebSocket `arguments`, subscription ids, `cancel` (axum)
- Files / symbols: `assets/websocket.rs` `ClientMessage`, `Head`, `SubscriptionHandle`, `Session`
  (`subscriptions` by id, `by_address`), `request`, `subscription_task`, `handle_text`; its unit
  tests at `:633` (`..` in the pattern) and `:645` (the `Head` literal gains `managed`,
  `subscription_id`).
- Change: Phase 2 §Interfaces and §Concurrency, including `cancel_requested` and the generic
  unknown-id error.
- Depends on: Steps 3–4, 6 (`is_managed`, `to_recipe`).
- Proof: `cargo test -p liquers-axum --lib assets::websocket` and `--test argument_overrides_ws`
  (AOW01–AOW13) `--test assets_websocket` (existing, unchanged behaviour).
- Rollback: revert; the protocol returns to address-only subscriptions.

### Step 9: README and examples (axum)
- Files / symbols: `liquers-axum/README.md` (endpoint list, `?format=` lines),
  `examples/WEBSOCKET_EXAMPLE.md`, `examples/assets_recipes_basic.rs`, `examples/websocket_client.rs:141`.
- Change: `_entry_format`; overrides on the Query API and `q/data` / `q/entry` / `q/submit`;
  `subscribe` with `arguments`.
- Depends on: Steps 5–8.
- Proof: `cargo build -p liquers-axum --examples`; run `assets_recipes_basic` and see the
  `_entry_format` request answered.
- Rollback: revert.

### Step 10: final validation
- Proof, in order, with `CARGO_INCREMENTAL=0` (`cargo clean` first if `df -h /` is low; measured
  need ≈ 5 GB): `cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-axum`;
  `cargo test -p liquers-lib --lib --tests`; `cargo test -p liquers-lib --test registry_export`
  (no command changed — must stay green); the wasm check of Step 3 (best-effort, or the wasm row of
  `scripts/check-build-matrix.sh`); `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 cargo check -p liquers-py`
  (Python here is 3.13). Then a re-read of the whole diff against Phase 2 (`rust-best-practices`).
- Rollback: none needed — it changes nothing; a failure sends work back to the step that owns it.

## Testing Plan

Each step runs its own suite before committing. The full core and axum suites run after Steps 3,
6 and 8; Step 10 adds `liquers-lib`'s default loop, the registry check, wasm and `liquers-py`. No
`#[cfg(feature)]` or optional dependency changes, so `scripts/check-build-matrix.sh` is not needed;
the new `uuid` dependency is unconditional in a native-only crate.

## Rollback Plan

The steps are independent commits. If only Steps 1–3 land, core is extended (an unused helper, a
stricter and friendlier `to_plan`, an unused trait method) and nothing in the API changes. If
Step 5 lands alone, only the option rename is visible. Steps 6–8 each revert cleanly onto the
previous one. A partial landing becomes an issue per `DOCS_STRUCTURE_GUIDE.md` §5.6.

## Documentation Updates

Phase 5, after Step 10, against the built behaviour (Phase 2 §Documentation Architecture):
`reference/ASSET_LIFECYCLE.md` (terms, `apply_queued`), `reference/WEB_API_SPECIFICATION.md`
(§2, §5.3, §5.8, §7, the "when an asset is managed" table), `guides/WEB_API_GUIDE.md` (§4, §5 —
`submit` of an unmanaged asset, side effects — §8, `_entry_format`; examples run against the
example server), `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` (`apply_queued`),
`reference/api/DOC_08_RECIPES_PLANS.md` (conversion, warnings, errors, sorted order, aliases). Each
gets a `## History` row and a `reviewed:` bump. Regenerated: `specs/index.csv` / `index.md`
(`docs_index.py`); `specs/command_registry.yaml` unchanged. `specs/README.md`: the line moves to
"built". The source issue is closed with a resolution note.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated (every Progress item ticked with its commit)
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
