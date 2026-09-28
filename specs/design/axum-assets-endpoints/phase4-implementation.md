# Phase 4: Implementation Plan - A working web and WebSocket interface for the asset manager

## Overview

Fourteen steps. Each one ends in a state that compiles and passes its own validation, and each is
one commit on `claude/fervent-cori-ew4kvn`. Phase 2 (re-approved 2026-09-28) is the contract, and
Phase 3 v2 (169 tests) is pasted verbatim except where a step says otherwise. The work falls in
four blocks:

1. **Core, Steps 1–5:**
   - the error type, internal plumbing and the new notification;
   - `AssetManager` behaviour (`remove`, `expire`, `set_description`, `removedir`,
     `lookup_query_asset`, `to_override`, `get_asset_info`);
   - core tests;
   - media types.
2. **Axum foundations, Steps 6–7:** the `unwrap()` removal and the WebSocket route fix, then
   `ValueDescription`.
3. **Axum features, Steps 8–12:**
   - handlers for both families;
   - routes and switches;
   - the WebSocket rewrite;
   - the query timeout;
   - the HTTP and WebSocket tests.
4. **Documentation and validation, Steps 13–14:** the specification audit and issue statuses, then
   full validation.

**Pre-flight facts** (verified 2026-09-28; no step has to rediscover them):

- **`remove` callers.** Only `test_remove_asset` (`assets.rs` ≈9146) calls `AssetManager::remove`,
  on a `Source`, and it still holds under the new semantics. No test matches on `expire`'s
  "Cannot expire…" messages.
- **`AssetManager` bounds.** The trait carries `#[allow(private_bounds)]` (≈3894), so the
  `pub(crate)` supertrait `KeyMutationAccess` needs no new lint allowance.
- **`AssetNotificationMessage` match sites.** Adding `Removed` touches:
  - `liquers-core/src/assets.rs` ≈2455 and ≈3160: wait loops with a pre-existing `_ =>` arm. They
    must treat `Removed` as **terminal**, like `Cancelled`, so a waiter cannot hang on an unmapped
    asset;
  - `assets.rs` ≈3446: exhaustive;
  - `liquers-axum/src/assets/websocket.rs` ≈319: exhaustive, and rewritten in Step 10;
  - `liquers-lib/src/ui/element.rs` ≈516: exhaustive; `Removed` joins the arm that ignores
    status-only messages.
- **`StatusConflict` match sites:**
  - `assets.rs` ≈2239–2252;
  - `liquers-axum/src/api_core/error.rs` ≈28/79/110/136/322;
  - `liquers-py/src/error.rs`, both directions and its own `ErrorType`;
  - `liquers-web/src/error.rs`, both directions, plus `tests/objects_OBJECT.rs` ≈39.
- **`get_asset_info`** has two bodies: the trait default (≈4067) and a `DefaultAssetManager`
  override (≈5518).
- **`dependency_blocks_fast_track`** is at ≈1076.
- **`mark_expired_status`** is at ≈3280; it may run on a non-keyed asset.
- **Query maps.** Both managers hold one: `DefaultAssetManager.query_assets` (`scc::HashMap`) and
  `ImmediateAssetManager.query_assets` (`Mutex<HashMap>`). So `lookup_query_asset` is a required
  method with two small bodies.
- **The nine `unwrap()` calls in `liquers-axum` library code:**
  - `axum_integration.rs` 37, 45, 70, 86, 96;
  - `recipes/handlers.rs` 102, 190;
  - `store/handlers.rs` 479;
  - `assets/handlers.rs` 290.
- **The WebSocket route `{}/*query`** panics in axum 0.8 (`AXUM-ASSETS-WEBSOCKET-ROUTE-PANICS`).
  Every router test is blocked on it, so Step 6 fixes it first.
- **Package layout.** `liquers-web` is wasm-only (checked with `--target
  wasm32-unknown-unknown`). `liquers-py` is a default member (`cargo check`).
- **Dev-dependencies.** `tokio-tungstenite` 0.23 and `reqwest` 0.12 are already dev-dependencies
  of `liquers-axum`. `tower` needs `features = ["util"]` in dev-dependencies.
