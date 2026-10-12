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
  `ActionOrigin::Alias { command }` → `cmr.get(command.clone()).cloned()` (the alias). Exhaustive match.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib plan::tests::last_action_command` —
  `last_action_command_returns_alias_metadata` (plus a direct-command case in the same test).
- Rollback: revert the commit; nothing calls it yet.

### Step 2: `Recipe::to_plan` conversion, warnings, sorted order, typed errors (core)
- Files / symbols: `liquers-core/src/recipes.rs` `Recipe::to_plan` (`:294`), new private
  `fn convert_override(arg: &ArgumentInfo, value: &serde_json::Value) -> Result<(serde_json::Value, Option<String>), Error>`
  and its element-wise use for `arg.multiple`; Enum alias expansion through
  `EnumArgument::expand_alias` (`command_metadata.rs:157`), a `CommandParameterValue::Query`
  result becoming `plan.override_link`.
- Change: iterate `arguments` / `links` by sorted name; resolve each name in
  `plan.last_action_command(cmr)`'s `arguments` (missing, `injected`, or no action →
  `Error::from_error(ErrorType::ParameterError, ..).with_query(&query)`); convert per the Phase 2
  table; `plan.init_warning(note)` for each note; then `override_value` / `override_link` as today.
  Existing recipe tests asserting the old `general_error` text are updated to the type.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-core --test recipe_override_conversion` (ROC01–ROC06) and
  `cargo test -p liquers-core --lib recipes` (existing recipe tests still pass).
- Rollback: revert; Step 4 onwards need it only for AC-4/6/12 assertions.

### Step 3: `AssetManager::apply_queued` on both managers (core) — parallel with Step 2
- Files / symbols: `liquers-core/src/assets.rs` trait `AssetManager` (provided method after `apply`,
  `:5674`); `DefaultAssetManager` (beside `:8305`); `ImmediateAssetManager` (beside `:9590`).
