---
id: STORE-CONDITIONAL-WRITE
kind: design
title: A conditional store write, and a written contract for concurrent writers
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/store, docs]
issues: [STORE-WRITE-HAS-NO-PRECONDITION]
created: 2026-10-19
---
# A conditional store write, and a written contract for concurrent writers

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

Give `AsyncStore` one way to make a read-modify-write safe — a write that happens only if the key
still holds what the writer read — and write down what every store guarantees when two writers
race, so last-writer-wins is a stated rule rather than an accident.

### Problem Example

Two writers edit `notes/todo.md` in an `AsyncFileStore` (or any store), at HEAD:

```rust
let (a, meta) = store.get(&key).await?;          // writer A reads "x"
let (b, _)    = store.get(&key).await?;          // writer B reads "x"
store.set(&key, b"x+A", &meta).await?;           // A writes
store.set(&key, b"x+B", &meta).await?;           // B writes: Ok(()), A's edit is gone
```

Nothing fails or records the loss: `AsyncStore` has no conditional write, and
`AsyncFileStore::acquire_lock` makes one `set` atomic, not the sequence. Should be: B writes with `set_if(&key, b"x+B", &meta, &WriteCondition::Version(v))`,
where `v = Version::from_content(b"x")`, and gets `Err` (a distinguishable conflict) with the store
still holding `x+A`.

Partly stale: `STORE_SEMANTICS.md` now assumes one metadata writer (introduction) and locks grouped
writes per process (§16, citing this issue), but says nothing on concurrent `set`.

### Scope and Acceptance Criteria

- **AC-1** Stale write refused
  WHEN `set_if` carries `WriteCondition::Version(v)` and the key's stored data no longer hashes to `v`
  THEN it returns `Err` with the conflict error type and the stored data and metadata are unchanged
- **AC-2** Matching write succeeds
  WHEN `set_if` carries the content version of the data currently stored
  THEN the write happens exactly as `set` would, and a later `get` returns the new data
- **AC-3** Create only if absent
  WHEN `set_if` carries `WriteCondition::Absent`
  THEN it creates a key with no data object (fresh, or metadata-only per §8) and refuses, unchanged, a key holding data
- **AC-4** Version on an absent key
  WHEN `set_if` carries `WriteCondition::Version(_)` for a key with no data object
  THEN it is refused and nothing is created
- **AC-5** One winner on an atomic store
  WHEN eight tasks race `set_if(Absent)` on one key of a memory, file or pipeline store, or a router over them
  THEN exactly one returns `Ok`, the rest the conflict error, and the stored data is the winner's
- **AC-6** Every store answers it
  WHEN a store has no `set_if` of its own (trait default, OpenDAL, a wrapper)
  THEN it still checks before writing, and a read-only store refuses it as it refuses `set`
- **AC-7** Atomicity is declared
  WHEN `write_atomicity(key)` is asked
  THEN the memory and file stores answer `Store`, the pipeline `Process`, a router its owning member's answer, and the default `CheckThenWrite`
- **AC-8** The contract is written and enforced
  WHEN `STORE_SEMANTICS.md` is read
  THEN a concurrency section states that `set` is last-writer-wins, what `set_if` guarantees per atomicity level, and cites rules `cond01`-`cond04`, which every writable in-tree store passes

Non-goals: conditional `set_metadata` / `remove`; native `If-Match` in `AsyncOpenDALStore` (gets
the default; follow-up filed at step 8); cross-process pipeline atomicity; callers (asset manager,
`liquers-axum`, `ns-mem`); `liquers-web` stores (inherit the default unchanged).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds `pub` trait methods and two `pub` enums to
  `liquers-core` (rules 3 and 4).
- **Leading issue:** the conflict error type (question 1).
- **Explanation:** local to `liquers-core`; every store has a correct answer (an atomic override
  or the check-then-write default). Three contract choices remain proposals; none changes the plan.
- **Open questions:**
  1. **Open design - conflict error type:** reuse `ErrorType::StatusConflict` through a new
     `Error::write_conflict` (recommended: core only; axum already maps it to 409, py and web carry
     it), or add `ErrorType::WriteConflict` (unambiguous, but the exhaustive matches in
     `liquers-py`, `liquers-axum` and `liquers-web` `error.rs` change: size `L`).
  2. **Proposed resolution - version token:** `Version::from_content` of the stored data
     (recommended: computable by every store, also for outside edits; costs one read). Rejected:
     the record's `version` (asset-layer, sometimes a clock value) and the ETag (rarely offered).
  3. **Proposed resolution - default:** check-then-write through the store's own `get_bytes` and
     `set`, declared `CheckThenWrite` (recommended: wrappers stay correct; the race window shrinks
     to two calls). Rejected: `NotSupported`, which excludes every OpenDAL and web store.

### Design Dependencies

- `agent-memory-mvp` — **required-by**: its W3 / AC-6 precondition is check-then-write until this
  lands; it should use `Version::from_content` tokens.
