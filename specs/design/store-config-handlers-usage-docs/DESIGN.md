---
id: STORE-CONFIG-HANDLERS-USAGE-DOCS
kind: design
title: The API docs say who runs store_config handlers, when, and over which stores
form: compact
status: complete
readiness: ready
autofix: eligible
area: [docs, core/store, store/config]
issues: [STORE-CONFIG-HANDLERS-USAGE-UNDOCUMENTED]
affects_docs: [guides/STORE_CONFIG_GUIDE.md, guides/STORE_IMPLEMENTATION_GUIDE.md]
created: 2026-11-01
---
# The API docs say who runs store_config handlers, when, and over which stores

Compact design (`specs/guides/autonomous_issue_fixing.md`), 2026-11-01, for GitHub #230.

> **Complete on 2026-11-01:** implemented as written, in the same change as this design.

## Phase 1: High-Level Design

### Purpose

Answer, in the rustdoc a reader of the API starts from and in the two store guides, the questions
of #230: does each store implement handlers, is there a wrapper that runs them automatically,
which stores support them, and how are they called — with a runnable example.

### The answers (checked at `d2f72fe`)

- **No store implements handlers.** A handler is a `store_config` entry compiled by
  `StoreConfigFunctions::compile` (`store/realm.rs`) into a closure over a `MetadataRecord`.
- **The wrapper is `MetadataPipelineStore`** (`store/pipeline.rs`). It holds the four compiled
  chains (`StageChains`) and runs `on_default` / `on_read` on `get` / `get_metadata`, and
  `on_validate` / `on_write` on `set` / `set_metadata`, around the backend's data-only methods.
- **It is applied automatically** by `StoreRouterBuilder::build_router` to every mount with a
  non-empty `metadata` section (`store_factory::pipeline` compiles the section), and by
  `build_store*` to a router-level section. `EnvironmentBuilder::build` calls `build_store` with the
  environment's registry, so the same holds for `EnvironmentBuilder::with_store_config`.
- **Not automatic:** a mount without a section (no pipeline, so `METADATA_DEFAULTS` do not apply
  either; the store's own `default_metadata` / `finalize_metadata` do), and a store passed to
  `EnvironmentBuilder::with_async_store` or built in code. The latter is filed as
  `STORE-METADATA-SECTION-CANNOT-WRAP-A-BUILT-STORE`, design `metadata-section-on-built-store`.
- **Which stores can be wrapped:** those whose `data_capabilities().data_access` is true —
  `AsyncMemoryStore`, `AsyncFileStore`, `AsyncOpenDALStore`, `LocalStorageStore`, `FetchStore`,
  `JsStore` with the data methods, and a router or `SharedStore` over such stores. `NoAsyncStore`
  and any store keeping the refusing defaults fail the build.

### Scope and Acceptance Criteria

- **AC-1** The API answers the questions where a reader starts
  WHEN a reader opens the rustdoc of `store::realm`, `store::handlers`, `store::pipeline`,
  `MetadataPipelineStore`, `StoreConfig::metadata`, `MetadataConfig`, `METADATA_DEFAULTS` or
  `AsyncStore::data_capabilities`
  THEN it states that stores do not implement handlers, that `MetadataPipelineStore` runs them, that
  `StoreRouterBuilder` applies it automatically, and the cases with no pipeline — or links the item
  that does, with links that resolve
- **AC-2** A runnable example
  WHEN `cargo test -p liquers-core --doc pipeline` runs
  THEN the example of the `store::pipeline` module builds a memory mount with a `metadata` section
  through `StoreRouterBuilder::build_store`, writes, and asserts the title and media type the
  `on_write` handlers set
- **AC-3** The guides agree
  WHEN a reader opens `STORE_CONFIG_GUIDE.md` §1 or `STORE_IMPLEMENTATION_GUIDE.md` §4a
  THEN §1 has "Who runs the handlers" naming the cases with no pipeline, and §4a says a store
  implements data-only access, never handlers; each guide has a History row and a bumped `reviewed:`

Non-goal: a public function wrapping a built store (rule 4 below; separate design).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — documentation only (doc comments, a doc test, two guides) in
  `liquers-core` and `specs/`; no `pub` item added or changed, no command, no config change.
- **Leading issue:** None.
- **Explanation:** The behaviour exists and is tested (`tests/store_adapters.rs`); only its
  description is missing where readers look.
- **Open questions:** None.

### Design Dependencies

None. `metadata-section-on-built-store` would add one sentence to the docs written here.

## Phase 2: Solution and Architecture

Doc comments only, in `liquers-core/src/store/{realm,handlers,pipeline}.rs`,
`liquers-core/src/store.rs` (data-only block comment and `data_capabilities`) and
`liquers-core/src/store_config.rs`. The doc example uses `core_store_factory()` (type `memory`),
`build_store`, and the built-in `set` handler in `on_write`, so it needs no feature and no
liquers-lib entry. Guides: one subsection in each.

Risk: a doc test that compiles on native only. `liquers-core` doc tests run natively; the wasm
rows of the build matrix do not run doc tests. Recovery: mark it `no_run` if it ever needs I/O.

## Phase 3: Examples and Tests

| Case | Proof |
|---|---|
| AC-1 | Review of the doc comments; `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc -p liquers-core --no-deps` reports no link added by this change (the 26 it reports already are filed as `RUSTDOC-STORE-SUBMODULE-LINKS-UNRESOLVED`) |
| AC-2 | The doc test of the `store::pipeline` module, `cargo test -p liquers-core --doc pipeline` |
| AC-3 | Review of the two guides; `python3 scripts/docs_index.py --check` |

## Phase 4: Implementation Plan

### Steps

- [x] Step 1: doc comments of AC-1 and the example of AC-2; doc test passes
- [x] Step 2: the two guides (AC-3), with History rows
- [x] Step 3: regenerate the index; issue status updated when merged
