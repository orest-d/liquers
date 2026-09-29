# Phase 3: Examples & Use-cases - Store Conformance Backlog

## High-Level Introduction

Phase 1 promised that every store, the contract and the test IDs would agree, and that each parking
mechanism (a `Blocked` rule, allowed failures, a test that never runs in a browser, IDs with two
meanings) would be deleted. The examples show that promise from a store *user's* side; the tests
show it from the suite's side.

- **Example 1** covers the representative workflow. A caller browses a directory: `children` lists
  one level, and a metadata-only key is visible. It runs against the file and memory stores.
- **Example 2** builds on Example 1 for the one store whose protocol changes, the JavaScript
  `JsStore`. It shows how a page says "not found" and how its directories get metadata.
- **Example 3** lists the pitfalls that tripped the original tests, each with the test that now
  protects against it.

The test plan then covers what the examples cannot show: that the directory read no longer walks the
subtree, that new and changed rules fail against deliberately broken stores, and how every
conformance report changes.

## Example Type

**User choice:** Runnable prototypes. Each example is a complete test file, or a set of added test
functions, at its intended path. Phase 4 adds them verbatim as its first, failing-first step.

## Overview Table

| # | Type | Name | Demonstrates / checks | Drafted by |
|---|---|---|---|---|
| 1 | Example | Browsing a directory (`store_directory_children_STORE.rs`) | §2 one-level `children` and §8 metadata-only listing, file and memory store | Author (Haiku Agent 1 did not start) |
| 2 | Example | A page store that says "not found" (`STORE12a`–`c` in `store_js_STORE.rs`) | Area C sentinel, area B directory fallback, the documented `getMetadata → null` break | Haiku Agent 2 |
| 3 | Example | Pitfalls | Five failure modes, each with symptom, cause, correction and protecting test | Haiku Agent 5 |
| 4 | Unit tests | Store and rule unit tests | Area A no-recursion; area D listing, dedup, reserved names; `sidecar04` and `dir07` refuted by broken stores; area E family scan | Haiku Agent 4 |
| 5 | Integration | Conformance reports C1–C10, D1, STORE10 | Allowed failures removed, `dir07` live, `sidecar04` per store, C9 in a browser | Haiku Agent 5 |

The author checked every draft against the code. Corrections made during synthesis:
- `AssetInfo.key` is `Option<Key>`.
- `liquers-core` has no allowed failures to remove; only `C10` has them.
- A router does not merge `children`: it forwards `get_metadata` to one member store.
- `getMetadata` throwing does **not** fall through to the directory branch.
- The D1 scan reads the rule registry rather than a literal family list.

## Example 1: Browsing a directory — one-level `children` and metadata-only keys

### Connection to the High-Level Design

This is the caller's view of areas A and D. A directory browser (the UI, or `liquers-py` reading
`.children`) asks for a directory's metadata and expects its direct contents. That includes an asset
whose metadata was recorded before its data was written. After this project every store gives the
same answer, and no store walks the subtree to produce it.

### Scenario

`reports/` holds a file `q1.csv`, a subfolder `2025/` containing `summary.txt`, and a key `q2.csv`
whose title was recorded but whose data does not exist yet. The caller wants one listing with three
described entries, `q1.csv`, `2025` (a directory) and `q2.csv`, and nothing below `2025/` should be
read to produce it.

### Sequence of Steps

1. The caller calls `AsyncStore::get_metadata(reports)`.
2. The store finds a directory (`is_dir`) and builds `default_metadata(reports, true)`.
3. It fills `children` from `listdir_asset_info(reports)`, which is `listdir` plus one
   `get_asset_info` per child.
4. For the child directory `2025`, the new default `get_asset_info` answers from
   `default_metadata(…, true)` without calling `get_metadata(reports/2025)`, so the walk stops at
   one level (area A).
5. `q2.csv` appears because `AsyncFileStore::listdir` now reports the implied key of the sidecar
   `q2.csv.__metadata__` (area D).

### Core Example Code

`liquers-core/tests/store_directory_children_STORE.rs`, run with
`cargo test -p liquers-core --test store_directory_children_STORE`:

