# Phase 4: Implementation Plan - Stale-Dependency Status Finalization

> **Revision 2 (2026-09-15).** Rewritten against HEAD and against Phase 2/3 Revision 2. Revision 1's
> Step 1 (a rename) is gone, its Step 2 lost the `serialize_to_binary` change to an upstream fix, and
> its Step 3 is replaced. Two steps are new: the dependency-manager extraction and the fast-track
> check.

## Overview

**Feature:** Stale-dependency status finalization (`ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY`, P1)

**Architecture:** An asset that consumed a stale dependency is decided `Expired` in
`finalize_status_with_version`, before the notification and before persistence, so the store agrees
with the manager. Because `track_asset` refuses an `Expired` asset and computed assets now carry real
versions, `evaluate` registers the version directly for that case. And `try_fast_track` gains the
reading half: a dependency it can see is expired refuses the fast track.

**Estimated complexity:** Low for the source (about 60 lines across two files), Medium for the tests
— the shared-store fixture and the mid-evaluation gate are where the time goes.

**Estimated time:** 4–6 hours. Steps 1–4 are perhaps an hour; the rest is tests, which is expected
for a defect whose whole nature is that it was invisible.

**Prerequisites:** Phases 1–3 approved (Revision 2). No blocker in the Phase 2 preflight. No
dependency, feature flag or `Cargo.toml` change.

**Line numbers are HEAD at 2026-09-15**, re-verified while writing this plan. They shift as soon as
Step 1 lands, so later steps name *what* to change, not only where.

## Implementation Steps

### Step 1 — The stale-dependency branch in `finalize_status_with_version`

**File:** `liquers-core/src/assets.rs`

**Action:** Add the branch between the volatile and ready arms of `finalize_status_with_version`
(`:1957`). Delete the relabel block from `finish_run_with_result` (`:2409-2422`).

```rust
// inside finalize_status_with_version, where `lock.data.is_some()`:
if should_be_volatile {
    // unchanged
} else if lock.stale_dependency {
    // A dependency expired mid-execution and its stale value was used (see
    // `wait_for_dependency`). The result is fresh but uncacheable: decide `Expired` here,
    // before persistence, so the store agrees and the next access recomputes.
    if let Err(e) = lock.set_status(Status::Expired) {          // status AND metadata — P1
        let _ = lock.metadata.add_log_entry(LogEntry::warning(/* … */));
    }
    let _ = lock.metadata.add_log_entry(LogEntry::warning(
        "Asset evaluated with an expired dependency value; labeled expired \
         for recomputation on next access".to_string(),
    ));
    if let Err(e) = lock.metadata.set_expiration_time_from(&metadata_expires) {  // mirror Ready — P5
        let _ = lock.metadata.add_log_entry(LogEntry::warning(/* … */));
    }
    lock.expiration_time = lock.metadata.expiration_time();
} else {
    // unchanged Ready arm
}
```

**Five ways to get this wrong**, each a Phase 3 pitfall:

- **P1** — go through `lock.set_status(...)`, never `lock.status = ...`. The metadata half *is* the defect.
- **P2** — the warning is written here, under the same lock, not left in the harness.
- **P5** — the two `expiration_time` lines are not optional; the `Ready` arm has them.
- **P6** — do **not** touch `prepared.version`. Staleness is freshness, not content.
- **P10** — `else if`, not a separate `if`. Volatility wins.

Errors are surfaced as `LogEntry::warning`, matching every other arm of this function — not
discarded with a bare `let _ =` on the `Result`.

**Validation:**
```bash
cargo check -p liquers-core
grep -n "stale_dependency" liquers-core/src/assets.rs
# Expected: the field, its initializer, note_expired_dependency, and exactly ONE reader —
# in finalize_status_with_version. A reader left in finish_run_with_result means the delete was missed.
cargo test -p liquers-core --lib test_wait_for_retained_expired_dependency
# Expected: PASSES UNCHANGED — the non-keyed regression guard.
```

**Rollback:** `git checkout liquers-core/src/assets.rs`

