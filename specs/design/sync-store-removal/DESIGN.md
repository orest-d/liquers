---
id: SYNC-STORE-REMOVAL
kind: design
title: Remove the synchronous Store trait and the legacy cache module
form: compact
workflow: liquers-project
status: complete
readiness: ready
autofix: not-eligible
area: [core/store, py, docs]
issues: [CORE-SYNC-STORE-TRAIT-OBSOLETE]
gh_pr: [101]
affects_docs: [reference/STORE_SEMANTICS.md, reference/PROJECT_OVERVIEW.md, reference/ASSET_SET_OPERATION.md, guides/LANGUAGE-INTEGRATION_GUIDE.md, guides/STORE_IMPLEMENTATION_GUIDE.md, reference/STORE_CONFIG_FSD.md, reference/api/API_DOCS_GAP_ANALYSIS.md, reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md]
created: 2026-10-10
---
# Remove the synchronous Store trait and the legacy cache module

Pre-approved after Phase 2 on 2026-10-10.

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

### Design Readiness

- **Readiness:** `ready` · **Automatic fixing:** `not-eligible` — it removes public API
  (`auto-fix.md`: no public interface change), though no in-tree user exists.
- **Leading issue:** None. **Explanation:** removal only; every symbol, consumer and test was
  opened at HEAD, and Phases 3-4 prove each scenario.
- **Decision log (pre-approval):** None needing a decision. Phase 3 chose `compile_fail` doctests as
  the proof of AC-1/AC-2 (implementation detail: core doctests already run, 15 pass at HEAD).

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

## Phase 3: Examples and Tests

### Examples

**Primary (AC-1):** a `compile_fail` doctest per removed name, in a new `store.rs` module-doc section
*There is no synchronous store*, so the removal is both documented and enforced:

```rust,compile_fail
use liquers_core::store::Store;   // likewise NoStore, MemoryStore, FileStore, StoreRouter
```

One block per name, so one unresolved import cannot mask another. **Secondary (AC-2):** the same
for `liquers_core::cache::BinCache` in the `lib.rs` crate doc. Written first, all six *fail* at HEAD —
the reproduction. **Edge cases:** none new; a lingering import or dangling doc link is AC-5 /
`cargo test --doc`.

### Tests

| Test | Proves |
|---|---|
| doctests `liquers-core/src/store.rs - store (line …)`, five `compile_fail` blocks | AC-1 |
| doctest `liquers-core/src/lib.rs - (line …)`, `compile_fail` on `liquers_core::cache` | AC-2 |
| `memsupport01`…`memsupport06` (helper reduced to `AsyncMemoryStore`), `keyabs07_memory_stores_refuse_relative_keys`, `keyabs10_routers_report_key_not_absolute` (async halves kept) | AC-3 |
| async twins kept unchanged: `test_async_memory_store_basic` (for `test_simple_store`), `filestore01_async_missing_directory_lists_empty` (`filestore02`), `async_router_listdir_at_store_prefix` (`sync_router_…`), `keyabs08_async_file_store_refuses_traversal` (`keyabs09`), `reserved02`/`reserved03`/`reserved06` (`reserved04`, `reserved07`) | AC-3 |
| `liquers-core/tests/store_conformance_CONF.rs`, unchanged | AC-3 |
| `liquers-py` unit tests, plus `git diff` showing the `#[pymodule]` body untouched (no Python interpreter run: the pyclass surface is a compile-time list) | AC-4 |
| `scripts/check-build-matrix.sh` | AC-5 |
| `grep -rnP '(?<!Async)\b(MemoryStore|FileStore|StoreRouter|NoStore)\b|liquers_core::cache|trait Store\b' specs/reference specs/guides CLAUDE.md .claude/skills` shows only removal statements; `docs_index.py --check` | AC-6 |

```bash
cargo test -p liquers-core --lib --tests --doc
cargo test -p liquers-py --lib --no-default-features --features async_store
```

## Phase 4: Implementation Plan

### Steps

Steps 1-4 land in one commit, so every commit builds the workspace (`liquers-py` uses `cache`).
Rollback for any step: `git checkout -- <files>` before commit, `git revert` after.

