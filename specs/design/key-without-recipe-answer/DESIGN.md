---
id: KEY-WITHOUT-RECIPE-ANSWER
kind: design
title: What a key answers when the store cannot serve it and no recipe makes it
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: not-eligible
area: [core/assets, core/error, py, web, axum]
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
(`AssetNotFound` with the key), unreadable here (naming the type), or a directory (a `Directory`
state a command can act on) — not a pointer to a recipe nobody meant to write.

### Problem Example

All three reproduce at HEAD (18732dd) by code reading and existing tests:

| Query | Store holds | Today | Should be |
|---|---|---|---|
| `-R/data/missing.txt` | nothing | `General` "No recipe found…", `key` empty (`with_key` writes `query`); HTTP 500 | `AssetNotFound` with `key`; HTTP 404 |
| `-R/o/obj` | a `py.Object` pickled elsewhere | "No recipe found"; the degraded reason goes to stderr | the `get_any_status` degraded error: type, reason, key |
| `-R/data/-/ns-rec/file_records/files.csv` | files under `data/` | "No recipe found for key data" | `GetAsset(data)` is a `Directory` state naming `data`; `file_records` lists it |

Evidence: `assets.rs` `try_fast_track` (degraded `Ok(None)`, "stored without bytes", not contained)
→ `evaluate_recipe_outcome` → `get_recipe_provider().recipe(&key)`; `recipes.rs`
`DefaultRecipeProvider::recipe`, `RecipeProviderChain::no_recipe_error`; tests
`metadata_only_entry_without_recipe_answers_like_an_absent_key`, Python
`test_VALUE_pickle_off_error_names_the_type` (strict `xfail`).

### Scope and Acceptance Criteria

- **AC-1** Missing key is `AssetNotFound`
  WHEN a key is neither stored (no data object, or not contained at all) nor made by any recipe provider
  THEN the evaluation fails with `ErrorType::AssetNotFound`, `error.key` set to the key, and a message saying the store holds no value for it and no recipe produces it
- **AC-1a** Errors fall into overlapping categories
  WHEN an error's type is in one or more of the categories `Store` (every store error: `KeyNotFound`, `KeyNotSupported`, `KeyNotAbsolute`, `KeyReadError`, `KeyWriteError`), `KeyedAsset` (the same for keyed assets: `AssetNotFound`, `AssetNotSupported`) and `NotFound` (`KeyNotFound`, `AssetNotFound`)
  THEN `ErrorType::categories()` lists exactly those, and every binding can catch by category: Python raises a class deriving from each category's class (`StoreError`, `KeyedAssetError`, `NotFoundError`, all `LiquersError`; `NotFoundError` is also a `LookupError`), so `KeyNotFoundError` is a `StoreError` and a `NotFoundError`; JavaScript's `LiquersError` reports `categories` (`["store", "not_found"]` for `"key_not_found"`); HTTP maps both misses to 404. A type in no category (`General`, `ParseError`, …) derives from `LiquersError` alone
- **AC-1b** Existing asset-level misses use the asset type
  WHEN an `AssetManager` operation, or an axum asset endpoint, refuses a key because it is neither stored nor made by a recipe (`can_make` false)
  THEN it fails with `AssetNotFound`; a store-level miss (`removedir` of a key the store does not contain, every `AsyncStore` read) stays `KeyNotFound`
- **AC-2** Degraded value names its type
  WHEN a key's stored value has a type this build cannot read (unregistered, or registered without data formats) and no recipe makes the key
  THEN the evaluation fails with `AssetNotSupported`, the same error `AssetManager::get_any_status` reports — type identifier, reason (`degraded_reason`), key — and the bytes are never decoded
