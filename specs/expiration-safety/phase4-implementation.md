# Phase 4: Implementation Plan - expiration-safety

## Overview

**Feature:** expiration-safety (WP-3, `plan20260707.md`)

**Architecture:** Five targeted changes confined to `liquers-core/src/assets.rs` (per Phase 2):
weak-ref monitor tracking, a `poll_state()` fix paired with a new `*_any_status` read family, and
two new required `AssetManager<E>` trait methods (`get_any_status`, `to_override`) implemented on
`DefaultAssetManager<E>`. Tests land in `liquers-core/tests/expiration_integration.rs` (integration)
and `assets.rs`'s own `#[cfg(test)] mod tests` (unit, monitor internals). No new crates, no new
public surface outside `liquers-core`.

**Estimated complexity:** Medium (mechanical monitor change + one focused piece of new business
logic — the `PersistenceStatus` branching in `to_override` — inside an already-large, well-tested
file).

**Estimated time:** 4-6 hours for an experienced Rust developer familiar with this codebase.

**Prerequisites:**
- Phases 1, 2, 3 approved ✅
- Both Phase 1 open questions resolved (trait placement, override-persistence branching) ✅
- No new dependencies — `async-trait`, `tokio`, `chrono` already in `liquers-core`'s `Cargo.toml` ✅
- `rust-best-practices` skill is **not installed** in this environment; every step below has been
  manually checked instead against this file's existing idioms (explicit matches, `Error::typed_*`
  constructors, `pub(crate)` visibility discipline, async-first) — flagged per step below rather
  than claimed as a skill invocation that didn't happen.

## Implementation Steps

### Step 1: Weak-ref expiration monitor

**File:** `liquers-core/src/assets.rs`

**Action:**
- Change `TimedAsset<E>.asset_ref` field type from `AssetRef<E>` to `WeakAssetRef<E>` (struct at
  `assets.rs:2399`).
- At both `Track` message construction sites inside `run_expiration_monitor` (`:2534`, `:2639`),
  downgrade at heap-insertion time: `asset_ref: asset_ref.downgrade()`. The
  `ExpirationMonitorMessage::Track` message itself is unchanged (still carries a strong
  `AssetRef<E>` — the sender already owns one; only the heap entry downgrades).
- At both fire sites (inside the `while let Some(Reverse(timed)) = heap.peek()` loop and its
  `Reverse(timed) = heap.pop()` in the timer-fire `select!` arm, around `:2548-2621`), replace the
  direct move `let asset_ref = timed.asset_ref;` with:
  ```rust
  let Some(asset_ref) = timed.asset_ref.upgrade() else {
      // Asset already dropped elsewhere (no strong refs remain) — nothing to expire or evict.
      continue;
  };
  ```

**Code changes:**
```rust
// MODIFY (assets.rs:2399-2403):
struct TimedAsset<E: Environment> {
    expiration: chrono::DateTime<chrono::Utc>,
    asset_id: u64,
    asset_ref: WeakAssetRef<E>,   // was: AssetRef<E>
}

// MODIFY (both Track construction sites, ~:2534 and ~:2639):
heap.push(Reverse(TimedAsset {
    expiration: dt,
    asset_id,
    asset_ref: asset_ref.downgrade(),   // was: asset_ref
}));

// MODIFY (both fire sites, inside the timer-fire select! arm):
// Before:
// let asset_ref = timed.asset_ref;
// let asset_id = timed.asset_id;
// After:
let asset_id = timed.asset_id;
let Some(asset_ref) = timed.asset_ref.upgrade() else {
    continue;
};
```

**Validation:**
```bash
cargo check -p liquers-core
# Expected: compiles. `Ord`/`Eq`/`PartialOrd`/`PartialEq` impls on TimedAsset are unaffected
# (they only read `expiration`/`asset_id`, never `asset_ref`), so no further changes needed there.
```

