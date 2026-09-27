# Phase 4: Implementation Plan - Assets API over the whole AssetManager

## Overview

Ten steps, each ending in a state that compiles and passes its own validation. Core first
(error type, then trait methods, then core tests); then axum (value description, handlers,
routes, HTTP tests); then documentation; then a full validation run. Steps 1–4 touch
`liquers-core` and, for the error variant only, `liquers-axum`, `liquers-py` and `liquers-web`.
Steps 5–8 touch only `liquers-axum`. The contract is Phase 2. The tests are Phase 3, pasted
verbatim except where this plan says otherwise.

**Pre-flight facts** (verified 2026-09-27, so no step has to rediscover them):

- Only one existing test calls `AssetManager::remove`: `test_remove_asset`, `assets.rs` ≈9146. It
  removes a `Source`, so its assertion (the store no longer contains the key) holds under the new
  semantics. No existing test matches on `expire`'s "Cannot expire…" messages.
- `AssetManager` already carries `#[allow(private_bounds)]` (`assets.rs` ≈3894), so adding the
  `pub(crate)` supertrait `KeyMutationAccess` needs no new lint allowance.
- `mark_expired_status` may run on a **non-keyed** asset, and `Error::status_conflict(&Key, …)`
  needs a key. There it uses `Error::from_error(ErrorType::StatusConflict, msg)`, plus
  `.with_key(&k)` when `self.key()` is `Some`. The keyed `AssetManager::expire` path uses
  `status_conflict`.
- `liquers-web` is not in `default-members`; check it with
  `--target wasm32-unknown-unknown`. `liquers-py` is a default member; `cargo check` is enough,
  because running its tests needs a Python toolchain.
- Disk: run `cargo clean` between the native loop and the wasm check if space is low (CLAUDE.md,
  "Building and testing").

## Implementation Steps

### Step 1: `ErrorType::StatusConflict` across every exhaustive match

**Files:**
- `liquers-core/src/error.rs`
- `liquers-core/src/assets.rs` (≈2239–2252)
- `liquers-axum/src/api_core/error.rs` (≈32, ≈83, ≈110–123, ≈136–149, ≈322–326)
- `liquers-py/src/error.rs` (both conversions, plus its own `ErrorType` enum)
- `liquers-web/src/error.rs` (both directions)
- `liquers-web/tests/objects_OBJECT.rs` (≈39)

**Action:**
- Add the `StatusConflict` variant, with the doc comment from Phase 2, after `DependencyCycle`.
- Add `pub fn status_conflict(key: &Key, status: Status, operation: &str) -> Self`. Its message is
  `"Cannot {operation} '{key}': asset status is {status:?}"`, and it sets `key` and `query` the
  same way `key_not_found` and `dependency_cycle` do. `Status` is imported from
  `crate::metadata`. Check that this creates no import cycle: `metadata` already imports `error`,
  which is fine inside one crate.
- Add an arm at each match site:
  - `assets.rs`: → `PersistenceStatus::NotPersisted`
  - axum `error_to_status_code`: → `StatusCode::CONFLICT`
  - axum `parse_error_type`: `"StatusConflict"`
  - axum test lists: the variant
  - py: the variant in both directions, and in the Python enum
  - web: `"status_conflict"` in both directions, and the test list

**Validation:**
```bash
cargo check -p liquers-core -p liquers-axum -p liquers-py
cargo test -p liquers-axum --lib api_core
cargo check -p liquers-web --target wasm32-unknown-unknown   # after `cargo clean` if disk is tight
```

**Rollback:** `git checkout -- <the files above>`. The variant is additive, so nothing else
depends on it until Step 3.

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 "New Enums"; the match-site list above; `Error::dependency_cycle` as the
  constructor pattern
- **Rationale:** mechanical, and the compiler lists every missed site

---

### Step 2: `KeyMutationAccess`, the refusals in `mark_expired_status`, and a doc fix

**File:** `liquers-core/src/assets.rs`

**Action:**
- Next to `DependencyManagerAccess`, add:

  ```rust
  pub(crate) trait KeyMutationAccess {
      fn key_mutation_lock(&self) -> &tokio::sync::Mutex<()>;
  }
  ```

  Implement it for `DefaultAssetManager<E>` and `ImmediateAssetManager<E>`, each returning
  `&self.key_mutation_lock`. Add `+ KeyMutationAccess` to the `AssetManager` supertraits.