- **AC-3** Other unusable stored values say why
  WHEN the fast track declines a stored value (unreadable bytes, a stored status that is not reused, a stale dependency) and no recipe makes the key
  THEN the evaluation fails with that reason (the deserializer's error, the stored error, or `StatusConflict`), carrying the key, not "No recipe found"
- **AC-4** A recipe still wins over an unusable stored value
  WHEN any of AC-1..AC-3 holds but a recipe makes the key
  THEN the recipe is evaluated exactly as today
- **AC-5** A directory key gives a `Directory` state
  WHEN `-R/<dir>` names a key the store reports as a directory and no recipe makes it
  THEN the asset with key `<dir>` finishes `Status::Directory` without a job, value none, with the store's directory metadata (key, `is_dir`), which the next action receives
- **AC-5a** A recipe for a directory key is refused
  WHEN the store reports a key as a directory and a recipe also makes it
  THEN the evaluation fails with `AssetNotSupported`, `key` set, a message naming the directory and saying a recipe for a directory key is not supported; the recipe is not run and nothing is written
- **AC-6** A directory state is never cached or stored
  WHEN a later request asks for a directory key, its contents having changed or not
  THEN it re-reads the store (evicted on entry like `Volatile`) and nothing is written for it
- **AC-7** Bindings see the type
  WHEN Python evaluates a missing resource, or a pickle-off environment reads a pickled object
  THEN it raises `AssetNotFoundError` with `key` set (caught by `except lq.NotFoundError`), or a `LiquersError` whose message names `py.Object`

Non-goals: the providers' own `recipe()` errors (Recipes API, `recipe_plan`); `Error::with_key`
(`error-with-key-field`); expiring what was built from a directory when it changes (as `-R-sdir/`
today); `JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE` (`liquers-web`; same symptom, store-side
cause — it reports `AssetNotFound` once this lands). Asset twins of `KeyReadError`, `KeyWriteError`
and `KeyNotAbsolute`: an asset that fails because its store did keeps the store's error, category
included. Reviewing and clustering every `ErrorType`, `General` above all, and placing the rest in
categories: `ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED`.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — two new `ErrorType` variants, `ErrorCategory`,
  `Error::asset_not_found` / `asset_not_supported` and `ErrorType::categories` (rule 4), new Python
  base classes and a JavaScript getter,
  documented behaviour changes (`ASSET_LIFECYCLE.md` metadata-only paragraph, `AssetManager::makedir`
  doc), tests in `liquers-py` and `liquers-lib` (rule 6).
- **Leading issue:** `MISSING-RESOURCE-REPORTED-AS-GENERAL-ERROR`.
- **Explanation:** one decision point (fast-track decline, then no recipe) serves all three sources;
  Orest decided all four questions on 2026-10-25.
- **Decisions:**
  1. **Decided (Orest, 2026-10-25) - error type of a missing key:** a distinct
     `ErrorType::AssetNotFound` (`Error::asset_not_found(key)`), and a directed graph of categories rather than one parent: `Store`,
     `KeyedAsset` (the keyed-asset counterpart of `Store`) and `NotFound`, a type belonging to any
     number of them (AC-1, AC-1a, AC-1b). The two misses mean different things: `KeyNotFound` says the store holds no
     data object at the key, which may only mean the recipe has not run; `AssetNotFound` says
     nothing stored and no recipe, so nothing can produce it. Rejected: a store/asset flag on
     `KeyNotFound` (Python and JavaScript cannot catch on a field, and the ~30 sites that read
     `KeyNotFound` as "not stored yet, recompute or return `None`" — `liquers-records` chunk
     fallback, `store/pipeline.rs`, `store/storage.rs` — would silently match asset misses unless
     each checked the flag; a distinct type propagates by default); a new type without categories
     (catching any miss would take two clauses in every client); a single-parent hierarchy (a store
     miss is both a store error and a miss); plain `KeyNotFound` (the design as first written).
  2. **Decided (Orest, 2026-10-25) - directory key:** `-R/<dir>` is the asset with key `<dir>` in a
     `Directory` state (AC-5, AC-6); `-R/data/-/ns-rec/file_records` then works as `record-streams`
     first wrote it. Rejected: a typed error pointing to `-R-sdir/<dir>`.
  3. **Decided (Orest, 2026-10-25) - precedence:** a recipe whose key is a store directory is not
     supported for now and ends with an error (AC-5a), `AssetNotSupported`. A store cannot hold a
     value where it holds a directory, so such a recipe's result could never be stored either.
  4. **Decided (Orest, 2026-10-25) - degraded error type:** `AssetNotSupported`, built by one helper
     for both paths (`get_any_status` moves off `General`). Orest left the choice open between a new
     type and `General` / `NotSupported`, since every error type is to be reviewed under
     `ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED`; the new type is reused here and by AC-5a, so it is
     no single-site type, and it is the keyed-asset counterpart of the store's `KeyNotSupported`
     ("this build cannot serve this key"). `NotSupported` stays for operations never available.

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

0. **The error types and categories.** `ErrorType::AssetNotFound` and `ErrorType::AssetNotSupported`
   (after `KeyNotFound`), `Error::asset_not_found` and `Error::asset_not_supported(key, message)`,
   `enum ErrorCategory { Store, KeyedAsset, NotFound }` and `ErrorType::categories(&self) ->
   &'static [ErrorCategory]`, an exhaustive `match` (no `_ =>`) so a new variant must be placed;
   `ErrorType::is(&self, ErrorCategory)`. Categories are fixed per type, never per instance.
   Bindings: `liquers-py/src/error.rs` mirrors both variants and exposes `ErrorCategory` and
   `ErrorType.categories`; `_errors.py` `_install` first creates one class per category
   (`StoreError`, `KeyedAssetError`, `NotFoundError`, each `(LiquersError,)`, `NotFoundError`
   adding `LookupError`), then each variant's class with its categories' classes as bases in
   `categories()` order, or `(LiquersError,)` when it has none (Python's MRO linearises the diamond:
   `KeyNotFoundError` → `StoreError` → `NotFoundError` → `LiquersError` → `LookupError`); a category
   class is abstract like `LiquersError` (constructing it needs an `ErrorType`), and no category class
   name may equal a variant class name (tested). `liquers-web/src/error.rs` adds both names to the
   two tables and a `categories` getter (`string[]`: `"store"`, `"keyed_asset"`, `"not_found"`),
   declared in the `.d.ts`. `liquers-axum/src/api_core/error.rs` maps `AssetNotFound` to 404 and
   `AssetNotSupported` as `KeyNotSupported` is mapped, and parses both names. Asset-level
   sites move from `key_not_found` to `asset_not_found` (AC-1b): the `can_make`-gated refusals and
   `get_asset_info` in `assets.rs`, `submit_key_asset` and `key_metadata` in
   `liquers-axum/src/assets/common.rs`, the two subscribe refusals in `assets/websocket.rs`. Each
   site is judged by what it checked: `store.contains` alone stays `KeyNotFound`.
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
   a recorded refusal that overrides the recipe (step 3) is returned first, before any recipe is
   planned. Otherwise `recipe(&key)` becomes `recipe_opt(&key)`; on `Ok(None)`:
   `return Err(refusal.unwrap_or_else(|| Error::asset_not_found(&key)))`, where `refusal` is
   `self.data.write().await.fast_track_refusal.take()`. `Err` from `recipe_opt` propagates as today.
3. **Directories.** In `try_fast_track`, when `store.get` fails with `KeyNotFound` and
   `store.is_dir(&key)` is true and `recipe_opt(&key)` is `None`: read
   `store.get_metadata(&key)`, make the asset a directory (private
   `AssetData::become_directory(metadata)`, the body `AssetManager::makedir` uses today, which now
   calls it), send `JobFinished`, return `Ok(true)`. With a recipe: decline, recording
   `Error::asset_not_supported(key, "… is a store directory; a recipe for a directory key is not
   supported")` as a refusal that overrides the recipe (`fast_track_refusal` becomes a small private
   struct `{ error, overrides_recipe: bool }`), so step 2 fails the asset through the normal failure
   path (AC-5a).
4. **Never reuse a directory asset.** Add `Status::Directory` to the entry-eviction `matches!` in
   `DefaultAssetManager::get`, `DefaultAssetManager::get_dependency_asset` and
   `ImmediateAssetManager::settle`. A fresh asset starts in `None`, so the loop cannot spin.
   Directory status is already not persisted (`save_to_store` is never reached) and not tracked
   (`DependencyManager::track_asset`).

Rejected: running the recipe for a directory key (its value could never be stored at a directory);
re-reading metadata in the no-recipe branch (a second read; blind to corrupt bytes);
setting `Error` inside the fast track (skips failure finalization and notifications); directories in
`evaluate_recipe_outcome` (would be finalized `Ready` and stored).

Known-issue preflight: `ERROR-WITH-KEY-SETS-QUERY-FIELD` — designed around (keys set directly);
`JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE` — not blocking; `CORE-TRACING-MIGRATION` edits the
same `eprintln!` lines — textual only. No command or namespace is altered.

### Changes

`liquers-core/src/error.rs`:

```rust
pub enum ErrorType {
    // ...
    KeyNotFound,
    /// Nothing is stored at the key and no recipe produces it: no evaluation can make the asset.
    /// Distinct from [`ErrorType::KeyNotFound`], a store miss a recipe may still answer.
    AssetNotFound,
    /// This build or this environment cannot serve the key as an asset: its stored value has a
    /// type that cannot be read here, or a recipe claims a key the store holds as a directory.
    /// The keyed-asset counterpart of [`ErrorType::KeyNotSupported`].
    AssetNotSupported,
    // ...
}

/// A group of error types a caller can test, or catch, at once. A type may be in several.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    /// Raised by a store: `KeyNotFound`, `KeyNotSupported`, `KeyNotAbsolute`, `KeyReadError`, `KeyWriteError`.
    Store,
    /// Raised for a keyed asset: `AssetNotFound`, `AssetNotSupported`.
    KeyedAsset,
    /// Nothing at the key: `KeyNotFound` (stored), `AssetNotFound` (stored or made).
    NotFound,
}

impl ErrorType {
    pub fn categories(&self) -> &'static [ErrorCategory];
    pub fn is(&self, category: ErrorCategory) -> bool;
}

/// A key the store holds no value for and no recipe produces (`ErrorType::AssetNotFound`, `key` set).
pub fn asset_not_found(key: &Key) -> Self;
/// A keyed asset this environment cannot serve (`ErrorType::AssetNotSupported`, `key` set).
pub fn asset_not_supported(key: &Key, message: String) -> Self;
```

Message: `Asset '<key>' not found: the store holds no value for it and no recipe produces it`.

Bindings: `liquers-py/src/error.rs`, `liquers-py/python/liquers/_errors.py` (category classes),
`liquers-web/src/error.rs` (name tables, `categories` getter) and its `.d.ts`,
`liquers-axum/src/api_core/error.rs`; asset-level call sites in `liquers-core/src/assets.rs` and
`liquers-axum/src/assets/{common,websocket}.rs` (step 0).

`liquers-core/src/assets.rs`: the field, `decline_fast_track`, `become_directory`, and private
`degraded_value_error(key, type_identifier, registry)` (`AssetNotSupported`, the `get_any_status`
message, `key` set), shared by `try_fast_track` and `AssetManager::get_any_status`; `evaluate_recipe_outcome`;
`makedir` (doc: `get` on its key now gives this state); three eviction sets. Async as today.

Documents (History rows, `reviewed:`): `PROJECT_OVERVIEW.md` / error section (the two misses),
`STORE_SEMANTICS.md` (store answers `KeyNotFound`, never `AssetNotFound`),
`WEB_API_SPECIFICATION.md` (404 for both), `PYTHON_GUIDE.md` and
`LANGUAGE-INTEGRATION_GUIDE.md` (category classes, JS `categories`), `ASSET_LIFECYCLE.md` (fast-track outcomes, directory key),
`ASSETS.md` (`Directory` reached by `get`), `VALUE_TYPE_SYSTEM.md` §Reading, `PYTHON_INTEGRATION.md`
(two limitations dropped), `RECORD_STREAMS.md` (plain `-R/<dir>` spelling),
`api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A custom provider whose `recipe` and `recipe_opt` disagree | the trait doc requires `Ok(None)` only for absence; full test run | medium |
| Clients matching "No recipe found" | one Rust test and one Python `xfail`, both updated | high |
| Missing key: HTTP 500 → 404 | intended (`WEB_API_SPECIFICATION.md` maps both misses to 404) | high |
| A caller matching `KeyNotFound` for an asset-level refusal (AC-1b) now misses it | grep every `== ErrorType::KeyNotFound` outside stores; switch to `is(ErrorCategory::NotFound)` where either miss is meant | medium |
| A caller matching `General` from `get_any_status` | grep; none in tree outside its own tests | high |
| A recipe today defined for a directory key stops working (AC-5a) | intended (Orest); its value could never be stored there; grep in-tree `recipes.yaml` | medium |
| `liquers-web` is maintained in another project | the change there is mechanical (exhaustive name tables fail to compile without it); land it in the same PR | high |
| Large directory: `get_metadata` lists children | as `-R-meta/<dir>`; only on a directory request | medium |

Recovery: steps separable; reverting step 5 leaves AC-1..AC-4.

## Phase 3: Examples and Tests

### Examples

```rust
let err = envref.evaluate("-R/data/missing.txt").await?.get().await
    .and_then(|s| s.value()).unwrap_err();
assert_eq!(err.error_type, ErrorType::AssetNotFound);
assert!(err.error_type.is(ErrorCategory::NotFound) && err.error_type.is(ErrorCategory::KeyedAsset));
assert_eq!(err.key.as_deref(), Some("data/missing.txt"));

let state = envref.evaluate("-R/data").await?.get().await?;   // the store holds data/a.txt
assert_eq!(state.status(), Status::Directory);
assert_eq!(state.metadata.key()?, Some(parse_key("data")?));
```

```python
try:
    env.evaluate("-R/data/missing.txt")
except lq.StoreError:
    ...                      # not reached: KeyNotFoundError and the other store errors only
except lq.NotFoundError as e:
    assert type(e) is lq.AssetNotFoundError and isinstance(e, lq.KeyedAssetError)
    assert isinstance(e, LookupError) and e.key == "data/missing.txt"
```

### Tests

Unit, `liquers-core/src/error.rs`: `asset_not_found_sets_type_and_key`; `categories_of_every_variant` (a table over every variant: the `Store`, `KeyedAsset` and `NotFound` members exactly, the rest none): AC-1a.

Unit, `liquers-core/src/assets.rs` `mod tests` (memory store, `SimpleEnvironment`):

- `missing_key_without_recipe_is_asset_not_found`, `metadata_only_key_without_recipe_is_asset_not_found` — type, `key`, message: AC-1
- `get_asset_info_of_unmakeable_key_is_asset_not_found`; `removedir_of_absent_key_stays_key_not_found`: AC-1b
- `degraded_stored_value_without_recipe_names_the_type` — `test.Formatless` registry: `AssetNotSupported`, identifier, "no data format", `key`; `get_any_status` gives the same error: AC-2
- `corrupt_stored_value_without_recipe_reports_the_read_error` (`SerializationError`), `stored_error_status_without_recipe_reports_the_stored_error`: AC-3
- `missing_key_with_recipe_still_evaluates`: AC-4
- `directory_key_with_recipe_is_asset_not_supported` (recipe not run, nothing written at `data`): AC-5a
- `directory_key_gives_a_directory_state` (no job run; metadata key `data`, `is_dir`), `router_directory_above_mounts_gives_a_directory_state`: AC-5
- `directory_state_is_reread_after_a_write_below_it` (new asset; children include `b.txt`; nothing stored at `data`): AC-6
- `immediate_manager_directory_key_gives_a_directory_state`: AC-5, AC-6

Integration: `metadata_only_entry_reload.rs` `metadata_only_entry_without_recipe_answers_like_an_absent_key`
asserts `AssetNotFound` and `key` (AC-1); `liquers-axum` `error_to_status_code(AssetNotFound)` is 404 and
a submit of an unmakeable key answers `AssetNotFound` (AC-1a, AC-1b); `liquers-web` `objects_OBJECT.rs`
round-trips `asset_not_found` / `asset_not_supported` and `categories` (AC-1a); `liquers-lib/tests/records_end_to_end.rs`
`file_records_lists_a_store_directory_through_a_plain_resource_query` (AC-5); `liquers-py`
`test_VALUE_pickle_off_error_names_the_type` un-marked, `test_EVAL_missing_resource_raises_asset_not_found`
(AC-7), and `test_ERROR_category_classes` (each variant's class derives from exactly its categories' classes;
`NotFoundError` is a `LookupError`; no category name clashes with a variant name; AC-1a).

Run: `cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-axum`; `cargo test -p liquers-lib --test records_end_to_end`;
`bash scripts/test-liquers-py.sh`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `error.rs` — `AssetNotFound`, `AssetNotSupported`, `ErrorCategory`, `categories` / `is`, the two constructors, error tests; the exhaustive mirrors in `liquers-py/src/error.rs`, `liquers-web/src/error.rs` (+ `categories` getter, `.d.ts`), `liquers-axum/src/api_core/error.rs` — `cargo test -p liquers-core --lib error`, `cargo test -p liquers-axum`. Rollback: revert (additive).
- [ ] 1a. Asset-level `key_not_found` sites → `asset_not_found` (step 0 list); AC-1b tests; grep `== ErrorType::KeyNotFound` outside stores for callers that mean either miss → `is(ErrorCategory::NotFound)` — `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-axum`. Depends on 1.
- [ ] 2. `assets.rs` — `degraded_value_error`, used by `get_any_status` — `cargo test -p liquers-core --lib degraded`.
- [ ] 3. `assets.rs` — `fast_track_refusal`, `decline_fast_track` at every decline site — `cargo test -p liquers-core --lib fast_track` (no behaviour change). Depends on 2.
- [ ] 4. `assets.rs` `evaluate_recipe_outcome` — `recipe_opt`, refusal or `asset_not_found`; AC-1..AC-4 tests; `metadata_only_entry_reload.rs` — `cargo test -p liquers-core --lib --tests`. Depends on 1, 3.
- [ ] 5. `assets.rs` — `become_directory` (also in `makedir`), directory branch, the overriding refusal, three eviction sets; AC-5/AC-5a/AC-6 tests — `cargo test -p liquers-core --lib directory`. Depends on 3.
- [ ] 6. `liquers-lib` — `records_end_to_end.rs` test; fix the comment in `src/records/commands.rs` — `cargo test -p liquers-lib --lib --tests`. Depends on 5.
- [ ] 7. `liquers-py` — category classes in `_errors.py` (native `ErrorType.categories`), un-mark the `xfail`, add the `AssetNotFound` and category tests — `bash scripts/test-liquers-py.sh`. Depends on 1, 4.
- [ ] 8. Phase 2 documents; close the three issues; note on `JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE` — `python3 scripts/docs_index.py --check`.

### Validation

The commands above, then after `cargo clean` the `liquers-web` Node loop (same evaluation path);
diff review limited to the named files.