- `store-adapters` (complete, E4) — **overlaps**: the pipeline's group locks (§16) are what make its
  `set_if` atomic within a process. Not extended.
- `CORE-SESSION-AND-KEY-ACL`, `NO-REMOTE-STORE-OR-ASSET-MANAGER` (issues) — **overlaps**: the
  authorization half of multi-writer use, and a remote store that must forward `set_if`. No order.

### Consolidated Findings

- Q1 is the decision; a new variant makes this a four-crate change (core, py, axum, web), size `L`.
- The default uses only `get_bytes` and `set`: it compiles on wasm and is correct for every store
  and wrapper unedited; only atomicity differs.
- Core wrappers (`SharedStore`, router, `ReadOnlyStore`, pipeline) must forward or override, or
  they silently downgrade an atomic member (guide §1, "a wrapper is not two methods").
- Four new rules change every "Rules run" count in `STORE_IMPLEMENTATION_GUIDE.md` §9, including
  the three browser rows that are merged by hand.
- No Liquers query is involved (nothing for `liquers-validate`).

## Phase 2: Architecture

### Solution

- **Default `set_if`**: `key.as_absolute()?`, read the current data with `self.get_bytes(key)`
  (`KeyNotFound` = absent, any other error is returned), compare with the condition, then
  `self.set(key, data, metadata)`; on mismatch return the conflict error. `write_atomicity`
  defaults to `CheckThenWrite`.
- **`AsyncMemoryStore`**: check and write under one `scc::HashMap::entry_async` guard (vacant or
  `data: None` = absent); the directory index is updated as `write_data` does. `Store`.
- **`AsyncFileStore`**: the body of `set` moves to a private `set_locked`; `set_if` takes
  `acquire_lock`, reads the data file (`NotFound` = absent), compares, then calls `set_locked`. The
  lock file is honoured by every Liquers writer of the directory, in any process. `Store`.
- **`MetadataPipelineStore`**: `commit` gains `condition: Option<&WriteCondition>`, checked inside
  the group guards against `self.backend().get_data(key)` before `commit_locked`. `Process`.
- **`AsyncStoreRouter`, `SharedStore`**: forward both methods to the owning member / inner store.
- **`ReadOnlyStore`**: `set_if` refuses exactly as `set` (`self.refuse(key, "set")`).
- **Error**: `Error::write_conflict(key, store_name, expected: &WriteCondition, found: Option<Version>)`
  with `ErrorType::StatusConflict` (Q1); `ErrorType::StatusConflict`'s doc widens to "a stored value
  in its current state". `set` is unchanged: last writer wins.

