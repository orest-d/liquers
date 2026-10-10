---
id: SYNC-STORE-REMOVAL
kind: design
title: Remove the synchronous Store trait and the legacy cache module
form: compact
workflow: liquers-project
status: in_review
phase: high-level
area: [core/store, py, docs]
issues: [CORE-SYNC-STORE-TRAIT-OBSOLETE]
created: 2026-10-10
---
# Remove the synchronous Store trait and the legacy cache module

## Phase 1: High-Level Design

### Purpose

Delete `liquers_core::store::Store` with its four implementations, and `liquers_core::cache`, which
`design/core-dead-code-hygiene/` assigned to this issue. Nothing in the system can hold either, so
they are dead weight that reads as live API and doubles the surface every store-contract rule has to
cover. `AsyncStore` becomes the only store trait, as `STORE_SEMANTICS.md` already says it is in
practice.

### Problem Example

A store author follows the trait whose name looks canonical:

```rust
use liquers_core::store::Store;            // the sync trait, liquers-core/src/store.rs:66
struct MyStore { /* … */ }
impl Store for MyStore { /* get, set, listdir, … */ }

let env = EnvironmentBuilder::new()
    .with_async_store(Arc::new(MyStore { .. }))   // environment_builder.rs:327
    .build()?;
```

**Today:** `impl Store` compiles, the author writes the whole backend, and only the last call fails
— `MyStore: AsyncStore` is not satisfied, and there is no adapter (`AsyncStoreWrapper` was deleted)
and no `Environment` method that takes a `Store`. The same is true for the bundled `MemoryStore`,
`FileStore`, `StoreRouter` and `NoStore`: four implementations nothing can use, with their own copy of
the ~20-method contract (`store.rs` ≈66-327) that drifts from `AsyncStore`'s. Tests in `store.rs`
(`reserved07`, `keyabs09`, …) exist only to stop that drift.

**After:** `use liquers_core::store::Store` is a compile error (`no Store in store`), so the author
is sent to `AsyncStore` at the first line, and the only store contract in the code is `AsyncStore`'s.

**Correction to the issue.** It expects a `liquers-py` *public API* change. Opened at HEAD, no
Python-visible surface changes: `liquers-py/src/store.rs` (`PyStore`) and `cache.rs` are orphans not
declared in `lib.rs` and never compiled (`PY-MODULES-NOT-DECLARED-IN-LIB`); `Environment::with_store`
and `with_cache` are inside a `/* … */` block in `context.rs`; and the `Environment` pyclass is not
registered in the `#[pymodule]`. What changes is the Rust-level `pub store` and `pub cache` fields of
`liquers_py::context::Environment`, which no crate depends on.

### Scope and Acceptance Criteria

- **AC-1** The synchronous store family is gone
  - WHEN code names `liquers_core::store::{Store, NoStore, MemoryStore, FileStore, StoreRouter}`
  - THEN it fails to compile, and `liquers_core::store` defines no store trait but `AsyncStore`
- **AC-2** The legacy cache module is gone
  - WHEN code names `liquers_core::cache` (`BinCache`, `Cache`, `NoCache`, `MemoryBinCache`, …)
  - THEN it fails to compile; result caching is the asset manager's alone
- **AC-3** Asynchronous store behaviour is unchanged
  - WHEN the `liquers-core` unit tests and the store conformance suites run
  - THEN every `AsyncStore` assertion that existed before still runs and passes; tests whose only
    subject was a sync store are removed only where an async twin asserts the same rule
- **AC-4** `liquers-py` builds with an unchanged Python surface
  - WHEN `cargo test -p liquers-py --lib --no-default-features --features async_store` runs
  - THEN it passes, the `#[pymodule]` registers the same classes and functions as before, and
    `Environment` has no `store` or `cache` field
- **AC-5** Every build configuration still compiles
  - WHEN `scripts/check-build-matrix.sh` runs (including the wasm32 rows)
  - THEN every row passes — imports used only by the sync stores do not linger as warnings-as-errors
    or wasm breakage
- **AC-6** No document teaches the removed API, and the door stays open
  - WHEN `specs/reference`, `specs/guides`, `CLAUDE.md` and `.claude/skills` are searched for
    `MemoryStore`, `FileStore`, `StoreRouter`, `NoStore`, the sync `Store` trait or `liquers_core::cache`
  - THEN nothing describes them as existing; `STORE_SEMANTICS.md` stays trait-neutral and records
    that a synchronous store was removed and that reintroducing one needs a synchronous evaluation
    path, not only the trait

