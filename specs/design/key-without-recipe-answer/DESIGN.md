---
id: KEY-WITHOUT-RECIPE-ANSWER
kind: design
title: What a key answers when the store cannot serve it and no recipe makes it
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets, core/error]
issues: [MISSING-RESOURCE-REPORTED-AS-GENERAL-ERROR, DEGRADED-STORED-VALUE-WITHOUT-RECIPE-REPORTS-NO-RECIPE, DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE]
merged: 2026-10-20
created: 2026-10-20
---
# What a key answers when the store cannot serve it and no recipe makes it

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. Three sources share it (`overlap.md` **T1**: whatever made `try_fast_track` decline, the
asset is queued and the recipe provider's "No recipe found" is the only diagnosis; **T2**: one
contract for every unservable key). `MISSING-RESOURCE-REPORTED-AS-GENERAL-ERROR` leads.

## Phase 1: High-Level Design

### Purpose

Give every key the store cannot serve and no recipe makes an answer that says why: missing
(`KeyNotFound` with the key), unreadable here (naming the type), or a directory (a `Directory`
state a command can act on) — not a pointer to a recipe nobody meant to write.

### Problem Example

All three reproduce at HEAD (18732dd) by code reading and existing tests:

| Query | Store holds | Today | Should be |
|---|---|---|---|
| `-R/data/missing.txt` | nothing | `General` "No recipe found…", `key` empty (`with_key` writes `query`); HTTP 500 | `KeyNotFound` with `key`; HTTP 404 |
| `-R/o/obj` | a `py.Object` pickled elsewhere | "No recipe found"; the degraded reason goes to stderr | the `get_any_status` degraded error: type, reason, key |
| `-R/data/-/ns-rec/file_records/files.csv` | files under `data/` | "No recipe found for key data" | `GetAsset(data)` is a `Directory` state naming `data`; `file_records` lists it |

Evidence: `assets.rs` `try_fast_track` (degraded `Ok(None)`, "stored without bytes", not contained)
→ `evaluate_recipe_outcome` → `get_recipe_provider().recipe(&key)`; `recipes.rs`
`DefaultRecipeProvider::recipe`, `RecipeProviderChain::no_recipe_error`; tests
`metadata_only_entry_without_recipe_answers_like_an_absent_key`, Python
`test_VALUE_pickle_off_error_names_the_type` (strict `xfail`).

### Scope and Acceptance Criteria

- **AC-1** Missing key is `KeyNotFound`
  WHEN a key is neither stored (no data object, or not contained at all) nor made by any recipe provider
  THEN the evaluation fails with `ErrorType::KeyNotFound`, `error.key` set to the key, and a message saying the store holds no value for it and no recipe produces it
- **AC-2** Degraded value names its type
  WHEN a key's stored value has a type this build cannot read (unregistered, or registered without data formats) and no recipe makes the key
  THEN the evaluation fails with the same error `AssetManager::get_any_status` reports — type identifier, reason (`degraded_reason`), key — and the bytes are never decoded