**Agent:** sonnet · rust-best-practices · Phase 2 §"Function Signatures", Phase 3 pitfalls
P1/P2/P5/P6/P10, and the existing `Ready` arm to mirror.
*Rationale:* small but load-bearing; all five failure modes are silent.

---

### Step 2 — Extract the keyed registration in `DependencyManager`

**File:** `liquers-core/src/dependencies.rs`

**Action:** Lift the keyed branch of `track_asset` (`:348`) into a `pub(crate)` method so both
callers share one definition. Behaviour unchanged; `track_asset` calls it after its status gate and
`bound_owner_key()` lookup.

```rust
pub(crate) async fn track_keyed_asset(
    &self,
    asset: &crate::assets::AssetRef<E>,
    key: &Key,
    records: &[DependencyRecord],
) -> ExpiredDependents<E>;
```

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core --test keyed_version_cascade
# Expected: green — this step must be a pure refactor.
```

**Rollback:** revert the file; Step 1 stands alone.

**Agent:** haiku · rust-best-practices · the body being extracted.
*Rationale:* mechanical extraction with a test suite that already covers it.

---

### Step 3 — Register the version for a stale-dependency keyed asset

**File:** `liquers-core/src/assets.rs`

**Action:** Extend the post-finalize read (`:2736`) to carry `stale_dependency`, then replace the
DM step (`:2762-2769`) with the three-way branch.

```rust
if lock_is_volatile {
    // unchanged: a volatile asset is not a graph node
} else if stale_dependency {
    // `track_asset` refuses `Expired`, and that refusal would drop the dependent invalidation
    // this asset owes: it holds NEW content with a new version, and is `Expired` only to say
    // "do not cache me". The gate means "no valid value"; this asset has one.
    if let Some(key) = self.bound_owner_key().await.ok().flatten() {   // P7: ownership-aware
        let deps = { self.data.read().await.metadata.get_dependencies().to_vec() };
        let expired = dm.track_keyed_asset(self, &key, &deps).await;
        manager.expire_dependencies_result(expired).await;
    }
} else {
    // unchanged: track_asset + expire_dependencies_result
}
```

- **P7** — `bound_owner_key()`, not `lock.key`. It returns `None` for a keyed non-owner, which is
  what makes a *delegating* stale-dependency asset register nothing, with no special branch.
- **P8** — do not let the gate refuse it; that is the bug this step exists to avoid.
- No `data` lock is held across the DM call — take the facts, release, then call.

**Validation:**
```bash
cargo check -p liquers-core && cargo test -p liquers-core --lib
cargo test -p liquers-core --test keyed_version_cascade --test expiration_integration
```

**Rollback:** revert this hunk only. Steps 1–2 already fix the persisted status; Step 3 is the
dependent-invalidation half and is separable.

**Agent:** sonnet · rust-best-practices · Phase 2 §"The dependency-manager step",
`dependencies.rs:348`, `assets.rs:1741` (`bound_owner_key`).

---

### Step 4a — Write F0 first: the fast-track success baseline

**File:** `liquers-core/tests/keyed_version_cascade.rs`

**Action:** Before touching `try_fast_track`, add `fast_track_succeeds_for_a_ready_stored_asset` and
confirm it passes against unmodified code.

**Why this comes first.** Searching the suite for `try_fast_track` finds one test, and it asserts
the function returns **`false`**. Nothing asserts it can return `true`. A Step 4 that fails closed
would stop fast-tracking entirely and **leave the suite green** — every result still correct, just
recomputed. There is currently nowhere for that failure to land.

Writing F0 first, and seeing it pass before the change, is what turns Step 4 from "hope the review
catches it" into a checkpoint.

**Validation:**
```bash
cargo test -p liquers-core --test keyed_version_cascade fast_track_succeeds
# Expected: PASSES, against code with no Step 4 in it.
```

**Agent:** sonnet · rust-best-practices, liquers-unittest.

---

### Step 4b — Fast-track declines an expired dependency

**File:** `liquers-core/src/assets.rs`

**Action:** In `try_fast_track`'s dependency validation loop (`:1119-1132`), add the status check
beside the existing version check.

```rust
for dep_record in mr.get_dependencies() {
    // existing version check, unchanged
    if let Some(dm_version) = dm.get_version(&dep_record.key).await { /* … */ }

    // NEW. Reuse the same predicate this function applies to the asset itself, so the two
    // cannot drift: a dependency must be in a status we would fast-track from.
    //   1. live in the manager  -> its status is authoritative, no store read
    //   2. otherwise            -> store.get_metadata(&key).status()
    //   3. not determinable     -> INCONCLUSIVE, proceed
}
```

**The rule that keeps this safe (P4):** inconclusive is **not** expired. `Key::try_from(&DependencyKey)`
fails for a command-implementation node such as `ns-dep/command_impl---world`, which nearly every
asset depends on; a missing or unreadable metadata read is equally inconclusive. Fail **open** on
absence, **closed** only on positive evidence. Getting this backwards disables fast-tracking almost
everywhere and presents as a performance collapse, not a failing test.

One level only — a fast-tracked dependency runs this same check on its own dependencies, and
in-process transitivity is the cascade's job. Stop at the first refusal.

**Validation:**
```bash
cargo check -p liquers-core && cargo test -p liquers-core --lib --tests
cargo test -p liquers-core --test keyed_version_cascade fast_track_succeeds
# Expected: green, and F0 still passes. F0 failing here is P4 inverted — the check is
# refusing a dependency it merely cannot determine.
```

**Rollback:** revert this hunk. Steps 1–3 are the writing half and stand without it.

**Agent:** sonnet · rust-best-practices · Phase 2 §"Fast-track must verify…", Phase 3 Example 3.
*Rationale:* the failure mode is silent and systemic rather than local.

---

### Step 5 — Unit tests U1–U8

**File:** `liquers-core/src/assets.rs`, existing `#[cfg(test)] mod tests`