- Change: the signature and bodies of Phase 2 §Interfaces: default →
  `self.apply(recipe, to, None)`; `DefaultAssetManager` → `AssetData::new_ext(self.next_id(), recipe,
  to, None, self.get_envref()).to_ref()`, `self.job_queue.submit(asset.clone()).await?`,
  `Ok(asset)`; `ImmediateAssetManager` → `new_ext(..)`, `let _ = asset.run_inline(None).await;`,
  `Ok(asset)`. Doc comment: not to be called from inside an evaluation (`ASSETS-FIX1` #17).
- Depends on: none (Step 1 only for `aq03`'s recipe with arguments).
- Proof: `cargo test -p liquers-core --test apply_queued` (AQ01–AQ03);
  `cargo check -p liquers-core --target wasm32-unknown-unknown` (the trait is `?Send` there; run
  `rustup target add wasm32-unknown-unknown` first — not installed in this environment).
- Rollback: revert; the method is additive.

### Step 4: override parser and shared test helpers (axum)
- Files / symbols: new `liquers-axum/src/api_core/overrides.rs` (`OverrideInput`, `RouteOptions`,
  `OverrideRequest::{parse, is_empty, to_recipe}`), re-exported from `api_core/mod.rs`; new
  `liquers-axum/tests/common/mod.rs` (`env_with` generic over `SimpleEnvironment` /
  `ImmediateEnvironment` with recipe `arguments`, the fixture commands of Phase 3, `start_server`,
  `connect`, `send_json`, `recv_matching`); `liquers-axum/Cargo.toml` `uuid = { version = "1",
  features = ["v4"] }` (used from Step 8; added here with the module set-up).
- Change: as Phase 2 §Interfaces; `to_recipe` builds the bare query's plan, converts URL text with
  `ParameterValue::from_string(arg, s, &Position::unknown())` (`EnumLink` → link, defaulting `""` →
  the default), passes JSON through, and validates with `recipe.to_plan(cmr)`.
- Depends on: Steps 1–2.
- Proof: `cargo test -p liquers-axum --lib api_core::overrides` (the eleven `parse_*` unit tests).
- Rollback: revert; nothing routes through it yet.

### Step 5: `_entry_format` on every entry route (axum)
- Files / symbols: `api_core/format.rs` new `entry_format_param`; callers
  `assets/common.rs:218` (`entry_response`), `assets/key_handlers.rs:293`,
  `store/handlers.rs:471,526`, `recipes/handlers.rs:166`; every existing test and example using
  `?format=` (`store_api_routes.rs`, `recipes_api_routes.rs`, `assets_api_endpoints.rs`,
  `assets_recipes_integration.rs`).
- Change: read `_entry_format`, reject `format` and unknown values with `ParameterError`, keep each
  caller's fallback (`Accept` for reads, `Content-Type` for bodies).
- Depends on: none (Step 4 only for the shared helpers in the new suite).
- Proof: `cargo test -p liquers-axum --test entry_format_option` (EFO01–EFO05; EFO06 lands in
  Step 6), and the updated `store_api_routes`, `recipes_api_routes`, `assets_api_endpoints` suites.
- Rollback: revert; the rename is one helper and its call sites.

### Step 6: Assets API `q/data`, `q/entry`, `q/submit` with overrides; managed reporting (axum)
- Files / symbols: `assets/common.rs` (`Requested`, `request_with_overrides`, `is_managed`,
  `MANAGED_HEADER`, `SubmitResult`, `asset_entry` error metadata with the `lookup_query_asset`
  fallback); `assets/query_handlers.rs` `q_get_data_handler`, `q_get_entry_handler`,
  `q_submit_handler` and new POST handlers reading `Query<Vec<(String, String)>>` and `Bytes`;
  `assets/builder.rs` `POST q/data`, `POST q/entry` outside the `read_only()` switch.
- Depends on: Steps 3–5.
- Proof: `cargo test -p liquers-axum --test argument_overrides_http` for the Assets API cases
  (AOH02, 04–06, 08–09, 11–21; EFO06) and `assets_api_endpoints` unchanged otherwise.
- Rollback: revert; routes return to ignoring query strings.

### Step 7: Query API with overrides (axum)
- Files / symbols: `query/handlers.rs` `serve_query`, `get_query_handler`, `post_query_handler`
  (the TODO and the doc comment claiming the body is unused go).
- Change: `OverrideRequest::parse(.., RouteOptions::None)`; empty → `env.evaluate` as today; else
  `request_with_overrides`; `x-liquers-managed` on the answer; the timeout wraps both paths.
- Depends on: Step 6.
- Proof: `cargo test -p liquers-axum --test argument_overrides_http` (AOH01, 03, 07, 10) and
  `--test query_api_routes` (the POST-body case updated).
- Rollback: revert.

### Step 8: WebSocket `arguments`, subscription ids, `cancel` (axum)
- Files / symbols: `assets/websocket.rs` `ClientMessage`, `Head`, `SubscriptionHandle`, `Session`
  (`subscriptions` by id, `by_address`), `request`, `subscription_task`, `handle_text`; its test at
  `:633` (`..` in the pattern).
- Change: Phase 2 §Interfaces and §Concurrency, including `cancel_requested` and the generic
  unknown-id error.
- Depends on: Steps 3–4, 6 (`is_managed`, `to_recipe`).
- Proof: `cargo test -p liquers-axum --test argument_overrides_ws` (AOW01–AOW13) and
  `--test assets_websocket` (existing, unchanged behaviour).
- Rollback: revert; the protocol returns to address-only subscriptions.

### Step 9: README and examples (axum)
- Files / symbols: `liquers-axum/README.md` (endpoint list, `?format=` lines),
  `liquers-axum/examples/WEBSOCKET_EXAMPLE.md`, `liquers-axum/examples/assets_recipes_basic.rs`.
- Change: `_entry_format`; overrides on the Query API and `q/data` / `q/entry` / `q/submit`;
  `subscribe` with `arguments`.
- Depends on: Steps 5–8.
- Proof: `cargo build -p liquers-axum --example assets_recipes_basic`; run it and see the
  `_entry_format` request answered.
- Rollback: revert.

### Step 10: final validation
- Proof, in order (each after `CARGO_INCREMENTAL=0`; `cargo clean` first if `df -h /` is low):
  `cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-axum`;
  `cargo test -p liquers-lib --lib --tests` (the default loop: `to_plan` is used by recipes there);
  `cargo test -p liquers-lib --test registry_export` (no command changed — must stay green);
  `cargo check -p liquers-core --target wasm32-unknown-unknown`; `cargo check -p liquers-py`.
  A re-read of the whole diff against Phase 2 (`rust-best-practices`).

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