```rust
//! Directory metadata carries its direct children, one level deep, and a key holding only
//! metadata is listed — on every store (STORE_SEMANTICS §2, §8).

use liquers_core::metadata::{Metadata, MetadataRecord};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::{AsyncFileStore, AsyncMemoryStore, AsyncStore};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn temp_root(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("lq-dirchildren-{tag}-{nanos}"))
}

async fn populate(store: &dyn AsyncStore) -> TestResult {
    let empty = Metadata::MetadataRecord(MetadataRecord::new());
    store.set(&parse_key("reports/q1.csv")?, b"a,b\n1,2\n", &empty).await?;
    store.set(&parse_key("reports/2025/summary.txt")?, b"fine", &empty).await?;
    let mut pending = MetadataRecord::new();
    pending.with_title("Q2 figures".to_owned());
    store
        .set_metadata(&parse_key("reports/q2.csv")?, &Metadata::MetadataRecord(pending))
        .await?;
    Ok(())
}

async fn check_listing(store: &dyn AsyncStore) -> TestResult {
    let reports = parse_key("reports")?;

    let mut names = store.listdir(&reports).await?;
    names.sort();
    assert_eq!(names, vec!["2025", "q1.csv", "q2.csv"]); // FAILS at HEAD on AsyncFileStore
    assert!(store.contains(&parse_key("reports/q2.csv")?).await?);

    let Metadata::MetadataRecord(record) = store.get_metadata(&reports).await? else {
        return Err("directory metadata should be a record".into());
    };
    let mut children: Vec<(String, bool)> = record
        .children
        .iter()
        .filter_map(|info| {
            let name = info.key.as_ref()?.filename()?.encode().to_string();
            Some((name, info.is_dir))
        })
        .collect();
    children.sort();
    assert_eq!(
        children,
        vec![("2025".into(), true), ("q1.csv".into(), false), ("q2.csv".into(), false)]
    );
    Ok(())
}

#[tokio::test]
async fn file_store_lists_children_and_metadata_only_keys() -> TestResult {
    let root = temp_root("file");
    tokio::fs::create_dir_all(&root).await?;
    let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());
    populate(&store).await?;
    let outcome = check_listing(&store).await;
    let _ = tokio::fs::remove_dir_all(&root).await;
    outcome
}

#[tokio::test]
async fn memory_store_lists_children_and_metadata_only_keys() -> TestResult {
    let store = AsyncMemoryStore::new(&Key::new());
    populate(&store).await?;
    check_listing(&store).await
}
```

### Guide and Executable Example

`STORE_IMPLEMENTATION_GUIDE.md` links this file as the caller-side statement of §2 and §8. Both
helpers take `&dyn AsyncStore`, so a store author can reuse them against their own store. The
invisible half of area A, that nothing recurses, is proven by the unit test
`default_directory_metadata_reads_one_level` in the test plan.

**Expected output:** both tests `ok`. At HEAD the file-store test fails on `listdir`, which returns
`["2025", "q1.csv"]`.

## Example 2: A page store that says "not found" and has directories

Builds on Example 1: the directory metadata shape is the same, and this example shows how
`JsStore` reaches it through a JavaScript delegate.

### Scenario

A page author implements a store in JavaScript: a `Map` of files, directories derived from key
prefixes, and no data for a directory. Their delegate must say "not found" without it being
mistaken for a failure, and a Liquers directory browser must be able to describe the page's
directories.

### Sequence of Steps (`JsStore::get_metadata`)

1. **Data path.** Call `getMetadata(key)` if the delegate has it, otherwise `get(key)`.
   `null`/`undefined` → `KeyNotFound`; a thrown value → `KeyReadError`; an object → metadata, done.
2. **Only on `KeyNotFound`:** if the delegate has `isDir` and `isDir(key)` is truthy, return
   `default_metadata(key, true)` with `children = listdir_asset_info(key)`. Otherwise return the
   `KeyNotFound`.
3. **Any other error** (a thrown `get`, `getMetadata` or `isDir`) is returned unchanged. A failure
   never becomes a directory.

### Core Example Code

Added to `liquers-web/tests/store_js_STORE.rs`, which already has `key()`, `object()`, `ErrorType`,
`Metadata` and `AsyncStore` in scope. The IDs `STORE12a`–`c` follow the file's `STORE01b` variant
style; `STORE12` is unused in `liquers-web`.