**Binding setup rules** (Phase 3 §"Verified Setup Facts"):

- construct with `AssetData::<SimpleEnvironment<Value>>::new(id, query.into(), None, envref).to_ref()`;
- **install the value under the write lock** — `lock.data = Some(Arc::new(value))`. **Never
  `set_value`**: it sets `Ready`, notifies, *and persists*, which would make U3 assert "before
  persistence" after persisting;
- read status back with `lock.metadata.status()`, not by matching `Metadata`;
- log assertions compare `entry.kind == LogEntryKind::Warning`; there is no `level` field;
- no `_ =>` arms.

**Validation:** `cargo test -p liquers-core --lib finalize` then `cargo test -p liquers-core --lib`

**Agent:** sonnet · rust-best-practices, liquers-unittest.

---

### Step 6 — The second environment, and a counting store

**File:** `liquers-core/tests/`

**Prefer re-hydration over a shared store.** `test_get_any_status_and_to_override_from_store_only`
(`expiration_integration.rs:1336`) already builds an independent second environment by reading the
bytes and metadata out of the first store, dropping the first environment entirely, and `set`-ing
those bytes into a fresh `AsyncMemoryStore`. No wrapper, already in the tree, already working. Use
it for I1 and F1.

**Build only what re-hydration cannot give: a read counter for F4.**

```rust
#[derive(Clone)]
struct CountingStore { inner: Arc<AsyncMemoryStore>, metadata_reads: Arc<AtomicUsize> }
```

Delegate the two **required** methods (`get`, `set_metadata`) **and** the three `AsyncMemoryStore`
**overrides** (`set`, `contains`, `remove`) — delegating only the required pair compiles and then
behaves differently from the store it wraps. `ToOverrideGateStore` (`:880`) is the proven shape.

**On `CROSS-PROCESS-RELOAD-IS-UNTESTED`:** re-hydration is a snapshot, not genuine sharing, so
whether this closes that issue depends on what it asks for. Read it before claiming the close; if it
wants concurrent access to one store, say so and leave it open.

**Agent:** haiku · rust-best-practices · `ToOverrideGateStore`, the `AsyncStore` trait.

---