- **Disk.** Run `cargo clean` before the wasm check if space is tight (CLAUDE.md).

## Implementation Steps

### Step 1: `ErrorType::StatusConflict` everywhere

**Files:** `liquers-core/src/error.rs`; the match sites listed above.

**Action:**
- Add the variant, with its Phase 2 doc comment, after `DependencyCycle`.
- Add `pub fn status_conflict(key: &Key, status: Status, operation: &str) -> Self`: message
  `"Cannot {operation} '{key}': asset status is {status:?}"`, setting `key` (encoded) and `query`.
- Add the match arms:

```rust
ErrorType::StatusConflict => PersistenceStatus::NotPersisted,                 // assets.rs
ErrorType::StatusConflict => StatusCode::CONFLICT,                            // axum error_to_status_code
"StatusConflict" => Ok(ErrorType::StatusConflict),                            // axum parse_error_type
ErrorType::StatusConflict => "status_conflict",                               // liquers-web, and back
ErrorType::StatusConflict => liquers_core::error::ErrorType::StatusConflict,  // liquers-py, and back
```

**Validation:** `cargo check -p liquers-core -p liquers-axum -p liquers-py -p liquers-lib`;
`cargo test -p liquers-axum --lib api_core`; `cargo check -p liquers-web --target
wasm32-unknown-unknown`.

**Rollback:** revert the commit (the change is additive).

**Agent:** haiku · rust-best-practices · Phase 2 "New Enums" + the site list · mechanical, and
the compiler finds every missed site.

---

### Step 2: Core plumbing — `KeyMutationAccess`, `Removed`, `lookup_query_asset`, refusals, doc fix

**File:** `liquers-core/src/assets.rs` (+ `liquers-lib/src/ui/element.rs` arm,
`liquers-axum/src/assets/websocket.rs` arm).

**Action:**
- `pub(crate) trait KeyMutationAccess { fn key_mutation_lock(&self) -> &tokio::sync::Mutex<()>; }`,
  implemented by both managers and added to the `AssetManager` supertraits.
- `AssetNotificationMessage::Removed`, with arms at every site in the pre-flight list. The two
  wait loops treat it as terminal and return the same error as for `Cancelled`, with the message
  "asset was removed".
- `fn lookup_query_asset(&self, query: &Query) -> Option<AssetRef<E>>`, **required**. A pure-key
  query delegates to `lookup_key_asset`; otherwise it reads the manager's `query_assets` map
  without inserting.
- `mark_expired_status` refusals become `Error::from_error(ErrorType::StatusConflict, …)`.
- Move the orphaned doc paragraph above `expire_stored_copy` back to `DependencyManagerAccess`.

**Validation:** `cargo test -p liquers-core --lib`; `cargo check -p liquers-lib -p liquers-axum`.

**Rollback:** revert the commit.

**Agent:** sonnet · rust-best-practices · Phase 2 "Trait Implementations", "WebSocket
Notifications" (the `Removed` part) · the wait-loop semantics need judgement.

---

### Step 3: Core behaviour

**File:** `liquers-core/src/assets.rs`.

**Action**, exactly as Phase 2 "Trait Implementations" and "Access Modes":
- **`remove`:** a default method implementing the decision table. Delete both per-manager bodies
  (≈5643, ≈6939). Send `Removed` before unmapping a live asset. Cascade **before**
  `dependency_manager().remove`. The drop-computed branch writes `store.set(key, &[], md)` with
  `status = Recipe` and the version kept. A live `None`/`Recipe` status defers to the stored
  status; a stored `Recipe` is a no-op; `LegacyMetadata` is deleted. Use an exhaustive `match`
  on `Status`.
- **`set_binary` and `set_state`:** send `Removed` before unmapping a live asset.
- **`expire`**, **`set_description`**, **`removedir`**, all as default methods:
  - `removedir` walks `listdir_keys_deep` deepest first, calls `remove` for each stored key
    (taking the lock per key, never across the walk), then calls `store.removedir`;
  - `set_description` uses the new `pub(crate) AssetRef::set_description_fields`.