```rust
/// STORE12a — `null` means absent, and a directory gets directory-shaped metadata.
#[wasm_bindgen_test]
async fn store12a_absence_sentinel_and_directory_metadata() {
    let source = r#"(function () {
        const data = new Map([
            ["d/file.txt", { data: new Uint8Array([102]), metadata: { title: "a file" } }],
        ]);
        const dirs = new Set(["d", "d/subdir"]);
        return {
            get(key) { return data.has(key) ? data.get(key) : null; },
            getMetadata(key) { return data.has(key) ? data.get(key).metadata : null; },
            isDir(key) { return dirs.has(key); },
            listdir(key) { return key === "d" ? ["file.txt", "subdir"] : []; },
        };
    })()"#;
    let store = JsStore::new(&key("d"), "mixed", object(source)).expect("adapts");

    let file = store.get_metadata(&key("d/file.txt")).await.expect("file metadata");
    let Metadata::MetadataRecord(file) = file else { panic!("record expected") };
    assert_eq!(file.title, "a file");

    let dir = store.get_metadata(&key("d")).await.expect("directory metadata");
    let Metadata::MetadataRecord(dir) = dir else { panic!("record expected") };
    assert!(dir.is_dir);
    let mut names: Vec<String> = dir
        .children
        .iter()
        .filter_map(|i| i.key.as_ref().map(|k| k.encode()))
        .collect();
    names.sort();
    assert_eq!(names, vec!["d/file.txt", "d/subdir"]);

    match store.get_metadata(&key("d/absent")).await {
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotFound, "{}", e.message),
        Ok(m) => panic!("absent key gave {m:?}"),
    }
    match store.get(&key("d/absent")).await {
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotFound, "{}", e.message),
        Ok(_) => panic!("absent key gave data"),
    }
}

/// STORE12b — a throwing delegate is a failure, and is never reinterpreted as a directory.
#[wasm_bindgen_test]
async fn store12b_thrown_error_is_not_a_directory() {
    let store = JsStore::new(
        &key("d"),
        "thrower",
        object(r#"({ get(key) { throw new Error("read boom"); }, isDir(key) { return true; } })"#),
    )
    .expect("adapts");
    match store.get_metadata(&key("d/x")).await {
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyReadError, "{}", e.message),
        Ok(m) => panic!("a thrown error became {m:?}"),
    }
}

/// STORE12c — documented break: `getMetadata` returning `null` now means absent, even when
/// `get` has data. It used to yield an empty record.
#[wasm_bindgen_test]
async fn store12c_get_metadata_null_is_not_found() {
    let store = JsStore::new(
        &key("d"),
        "nullmeta",
        object(r#"({
            get(key) { return key === "d/x.bin" ? { data: new Uint8Array([255]) } : null; },
            getMetadata(key) { return null; },
        })"#),
    )
    .expect("adapts");
    match store.get_metadata(&key("d/x.bin")).await {
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotFound, "{}", e.message),
        Ok(m) => panic!("getMetadata → null gave {m:?}"),
    }
}
```

TypeScript declaration (`liquers-web/src/typescript.rs`, the store protocol interface):

```typescript
/** Returns `null`/`undefined` when the key is absent; throws only on failure. */
get(key: string): { data: Uint8Array; metadata?: object } | null | undefined
  | Promise<{ data: Uint8Array; metadata?: object } | null | undefined>;
/** Optional; derived from `get` when absent. `null`/`undefined` means the key is absent. */
getMetadata?(key: string): object | null | undefined | Promise<object | null | undefined>;
```

**Run:** `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test
store_js_STORE` (Node; no browser). **Expected at HEAD:** `STORE12a` fails at the directory
assertion (`KeyNotFound`), and `STORE12c` fails because it gets an empty record. `STORE12b` already
passes and pins the rule that a failure is never a directory.

## Example 3 (Optional): Pitfalls

| Symptom | Cause | Correction | Protected by |
|---|---|---|---|
| A page store reports `KeyReadError` for a key that simply is not there | The delegate throws for absence | Return `null`/`undefined` from `get`/`getMetadata` | `STORE12a`; `C10` `absence01` with no allowed failure |
| A page's existing key reads as `KeyNotFound` after upgrading | `getMetadata` returned `null` meaning "no metadata" | Return `{}` or omit `getMetadata` | `STORE12c`; README and TypeScript note |
| `D1` fails naming `dir03_…` | A unit test reused a conformance rule ID | Name the test by its subject, e.g. `opendal_…` | `D1` family scan |
| C9 panics at `window()` under Node | `run_in_browser` is missing from the test file | The test lives in `store_conformance_browser_CONF.rs`, which configures it | That file's `wasm_bindgen_test_configure!` |
| `csv.media_type` is `null` for `input.csv` | Raw metadata holds only *declared* overrides; the extension sets `data_format` | Read `data_format`; an absent `media_type` means derived | corrected `STORE10` |

## Corner Cases

### 1. Memory

- **Wide directories.** `children` holds one `AssetInfo` per direct child, so a 10 000-entry folder
  yields 10 000 records, the same as `listdir_asset_info` today. Only depth is bounded, not width.
  This is recorded in §2 as a known cost. Paging is out of scope.
- **Directory `AssetInfo`** is built from `default_metadata`, with no allocation for grandchildren.

### 2. Concurrency