### Step 7 — Integration tests I1–I8 and F1–F4

**Files:** `liquers-core/tests/expiration_integration.rs` (I1–I8),
`liquers-core/tests/keyed_version_cascade.rs` (F1–F4, which already owns `chain_env`)

**Reuse `chain_env` (`keyed_version_cascade.rs:36`)** — the three-link chain, the counting command
and the non-serializable case already exist. This design introduces no new query strings.

**Three things decide whether these tests are worth anything:**

1. **Force the mid-evaluation window.** Copy `test_dependency_expiring_during_parent_evaluation_is_allowed`
   (`expiration_integration.rs:748`): the parent holds a `oneshot`, reads its dependency, then
   blocks; the test **polls until the child is `Ready`** (bounded, 200 × 2 ms) before expiring it.
   A `sleep` takes the scheduling-time path sometimes and passes for the wrong reason.
2. **I1 asserts a counter, not a value.** The recomputed value equals the stale one.
3. **`get().await` before every `status()`** — `evaluate()` can return while `Processing`.

**Two checkpoints that prove the tests test something.** Before this step is complete, run each
against a build with the relevant source step stashed:

```bash
git stash && cargo test -p liquers-core --test expiration_integration keyed_stale_dependency_is_stored_expired
# Expected: FAILS — stored status is Ready. Then `git stash pop` and confirm it passes.
git stash && cargo test -p liquers-core --test keyed_version_cascade fast_track_declines_a_dependency_expired_in_the_store
# Expected: FAILS. Same pop-and-confirm.
```

A test green both before and after is testing nothing, and this is the cheapest way to find out.

**Agent:** sonnet · rust-best-practices, liquers-unittest.

---

### Step 8 — Documentation and issue bookkeeping

**Files:** `specs/reference/ASSET_LIFECYCLE.md`, `specs/reference/ASSETS.md`,
`specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`, `liquers-core/src/assets.rs` module
rustdoc, `specs/issues/ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY.md`,
`specs/issues/CROSS-PROCESS-RELOAD-IS-UNTESTED.md`

Per Phase 2 §"Documentation Architecture". Each reference gets a `## History` row and a `reviewed:`
bump **in the same commit** (§9.2). `ASSETS.md` §Expiry must say the asset is *born* expired rather
than relabelled, and the fast-track rule belongs in `ASSET_LIFECYCLE.md` beside the load path.

**Validation:** `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`
— expected 0 errors, warning count unchanged.

**Agent:** sonnet · Phase 2 §"Documentation Architecture", `DOCS_STRUCTURE_GUIDE.md` §9.2.

---

### Step 9 — Full validation