- **`to_override`:** the store-only branch skips a `Source`.
- **`get_asset_info`:** the live branch uses `lookup_key_asset`, in both bodies (the simplest way
  is to delete the override). Not found → `Error::key_not_found`.
- **`dependency_blocks_fast_track`:** in the store branch, a stored `Recipe` does not block.
- **Lock discipline** (Phase 2): while holding the guard, never call `get`, `owned_key_asset`,
  `to_override`, `set_binary`, `set_state` or `remove_expired_from_maps`.

**Validation:** `cargo test -p liquers-core --lib`; `cargo test -p liquers-core --tests`. If
`EXPIRATION-INTEGRATION-SUITE-FAILING-AT-HEAD` is still open, compare any expiration failure
against the base commit before attributing it here.

**Rollback:** revert to the Step 2 commit.

**Agent:** sonnet · rust-best-practices · Phase 2 decision tables + lock discipline; `ASSETS.md`
"Remove Semantics" · the only step with real concurrency risk.

---

### Step 4: Core tests (AMR, 37)

**File:** `liquers-core/tests/asset_manager_remove_expire_describe.rs` (new): Phase 3's shared
helpers and AMR01–AMR61, including AMR24 (the restart fast-track).

**Validation:** `cargo test -p liquers-core --test asset_manager_remove_expire_describe`. A
failure is judged **against Phase 2**, and any divergence is recorded under "Implementation
Notes" below; tests are never edited silently.

**Agent:** sonnet · liquers-unittest, rust-best-practices · Phase 3 AMR sections.

---

### Step 5: Tabular media types (I9, MT 10)

**File:** `liquers-core/src/media_type.rs`.

**Action:**
- Set the Phase 2 I9 values, **each confirmed against the IANA media-types registry**. Where a
  value is not registered (`application/x-ndjson`, `application/jsonl`), keep the common
  unregistered form and say so in a comment.
- Add the MT01–MT10 tests.

**Validation:** `cargo test -p liquers-core --lib media_type`.

**Agent:** haiku · — · Phase 2 I9, Phase 3 MT.

---

### Step 6: Axum foundations — `unwrap()` removal (I7) and the WebSocket route (I10)

**Files:** `axum_integration.rs`, `recipes/handlers.rs`, `store/handlers.rs`,
`assets/handlers.rs`, `assets/builder.rs`.

**Action:**
- Add a `build_or_500(builder, body) -> Response` helper, and replace
  `mime_type().parse().unwrap()` with `HeaderValue::from_static`, at the nine sites.
- Change the WebSocket route to `{{*query}}`, so `build()` stops panicking before the Step 10
  rewrite.

**Validation:**
- `cargo clippy -p liquers-axum -- -D clippy::unwrap_used -D clippy::expect_used` passes
  crate-wide. This replaces the file-scoped `awk` check of the previous plan.
- `cargo test -p liquers-axum`.

**Agent:** haiku · rust-best-practices · Phase 2 I7 table.

---

### Step 7: `ValueDescription` (VD 11)

**Files:** `liquers-axum/src/assets/value_description.rs` (new) and `mod.rs`.

**Action:** the struct, `from_json`, `from_params`, `or_previous` (the pair rule),
`into_metadata_record` (`Bytes` default, registry `type_name`, `ParameterError` for an unknown
type), plus the VD tests.

**Validation:** `cargo test -p liquers-axum --lib value_description`.

**Agent:** haiku · rust-best-practices, liquers-unittest.

---

### Step 8: Handlers for both families

**Files:**
- `liquers-axum/src/assets/common.rs` (new): `key_from_path`, `query_from_path`,
  `error_response`, `created`, `status_after_remove` and `asset_bytes` (I1).
- `query_handlers.rs`: `git mv` of `handlers.rs`, then adapted.
- `key_handlers.rs` (new).