- In `mark_expired_status`, replace both `Error::general_error(...)` refusals with
  `Error::from_error(ErrorType::StatusConflict, <same message>)`, adding `.with_key(&k)` when
  `owner_key` is `Some`. The messages stay the same.
- The paragraph "Internal access to the runtime dependency graph…" currently sits above
  `expire_stored_copy`. Move it back above `pub(crate) trait DependencyManagerAccess`, which it
  describes.

**Validation:**
```bash
cargo test -p liquers-core --lib assets
```

**Rollback:** `git checkout liquers-core/src/assets.rs`

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2, "Why defaults behind a lock accessor"; `assets.rs` ≈3822–3900 and
  ≈3280–3320
- **Rationale:** small and pattern-following; `DependencyManagerAccess` is the template

---

### Step 3: The new `AssetManager` behaviour

**File:** `liquers-core/src/assets.rs`

**Action:**
- **`remove` becomes a default method** implementing Phase 2's decision table. Delete both
  per-manager `remove` bodies (≈5643 `DefaultAssetManager`, ≈6939 `ImmediateAssetManager`). The
  body runs in this order:
  1. `let _g = self.key_mutation_lock().lock().await;`
  2. Read the status: `lookup_key_asset(key)` then `asset.status().await`, otherwise the store:
     `contains`, then `get_metadata(key)?.status()`.
  3. Compute `has_recipe = self.recipe_opt(key).await?.is_some()`.
  4. Decide with an **exhaustive `match` on `Status`**; no `_ =>`.
  5. **Delete branch:** cancel the live asset, `untrack_expiration(id)`, `remove_key_asset(key)`,
     then `cascade_expire_dependents(&dep_key)`, **then** `dependency_manager().remove(&dep_key)`,
     then `store.remove(key)` if the store contains the key.
  6. **Drop-computed branch:** cancel, untrack and unmap the live asset; if the store contains the
     key, take the stored `MetadataRecord` and set `status = Recipe`, `file_size = None`,
     `is_error = false`, `error_data = None` and `progress` cleared, keeping `version`; then
     `store.set(key, &[], &md.into())`. Leave the dependency graph alone.
  7. **`Directory`:** `Err(Error::status_conflict(key, Status::Directory, "remove"))`.
  8. **Nothing live and nothing stored:** `Ok(())` if a recipe exists, otherwise
     `Err(Error::key_not_found(key))`.
- **`expire`** becomes a default method, holding the lock:
  - a live asset: `asset.expire().await`;
  - stored `Ready` or `Override`: `expire_stored_copy(store, key).await`, then
    `cascade_expire_dependents`;
  - stored `Expired`: `Ok(())`;
  - any other stored status: `status_conflict(key, status, "expire")`;
  - nothing live or stored: `status_conflict(key, Status::Recipe, "expire")` if a recipe exists,
    otherwise `key_not_found`.
- **`set_description`** becomes a default method, holding the lock:
  - if both arguments are `None`: `Err(Error::from_error(ErrorType::ParameterError, …))`;
  - a live asset: its status must be `Source`, otherwise `status_conflict(…, "set_description")`;
    then call `asset.set_description_fields(title.clone(), description.clone()).await`;
  - stored: `get_metadata`; the status must be `Source`; set the fields on the record, then
    `set_metadata`. When both a live asset and a stored copy exist, update both.
  - neither: `key_not_found`.
- **`AssetRef::set_description_fields(&self, title: Option<String>, description: Option<String>)`**
  is new and `pub(crate)`. It takes a write lock on `data`, and each `Some` field replaces
  `metadata`'s `title` or `description`. For a `Metadata::LegacyMetadata`, ignore the call and
  write a note with `eprintln!`: legacy metadata cannot hold a `Source` set by this path.
- **`get_asset_info`**: replace `let assetref = self.get(key).await?; assetref.get_asset_info()`
  with `asset.get_asset_info().await`, using the asset returned by `lookup_key_asset`.
- **Lock discipline** (Phase 2): while holding the guard, never call `get`, `owned_key_asset`,
  `to_override`, `set_binary`, `set_state` or `remove_expired_from_maps`.

**Validation:**
```bash
cargo test -p liquers-core --lib            # existing suite, incl. test_remove_asset
cargo test -p liquers-core --tests          # integration suites, incl. the expiration ones
```
If `EXPIRATION-INTEGRATION-SUITE-FAILING-AT-HEAD` is still open, compare any expiration failure
with a run at the base commit before attributing it to this step.