```bash
cargo test -p liquers-core --lib --tests
cargo test -p liquers-lib --no-default-features --lib --tests   # see note
cargo check -p liquers-py
bash scripts/check-build-matrix.sh
cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

**Note on `liquers-lib`.** `cargo test -p liquers-lib --lib --tests` does not run on this
toolchain (rustc 1.94.1) — `BUILD-SYSINFO-REQUIRES-NEWER-RUSTC`. Its 2026-09-06 update records the
`--no-default-features` loop as the working substitute, which `keyed-expiry-cascade-fix` used for
the same reason. Use it, and **say so in the PR** rather than reporting a clean `liquers-lib` run
that did not happen.

The wasm loop matters because `assets.rs` is shared with the inline path: the one way this change
reaches wasm is a `tokio::` primitive slipping into a new branch, which it must not.

**Agent:** haiku · `CLAUDE.md` §"Building and testing".

## Testing Plan

| When | Command | Expected |
|---|---|---|
| After Step 1 | `cargo test -p liquers-core --lib test_wait_for_retained_expired_dependency` | **Passes unchanged** — the non-keyed guard |
| After Step 2 | `cargo test -p liquers-core --test keyed_version_cascade` | Green — the extraction is a pure refactor |
| Before Step 4b | `cargo test … fast_track_succeeds` | **Passes on unmodified code** — the baseline, established before the change |
| After Step 4b | `cargo test -p liquers-core --lib --tests` + F0 | Green, F0 still passing. F0 failing is P4 inverted |
| After Step 5 | `cargo test -p liquers-core --lib` | 8 new unit tests pass |
| During Step 7 | the two stash checkpoints | **Fail**, then pass |
| After Step 7 | `cargo test -p liquers-core --tests` | I1–I8 and F1–F4 pass |
| After Step 8 | `docs_index.py --check` | 0 errors |
| After Step 9 | the matrix above | Green, wasm included |

**No manual validation.** There is no binary to run and no query whose output changes; the entire
observable difference is a status in stored metadata and a refused fast track, both asserted
directly. Saying so beats inventing a ritual command.

## Task Splitting (Agent Assignment)

| Step | Model | Skills | Rationale |
|---|---|---|---|
| 1 Finalization branch | sonnet | rust-best-practices | Five silent failure modes |
| 2 DM extraction | haiku | rust-best-practices | Mechanical; covered by an existing suite |
| 3 Register directly | sonnet | rust-best-practices | Ownership-aware key derivation; deliberate behaviour change |
| 4a F0 baseline | sonnet | rust-best-practices, liquers-unittest | Must exist before 4b, or 4b has no failure mode |
| 4b Fast-track check | sonnet | rust-best-practices | Failure mode is systemic and silent |
| 5 Unit tests | sonnet | rust-best-practices, liquers-unittest | The setup traps |
| 6 Shared store | haiku | rust-best-practices | Copying a proven local pattern |
| 7 Integration tests | sonnet | rust-best-practices, liquers-unittest | Subtle timing; failure mode is a false pass |
| 8 Documentation | sonnet | — | Prose that must be true |
| 9 Validation | haiku | — | Running listed commands |

No step needs opus: Phase 2 closed every architectural question.

## Rollback Plan

Ordered so each prefix is coherent:

- **Steps 1–2** fix the persisted status. The defect is closed.
- **Step 3** adds dependent invalidation; revertible alone without reopening the bug.
- **Steps 4a–4b** add the reading half; revertible alone, leaving the writing half intact.
- **Steps 5–7** add no source behaviour; reverting loses coverage, not correctness.

If Step 3 or Step 4 proves wrong in review, revert that hunk and file the gap rather than widening
the change. If Step 1 cannot be made to work as specified, the design's premise is wrong and the
correct move is Phase 2, not a workaround here.

## Phase 5 Entry Criteria

Phase 5 is **mandatory** for `workflow: liquers-project`. It starts when Steps 1–9 are complete and
green, every review comment is answered, and nothing in the Definition of Done is outstanding.

Phase 5 owns four things this phase does not:

1. The one-to-three-page summary of what was actually implemented, with deviations and reasons.
2. Closing `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY` and `CROSS-PROCESS-RELOAD-IS-UNTESTED` with
   resolution notes (§4.3). **Step 8 corrects the first issue's text but does not close it** — an
   issue closes when the work is done and validated, not when the plan says it will be.
3. Re-reviewing the three references against the behaviour that shipped, not this plan's account.
4. Deciding whether Phase 3's Verified Setup Facts belong in `specs/guides/UNITTEST_GUIDE.md`.

`EXPIRY-RECORDS-NO-REASON` stays open: separate work, for which this design only sets the ordering
precedent and the recommended shape.

## Definition of Done

- [ ] Exactly one reader of `stale_dependency`, in `finalize_status_with_version`
- [ ] `test_wait_for_retained_expired_dependency_labels_asset_expired_on_completion` passes
      **unchanged** — not edited to agree
- [ ] `keyed_expiry_cascades_to_keyed_dependents` still green
- [ ] Both stash checkpoints observed failing, then passing
- [ ] F0 passed on unmodified code before Step 4b was written, and still passes after
- [ ] F3 passes: an inconclusive dependency still fast-tracks
- [ ] Step 9's matrix green, wasm included, with the `liquers-lib` substitution stated
- [ ] Three references updated with `## History` rows and `reviewed:` bumps
- [ ] `docs_index.py --check` at 0 errors
- [ ] Phase 5 entered