**Action:** every handler in Phase 2 "Handlers — new or changed", with modes (a), (b) and (c)
exactly as specified:
- observe handlers never call `get`/`get_asset`;
- submit handlers do not await the value;
- the reads use `asset_bytes`;
- `Accept` is honoured;
- every status output is an `ApiResponse`;
- the version is never null.

Phase 2 is the single source of truth for extractors, shapes and codes.

**Validation:** `cargo check -p liquers-axum`; clippy as in Step 6.

**Agent:** sonnet · rust-best-practices · Phase 2 "Access Modes", "Handlers", "Web Endpoints",
"Error Handling"; `store/handlers.rs` `put_entry_handler` for entry decoding.

---

### Step 9: Routes and builder switches

**Files:** `liquers-axum/src/assets/builder.rs`, `examples/assets_recipes_basic.rs`.

**Action:**
- Register the Phase 2 route table: `q/`, `key/`, `admin/`, `ws/q` and `ws/key`.
- Remove the unprefixed routes (O1).
- Add the builder options:
  - `read_only()`;
  - `with_admin(bool)`;
  - `with_destructive_gets()` for the GET alternatives, default off (O14); `submit` GETs are
    always on;
  - `with_websocket_limits(WebSocketLimits)`;
  - `with_websocket_path(base)`, which replaces the `{base}/ws` prefix.
- Update the example's printed URLs to the new families.

**Validation:** `cargo test -p liquers-axum --lib`; add a `build()` test for each switch
combination.

**Agent:** haiku · rust-best-practices · Phase 2 route table.

---

### Step 10: WebSocket rewrite

**File:** `liquers-axum/src/assets/websocket.rs`.

**Action:** Phase 2 "WebSocket Notifications", in full:
- `ws_query_handler` and `ws_key_handler`, with subscription on the URL path;
- a writer task, plus one task per subscription holding the `AssetRef` and its `watch::Receiver`;
- an `AssetInfo` snapshot re-read on every change;
- snake_case client messages with a `query` or `key` field, and `Error` replies;
- the spec-named server messages, including `Removed`;
- the subscription ends after its terminal notification (O10);
- `WebSocketLimits` (I6): `max_message_size` through `WebSocketUpgrade::max_message_size`, a
  subscription cap, and task abort on disconnect;
- a bounded writer channel.

**Validation:** `cargo check -p liquers-axum`; clippy.

**Agent:** sonnet · rust-best-practices · Phase 2 WebSocket section; tokio `watch`/`mpsc`; the
axum 0.8 `ws` API.

---

### Step 11: Query timeout (I5)

**Files:** `liquers-axum/src/query/builder.rs` and `handlers.rs`.

**Action:**
- `QueryApiBuilder::with_timeout(Duration)`, default 30 s, carried to the handler in
  `QueryApiConfig` through an `Extension` layer.
- The timeout message names the duration and points to `{assets}/q/submit`, `q/info` and `ws/q`.

**Validation:** `cargo check -p liquers-axum`.

**Agent:** haiku · rust-best-practices.

---

### Step 12: HTTP and WebSocket tests

**Files:**
- `liquers-axum/Cargo.toml`: `[dev-dependencies] tower = { version = "0.5.3", features =
  ["util"] }`.
- New test files: `tests/assets_api_endpoints.rs` (AAE 63), `tests/assets_websocket.rs`
  (AWS 16), `tests/store_api_routes.rs` (SAR 16), `tests/query_api_routes.rs` (QAR 7) and
  `tests/recipes_api_routes.rs` (RAR 9).
- Delete the placeholder `src/assets/tests.rs` and its `mod tests;` line.

**Action:** paste the Phase 3 test files with their shared helpers. Tests that use a blocking
command run as `multi_thread`. `AWS12b` pins down whichever axum behaviour it observes for an
oversized message, and records it in "Implementation Notes".

**Validation:** `cargo test -p liquers-axum`, run as five `--test` targets plus `--lib`. As in
Step 4, a failure is judged against Phase 2.

**Agent:** sonnet · liquers-unittest, rust-best-practices · Phase 3 in full.

---

### Step 13: Documentation, issue statuses, index