Rejected: `set_if_version(…, Option<Version>)` as filed (`None` is ambiguous); record versions
(Q2); refusing by default (Q3); a conformance `StoreCapabilities` field (no `Default`: it edits
every fixture in four crates; atomicity is proved by per-store race tests, as §16's is).

Known-issue preflight: nothing blocks; `CORE-STORE-OPENBIN-MISSING` and
`STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` touch the trait, not these methods. No command namespace.

### Changes

`liquers-core/src/store.rs` — new items beside `DataFacts`, and two trait methods:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WriteCondition {
    /// No data object exists (a metadata-only key counts as absent, §8).
    Absent,
    /// The stored data hashes to this `Version::from_content`; any other version never matches.
    Version(Version),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WriteAtomicity { CheckThenWrite, Process, Store }

// AsyncStore, both defaulted:
async fn set_if(&self, key: &Key, data: &[u8], metadata: &Metadata,
                condition: &WriteCondition) -> Result<(), Error>;
fn write_atomicity(&self, _key: &Key) -> WriteAtomicity { WriteAtomicity::CheckThenWrite }
```

Overrides: `AsyncMemoryStore`, `AsyncFileStore` (+ private `set_locked`), `AsyncStoreRouter`,
`SharedStore` in `store.rs`; `ReadOnlyStore` in `store/read_only.rs`; `MetadataPipelineStore` and
`commit` in `store/pipeline.rs`. `liquers-core/src/error.rs`: `Error::write_conflict`, doc of
`ErrorType::StatusConflict`. Conformance: new `store_conformance/rules/conditional.rs` with
`cond01`-`cond04`, registered in `rules/mod.rs` `RULES`.

Documents: `STORE_SEMANTICS.md` new §17 "Concurrent writers and conditional writes" (`set` is
last-writer-wins, `set_if`, the atomicity table; introduction and §16 point to it);
`STORE_IMPLEMENTATION_GUIDE.md` §2 (what to override), §8 rule row, §9 regenerated; History rows,
`reviewed:` bumps. No command or registry change; async as the trait, no blocking I/O.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A wrapper not forwarding `set_if` downgrades atomicity silently | core wrappers forwarded (step 3-4); `condwrite04` asserts the router keeps `Store` | high |
| Memory entry guard held across the index update | update the index after the guard drops | medium |
| New rules expose a latent bug in a web or OpenDAL store | a real defect: file it, list it in `AllowedFailure` | medium |
| Reading whole data to hash a large object | documented cost; ETag token is a later option (Q2) | high |
| Reused `StatusConflict` confuses asset-status callers | they never call `set_if` | high |

Existing tests unchanged; suites gain four rules. Recovery: one commit per step.

## Phase 3: Examples and Tests

### Examples

The Problem Example is `cond01`'s scenario. A safe edit loop:

```rust
loop {
    let (bytes, meta) = store.get(&key).await?;
    let expected = WriteCondition::Version(Version::from_content(&bytes));
    match store.set_if(&key, &edit(&bytes), &meta, &expected).await {
        Ok(()) => break,
        Err(e) if e.error_type == ErrorType::StatusConflict => continue, // re-read and retry
        Err(e) => return Err(e),
    }
}
```

Create-once: `store.set_if(&key, b"init", &meta, &WriteCondition::Absent)`.

### Tests

Rules in `store_conformance/rules/conditional.rs`, run by every existing suite:

- `cond01` — stale `Version` refused with `StatusConflict`, data unchanged; `[Write]`, `Scratch`: AC-1, AC-8
- `cond02` — current `Version` writes; `get` returns the new bytes; `[Write]`, `Scratch`: AC-2, AC-8
- `cond03` — `Absent` creates a fresh key, then is refused on it; `Version` on a fresh key refused and creates nothing (recorded if it does); `[Write]`, `CreateOnly`: AC-3, AC-4, AC-8
- `cond04` — refuting `Write`: `set_if(Absent)` on a fresh key is an `Err` and the key stays absent; `CreateOnly`: AC-6, AC-8

Unit tests in `liquers-core/src/store.rs` `mod tests`:

- `condwrite01_default_set_if_checks_before_writing` — a `get`/`set`-only store (like `keyabs17`'s): AC-1, AC-2, AC-6
- `condwrite02_memory_store_race_has_one_winner` — `multi_thread`, 8 spawned `set_if(Absent)`: AC-5
- `condwrite03_file_store_race_has_one_winner` — same over a temp `AsyncFileStore`: AC-5
- `condwrite04_router_forwards_set_if_and_atomicity` — router over memory + file stores: AC-5, AC-7
- `condwrite05_metadata_only_key_counts_as_absent` — memory and file store: AC-3
- `condwrite06_write_atomicity_per_store` — memory, file, default, `SharedStore`: AC-7

In `store/read_only.rs`: `read_only_refuses_set_if` — AC-6. In `store/pipeline.rs`:
`conditional_writes_are_serialized_within_the_process` (race over `per_entry` and `grouped`;
`write_atomicity` is `Process`) — AC-5, AC-7. Contract: `conformance_docs_CONF` (D1) passes with
`cond01`-`cond04` cited — AC-8.

Run: `cargo test -p liquers-core --features store-conformance --lib --tests`, then
`cargo test -p liquers-store --features store-conformance --test store_conformance_CONF`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/error.rs` — `Error::write_conflict`, widen `ErrorType::StatusConflict` doc — `cargo test -p liquers-core --lib error`. Rollback: revert.
- [ ] 2. `store.rs` — `WriteCondition`, `WriteAtomicity`, defaulted `set_if` / `write_atomicity`; `condwrite01` — `cargo test -p liquers-core --lib condwrite`. Rollback: revert (additive).
- [ ] 3. `store.rs` `AsyncMemoryStore`, `AsyncFileStore` (`set_locked`) overrides — `condwrite02`, `condwrite03`, `condwrite05`, `condwrite06`, then `cargo test -p liquers-core --lib store`. Rollback: revert; the default still applies.
- [ ] 4. `store.rs` `AsyncStoreRouter`, `SharedStore`; `store/read_only.rs`; `store/pipeline.rs` `commit` + override — `condwrite04`, `read_only_refuses_set_if`, `conditional_writes_are_serialized_within_the_process`. Rollback: revert step 4.
- [ ] 5. `store_conformance/rules/conditional.rs`, `rules/mod.rs` — `cond01`-`cond04` — `cargo test -p liquers-core --features store-conformance --test store_conformance_CONF` and the `liquers-store` suite. Depends on 1-4.
- [ ] 6. `STORE_SEMANTICS.md` §17 and pointers; guide §2, §8, §9 (both `status_table` generators); History rows, `reviewed:` — `cargo test -p liquers-core --features store-conformance --test conformance_docs_CONF`.
- [ ] 7. After `cargo clean`, the `liquers-web` Node loop (no web code change; the default on `JsStore`, `FetchStore`); update their §9 rows.
- [ ] 8. Records — close `STORE-WRITE-HAS-NO-PRECONDITION` with a resolution note; file a feature for native conditional writes in `AsyncOpenDALStore` (OpenDAL `if_match` / `if_not_exists`) — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --features store-conformance --lib --tests`;
`cargo test -p liquers-store --features store-conformance --lib --tests`;
`cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh` (wasm32 rows);
`python3 scripts/docs_index.py --check`; a diff review for edits outside `liquers-core` and the
two documents.