**Rollback:** `git checkout liquers-core/src/assets.rs`. Step 2's edits sit in the same file, so
revert to the Step 2 commit rather than to HEAD~.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2, the `remove` decision table and the lock discipline; the current
  `remove`, `set_binary`, `to_override` and `expire_stored_copy`; `ASSETS.md` "Remove Semantics"
- **Rationale:** judgement is needed on status reads, lock scope and cascade order; this is the
  only step with real concurrency risk

---

### Step 4: Core tests

**File:** `liquers-core/tests/asset_manager_remove_expire_describe.rs` (new)

**Action:** paste Phase 3's AMR tests: the shared helpers from Example 2, then AMR01–AMR07 and
AMR10–AMR23 from "Integration Tests" (20 tests). Only public API is used. Replace any `println!`
with `eprintln!`.

**Validation:**
```bash
cargo test -p liquers-core --test asset_manager_remove_expire_describe
```
If a test fails, decide whether the code or the test is wrong **against Phase 2**, not against
the test. Record a divergence in this document's "Implementation Notes" rather than silently
editing the test.

**Rollback:** delete the file.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** liquers-unittest, rust-best-practices
- **Knowledge:** Phase 3 (Example 2 and the AMR sections); Phase 2's decision table
- **Rationale:** failing tests need a diagnosis, not a transcription

---

### Step 5: `ValueDescription`

**Files:**
- `liquers-axum/src/assets/value_description.rs` (new)
- `liquers-axum/src/assets/mod.rs` (`pub mod value_description;`)

**Action:**
- Implement the struct, `VALUE_DESCRIPTION_FIELDS`, `from_json`, `from_params`, `or_previous` and
  `into_metadata_record`, exactly as specified in Phase 2.
- Errors are built with `Error::from_error(ErrorType::ParameterError, …)`.
- `into_metadata_record` builds a record with `MetadataRecord::new()` and sets only
  `type_identifier`, `type_name` (from the registry's `TypeInfo`), `data_format`, `media_type`,
  `title` and `description`.
- Paste the VD01–VD10 tests into `#[cfg(test)] mod tests` at the end of the file.

**Validation:**
```bash
cargo test -p liquers-axum --lib value_description
```

**Rollback:** delete the file and the `mod` line.

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Phase 2 "`ValueDescription`"; Phase 3 VD tests; `TypeRegistry::get`;
  `MetadataRecord` fields
- **Rationale:** self-contained pure code with its tests given

---

### Step 6: Handlers

**File:** `liquers-axum/src/assets/handlers.rs`

**Action:**
- Add the helpers `key_from_path`, `error_response`, `created` and `status_after_remove`, plus the
  DTOs, as specified in Phase 2.
- Fill in the seven existing stubs and add the eleven new handlers, with the signatures and result
  shapes from Phase 2 ("Handlers" and "Web Endpoints").
- In `post_data_handler` and `post_entry_handler`, run this sequence:
  `key_from_path` → `ValueDescription::from_params` / `from_json` →
  `.or_previous(am.get_asset_info(&key).await.ok().as_ref())` → `into_metadata_record(registry)` →
  `am.set_binary(&key, &bytes, record)` → `am.get_asset_info(&key)` → `created(info, message)`,
  where `message` names any dropped fields.
- `get_entry_handler`: pass the request's real `HeaderMap` to `select_format`, and replace
  `.parse().unwrap()` with `HeaderValue::from_static(format.mime_type())`.
- Set `query` on every `ApiResponse` built here.
- Update the module doc comment to point at `specs/design/axum-assets-endpoints/` as well as the
  original design.

**Validation:**
```bash
cargo check -p liquers-axum
cargo clippy -p liquers-axum -- -D clippy::unwrap_used   # library code only; tests excluded by default
```

**Rollback:** `git checkout liquers-axum/src/assets/handlers.rs`

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2, sections "Handlers", "Web Endpoints" and "Error Handling"; the existing
  `store/handlers.rs` `put_entry_handler` (format detection and entry decoding to reuse);
  `api_core/format.rs`
- **Rationale:** the largest step, with many small decisions about response shape

---

### Step 7: Routes and builder switches

**File:** `liquers-axum/src/assets/builder.rs`

**Action:**
- Add the `read_only: bool` (default `false`) and `admin: bool` (default `true`) fields and the
  `read_only()` and `with_admin(bool)` methods.
- In `build()`, register each route from Phase 2's route table conditionally. A route whose
  method must be omitted is simply not chained onto the `MethodRouter`: `get(h)` alone rather
  than `get(h).post(p)`.
- `POST metadata` is always registered, because it always refuses.
- `POST cancel` stays registered under `read_only()`.
- Update the module doc comment as in Step 6.

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum --lib
```

**Rollback:** `git checkout liquers-axum/src/assets/builder.rs`

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 route table; axum 0.8 `MethodRouter` chaining
- **Rationale:** mechanical once the table is fixed

---

### Step 8: HTTP tests

**Files:**
- `liquers-axum/Cargo.toml`: add `tower = { version = "0.5.3", features = ["util"] }` to
  `[dev-dependencies]`.
- `liquers-axum/tests/assets_api_endpoints.rs` (new).
- `liquers-axum/src/assets/tests.rs` and the `#[cfg(test)] mod tests;` line in `assets/mod.rs`:
  delete. The file is an empty placeholder, which the new suite replaces.

**Action:**
- Paste Phase 3's shared helpers (`env_with`, `metadata_text`, `build_app`, `send`, `send_raw`,
  `send_json`), then all 44 AAE tests: Example 1, "Review-round additions", Example 3, and the
  AAE20–AAE60 integration tests.
