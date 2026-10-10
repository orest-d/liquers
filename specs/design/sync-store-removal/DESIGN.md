---
id: SYNC-STORE-REMOVAL
kind: design
title: Remove the synchronous Store trait and the legacy cache module
form: compact
workflow: liquers-project
status: in_review
phase: architecture
area: [core/store, py, docs]
issues: [CORE-SYNC-STORE-TRAIT-OBSOLETE]
created: 2026-10-10
---
# Remove the synchronous Store trait and the legacy cache module

## Phase 1: High-Level Design

### Purpose

Delete `liquers_core::store::Store` with its four implementations, and `liquers_core::cache`
(assigned here by `design/core-dead-code-hygiene/`). Nothing can hold either, so they read as live
API while being dead, and double the surface of every store-contract rule. `AsyncStore` becomes the
only store trait.

### Problem Example

```rust
use liquers_core::store::Store;                 // store.rs:66
impl Store for MyStore { /* get, set, listdir, … */ }
EnvironmentBuilder::new().with_async_store(Arc::new(MyStore { .. }))  // environment_builder.rs:327
```

**Today:** `impl Store` compiles; only the last call fails (`MyStore: AsyncStore` unsatisfied). No
adapter exists (`AsyncStoreWrapper` was deleted) and no `Environment` takes a `Store`. The same holds
for `MemoryStore`, `FileStore`, `StoreRouter` and `NoStore`, which carry their own copy of the
~20-method contract and drift-guard tests (`reserved07`, `keyabs09`, …).
**After:** the `use` line fails (`no Store in store`), sending the author to `AsyncStore` at once.

**Correction to the issue:** no Python-visible API changes. `liquers-py/src/store.rs` (`PyStore`) and
`cache.rs` are undeclared orphans (`PY-MODULES-NOT-DECLARED-IN-LIB`); `with_store` / `with_cache`
are inside a `/* … */` block; the `Environment` pyclass is not registered in the `#[pymodule]`. Only
the Rust fields `pub store` / `pub cache` of `liquers_py::context::Environment` change, and no crate
uses them.

### Scope and Acceptance Criteria

- **AC-1** The synchronous store family is gone
  - WHEN code names `liquers_core::store::{Store, NoStore, MemoryStore, FileStore, StoreRouter}`
  - THEN it fails to compile, and `liquers_core::store` defines no store trait but `AsyncStore`
- **AC-2** The legacy cache module is gone
  - WHEN code names `liquers_core::cache` (`BinCache`, `Cache`, `NoCache`, `MemoryBinCache`, …)
  - THEN it fails to compile; result caching is the asset manager's alone
- **AC-3** Asynchronous store behaviour is unchanged
  - WHEN the `liquers-core` unit tests and the store conformance suites run
  - THEN every pre-existing `AsyncStore` assertion still runs and passes; a sync-only test is removed
    only where an async twin asserts the same rule
- **AC-4** `liquers-py` builds with an unchanged Python surface
  - WHEN `cargo test -p liquers-py --lib --no-default-features --features async_store` runs
  - THEN it passes, the `#[pymodule]` registers the same items as before, and `Environment` has no
    `store` or `cache` field
- **AC-5** Every build configuration still compiles
  - WHEN `scripts/check-build-matrix.sh` runs (including the wasm32 rows)
  - THEN every row passes
- **AC-6** No document teaches the removed API, and the door stays open
  - WHEN `specs/reference`, `specs/guides`, `CLAUDE.md` and `.claude/skills` are searched for the
    removed names
  - THEN none describes them as existing; `STORE_SEMANTICS.md` stays trait-neutral and records that
    a future synchronous store needs a synchronous evaluation path, not only the trait

**Non-goals:** a blocking store for Python (`design/python-wrapper/` wraps `AsyncStore`); the other
`liquers-py` orphans (`commands`, `interpreter`, `state`); any change to `AsyncStore`,
`AsyncStoreRouter`, `ReservedNames` or the store factory.

**Placement:** `liquers-core` (`store.rs`, `cache.rs`, `lib.rs`, `store_dir_index.rs` comment) and
`liquers-py` (`context.rs`; delete `store.rs`, `cache.rs`). Removal only.

**Documentation intent:** no new reference or guide. Update the references `STORE_SEMANTICS.md`,
`PROJECT_OVERVIEW.md`, `ASSET_SET_OPERATION.md`; the guide `LANGUAGE-INTEGRATION_GUIDE.md`; and
`CLAUDE.md`, `DOCS_STRUCTURE_GUIDE.md` §3 and three skill files (Phase 2 table). Close the issue;
narrow `PY-MODULES-NOT-DECLARED-IN-LIB`.

### Design Dependencies

| Relation | Item | Note |
|---|---|---|
| overlaps | `PY-MODULES-NOT-DECLARED-IN-LIB` | T3 on two files only (py `store.rs`, `cache.rs`), deleted here; its subject (the Python command path) is independent, so not merged |
| covers | `design/core-dead-code-hygiene/` (complete) | assigned `liquers_core::cache`'s deletion here; this design also takes the py `cache.rs` orphan it gave to `PY-MODULES-…` |
| related | `design/store-conformance-suite/`, `design/python-wrapper/` (complete) | `AsyncStore`-only; consistent |

