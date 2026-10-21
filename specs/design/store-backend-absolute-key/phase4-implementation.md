# Phase 4: Implementation Plan — store-backend-absolute-key

## Overview

One PR, because the sealed facade makes a partly migrated tree uncompilable. Inside it, the order
keeps each crate compiling before the next one is touched: the type, then the traits with the
blanket impl, then core's own stores, then core's test stores, then the other crates, then the
documents. Prerequisites: the three Phase 1 decisions. If `store-conditional-write` has landed,
`set_if` and `write_atomicity` are moved like every other method in Step 2.

## Progress

- [ ] Step 1: add `AbsoluteKey<'a>`
- [ ] Step 2: split the trait — `StoreBackend`, sealed `AsyncStore`, blanket impl
- [ ] Step 3: migrate `liquers-core`'s library stores and decorators
- [ ] Step 4: migrate `liquers-core`'s test stores and integration-test stores
- [ ] Step 5: add the facade tests and doc tests
- [ ] Step 6: migrate `liquers-store` (OpenDAL)
- [ ] Step 7: migrate `liquers-records` test stores
- [ ] Step 8: migrate `liquers-web` stores and `key_guard`
- [ ] Step 9: check callers are untouched
- [ ] Step 10: documents and issue

## Implementation Steps

### Step 1: add `AbsoluteKey<'a>`
- Files / symbols: `liquers-core/src/query.rs` — new `AbsoluteKey` beside `Key::as_absolute`
- Change: the struct, `new`, `key`, `Deref`, `Display` (Phase 2 Interfaces); reimplement
  `Key::as_absolute` through `AbsoluteKey::new`. Doc tests for AC-5.
- Depends on: none
- Proof: `cargo test -p liquers-core --lib query && cargo test -p liquers-core --doc AbsoluteKey`
  (the existing `keyabs` tests still pass)
- Rollback: delete the type; `as_absolute` returns to its body

### Step 2: split the trait
- Files / symbols: `liquers-core/src/store.rs` — rename today's `AsyncStore` trait to
  `StoreBackend`, changing fallible key-taking parameters to `AbsoluteKey<'_>` and keeping every
  default body; add `mod sealed`; declare `AsyncStore` with today's signatures and no defaults; add
  `impl<T: StoreBackend + ?Sized> AsyncStore for T`; export `StoreBackend` (and re-export it where
  `AsyncStore` is re-exported, e.g. `liquers-core/src/lib.rs` preludes if any)
- Change: in default bodies that join a listed name onto a key, check the joined key with
  `AbsoluteKey::new` before calling a sibling
- Depends on: Step 1
- Proof: `cargo check -p liquers-core --lib` fails only at the `impl AsyncStore for …` sites of
  Step 3 (expected); confirm the first converted method compiles under both `async_trait`
  attributes with `cargo check -p liquers-core --target wasm32-unknown-unknown --lib` after Step 3
- Rollback: revert the file

### Step 3: migrate core's library stores
- Files / symbols: `store.rs` `SharedStore`, `NoAsyncStore`, `AsyncMemoryStore`, `AsyncFileStore`
  (and its `key_to_path`, `key_to_path_metadata`, `key_to_lock_path`), `AsyncStoreRouter`;
  `store/read_only.rs` `ReadOnlyStore`; `store/pipeline.rs` `MetadataPipelineStore` and its
  `check_key`; `store/test_support.rs` `forward_with_hook!`
- Change: `impl StoreBackend for …`; parameter types; delete the `key.as_absolute()?` lines;
  decorators forward `&key` (deref) to their inner `dyn AsyncStore`
- Depends on: Step 2
- Proof: `cargo check -p liquers-core --lib`; `cargo test -p liquers-core --lib store`
- Rollback: with Step 2 (same PR)

### Step 4: migrate core's test stores
- Files / symbols: test-module stores in `liquers-core/src/{assets,recipes,store}.rs`,
  `store_conformance/mod.rs`, `store/pipeline.rs`; `liquers-core/tests/{fixtures/mod.rs,
  common/manager_scenarios.rs, store_conformance_CONF.rs, external_change_integration.rs,
  dependency_audit_integration.rs, expiration_integration.rs}`
- Change: `impl StoreBackend`, parameter types only; assertions untouched
- Depends on: Step 3
- Proof: `cargo test -p liquers-core --lib --tests` and
  `cargo test -p liquers-core --features store-conformance --test store_conformance_CONF` (AC-4)
- Rollback: with Step 2

### Step 5: facade tests
- Files / symbols: `liquers-core/src/store.rs` `mod tests` — `Probe` and the three `facade_*`
  tests; the `compile_fail,E0277` doc test on `AsyncStore`
- Depends on: Step 3
- Proof: `cargo test -p liquers-core --lib store::tests::facade && cargo test -p liquers-core --doc AsyncStore` (AC-1, AC-2, AC-3, AC-6)
- Rollback: remove the tests

### Step 6: `liquers-store`
- Files / symbols: `liquers-store/src/opendal_store.rs` `AsyncOpenDALStore`, `key_to_path`,
  `key_to_path_dir`, `key_to_path_metadata`
- Depends on: Step 2
- Proof: `cargo test -p liquers-store`
- Rollback: with Step 2

### Step 7: `liquers-records`
- Files / symbols: `liquers-records/src/provider.rs` test stores (`CountingStore`, `Shared`,
  `RefusingStore`)
- Depends on: Step 2
- Proof: `cargo test -p liquers-records --lib`
- Rollback: with Step 2

### Step 8: `liquers-web`
- Files / symbols: `liquers-web/src/store/{local_storage,fetch,js_store}.rs`; `key_guard.rs`
  `check_key` keeps the empty-segment refusal only and takes `AbsoluteKey<'_>`
- Depends on: Step 2
- Proof: after `cargo clean`, `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` (and with `,records`)
- Rollback: with Step 2

### Step 9: callers untouched
- Proof: `cargo test -p liquers-lib --lib --tests`, `cargo check -p liquers-axum -p liquers-py -p liquers-mcp -p liquers-agent`; `git diff --stat -- liquers-lib liquers-axum liquers-py liquers-mcp liquers-agent` is empty (AC-7)

### Step 10: documents and issue
- Files: the `affects_docs` set and `CLAUDE.md` (Phase 2 Documentation Architecture), each with a
  `## History` row and `reviewed:` bump; `liquers-core/src/store.rs` module docs; close
  `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` with a resolution
- Proof: `python3 scripts/docs_index.py --check`

## Testing Plan

Steps 3-5 run `cargo test -p liquers-core --lib --tests` and the conformance test after each.
Steps 6-8 run their own crate's tests. Final: `cargo test -p liquers-lib --lib --tests`,
`bash scripts/check-build-matrix.sh` (wasm32 rows included, since the trait carries
`async_trait(?Send)` on wasm), then the `liquers-web` loops after `cargo clean`.

## Rollback Plan

Revert the PR. No data, wire format or configuration changes, so nothing persists beyond the code.

## Documentation Updates

`specs/reference/STORE_SEMANTICS.md` §7, `specs/guides/STORE_IMPLEMENTATION_GUIDE.md`,
`specs/reference/PROJECT_OVERVIEW.md` §5, `DOC_02` §Relative keys, `LANGUAGE-INTEGRATION_GUIDE.md`
STORE05, `CLAUDE.md`. No generated files change (`command_registry.yaml` untouched);
`specs/index.csv` regenerates when the issue closes.

## Phase 5 Entry Criteria

None — four-phase design.