**Rollback:**
```bash
git diff liquers-core/src/assets.rs   # review
git checkout liquers-core/src/assets.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** (none required — mechanical, single-file, pattern-following change)
- **Knowledge:** Phase 2 architecture doc's "Expiration Monitor Fire Logic" section; the exact
  current code at `assets.rs:2399-2656` (both `select!` arms; note the two arms are structurally
  duplicated in the current code, so the same edit applies twice)
- **Rationale:** Small, self-contained, mechanical substitution with a clearly specified before/
  after; no architectural judgment needed.

---

### Step 2: `poll_state` fix + new `*_any_status` read family

**File:** `liquers-core/src/assets.rs`

**Action:**
- In `AssetData::poll_state()` (`:596-636`), move `Status::Expired` out of the
  `Ready | Expired | Source | Override | Volatile` arm into its own arm returning `None`.
- Add `AssetData::poll_state_any_status(&self) -> Option<State<E::Value>>` immediately after
  `poll_state()`: for `Status::Expired`, build the state the same way the old `poll_state` arm did
  (the exact `metadata.with_type_identifier`/`with_type_name` + `State::from_parts` construction
  currently at `:625-630`); for every other status, delegate to `self.poll_state()`.
- Add `AssetRef::poll_state_any_status(&self) -> Option<State<E::Value>>` immediately after the
  existing async `poll_state()` wrapper (`:2075-2078`), mirroring its `self.data.read().await...`
  pattern.
- Add `AssetRef::get_any_status(&self) -> Option<State<E::Value>>` as a thin wrapper:
  `self.poll_state_any_status().await`. Doc-comment it as peek-only/non-blocking (unlike `get()`,
  it never waits — Phase 2's explicit design choice).
- **No change to `AssetRef::get()`** — verified in Phase 2/3 review that its existing
  `AssetNotificationMessage::Expired` arm already returns
  `Err("Asset expired while waiting for data")`, and `mark_expired_status()` already sends that
  notification in the same lock scope that sets `Status::Expired`, so `get()`'s behavior on an
  expired asset is corrected automatically by the `poll_state()` fix above.

**Code changes:**
```rust
// MODIFY (assets.rs:596-636), split Status::Expired out:
pub fn poll_state(&self) -> Option<State<E::Value>> {
    match self.status {
        Status::None => None,
        Status::Directory => { /* unchanged */ }
        Status::Recipe => None,
        Status::Submitted => None,
        Status::Dependencies => None,
        Status::Processing => None,
        Status::Partial => None,
        Status::Error | Status::Cancelled => { /* unchanged */ }
        Status::Storing => None,
        Status::Expired => None,   // NEW: was grouped with the Ready arm below
        Status::Ready | Status::Source | Status::Override | Status::Volatile => {
            /* unchanged body, minus Expired */
        }
    }
}

// NEW, placed directly after poll_state() in AssetData's impl block:
/// Like `poll_state()`, but also returns data for `Status::Expired` — the explicit
/// "I know it's expired, give it to me anyway" recovery read. Not gated to keyed assets (see
/// Phase 2 rationale); the keyed-only restriction lives in the manager-level API below.
pub fn poll_state_any_status(&self) -> Option<State<E::Value>> {
    match self.status {
        Status::Expired => {
            if let Some(data) = &self.data {
                let mut metadata = self.metadata.clone();
                metadata.with_type_identifier(data.identifier().to_string());
                metadata.with_type_name(data.type_name().to_string());
                Some(State::from_parts(data.clone(), Arc::new(metadata)))
            } else {
                None
            }
        }
        _ => self.poll_state(),
    }
}

// NEW, placed directly after the async poll_state() wrapper on AssetRef (~:2075-2078):
pub async fn poll_state_any_status(&self) -> Option<State<E::Value>> {
    self.data.read().await.poll_state_any_status()
}

/// Peek-only, no waiting: unlike `get()`, never blocks on notifications — Expired (and any
/// other data-bearing status) returns its value immediately; non-terminal or data-less statuses
/// return `None` immediately.
pub async fn get_any_status(&self) -> Option<State<E::Value>> {
    self.poll_state_any_status().await
}
```

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core --lib assets::   # existing unit tests must stay green
```