Overlap search: `index.csv` for sync store / Store trait / cache / liquers-py; open designs' phase
2/4 for the removed symbols. No open design edits them.

### Open Questions

All accepted by the user at the Phase 1 gate (2026-10-10):

1. **Resolved — py orphans:** delete `liquers-py/src/store.rs` and `cache.rs` here.
2. **Resolved — sync-only tests:** delete where an async twin asserts the rule; reduce mixed tests
   to their async half (mapping in Phase 3).
3. **Resolved (detail) — module docs:** rewrite `store.rs` ≈35-43 against `AsyncStore`,
   `AsyncStoreRouter`, `AsyncFileStore`.

## Phase 2: Architecture

### Solution

**Delete, do not deprecate**, in one change, adjusting the one compiled consumer (`liquers-py`
`Environment`) in the same commit. No `src`, `tests` or `examples` directory of any crate names the
removed items outside `store.rs`, `cache.rs` and `liquers-py`.

Rejected: *`#[deprecated]` first* — no caller to warn, and the drift tests live on; *a `sync-store`
feature* — two contracts behind a flag nobody enables, plus a matrix row; *an adapter* — makes the
trait reachable again, the opposite of the issue.

**Known-issue preflight** (open `core/store` / `py` issues and any naming a removed symbol): none
blocks or must go first. `PY-MODULES-NOT-DECLARED-IN-LIB`, `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED`
(counts `Store` call sites, cites `StoreRouter::find_store`) and
`STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` (lists `FileStore`) got dated scope notes.
`CORE-STORE-OPENBIN-MISSING` and `STORES-DISAGREE-ON-…` name no removed symbol.

**Command namespaces:** none; `specs/command_registry.yaml` is untouched.

### Changes

No new or changed signature anywhere — removal only.

- **`liquers-core/src/store.rs`:** delete `pub trait Store` (≈60-327, with the commented-out Python
  sketch), `NoStore` and its impls (≈583-593), and `FileStore`, `MemoryStore`, `StoreRouter` with
  their impls (≈1377-2040). Drop imports used only there: `std::fs::File`, `std::io::{Read, Write}`,
  `RwLock` (`Arc`, `PathBuf`, `BTreeSet` stay; verified by counting uses in the remainder). Module
  doc: line 1 names only `AsyncStore`; ≈35-43 per question 3, so no intra-doc link dangles. Tests per
  Phase 3.
- **`liquers-core/src/cache.rs`:** delete; drop `pub mod cache;` from `lib.rs` ≈118. `chrono` stays
  (used by `assets.rs`, `metadata.rs`, …).
- **Comments:** `store_dir_index.rs` ≈8 drops the sync `MemoryStore` clause;
  `liquers-store/src/opendal_store.rs` ≈261 "matching `AsyncFileStore`".
- **`liquers-py/src/context.rs`:** remove fields `store`, `cache`, their initialisers, the imports
  `cache::{Cache, NoCache}`, `store::Store`, `std::sync::Mutex`, and the commented-out
  `with_store` / `with_cache`. `get_async_store` unchanged. Delete `store.rs`, `cache.rs`; the
  `lib.rs` orphan comment drops both names.
- **Errors / async / ownership:** unchanged; `NoAsyncStore` remains the "no store" default; no
  `unwrap`, no new `match`.

**Documents** (proposed `affects_docs`; reference/guide edits add a History row and bump
`reviewed:`):

| Document | Change |
|---|---|
| `reference/STORE_SEMANTICS.md` | ≈36-40 "was removed", keeping the trait-neutral rationale; ≈197 `AsyncFileStore::key_to_path`; ≈212 drop `MemoryStore` |
| `reference/PROJECT_OVERVIEW.md` | ≈93 `store.rs` row without `Store`; ≈103 `cache.rs` row removed |
| `reference/ASSET_SET_OPERATION.md` | ≈198 `AsyncStoreRouter` |
| `guides/LANGUAGE-INTEGRATION_GUIDE.md` | ≈244 cache module "was removed" |
| `DOCS_STRUCTURE_GUIDE.md` | §3 `core/store` row: `store.rs` only |
| `CLAUDE.md` | Async Patterns: no sync store trait exists; constraint: no synchronous store trait or implementation |
| `.claude/skills/liquers-unittest/references/testable-components.md`, `test-patterns.md`; `.claude/skills/rust-best-practices/SKILL.md` ≈56 | async names and `.await` examples only |

### Risks

| Category | Assessment |
|---|---|
| Files | ≈1 000 lines out of `store.rs`, 355 `cache.rs`, ≈300 `liquers-py`; nothing gains code |
| Tests | only the nine `store.rs` tests of question 2; no integration test names a sync store |
| Compatibility | Rust-level break with no in-tree or Python-visible user; crates unpublished |
| Build configs | nothing was `cfg`-gated; the import trim is the only warning risk — AC-5 |
| Recovery | single revert; no stored data, config or registry changes |
| Certainty | high — every symbol and range opened at HEAD |