**Files and actions:**
- **`specs/reference/WEB_API_SPECIFICATION.md` (I8):**
  - rewrite §5 for `/q/`, `/key/`, `admin/`, the access modes, removal, the GET alternatives, the
    builder options and the WebSocket (`ws/q`, `ws/key`, messages, lifecycle, limits);
  - §3.3: 409 for `StatusConflict`;
  - apply Phase 3's divergences D1–D7, and audit §2–§4 and §6–§10 route by route against the I4
    tests;
  - replace `FullApiBuilder` with the `Router::merge` assembly;
  - document `with_timeout`;
  - add a History row and bump `reviewed:`.
- **`specs/reference/ASSETS.md`:** rewrite "Remove Semantics" and "Scenario 5" (with the
  `Removed` notification); document `expire`, `set_description`, `removedir` and
  `lookup_query_asset`; add a History row and bump `reviewed:`.
- **Issue statuses (§4.3, each with a resolution note that cites its tests):**
  - **→ `closed`:**
    - `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED`;
    - `AXUM-ASSETS-WEBSOCKET-ROUTE-PANICS`;
    - `WEB-API-SPECIFICATION-DIVERGES-FROM-IMPLEMENTATION`;
    - `AXUM-ASSETS-API-SERVES-ONLY-BYTES-AND-TEXT`;
    - `EXPIRATION-RECOVERY-WEB-API`;
    - `AXUM-ASSETS-CANCEL-STARTS-EVALUATION`;
    - `AXUM-HANDLER-TEST-COVERAGE`;
    - `AXUM-QUERY-TIMEOUT-HARDCODED`;
    - `MEDIA-TYPES-MISSING-FOR-TABULAR-FORMATS`;
    - `ASSET-REMOVE-FORGETS-DEPENDENTS`;
    - `ASSET-TO-OVERRIDE-SOURCE-INCONSISTENT`;
    - `DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION`.
  - **Progress note, status unchanged:** `AXUM-WEBSOCKET-HARDENING` (stays `accepted`) and
    `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` (`liquers-axum` is clean).
  - Set `design: axum-assets-endpoints` on each of them.
- **`specs/design/axum-assets-endpoints/DESIGN.md`:** tick Phase 4; add `gh_pr` once the PR
  exists (then `status` carries no derived value, §5.5).
- **`specs/README.md`:** update the capability map line for the web API.
- **`liquers-axum/README.md`:** document the builder options.
- Run `python3 scripts/docs_index.py`, then `python3 scripts/docs_index.py --check`.

**Not needed:** `specs/command_registry.yaml` (no command signature changes).

**Agent:** sonnet · — · `DOCS_STRUCTURE_GUIDE.md` §4.3/§5.5/§8.1/§9.2; Phases 1–3; the
implemented code.

---

### Step 14: Full validation

```bash
cargo test -p liquers-core
cargo test -p liquers-axum
cargo test -p liquers-lib --lib --tests          # AssetNotificationMessage arm, core changes
cargo check -p liquers-py
cargo check -p liquers-core --no-default-features
cargo clippy -p liquers-axum -- -D clippy::unwrap_used -D clippy::expect_used
cargo clean && cargo check -p liquers-web --target wasm32-unknown-unknown
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles   # if node is available
```

`scripts/check-build-matrix.sh` is not needed: there are no `#[cfg(feature)]`, optional-dependency
or `ExtValue` changes.

**Agent:** haiku · — · CLAUDE.md "Building and testing".

## Testing Plan

### Unit Tests

| Suite | Command | Step |
|---|---|---|
| `api_core::error` | `cargo test -p liquers-axum --lib api_core` | 1 |
| core lib (incl. `test_remove_asset`) | `cargo test -p liquers-core --lib` | 2, 3 |
| MT01–MT10 | `cargo test -p liquers-core --lib media_type` | 5 |
| VD01–VD11 | `cargo test -p liquers-axum --lib value_description` | 7 |
| builder switch combinations | `cargo test -p liquers-axum --lib` | 9 |

### Integration Tests