**Rollback:**
```bash
git diff liquers-core/src/assets.rs
git checkout liquers-core/src/assets.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** `liquers-unittest` (for verifying the change doesn't disturb the existing `Status`
  match-arm conventions)
- **Knowledge:** Phase 2 architecture doc's "New `AssetData`/`AssetRef` methods" section; current
  `assets.rs:594-636` (`poll_state`) and `:2075-2078` (async wrapper); `AssetRef::get()` at
  `:1985-2040` (read-only — confirm no change needed, do not edit)
- **Rationale:** Requires care to preserve the exact existing construction logic for the `Expired`
  arm verbatim inside the new method (a haiku model is more likely to subtly alter the metadata
  construction); also must NOT touch the `Error | Cancelled` arm (WP-2 territory) — this
  discipline needs judgment, not just pattern-copying.

---

### Step 3: `AssetManager` trait additions + `DefaultAssetManager` implementation

**File:** `liquers-core/src/assets.rs`

**Action:**
- Add two new **required** async methods to the `AssetManager<E>` trait (`:2248`, no default body
  — see Phase 2 rationale: a generic default would force double-serialization):
  ```rust
  async fn get_any_status(&self, key: &Key) -> Result<Option<State<E::Value>>, Error>;
  async fn to_override(&self, key: &Key) -> Result<(), Error>;
  ```
- Extract the existing inline deserialization logic inside `try_fast_track` (`:486-504`: the
  `is_binary_type_identifier` check + `E::Value::deserialize_from_bytes(...)` call + error
  handling) into a small `pub(crate) fn deserialize_stored_value(binary: &[u8], type_identifier:
  &str, data_format: &str) -> Result<E::Value, Error>` free function (or associated function on
  `AssetData<E>`), used by BOTH `try_fast_track` and the new `get_any_status` store-fallback below.
  This avoids duplicating the ~20-line deserialize-or-treat-as-corrupted logic.
- Implement `DefaultAssetManager::get_any_status(key)`:
  1. If `self.assets.get_async(key)` has an entry, return `Ok(asset_ref.get_any_status().await)`.
  2. Else, `store.get(key)` (the raw store, NOT through `try_fast_track`'s status allow-list); if
     `Err`, propagate via `?`; if `Ok((binary, metadata))` and `metadata.status().has_data()`,
     deserialize via the extracted helper and return `Ok(Some(State::from_parts(...)))`.
     Deliberately do **not** call `dependency_manager.register_version`/`load_from_records` and do
     **not** insert into `self.assets` — no side effects on normal evaluation (Phase 2 constraint).
  3. Else `Ok(None)`.
- Implement `DefaultAssetManager::to_override(key)`:
  1. If `self.assets.get_async(key)` has an entry:
     - `asset_ref.to_override().await?` (existing method, handles every data-bearing status).
     - `match asset_ref.persistence_status().await { Persisted => store.set_metadata(key,
       &metadata_with_override_status).await?, NonSerializable => {} (no store write), NotPersisted
       | None => { serialize + store.set(key, &binary, &metadata).await; record via the existing
       record_persistence_result-style tracking } }`.
     - Ensure the (now `Override`) entry remains in `self.assets` (it already is, unless a
       concurrent monitor eviction raced it out — in that case re-`insert_async`).
  2. Else, load from the store exactly as `get_any_status`'s fallback does; if found, rewrite
     **only** the metadata `status` field to `Override` via `store.set_metadata` (bytes are
     provably already persisted, since we just deserialized them) — do not construct or insert an
     in-memory `AssetRef`.
  3. If neither exists, `Err(Error::key_not_found(key))`.

**Code changes:** (signatures only; full bodies per the algorithm above — this step is the one
requiring genuine implementation, not a mechanical copy)
```rust
// MODIFY trait (assets.rs:2248), add two required methods (placed near `get`/`set_state`):
async fn get_any_status(&self, key: &Key) -> Result<Option<State<E::Value>>, Error>;
async fn to_override(&self, key: &Key) -> Result<(), Error>;

// NEW impl block additions on DefaultAssetManager<E>'s `impl AssetManager<E> for
// DefaultAssetManager<E>` (near `get`/`set_state`, ~:2975 onward):
async fn get_any_status(&self, key: &Key) -> Result<Option<State<E::Value>>, Error> {
    if let Some(entry) = self.assets.get_async(key).await {
        let asset_ref = entry.get().clone();
        drop(entry);
        return Ok(asset_ref.get_any_status().await);
    }
    let store = self.get_envref().get_async_store();
    if !store.contains(key).await? {
        return Ok(None);
    }
    let (binary, metadata) = store.get(key).await?;
    if !metadata.status().has_data() {
        return Ok(None);
    }
    let type_identifier = metadata.type_identifier()?;
    let data_format = metadata.get_data_format();
    let value = deserialize_stored_value(&binary, &type_identifier, &data_format)?;
    Ok(Some(State::from_parts(Arc::new(value), Arc::new(metadata))))
}