- **File store:** `set_metadata` racing `listdir`. The listing sees the sidecar before or after it
  is written, and both results are valid. De-duplication is per call, with no shared state.
- **`LocalStorageStore`** (`RefCell` state, single-threaded wasm): the new `keys()` and
  `set_metadata` indexing borrow and release before any `.await`, as the existing methods do.
- **OpenDAL:** a child removed between `listdir` and its `get_asset_info` gives that child's
  `KeyNotFound`, which the directory read propagates, as every store does today. Unchanged, and
  noted in §2.

### 3. Errors

| Case | Result | Test |
|---|---|---|
| Relative key to `LocalStorageStore::contains` / `is_dir` / `listdir` | `KeyNotAbsolute` | C9 `keyshape01` |
| `removedir` on an absent directory (`LocalStorageStore`) | `Ok(())` | C9 `absence03` |
| `set_metadata` refused for a key with no data | `KeyNotFound`; `sidecar04` passes | `sidecar04_accepts_a_refusal` |
| `JsStore` `isDir` absent, data `null` | `KeyNotFound` | `STORE12a` (absent key) |

### 4. Serialization

`MetadataRecord.children` keeps its type and JSON shape. `liquers-py` (`metadata.rs:406`) reads the
same list, which now also arrives from the OpenDAL store. `AssetInfo` has no `children` field, so
nothing nests.

### 5. Integration (Cross-Crate Interactions)

- `AsyncStoreRouter` forwards `get_metadata` to the one member owning the key, and there is no
  merging. A router-level directory above several members uses the trait default and so gets the
  one-level behaviour of area A. The `C7` router suite covers this through `dir07`.
- `liquers-axum`'s store handlers and the UI read `get_metadata`/`listdir` and see the corrected
  answers. No code changes there.

## Documentation and Learning Log

### Guide Candidate Workflows and Examples

- "Signal absence from a page store": Example 2's delegate, linked from the guide and from
  `liquers-web/README.md`.
- "Name your store's tests": the pitfall row about `dir03_…`, with the D1 message as the example.
- The guide links Example 1's file as the caller-side check and does not copy it.

### Usage, Meaning, and Connections

For the reference: `children` is one level deep, and a directory's `AssetInfo` is directory-shaped.
A metadata-only key is listed, unless the store refuses it at `set_metadata`. Raw metadata carries
declared overrides only.

### Repeatable Development Guidance

Every changed rule gets a broken-store unit test beside it (`PrefixDeletingStore` pattern in
`store_conformance/mod.rs`), so a rule can never be green against a store unable to fail it.

### Corrections and Unexpected Learning

- Two of the seven issues were narrower than written: the `JsStore` sentinel already existed for
  `get`, and the `http` store was correct.
- The ID collision included `dir01`–`dir03` and `sibling01`.
- `LocalStorageStore` had the same metadata-only listing gap as the file store.

## Test Plan

### Unit Tests

| Test | File | Checks | At HEAD |
|---|---|---|---|
| `default_directory_metadata_reads_one_level` | `liquers-core/src/store.rs` tests | A counting wrapper over `AsyncMemoryStore` (only `get`, `set`, `set_metadata`, `is_dir`, `listdir` forwarded, so the trait defaults run) over `a/b/c/leaf`: `get_metadata(a)` calls `get_metadata`/`get` for no key below `a/b` | fails (recurses) |
| `default_directory_asset_info_is_directory_shaped` | same | `get_asset_info(dir)` gives `is_dir == true` and the key, with zero `get` calls | fails (reads) |
| `file_store_lists_a_metadata_only_key` | same | `set_metadata` only → `listdir` contains the name exactly once | fails |
| `file_store_lists_data_and_sidecar_once` | same | data + sidecar → one name | passes (pins dedup) |
| `file_store_skips_sidecars_implying_reserved_names` | same | `x.__lock__.__metadata__` and a bare `__metadata__/` folder are not listed | passes (pins filter) |
| `opendal_directory_metadata_lists_children` | `liquers-store/src/opendal_store.rs` tests (memory service) | directory `children` equals the direct children | fails |
| `sidecar04_passes_a_store_that_lists_metadata_only_keys` | `liquers-core/src/store_conformance/mod.rs` tests | `AsyncMemoryStore` → `Passed` | new |
| `sidecar04_accepts_a_refusal` | same | wrapper whose `set_metadata` on an absent key returns `KeyNotFound` → `Passed` | new |
| `sidecar04_fails_a_store_that_hides_metadata_only_keys` | same | wrapper filtering such keys out of `listdir` → `Failed` | new |
| `dir07_passes_one_level_children` / `dir07_fails_empty_children` / `dir07_fails_children_that_differ_from_listdir` | same | the inverted rule, with a wrapper that clears `children` and one that adds a grandchild | new |
| `owned_rule_families_come_from_the_registry` | `liquers-core/tests/conformance_docs_CONF.rs` | the family set is derived from the rule IDs (`dir07` → `dir`); it includes `nomakedir` and `sidecar` | new |
| `no_unit_test_uses_an_owned_rule_id` | same | a text scan of the `src/` and `tests/` directories of `liquers-core`, `liquers-store` and `liquers-web` for `fn <family><digits>_`; the message names file and line | fails at HEAD (11 `liquers-store` names) |