- [x] 1. `store.rs` module doc, `lib.rs` crate doc — add the six `compile_fail` blocks — `cargo test -p liquers-core --doc` fails on exactly those six
- [x] 2. `store.rs` — delete `Store`, `NoStore`, `FileStore`, `MemoryStore`, `StoreRouter`; trim imports; rewrite module doc ≈1, ≈35-43; delete the six sync-only tests and reduce `memory_store_support`, `keyabs07`, `keyabs10` — `cargo test -p liquers-core --lib --doc`
- [x] 3. delete `cache.rs`, `pub mod cache;` in `lib.rs` — `cargo test -p liquers-core --doc` (all six blocks pass)
- [x] 4. `liquers-py/src/context.rs` fields, imports, commented setters; delete py `store.rs`, `cache.rs`; `lib.rs` orphan comment — `cargo test -p liquers-py --lib --no-default-features --features async_store`
- [x] 5. comments in `store_dir_index.rs` ≈8 and `liquers-store/src/opendal_store.rs` ≈261 — `cargo check -p liquers-store`
- [x] 6. full checks — `cargo test -p liquers-core --lib --tests --doc`; `cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh`
- [x] 7. documents per the Phase 2 table, History rows and `reviewed:` bumps — the AC-6 grep; `python3 scripts/docs_index.py --check`

### Validation

Every Phase 3 command passes; `git diff --stat` shows deletions only outside docs; no
`command_registry.yaml` change. Then Phase 5: close `CORE-SYNC-STORE-TRAIT-OBSOLETE`, set
`affects_docs`.

## Phase 5: Documentation

**Built versus approved.** As approved, with no scope change. `liquers-core` lost `Store`, `NoStore`,
`FileStore`, `MemoryStore`, `StoreRouter` and the `cache` module; `liquers-py` lost the
`Environment.store` / `cache` fields and the orphans `store.rs`, `cache.rs`. Six sync-only tests were
deleted and three reduced to their async halves, as Phase 3 mapped. Added within plan: the
`compile_fail` blocks name error `E0432`, so they fail only on an unresolved import; the
`store_dir_index.rs` comment keeps its history ("since removed") rather than dropping the clause.

**Proof.** `liquers-core`: lib 1041 passed, doc 21 (the six new blocks failed before step 2),
integration tests, conformance 5 (`--features store-conformance`); `liquers-store` conformance 2;
`liquers-py` 6 passed, same 13 pre-existing warnings (needs `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` on
Python 3.13, `PY-PYO3-REJECTS-PYTHON-3-13`); `liquers-lib` 35 suites; build matrix incl. wasm32. The
AC-6 search leaves only History rows and removal statements (`lq.MemoryStore` in the language guide is
a hypothetical Python name). `liquers-py/src/lib.rs` changed only in a comment (AC-4).

**Documents.** No new reference or guide. `affects_docs` reviewed against the code (History row,
`reviewed:` bump each): the four updated as in the Phase 2 table, `STORE_IMPLEMENTATION_GUIDE.md`
without change. Area candidates discarded (no store-trait or cache content): `ENVIRONMENT_CONFIG.md`,
`ENVIRONMENT_CONSTRUCTION_GUIDE.md`, `STORE_FACTORY_GUIDE.md`, `COMMAND_DECLARATION.md`.
PR review (Codex) found plain `Store` mentions the AC-6 search pattern missed; a widened search
(`` `Store` ``, `Store::`, "sync store") added `STORE_CONFIG_FSD.md`, `api/API_DOCS_GAP_ANALYSIS.md`
and `api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`, whose `with_store` / `with_cache` setters no
longer existed.

**Issues.** Closed: `CORE-SYNC-STORE-TRAIT-OBSOLETE`. Narrowed by dated notes:
`PY-MODULES-NOT-DECLARED-IN-LIB`, `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED`,
`STORE-METADATA-LAYOUT-HARDCODED-PER-STORE`. Discovered and filed:
`AXUM-STORE-MAKEDIR-TEST-IGNORED-FOR-A-FIXED-LIMITATION` (eligible for automatic fixing; left for its
own branch because this session may push only to its designated branch). Also `STORE-CONFORMANCE-FEATURE-BUILD-WARNINGS` (pre-existing warnings seen in the matrix log).

**Learning.** Checking what is *compiled and registered*, not what sits in `src/`, showed no Python
API change. `compile_fail` doctests with an error code are a cheap, runnable proof an API is gone.