async fn to_override(&self, key: &Key) -> Result<(), Error> {
    if let Some(entry) = self.assets.get_async(key).await {
        let asset_ref = entry.get().clone();
        drop(entry);
        asset_ref.to_override().await?;
        // ... PersistenceStatus branch per algorithm above ...
        return Ok(());
    }
    // ... store-fallback branch: load, rewrite metadata status only, per algorithm above ...
    Err(Error::key_not_found(key))
}
```

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core --lib assets::
cargo check -p liquers-py   # public AssetManager trait gained required methods (CLAUDE.md rule)
```

**Rollback:**
```bash
git diff liquers-core/src/assets.rs
git checkout liquers-core/src/assets.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** `liquers-unittest`
- **Knowledge:** Phase 2 architecture doc in full (this step implements its central algorithm);
  `assets.rs:457-560` (`try_fast_track`, for the extraction and the exact store-read/allow-list
  pattern to bypass); `assets.rs:1833-1887` (`AssetRef::to_override`); `assets.rs:1066-1130`
  (`persistence_status`/`record_persistence_result`/`classify_persistence_error`); `assets.rs:475`
  (fast-track's status allow-list, to confirm `Override` is already in it so the store-fallback
  promotion needs no fast-track changes)
- **Rationale:** This is the one step with real architectural/business logic (the
  `PersistenceStatus` three-way branch, and the two-path in-memory-vs-store algorithm) — requires
  judgment, not pattern-following. **Do not delegate to haiku.**

---

### Step 4: Unit tests (monitor internals)

**File:** `liquers-core/src/assets.rs`, existing `#[cfg(test)] mod tests { ... }` block (end of
file, `~:4156` onward)

**Action:** Add the three unit tests from Phase 3 ("Test Plan > Unit Tests"):
`test_untrack_releases_strong_ref`, `test_retrack_earlier_deadline_fires_once`,
`test_expire_failure_preserves_processing_asset`. Adapt Phase 3's sketches to whatever exact
imports/helpers already exist in this test module (it already has `use super::*;` plus
`command_metadata::CommandKey`, `AsyncMemoryStore`, etc. — confirm exact names in place rather than
re-guessing import paths, since Phase 3 wrote these assuming module-local access it could not
fully verify from outside `assets.rs`).

**Code changes:** As drafted in `specs/expiration-safety/phase3-examples.md` "Test Plan > Unit
Tests", adjusted for whatever the module's actual existing imports turn out to be (a Read of the
current `mod tests` header block is the first action this step's agent takes).

**Validation:**
```bash
cargo test -p liquers-core --lib assets::tests::test_untrack_releases_strong_ref
cargo test -p liquers-core --lib assets::tests::test_retrack_earlier_deadline_fires_once
cargo test -p liquers-core --lib assets::tests::test_expire_failure_preserves_processing_asset
cargo test -p liquers-core --lib assets::
```

**Rollback:**
```bash
git diff liquers-core/src/assets.rs
# Remove the three added test functions if they don't compile/pass after reasonable effort;
# re-open as a Phase 3 revision if the gate-command pattern needs redesign.
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** `liquers-unittest`
- **Knowledge:** Phase 3 examples doc's "Unit Tests" section; the actual current imports at the
  top of `assets.rs`'s test module; `CommandRegistry::register_async_command` signature
  (`commands.rs:486-527`) for the gate-command pattern in test 3
- **Rationale:** Timing-sensitive tests (retrack-fires-once, gate-based Processing preservation)
  need judgment to keep deterministic per this repo's "no sleeps for correctness" convention —
  not simple pattern-copying.

---

### Step 5: Integration tests

**File:** `liquers-core/tests/expiration_integration.rs` (existing file, append)

**Action:** Add all Phase 3 integration-level tests: Example 1 (3 tests), Example 2 (4 tests,
including the shared `keyed_counter_env()` helper), Example 3 (3 tests, one marked as a sketch
needing gate-synchronization rework), and the "Integration Tests" section (`CountingStore` double,
`test_to_override_metadata_only_when_persisted`, the deferred `#[ignore]`d
`test_to_override_retries_persist_when_not_persisted` and
`test_to_override_skips_store_write_when_nonserializable`, and
`test_get_any_status_has_no_side_effects_on_normal_get`) — all as drafted in
`specs/expiration-safety/phase3-examples.md`, verified compilable against the real APIs added in
Steps 1-3.