- `liquers-macro` is not a dependency of liquers-axum; the tests use the closure form of
  `register_command`.

**Validation:**
```bash
cargo test -p liquers-axum --test assets_api_endpoints
cargo test -p liquers-axum                 # the whole crate, including the existing suites
```

**Rollback:** delete the new test file, revert the `Cargo.toml` and `mod.rs` edits, and restore
`tests.rs`.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** liquers-unittest, rust-best-practices
- **Knowledge:** Phase 3 in full; Phase 2 "Web Endpoints"
- **Rationale:** as in Step 4, a failure must be judged against Phase 2

---

### Step 9: Documentation, issue status, index

**Files and actions:**

- **`specs/reference/WEB_API_SPECIFICATION.md`**
  - §3.3: add `StatusConflict` → 409.
  - §5.0.1: update the table. Drop `GET /api/assets/remove`; add `info`, `contains`, `version`,
    `recover`, `description`, `expire`, `override`, `makedir`, `audit` and
    `refresh_command_versions`.
  - §5.0: replace "Limited (POST entry…)" with the real write surface, and add
    "Metadata ownership".
  - §5.1: rewrite 5.1.3–5.1.9 to Phase 2's "Web Endpoints": `POST metadata` refuses with 501,
    POSTs answer 201, and removals report `new_status`. Add a subsection for each new endpoint,
    plus one for `read_only()` and `with_admin()`.
  - Add a `## History` row and bump `reviewed:`.
- **`specs/reference/ASSETS.md`**: rewrite "Remove Semantics (RESOLVED)" and "Scenario 5: Remove
  and Recalculate" with the decision table. Document `expire(key)` and `set_description`. Add a
  History row and bump `reviewed:`.
- **Issues** (§4.3):
  - `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` and `ASSET-REMOVE-FORGETS-DEPENDENTS` → `closed`,
    with a resolution note naming the tests (AAE01–AAE60; AMR01–AMR07) and the commit. Set
    `design: axum-assets-endpoints`.
  - `AXUM-HANDLER-TEST-COVERAGE` stays `accepted`; add a progress note that the assets handlers
    now have a scaffold and the Store, Query and Recipes APIs remain.
- **`specs/design/axum-assets-endpoints/DESIGN.md`**: tick Phase 4; add `gh_pr` once the PR exists.
  With `gh_pr` set, `status` must not carry `in_implementation` or `implemented` (§5.5).
- **`specs/README.md`**: capability map line for the assets API.
- Run `python3 scripts/docs_index.py`.

**Validation:**
```bash
python3 scripts/docs_index.py && git diff --stat specs/index.csv
python3 scripts/docs_index.py --check 2>/dev/null || true   # if the script offers a check mode
```

**Rollback:** `git checkout -- specs/`

**Agent Specification:**
- **Model:** sonnet
- **Skills:** none beyond reading `specs/DOCS_STRUCTURE_GUIDE.md` §4.3, §5.5, §8.1 and §9.2
- **Knowledge:** Phases 1–3; the implemented code; the current §5 of `WEB_API_SPECIFICATION.md`
- **Rationale:** a specification must end up true at HEAD, which needs the code open beside it

---

### Step 10: Full validation