**Non-goals.** A synchronous or blocking store for Python: `design/python-wrapper/` plans blocking
wrappers *over* `AsyncStore`, which this removal does not affect. Declaring or repairing the other
orphaned `liquers-py` modules (`commands`, `interpreter`, `state`). Any change to `AsyncStore`,
`AsyncStoreRouter`, `ReservedNames` or the store factory.

**Systems touched and crate placement.** `liquers-core` (`store.rs`, `cache.rs`, `lib.rs`, the
`store_dir_index.rs` module comment) and `liquers-py` (`context.rs`; deletion of the orphans
`store.rs`, `cache.rs`). Removal only; no crate gains a dependency.

**Documentation intent.**
1. *Reference:* no new document. Update `STORE_SEMANTICS.md` (the trait-neutral paragraph ≈36-40,
   `FileStore::key_to_path` ≈197, `MemoryStore` ≈212), `PROJECT_OVERVIEW.md` (module table rows
   `store.rs` ≈93 and `cache.rs` ≈103) and `ASSET_SET_OPERATION.md` (`StoreRouter` ≈198).
2. *Guide:* no new document. Update `LANGUAGE-INTEGRATION_GUIDE.md` ≈244 (cache "scheduled for
   removal" → removed).
3. *Other:* `CLAUDE.md` (the Async Patterns line and the "Add sync Store implementations" constraint),
   `DOCS_STRUCTURE_GUIDE.md` §3 `core/store` row (drops `cache.rs`), the `liquers-unittest`
   references `testable-components.md` and `test-patterns.md`, and `rust-best-practices/SKILL.md`.
4. *Records:* close `CORE-SYNC-STORE-TRAIT-OBSOLETE`; narrow `PY-MODULES-NOT-DECLARED-IN-LIB`'s
   orphan list (`store` and `cache` are deleted, not declared).

### Design Dependencies

| Relation | Item | Note |
|---|---|---|
| overlaps | `PY-MODULES-NOT-DECLARED-IN-LIB` | T3 holds for two files only (`liquers-py/src/store.rs`, `cache.rs`), which this design deletes; its real subject — the unreachable Python command path in `commands.rs` — is independent, so not merged. Its orphan list is narrowed here. |
| covers | `design/core-dead-code-hygiene/` (complete) | Assigned the deletion of `liquers_core::cache` to this issue (`covered-by`). Its non-goal gave the orphan `liquers-py/src/cache.rs` to `PY-MODULES-…`; this design takes it because it would no longer compile against anything (open question 1). |
| related | `design/store-conformance-suite/` (complete) | Scoped itself to `AsyncStore`; unaffected. |
| related | `design/python-wrapper/` (complete) | Plans a Python store as a blocking wrapper over `AsyncStore`; consistent with this removal. |

Overlap search: `specs/index.csv` for `sync store`, `Store trait`, `cache`, `liquers-py`; every open
design's phase 2/4 for `store.rs` sync symbols and `cache.rs`. No open design plans to edit them.

### Open Questions

1. **Proposed resolution — orphan `liquers-py/src/store.rs` and `cache.rs`:** delete them here.
   They wrap `liquers_core::store::FileStore` and `liquers_core::cache::BinCache`, which this design
   removes; kept, they become files that cannot compile even once declared, and a future Python store
   is to wrap `AsyncStore` (`design/python-wrapper/`) rather than revive `PyStore`. Alternative:
   leave them to `PY-MODULES-NOT-DECLARED-IN-LIB`, which would then have to delete them anyway.
2. **Proposed resolution — sync-only tests:** delete `test_simple_store`,
   `filestore02_sync_missing_directory_lists_empty`, `sync_router_listdir_at_store_prefix`,
   `keyabs09_file_store_refuses_traversal`, `reserved04_…` and `reserved07_…`, and reduce
   `memory_store_support`, `keyabs07` and `keyabs10` to their async halves. Each deleted rule has an
   async twin (`test_async_memory_store_basic`, `filestore01`, `async_router_listdir_at_store_prefix`,
   `keyabs08`, `reserved02`/`03`/`06`); `reserved04`/`07`'s per-store point (`__lock__` addressable in
   a lock-free store) disappears with the only lock-free file store. Phase 3 lists the mapping.
3. **Implementation detail — module docs:** `store.rs`'s module comment (≈35-43) cites
   `Store::is_supported`, `StoreRouter` and `FileStore`; rewrite it against `AsyncStore`,
   `AsyncStoreRouter` and `AsyncFileStore` with the same rule.