**Code changes:** As drafted in Phase 3, appended to `expiration_integration.rs` with the shared
`keyed_counter_env()` helper and `CountingStore` struct defined once near the top of the new
additions (not duplicated per test).

**Validation:**
```bash
cargo check -p liquers-core --test expiration_integration
cargo test -p liquers-core --test expiration_integration
# Expected: all non-#[ignore]'d tests pass. The two #[ignore]'d tests
# (test_to_override_retries_persist_when_not_persisted,
# test_to_override_skips_store_write_when_nonserializable) remain ignored — see their doc
# comments for what unblocks them (a per-key-failing store double; a non-serializable Value
# construction). test_dependency_expiring_during_parent_evaluation_is_allowed may need its gate
# synchronization reworked per its own doc comment (fixed sleep -> precise signal) if it proves
# flaky in CI.
```

**Rollback:**
```bash
git diff liquers-core/tests/expiration_integration.rs
git checkout liquers-core/tests/expiration_integration.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** `liquers-unittest`
- **Knowledge:** Phase 3 examples doc in full; the real signatures landed in Steps 1-3 (may differ
  slightly from Phase 2's sketch in minor ways — e.g. exact private-helper name); existing
  `expiration_integration.rs` content (to avoid naming collisions with its current tests)
- **Rationale:** Requires reconciling Phase 3's aspirational code against whatever Steps 1-3
  actually landed (parameter order, exact error variants), plus judgment on the one sketch test's
  gate timing — not mechanical.

---

### Step 6: Full workspace validation and fallout fixes

**File:** (all touched files; primarily `liquers-core`)

**Action:**
- Run the full validation suite from `plan20260707.md`'s conventions.
- Fix any fallout: other `AssetManager` call sites in `liquers-core` (or, if any exist,
  `liquers-store`/`liquers-lib`) that use a `match` on `Status` without the new methods affecting
  them (the two new trait methods don't change any existing enum, so this should be a non-issue,
  but confirm via `cargo clippy`'s exhaustiveness checks finding nothing new).
- Confirm no other `impl AssetManager<E> for ...` exists outside `DefaultAssetManager` in the
  current codebase (Phase 2's "Integration Points" claim) — if one is found (e.g. a test-only mock
  implementing the trait), it also needs the two new methods.

**Validation:**
```bash
cargo test -p liquers-core
cargo test --workspace --exclude liquers-py
cargo clippy --workspace --exclude liquers-py --all-targets
cargo check -p liquers-py
```

**Rollback:** N/A (this step only fixes fallout from Steps 1-5; if fallout is extensive enough to
suggest a design flaw, stop and return to Phase 2/3 rather than patching around it).

**Agent Specification:**
- **Model:** sonnet
- **Skills:** `liquers-unittest`
- **Knowledge:** All files touched in Steps 1-5; `plan20260707.md`'s "Validation commands"
  convention
- **Rationale:** Diagnosing and fixing any fallout (e.g. an unexpected second trait implementor,
  a clippy lint on the new code) requires judgment about whether the fix is safe or indicates a
  need to revisit the design.

---

### Step 7: Close out specs

**File:** `specs/FEATURES/EXPIRATION-SAFETY.md`, `specs/FEATURES/EXPIRATION-SAFETY-IMPLEMENTATION-PLAN.md`,
`specs/expiration-safety/DESIGN.md`

**Action:**
- `EXPIRATION-SAFETY.md`: change `Status: Draft` to `Status: Closed`; add a short "Implemented via
  `specs/expiration-safety/` (WP-3)" pointer, noting the acceptance criteria this WP adds beyond
  the original doc (weak-ref monitor, `get_any_status`/`to_override` recovery API — the original
  doc's own scope, e.g. earliest-deadline-wins tracking and status-aware eviction, was already
  implemented before this WP started, confirmed by code audit in Phase 1).
- `EXPIRATION-SAFETY-IMPLEMENTATION-PLAN.md`: mark phases as complete; note that Phases 1-2
  (duplicate-scheduling removal, monitor state normalization) were already done prior to this WP.
- `specs/expiration-safety/DESIGN.md`: mark all phases complete, add final implementation notes/
  links to the merged PR.

**Validation:**
```bash
git diff specs/FEATURES/EXPIRATION-SAFETY.md specs/FEATURES/EXPIRATION-SAFETY-IMPLEMENTATION-PLAN.md specs/expiration-safety/DESIGN.md
# Expected: status fields updated, no other content regressions
```

**Rollback:**
```bash
git checkout specs/FEATURES/EXPIRATION-SAFETY.md specs/FEATURES/EXPIRATION-SAFETY-IMPLEMENTATION-PLAN.md specs/expiration-safety/DESIGN.md
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** (none required)
- **Knowledge:** The three files being edited; a short summary of what Steps 1-6 actually shipped
- **Rationale:** Documentation status update following a clear template — no judgment needed.