```bash
cargo test -p liquers-core
cargo test -p liquers-axum
cargo test -p liquers-lib --lib --tests          # core's AssetManager changed; lib builds on it
cargo check -p liquers-py
cargo check -p liquers-core --no-default-features
cargo clean && cargo check -p liquers-web --target wasm32-unknown-unknown
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles   # if node is available
```

`scripts/check-build-matrix.sh` is **not** needed. No `#[cfg(feature)]`, optional dependency or
`ExtValue` match changes.

**Agent Specification:**
- **Model:** haiku
- **Skills:** none
- **Knowledge:** CLAUDE.md "Building and testing"
- **Rationale:** runs commands and reports

## Testing Plan

### Unit Tests

| Suite | Command | When |
|---|---|---|
| `api_core::error` variant lists and mapping | `cargo test -p liquers-axum --lib api_core` | Step 1 |
| existing core lib suite (incl. `test_remove_asset`) | `cargo test -p liquers-core --lib` | Steps 2–3 |
| VD01–VD10 | `cargo test -p liquers-axum --lib value_description` | Step 5 |

### Integration Tests

| Suite | Command | When |
|---|---|---|
| AMR01–AMR23 (20) | `cargo test -p liquers-core --test asset_manager_remove_expire_describe` | Step 4 |
| AAE01–AAE60 (44) | `cargo test -p liquers-axum --test assets_api_endpoints` | Step 8 |
| every existing suite | Step 10 list | Step 10 |

### Manual Validation

```bash
cargo run -p liquers-axum --example assets_recipes_basic &   # mounts /liquer/api/assets on :3000
curl -s -X POST 'localhost:3000/liquer/api/assets/data/notes/a.txt?type_identifier=Text&title=A' --data-binary hello
curl -s localhost:3000/liquer/api/assets/listdir/notes | jq .
curl -s -X DELETE localhost:3000/liquer/api/assets/data/notes/a.txt | jq .result
```
Adjust the base path and port to what `basic_server` prints on startup.

## Agent Assignment Summary

| Step | Model | Skills | Parallel with |
|---|---|---|---|
| 1 error variant | haiku | rust-best-practices | — |
| 2 lock access + refusals | haiku | rust-best-practices | 5 |
| 3 AssetManager behaviour | sonnet | rust-best-practices | 5 |
| 4 core tests | sonnet | liquers-unittest, rust-best-practices | 5, 6 |
| 5 ValueDescription | haiku | rust-best-practices, liquers-unittest | 2, 3, 4 |
| 6 handlers | sonnet | rust-best-practices | 4 (needs 1, 3 and 5) |
| 7 routes | haiku | rust-best-practices | — (needs 6) |
| 8 HTTP tests | sonnet | liquers-unittest, rust-best-practices | — (needs 7) |
| 9 docs | sonnet | — | — (needs 8 green) |
| 10 validation | haiku | — | — |

Parallel steps edit disjoint files. Step 5 edits only `liquers-axum/src/assets/`; Steps 2–4 edit
only `liquers-core`.

## Rollback Plan

### Per-Step Rollback

Each step is one commit on `claude/fervent-cori-ew4kvn`. Revert that commit
(`git revert <sha>`), never rewriting pushed history. Steps 2 and 3 share `assets.rs`; revert them
in reverse order.

### Full Feature Rollback

`git revert` the step commits from newest to oldest. The design documents stay: they record the
decision, not the code. Reopen the issues that Step 9 closed, with a note.

### Partial Completion

- **Steps 1–4 alone** are a coherent core change: status-aware `remove`, `expire(key)`,
  `set_description` and `StatusConflict`, usable from commands through `Context`, with the axum
  stubs still at 501. If work stops there, update `ASSETS.md` (the part of Step 9 about it) and
  leave `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` `in_progress`.
- **Steps 5–8 depend on 1 and 3.**

## Documentation Updates

### CLAUDE.md

No change. No new convention is introduced: `KeyMutationAccess` is internal, and the
`register_command` and test conventions are unchanged.

### PROJECT_OVERVIEW.md

No change. Query and Key encoding are untouched. `ASSETS.md` and `WEB_API_SPECIFICATION.md` carry
the behaviour (Step 9).

### README.md

`liquers-axum/README.md`: add a short `read_only()` / `with_admin()` note if it lists builder
options. Check during Step 9.

## Execution Options

After approval:
1. **Execute now:** run Steps 1–10 in this session, committing and pushing after each step.
2. **Create a task list:** record Steps 1–10 as tasks for a later session.
3. **Revise the plan:** return to this document.
4. **Exit:** implement manually from this plan.
