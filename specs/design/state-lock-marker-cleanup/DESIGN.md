---
id: STATE-LOCK-MARKER-CLEANUP
kind: design
title: Replace the stale "remove rwlock" marker on State with the ownership model it no longer needs
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/value]
issues: [CORE-STATE-LOCK-API-CLEANUP]
created: 2026-10-20
---
# Replace the stale "remove rwlock" marker on State with the ownership model it no longer needs

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

`State` (`liquers-core/src/state.rs`) no longer holds a lock, but its struct still carries
`// TODO: try to remove rwlock`, which sends every reader looking for a lock that is not there and
keeps an issue open for work that is done. Replace the marker with a short statement of the model
`State` actually follows, so the next reader does not reopen the question.

### Problem Example

```rust
// liquers-core/src/state.rs, HEAD 18732dd
use std::{borrow::Cow, sync::Arc};          // no RwLock, no Mutex imported
pub struct State<V: ValueInterface> {
    // TODO: try to remove rwlock          <- line 15
    data: Arc<V>,
    pub metadata: Arc<Metadata>,
}
```

Today: `grep -n RwLock liquers-core/src/state.rs` finds only the comment. The 2026-02-19 audit
(`specs/archive/2026-02-19-todo-audit.md` #13) recorded the marker at lines 15-16 together with a
second one about the public surface; the second is gone (`data` became private, with the comment
that replaced it), the lock is gone, and this one stayed.
Should be: the marker is replaced by a doc comment saying that `State` is an immutable pair of
shared handles, cloned by two reference-count increments, never locked, and changed only by
building a new `State`; and `set_status`'s "avoid this method" note says what it costs.

### Scope and Acceptance Criteria

- **AC-1** No stale marker
  WHEN `liquers-core/src/state.rs` is searched for `rwlock` (any case) and `TODO`
  THEN neither occurs
- **AC-2** The model is stated
  WHEN the rustdoc of `State` is read
  THEN it says that the value and the metadata are shared through `Arc`, that a clone copies two
  pointers, that there is no lock, and that every "mutation" (`with_data`, `with_metadata`,
  `set_status`) produces new handles rather than changing what other clones see
- **AC-3** Behaviour unchanged
  WHEN `cargo test -p liquers-core --lib state::` runs
  THEN every existing test passes with no test edited

Non-goals: making `State::metadata` private (no caller outside `state.rs` writes the field, so no
invariant is at risk, and the change would touch several hundred read sites); copy-on-write
metadata (`Arc::make_mut` in `set_status`, a shared `MetadataRecord`) — that is
`shared-metadata-and-bytes`; benchmarking (`BENCHMARK-SUITE` was named as a prerequisite only for
removing a lock, which has already happened).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — documentation fix in `liquers-core/src/state.rs` only (rule 2:
  doc comments), `S`, no structure or interface change, one crate.
- **Leading issue:** None
- **Explanation:** the issue's first expected outcome ("the lock is removed and `State` becomes a
  plain value") is already true at HEAD; what remains is a comment. Re-sized `M` → `S`.
- **Open questions:** None.

### Design Dependencies

- `shared-metadata-and-bytes` — **overlaps**, weak: it changes how `State`'s metadata is shared
  (copy-on-write inside `Metadata`) and may rewrite `set_status`'s note again. Kept separate under
  overlap exclusion E1 (this item is eligible for automatic fixing; that design is not). Either
  order; whichever lands second adjusts the `set_status` sentence.

### Consolidated Findings

- The lock was removed before the shallow history available here begins; the evidence is the
  code at HEAD (imports and fields), not a commit.
- The archived audit's second concern, the public surface, is half done: `data` is private with
  guarded accessors; `metadata` stays `pub` and is only read from outside. Nothing requires
  closing it now.

## Phase 2: Architecture

### Solution

Edit comments only, in `liquers-core/src/state.rs`:

1. Delete `// TODO: try to remove rwlock` (struct body, line 15).
2. Extend the struct's `///` doc (lines 9-12, which also say "chached" and "considered as
   read-only") into the model of AC-2: two `Arc` handles; `Clone` copies the handles; no interior
   mutability, so no lock and nothing to synchronize; `with_data`, `with_metadata`, `set_status`
   and `from_value_and_metadata` build new handles; the metadata is copied when changed (pointing
   at `shared-metadata-and-bytes` is not needed — the sentence describes HEAD).
3. `set_status` (lines 93-99): replace "Avoid this method, since it creates a copy of the metadata
   with a changed status." with the precise cost: it clones the whole `Metadata` (record, log and
   all) and replaces this state's handle; other clones keep the old metadata.

Rejected: closing the issue as fixed without the edit (the marker would keep generating the same
triage); making `metadata` private (non-goal above).

### Changes

- `liquers-core/src/state.rs`: doc comments of `State` and `State::set_status`; the marker removed.
- No code, signature, test or document outside the file changes. `specs/reference/` does not
  mention the lock.

### Risks

Comment-only change; `cargo doc` and the existing tests are the whole check. Certainty: high.
Recovery: revert the commit.

## Phase 3: Examples and Tests

### Examples

```rust
let a: State<Value> = State::new().with_data(Value::from("x"));
let b = a.clone();                       // two Arc increments, no lock
let c = b.clone().with_data(Value::from("y"));
assert_eq!(a.try_into_string()?, "x");   // a and b unaffected by c
```

### Tests

No new test: the change is documentation. Proof:
- `grep -niE 'rwlock|TODO' liquers-core/src/state.rs` prints nothing — AC-1
- reading `cargo doc -p liquers-core --no-deps` output for `State` — AC-2
- `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib state::` passes unchanged — AC-3

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-core/src/state.rs` `State` doc and struct body — remove the marker, write the model (AC-1, AC-2) — `grep -niE 'rwlock|TODO' liquers-core/src/state.rs` empty; `cargo test -p liquers-core --lib state::`. Rollback: revert.
- [x] 2. `liquers-core/src/state.rs` `State::set_status` doc — state the cost (AC-2) — `cargo doc -p liquers-core --no-deps`.
- [x] 3. Close `CORE-STATE-LOCK-API-CLEANUP` (`status: closed`, `## Resolution` naming the commit), design `status: complete` — `python3 scripts/docs_index.py --check`.

### Validation

`cargo fmt --check -p liquers-core`; `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib state::`;
`python3 scripts/docs_index.py --check`; diff review: only `state.rs` comments and the two records.