## Testing Plan

### Unit Tests

**When to run:** After Step 4.

**File:** `liquers-core/src/assets.rs` (`#[cfg(test)] mod tests`)

**Command:**
```bash
cargo test -p liquers-core --lib assets::
```

**Expected:** All new unit tests pass (`test_untrack_releases_strong_ref`,
`test_retrack_earlier_deadline_fires_once`, `test_expire_failure_preserves_processing_asset`);
every existing test in this module (there are many — monitor, persistence, fast-track, etc.)
stays green, since Steps 1-3 are designed as additive/narrowing changes to already-existing
behavior, not rewrites.

### Integration Tests

**When to run:** After Step 5.

**File:** `liquers-core/tests/expiration_integration.rs`

**Command:**
```bash
cargo test -p liquers-core --test expiration_integration
```

**Expected:** All 14 non-`#[ignore]`'d Phase 3 tests pass (17 total minus the 2 deferred plus the
1 sketch counted as passing once its gate is finalized — see Step 5's validation note for exact
caveats). Every pre-existing test in this file (the `test_timed_expiration`,
`test_dependent_expiration`, etc. tests already there) stays green.

### Manual Validation

**When to run:** After Step 6.

```bash
cargo build -p liquers-core
# Expected: builds clean

cargo test -p liquers-core
# Expected: full crate test suite green (unit + all integration test files)

cargo test --workspace --exclude liquers-py
# Expected: green — confirms liquers-store/liquers-lib/liquers-axum are unaffected (Phase 2's
# "no changes outside liquers-core" claim)

cargo clippy --workspace --exclude liquers-py --all-targets
# Expected: no new warnings from this WP's code (pre-existing warnings elsewhere are out of scope)

cargo check -p liquers-py
# Expected: compiles — confirms the two new required AssetManager trait methods don't break the
# Python bindings (they don't wrap AssetManager directly as of this audit, but this is the
# CLAUDE.md-mandated gate whenever a public core trait changes)
```

**Success criteria:** All commands above exit 0; the specific test names listed in "Integration
Tests" above are individually confirmed passing (not just "suite green"), since several encode the
exact WP-3 acceptance criteria (cache-miss semantics, no-double-serialization, dependency
freshness).

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|------|-------|--------|-----------|
| 1 | haiku | — | Mechanical strong-ref → weak-ref substitution, fully specified |
| 2 | sonnet | liquers-unittest | Must preserve exact construction logic while splitting one match arm; must NOT touch Error/Cancelled (scope discipline) |
| 3 | sonnet | liquers-unittest | Core new business logic — `PersistenceStatus` branching, two-path algorithm. **Not haiku-safe.** |
| 4 | sonnet | liquers-unittest | Timing-sensitive unit tests need deterministic-test judgment |
| 5 | sonnet | liquers-unittest | Reconciling Phase 3 sketches against Steps 1-3's real signatures |
| 6 | sonnet | liquers-unittest | Fallout triage requires judging safe-fix vs. design-flaw |
| 7 | haiku | — | Status-field doc updates, template-following |

## Rollback Plan

### Per-Step Rollback
Documented individually above; every step touches a small, identifiable set of files and can be
reverted with `git checkout <file>` since no step deletes pre-existing code paths (Steps 1-3 are
additive/narrowing inside `assets.rs`; Steps 4-5 only append tests; Step 7 only edits doc status
fields).