The `local_storage.rs` changes run only in a browser and are covered by C9 below, not by a
Node-side unit test.

Sketch of the rule-refutation pattern, which reuses the existing `PrefixDeletingStore` shape:

```rust
/// Hides keys that hold only metadata from `listdir`: `sidecar04` must fail against it.
struct MetadataOnlyHidingStore { inner: crate::store::AsyncMemoryStore }
// impl AsyncStore: forward get/set/set_metadata/contains/is_dir/remove to `inner`;
// listdir filters out names whose key has an empty data body.
```

### Integration Tests

| Suite | Command | Change in the report |
|---|---|---|
| C1–C7 (`liquers-core/tests/store_conformance_CONF.rs`) | `cargo test -p liquers-core --features store-conformance --test store_conformance_CONF` | `dir07` is `Passed` instead of `Blocked` wherever `Directories` is declared; new `sidecar04` is `Passed` (memory, file) or skipped by capability (C4 defaults). There were no allowed failures to remove. |
| OpenDAL suites (`liquers-store`) | `cargo test -p liquers-store --lib --tests` | `dir07` `Passed` (now populates `children`); `sidecar04` `Passed` |
| C8 `FetchStore` | `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test store_conformance_CONF` | unchanged; `sidecar04` skipped (read-only) |
| C10 `JsStore` | same | all four allowed failures deleted; stub `get` returns `null` for a missing key |
| C9 `LocalStorageStore` → new `liquers-web/tests/store_conformance_browser_CONF.rs` | `CHROMEDRIVER=$(which chromedriver) cargo test -p liquers-web --target wasm32-unknown-unknown --features browser-tests --test store_conformance_browser_CONF` | `absence03`, `keyshape01`, `keys02`, `sidecar04` all `Passed`, with no allowed failures |
| D1 | `cargo test -p liquers-core --features store-conformance --test conformance_docs_CONF` | the new family scan; the §2 and §8 texts cite `dir07` and `sidecar04` |
| e2e `STORE10` | `cd liquers-web/tests/e2e && npx playwright test store.spec.ts` | corrected assertions pass |

`store_conformance_browser_CONF.rs` is the moved `c9_local_storage_store` body unchanged, under:

```rust
#![cfg(all(target_arch = "wasm32", feature = "browser-tests"))]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
```

It has its own three-line `report` helper. The `#[cfg(feature = "browser-tests")]` C9 block and its
doc comment are removed from `store_conformance_CONF.rs`, and that file's header comment is updated.

Corrected `STORE10` assertions:

```typescript
return { format: csv.data_format, csvType: csv.media_type ?? null,
         blobType: blob.media_type, size: csv.file_size };
// …
expect(result.format).toBe('csv');   // the extension decided the format
expect(result.csvType).toBeNull();   // no override declared; the media type derives from it
expect(typeof result.blobType).toBe('string');
expect(result.blobType.length).toBeGreaterThan(0);
```

### Manual Validation

1. Run the default loop: `cargo test -p liquers-lib --lib --tests` (it builds core and store).
2. Run the core conformance suite and `liquers-store`, and compare the printed reports with the
   table above.
3. After `cargo clean`: run the Node wasm loop, then the browser loop, then the e2e loop, per
   `CLAUDE.md` §liquers-web. If chromedriver and Chromium majors do not match, use the README's
   `NO_HEADLESS=1` route and record which route was used.
4. Run `bash scripts/check-build-matrix.sh`, because wasm32 rows compile `JsStore` and
   `LocalStorageStore`.

## Auto-Invoke: liquers-unittest Skill Output

The liquers-unittest conventions were applied to the plan above:
- Tests are in the same file, or `tests/` for integration.
- `#[tokio::test]` for async and `#[wasm_bindgen_test]` for wasm.
- `Result<(), Box<dyn std::error::Error>>` returns in new integration files.
- `parse_key` for keys and `AsyncMemoryStore::new(&Key::new())` for memory stores.
- Error paths are asserted by `error_type`, never by message text.