| Suite | Command | Step |
|---|---|---|
| AMR (37) | `cargo test -p liquers-core --test asset_manager_remove_expire_describe` | 4 |
| AAE (63) | `cargo test -p liquers-axum --test assets_api_endpoints` | 12 |
| AWS (16) | `cargo test -p liquers-axum --test assets_websocket` | 12 |
| SAR (16), QAR (7), RAR (9) | `cargo test -p liquers-axum --test store_api_routes`, `query_api_routes`, `recipes_api_routes` | 12 |
| all existing suites | Step 14 | 14 |

### Manual Validation

```bash
cargo run -p liquers-axum --example assets_recipes_basic &    # /liquer/api/assets on :3000
curl -s -X POST 'localhost:3000/liquer/api/assets/key/data/notes/a.txt?type_identifier=Text&title=A' --data-binary hello
curl -s localhost:3000/liquer/api/assets/key/listdir/notes | jq .result
curl -s -X POST localhost:3000/liquer/api/assets/q/submit/text-hello/upper | jq .result.status
curl -s localhost:3000/liquer/api/assets/q/info/text-hello/upper | jq .result.status
curl -s localhost:3000/liquer/api/assets/q/data/text-hello/upper
```

## Agent Assignment Summary

| Step | Model | Needs | Can run in parallel with |
|---|---|---|---|
| 1 error variant | haiku | — | — |
| 2 core plumbing | sonnet | 1 | 5, 6, 7 |
| 3 core behaviour | sonnet | 2 | 5, 6, 7 |
| 4 core tests | sonnet | 3 | 5, 6, 7, 11 |
| 5 media types | haiku | — | 2–4 |
| 6 unwrap + WebSocket route | haiku | 1 | 2–5 |
| 7 ValueDescription | haiku | — | 2–6 |
| 8 handlers | sonnet | 3, 6, 7 | 11 |
| 9 routes and switches | haiku | 8 | 11 |
| 10 WebSocket | sonnet | 2, 9 | 11 |
| 11 query timeout | haiku | 6 | 8–10 |
| 12 tests | sonnet | 8–11 | — |
| 13 docs | sonnet | 12 green | — |
| 14 validation | haiku | 13 | — |

Parallel steps edit disjoint files:
- Steps 2–4 edit `liquers-core/src/assets.rs` and the test file.
- Step 5 edits `media_type.rs`.
- Step 6 edits the axum files other than `assets/handlers.rs`'s later rewrite (Step 8 runs after
  it).
- Step 7 edits `value_description.rs`.
- Step 11 edits `query/`.

## Rollback Plan

### Per-Step Rollback

`git revert <sha>` of that step's commit; never rewrite pushed history. Steps 2 and 3 share
`assets.rs` and must be reverted in reverse order. Steps 8 and 9 go together: routes without
handlers do not compile.

### Full Feature Rollback

Revert the step commits newest first, and reopen the issues that Step 13 closed, with a note. The
design documents stay.

### Partial Completion

These are coherent stopping points:
- **After Step 5:** the core is complete and usable from commands through `Context`. The HTTP
  stubs stay, `ASSETS.md` is updated, and `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` stays
  `in_progress`.
- **After Step 7:** the Step 6 fixes (`build()` no longer panics, crate-wide clippy is clean)
  also stand alone and close `AXUM-ASSETS-WEBSOCKET-ROUTE-PANICS`.

## Documentation Updates

### CLAUDE.md

No change: no new convention.

### PROJECT_OVERVIEW.md

No change: Query and Key encoding are untouched.

### README.md

`liquers-axum/README.md` gains the builder options and the two families (Step 13).

## Implementation Notes

(Filled in during execution: divergences from Phase 2 found by the tests, the observed axum
behaviour for an oversized WebSocket message, and the IANA outcome for each media type.)

## Execution Options

After approval:
1. **Execute now:** Steps 1–14 in this session, committing and pushing each step.
2. **Create a task list:** record Steps 1–14 for a later session.
3. **Revise the plan.**
4. **Exit:** implement manually from this plan.