### Full Feature Rollback
```bash
git diff main -- liquers-core/src/assets.rs liquers-core/tests/expiration_integration.rs \
  specs/FEATURES/EXPIRATION-SAFETY.md specs/FEATURES/EXPIRATION-SAFETY-IMPLEMENTATION-PLAN.md \
  specs/expiration-safety/DESIGN.md
git checkout main -- liquers-core/src/assets.rs liquers-core/tests/expiration_integration.rs \
  specs/FEATURES/EXPIRATION-SAFETY.md specs/FEATURES/EXPIRATION-SAFETY-IMPLEMENTATION-PLAN.md \
  specs/expiration-safety/DESIGN.md
```
No new files are created by this WP (all changes are to existing files), and no `Cargo.toml`
changes are needed — full rollback is a pure revert of the files above.

### Partial Completion
```bash
git checkout -b feature/expiration-safety   # if not already on a dedicated branch
git add -A && git commit -m "WIP: expiration-safety - completed steps 1-N"
```
Update `specs/expiration-safety/DESIGN.md`'s "Notes" section with which steps are done; resume by
checking out the branch and continuing from the next unchecked step. Since Steps 1-3 must land
together for the crate to compile at each checkpoint used by CI (Step 3 depends on Step 2's new
`AssetRef::get_any_status` existing, and the trait+impl in Step 3 must be added in the same commit
to stay compilable), the smallest safely-pausable checkpoint is after Step 3, not mid-step.

## Documentation Updates

### CLAUDE.md
**No updates needed** — no new command-registration pattern, no new value type, no new store
backend; this WP only extends an existing trait (`AssetManager`) and an existing internal type
(`TimedAsset`), both already documented patterns in this codebase.

### PROJECT_OVERVIEW.md
**Update recommended, not required:** if `PROJECT_OVERVIEW.md` documents the `AssetManager` trait
surface or the expiration/status lifecycle, add a short note that `Status::Expired` is a cache-miss
for normal access with a separate `get_any_status`/`to_override` recovery path for keyed assets —
this is a user-visible semantic worth surfacing at the architecture-doc level even though it's not
a new "concept" requiring a new section. Defer to whoever executes this plan to check
`PROJECT_OVERVIEW.md`'s current content and decide if a note fits.

### README.md
**No updates needed** — internal library-correctness fix, not a new user-facing feature to
advertise.

### specs/FEATURES/EXPIRATION-SAFETY.md and EXPIRATION-SAFETY-IMPLEMENTATION-PLAN.md
Covered by Step 7 above.

## Execution Options

After Phase 4 approval, the user chooses one of:
1. **Execute now** — implement Steps 1-7 sequentially, running each step's validation before
   moving to the next; Steps 1-3 must be committed together (see Partial Completion note above).
2. **Create task list** — use `TaskCreate` for each of the 7 steps, respecting the ordering
   dependency (3 depends on 1+2 for a clean compile checkpoint; 4-5 depend on 3; 6 depends on 4-5;
   7 is independent and can run anytime).
3. **Revise plan** — return to this Phase 4 document for changes.
4. **Exit** — this document is saved at `specs/expiration-safety/phase4-implementation.md` for
   manual execution.

## Critical Review (self-assessment before multi-agent review)

- All 7 steps have exact file paths, specific actions, validation commands, and agent
  specifications ✅
- Steps ordered so the crate compiles at each checkpoint (1 → 2 → 3 is a hard dependency chain for
  compilability; 4/5 depend on 3; 6 depends on 4/5; 7 is independent) ✅
- Testing plan covers unit + integration + manual, with exact test names, not just "run the suite" ✅
- Rollback plan is a pure `git checkout` at every level (no destructive irreversible step exists) ✅
- Two known gaps carried forward honestly rather than hidden: the `NotPersisted` retry-branch test
  and the `NonSerializable` test remain `#[ignore]`d pending test-double work not resolved in this
  design; the in-flight dependency-race test's gate timing is a sketch, not a finished mechanism.
  **These are execution risks, not design risks** — the underlying `AssetManager`/`AssetRef` code
  changes (Steps 1-3) do not depend on those two tests existing to be correct; they exist to prove
  correctness, and their absence-for-now is a testing-coverage gap explicitly flagged for whoever
  executes Step 5, not a silent one.
- Confidence: **~90%**, not 95%+ — held back specifically by the two deferred tests and the one
  sketch test above, which is why this plan flags them explicitly rather than presenting false
  certainty. The multi-agent review below (especially the opus final reviewer) should weigh in on
  whether 90% is acceptable to proceed or whether Step 5's deferred tests should be resolved as a
  blocking prerequisite before "Execute now" is offered.