- **AC-3** Other unusable stored values say why
  WHEN the fast track declines a stored value (unreadable bytes, a stored status that is not reused, a stale dependency) and no recipe makes the key
  THEN the evaluation fails with that reason (the deserializer's error, the stored error, or `StatusConflict`), carrying the key, not "No recipe found"
- **AC-4** A recipe still wins
  WHEN any of AC-1..AC-3 holds but a recipe makes the key
  THEN the recipe is evaluated exactly as today
- **AC-5** A directory key gives a `Directory` state
  WHEN `-R/<dir>` names a key the store reports as a directory and no recipe makes it
  THEN the asset finishes `Status::Directory` without a job, value none, with the store's directory metadata (key, `is_dir`), which the next action receives
- **AC-6** A directory state is never cached or stored
  WHEN a later request asks for a directory key, its contents having changed or not
  THEN it re-reads the store (evicted on entry like `Volatile`) and nothing is written for it
- **AC-7** Bindings see the type
  WHEN Python evaluates a missing resource, or a pickle-off environment reads a pickled object
  THEN it raises `KeyNotFoundError` with `key` set, or a `LiquersError` whose message names `py.Object`

Non-goals: the providers' own `recipe()` errors (Recipes API, `recipe_plan`); `Error::with_key`
(`error-with-key-field`); expiring what was built from a directory when it changes (as `-R-sdir/`
today); `JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE` (`liquers-web`; same symptom, store-side
cause — it reports `KeyNotFound` once this lands).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — a new `pub` constructor `Error::resource_not_found` (rule 4),
  documented behaviour changes (`ASSET_LIFECYCLE.md` metadata-only paragraph, `AssetManager::makedir`
  doc), tests in `liquers-py` and `liquers-lib` (rule 6).
- **Leading issue:** Q2, the answer for a directory key.
- **Explanation:** one decision point (fast-track decline, then no recipe) serves all three sources;
  the plan is complete under the recommended answers below, none of which changes the plan's shape.
- **Open questions:**
  1. **Open design - error type of a missing key:** `KeyNotFound` through a new
     `Error::resource_not_found(key)` (recommended: HTTP 404, Python `KeyNotFoundError`, matches
     `STORE_SEMANTICS.md`), or keep `General` and only fix `key` and the message.
  2. **Open design - directory key:** a `Directory` state (recommended; the issue's expectation,
     and `-R/data/-/ns-rec/file_records` then works as `record-streams` first wrote it), or a typed
     error pointing to `-R-sdir/<dir>` (keeps `-R/<dir>` failing; no caching rule needed).
  3. **Proposed resolution - precedence:** a recipe whose key is also a store directory is evaluated
     (AC-4); the directory answer applies only when no recipe makes the key.
  4. **Proposed resolution - degraded error type:** keep `General` as `get_any_status` has it, both
     paths built by one helper; `NotSupported` is the alternative.

### Design Dependencies

- `stored-value-default-data-format` — **overlaps**: edits `deserialize_stored_value`, from which
  this design extracts the degraded test. No order; kept separate by exclusion E1.
- `error-with-key-field` — **overlaps**: fixes `with_key`; this design sets `key` directly, so
  neither waits for the other.
- `record-streams` (complete, E4) — origin of `DIRECTORY-KEY-…`; not extended.

### Consolidated Findings

- One branch serves all three: classify the decline in `try_fast_track` (reason known), report it
  in `evaluate_recipe_outcome` (absence of a recipe known). No second store read.
- `recipe()` must become `recipe_opt()` there: only absence may be replaced, never a malformed
  `recipes.yaml`.
- A directory is detected on `get` → `KeyNotFound` (every in-tree store, the router above its
  mounts included), so an ordinary fast track pays no extra `is_dir`.
- Directory assets are evicted at entry at three sites in two managers, or the listing in their
  metadata goes stale; `wait_for_dependency` already accepts `Directory`.
- A strict `xfail` in `liquers-py` flips to a pass and is un-marked in the same change.

## Phase 2: Architecture

### Solution

1. **Record why the fast track declined.** `AssetData` gains a private
   `fast_track_refusal: Option<Error>`, reset at the start of `try_fast_track`. Each decline site
   calls a private `decline_fast_track(&mut self, refusal: Option<Error>)` (which also does what
   `clear_fast_track_payload` does):

   | Decline site in `try_fast_track` | Refusal recorded |
   |---|---|
   | key not contained; metadata-only entry (`get` → `KeyNotFound`, not a directory) | `None` (absent) |
   | stored status not `Ready`/`Source`/`Override` | the stored error for `Error`/`Cancelled` (`metadata.error_result()`), else `Error::status_conflict(key, status, "read")` |
   | `deserialize_stored_value` → `Ok(None)` | `degraded_value_error(key, type_identifier, registry)` |
   | `deserialize_stored_value` → `Err(e)` | `e` with `key` set when unset |
   | outside change treated as corrupted; stale dependency (two sites) | `Error::general_error` with the message already printed there, `key` set |

2. **Report it when nothing can recompute.** In `evaluate_recipe_outcome`'s `Some(_) | None` branch,
   `recipe(&key)` becomes `recipe_opt(&key)`; on `Ok(None)`:
   `return Err(refusal.unwrap_or_else(|| Error::resource_not_found(&key)))`, where `refusal` is
   `self.data.write().await.fast_track_refusal.take()`. `Err` from `recipe_opt` propagates as today.
3. **Directories.** In `try_fast_track`, when `store.get` fails with `KeyNotFound` and
   `store.is_dir(&key)` is true and `recipe_opt(&key)` is `None`: read
   `store.get_metadata(&key)`, make the asset a directory (private
   `AssetData::become_directory(metadata)`, the body `AssetManager::makedir` uses today, which now
   calls it), send `JobFinished`, return `Ok(true)`. With a recipe: decline (AC-4).
4. **Never reuse a directory asset.** Add `Status::Directory` to the entry-eviction `matches!` in
   `DefaultAssetManager::get`, `DefaultAssetManager::get_dependency_asset` and
   `ImmediateAssetManager::settle`. A fresh asset starts in `None`, so the loop cannot spin.
   Directory status is already not persisted (`save_to_store` is never reached) and not tracked
   (`DependencyManager::track_asset`).

Rejected: re-reading metadata in the no-recipe branch (a second read; blind to corrupt bytes);
setting `Error` inside the fast track (skips failure finalization and notifications); directories in
`evaluate_recipe_outcome` (would be finalized `Ready` and stored).

Known-issue preflight: `ERROR-WITH-KEY-SETS-QUERY-FIELD` — designed around (keys set directly);
`JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE` — not blocking; `CORE-TRACING-MIGRATION` edits the
same `eprintln!` lines — textual only. No command or namespace is altered.

### Changes

`liquers-core/src/error.rs`:

```rust
/// A key the store holds no value for and no recipe produces (`ErrorType::KeyNotFound`, `key` set).
pub fn resource_not_found(key: &Key) -> Self;
```

Message: `Resource '<key>' not found: the store holds no value for it and no recipe produces it`.

`liquers-core/src/assets.rs`: the field, `decline_fast_track`, `become_directory`, and private
`degraded_value_error(key, type_identifier, registry)` (General, the `get_any_status` message, `key`
set), shared by `try_fast_track` and `AssetManager::get_any_status`; `evaluate_recipe_outcome`;
`makedir` (doc: `get` on its key now gives this state); three eviction sets. Async as today.

Documents (History rows, `reviewed:`): `ASSET_LIFECYCLE.md` (fast-track outcomes, directory key),
`ASSETS.md` (`Directory` reached by `get`), `VALUE_TYPE_SYSTEM.md` §Reading, `PYTHON_INTEGRATION.md`
(two limitations dropped), `RECORD_STREAMS.md` (plain `-R/<dir>` spelling),
`api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A custom provider whose `recipe` and `recipe_opt` disagree | the trait doc requires `Ok(None)` only for absence; full test run | medium |
| Clients matching "No recipe found" | one Rust test and one Python `xfail`, both updated | high |
| Missing key: HTTP 500 → 404 | intended (`WEB_API_SPECIFICATION.md` maps `KeyNotFound` to 404) | high |
| Large directory: `get_metadata` lists children | as `-R-meta/<dir>`; only on a directory request | medium |

Recovery: steps separable; reverting step 5 leaves AC-1..AC-4.

## Phase 3: Examples and Tests

### Examples

```rust
let err = envref.evaluate("-R/data/missing.txt").await?.get().await
    .and_then(|s| s.value()).unwrap_err();
assert_eq!(err.error_type, ErrorType::KeyNotFound);
assert_eq!(err.key.as_deref(), Some("data/missing.txt"));

let state = envref.evaluate("-R/data").await?.get().await?;   // the store holds data/a.txt
assert_eq!(state.status(), Status::Directory);
assert_eq!(state.metadata.key()?, Some(parse_key("data")?));
```

### Tests

Unit, `liquers-core/src/assets.rs` `mod tests` (memory store, `SimpleEnvironment`):

- `missing_key_without_recipe_is_key_not_found`, `metadata_only_key_without_recipe_is_key_not_found` — type, `key`, message: AC-1
- `degraded_stored_value_without_recipe_names_the_type` — `test.Formatless` registry: identifier, "no data format", `key`: AC-2
- `corrupt_stored_value_without_recipe_reports_the_read_error` (`SerializationError`), `stored_error_status_without_recipe_reports_the_stored_error`: AC-3
- `missing_key_with_recipe_still_evaluates`, `directory_key_with_recipe_evaluates_the_recipe`: AC-4
- `directory_key_gives_a_directory_state` (no job run; metadata key `data`, `is_dir`), `router_directory_above_mounts_gives_a_directory_state`: AC-5
- `directory_state_is_reread_after_a_write_below_it` (new asset; children include `b.txt`; nothing stored at `data`): AC-6
- `immediate_manager_directory_key_gives_a_directory_state`: AC-5, AC-6

Integration: `metadata_only_entry_reload.rs` `metadata_only_entry_without_recipe_answers_like_an_absent_key`
asserts `KeyNotFound` and `key` (AC-1); `liquers-lib/tests/records_end_to_end.rs`
`file_records_lists_a_store_directory_through_a_plain_resource_query` (AC-5); `liquers-py`
`test_VALUE_pickle_off_error_names_the_type` un-marked and `test_EVAL_missing_resource_raises_key_not_found` (AC-7).

Run: `cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --test records_end_to_end`;
`bash scripts/test-liquers-py.sh`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `error.rs` — `Error::resource_not_found`, test `resource_not_found_sets_type_and_key` — `cargo test -p liquers-core --lib error`. Rollback: revert (additive).
- [ ] 2. `assets.rs` — `degraded_value_error`, used by `get_any_status` — `cargo test -p liquers-core --lib degraded`.
- [ ] 3. `assets.rs` — `fast_track_refusal`, `decline_fast_track` at every decline site — `cargo test -p liquers-core --lib fast_track` (no behaviour change). Depends on 2.
- [ ] 4. `assets.rs` `evaluate_recipe_outcome` — `recipe_opt`, refusal or `resource_not_found`; AC-1..AC-4 tests; `metadata_only_entry_reload.rs` — `cargo test -p liquers-core --lib --tests`. Depends on 1, 3.
- [ ] 5. `assets.rs` — `become_directory` (also in `makedir`), directory branch, three eviction sets; AC-5/AC-6 tests — `cargo test -p liquers-core --lib directory`. Depends on 3.
- [ ] 6. `liquers-lib` — `records_end_to_end.rs` test; fix the comment in `src/records/commands.rs` — `cargo test -p liquers-lib --lib --tests`. Depends on 5.
- [ ] 7. `liquers-py` — un-mark the `xfail`, add the `KeyNotFound` test — `bash scripts/test-liquers-py.sh`. Depends on 4.
- [ ] 8. Phase 2 documents; close the three issues; note on `JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE` — `python3 scripts/docs_index.py --check`.

### Validation

The commands above, then after `cargo clean` the `liquers-web` Node loop (same evaluation path);
diff review limited to the named files.
