# Phase 4: Implementation Plan - Stale-Dependency Status Finalization

## Overview

**Feature:** Stale-dependency status finalization (`ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY`, P1)

**Architecture:** The stale-dependency rule moves from the run harness into the status authority,
which is renamed `try_to_set_ready` → `finalize_status`, so the status is final before the
`ValueProduced` notification and before persistence. `evaluate`'s dependency-manager step becomes a
three-way branch: volatile → nothing; stale-dependency + keyed → `cascade_expire_dependents`;
otherwise → `track_asset` as today.

**Estimated complexity:** Low for the source change (roughly 40 lines across one file), Medium for
the tests — two of them need machinery that does not exist yet (`SharedMemoryStore`) or timing that
is easy to get silently wrong (the mid-evaluation gate).

**Estimated time:** 3–5 hours. The ratio is the point: Steps 1–4 are perhaps 45 minutes, and the
rest is tests. That is expected for a defect whose whole nature is that it was invisible.

**Prerequisites:** Phases 1–3 approved; all gate questions resolved (DM branch = cascade, priority
P1, rename confirmed). No unresolved blocker in the Phase 2 preflight. No dependency, feature flag
or `Cargo.toml` change anywhere in the workspace.

**Line numbers below are HEAD at 2026-09-04** and were re-verified immediately before writing this
plan. They shift as soon as Step 1 lands, so later steps name *what* to change, not only where.

## Implementation Steps

### Step 1 — Rename `try_to_set_ready` to `finalize_status`

**File:** `liquers-core/src/assets.rs`

**Action:** Rename the definition (`:1818`) and both call sites (`:2224` in
`finish_run_with_result`, `:2553` in `evaluate`). Update the rustdoc to describe four outcomes
rather than one. No behaviour change in this step — it is deliberately separated so that Step 2's
diff shows only the rule.

```rust
/// Decide and install this asset's terminal status — the single status authority.
///
/// Produces exactly one of `Volatile`, `Expired`, `Ready` or `Error`. Runs **before** the
/// `ValueProduced` notification and **before** persistence, so nothing observes or stores a
/// non-final status (`ASSET_LIFECYCLE.md` §"the one evaluation path", step 6).
async fn finalize_status(&self) { /* body unchanged in this step */ }
```

**Validation:**
```bash
cargo check -p liquers-core
grep -rn "try_to_set_ready" liquers-core/ liquers-lib/ liquers-axum/ liquers-web/ liquers-py/
# Expected: compiles; grep returns nothing. A surviving occurrence is Phase 3 pitfall P3.
```

**Rollback:** `git checkout liquers-core/src/assets.rs`

**Agent:** haiku · skills: none · knowledge: the three line numbers above.
*Rationale:* a mechanical rename of a private method with two call sites; the compiler is the check.

---

### Step 2 — Move the stale-dependency rule into `finalize_status`

**File:** `liquers-core/src/assets.rs`

**Action:** Add the branch between the volatile and ready arms, and move the warning into it.
Delete the block at `:2249-2261` in `finish_run_with_result` (the `if lock.stale_dependency &&
lock.status == Status::Ready` block, at `:2253`) together with its comment.

```rust
// inside finalize_status, where `lock.data.is_some()`:
if should_be_volatile {
    // unchanged
} else if lock.stale_dependency {
    // A dependency expired mid-execution and its stale value was used (see
    // `wait_for_dependency`). The result is fresh but uncacheable: label it `Expired`
    // here, before persistence, so the store agrees and the next access recomputes.
    let _ = lock.set_status(Status::Expired);           // status AND metadata — P1
    let _ = lock.metadata.add_log_entry(LogEntry::warning(
        "Asset evaluated with an expired dependency value; labeled expired \
         for recomputation on next access".to_string(),
    ));
    let _ = lock.metadata.set_expiration_time_from(&metadata_expires);  // mirror Ready — P5
    lock.expiration_time = lock.metadata.expiration_time();
} else {
    // unchanged Ready arm
}
```

**Three things this step must not get wrong**, each a Phase 3 pitfall:

- **P1** — go through `lock.set_status(...)`, never `lock.status = ...`. The metadata half is the
  entire defect; setting only the field reproduces it one layer down.
- **P2** — the warning is added *here*, under the same lock, not left behind in the harness.
- **P5** — the two `expiration_time` lines are not optional; the `Ready` arm has them and the
  scheduling step in `finish_run_with_result` reads what they set.
- **P10** — `else if`, not a separate `if`. Volatility wins.

**Validation:**
```bash
cargo check -p liquers-core
grep -n "stale_dependency" liquers-core/src/assets.rs
# Expected: the field (:584), its initializer (:955), note_expired_dependency, and exactly
# ONE reader — in finalize_status. A reader still in finish_run_with_result means the delete
# was missed.
cargo test -p liquers-core --lib test_wait_for_retained_expired_dependency
# Expected: PASSES UNCHANGED. It uses a non-keyed asset, so Steps 1-2 must not disturb it.
```

**Rollback:** `git checkout liquers-core/src/assets.rs` and redo Step 1 alone.

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 2 §"The decision inside
`finalize_status`", Phase 3 pitfalls P1/P2/P5/P10, and the existing `Ready` arm to mirror.
*Rationale:* small but load-bearing; the four ways to get it wrong are all silent.

---

### Step 3 — The dependency-manager branch in `evaluate`

**File:** `liquers-core/src/assets.rs`

**Action:** Extend the post-finalize read (`:2555-2566`) to also take `stale_dependency`, then
replace the `if !lock_is_volatile { track_asset }` block (`:2580-2586`) with the three-way branch.

```rust
let (save_in_background, cancelled, lock_is_volatile, stale_dependency) = {
    let lock = self.data.read().await;
    let _ = lock.notification_tx.send(AssetNotificationMessage::ValueProduced);
    (lock.save_in_background, lock.is_cancelled(), lock.is_volatile, lock.stale_dependency)
};

// … persistence, unchanged …

// The DM step. `track_asset` refuses `Expired`, so a stale-dependency asset would silently
// skip registration *and* the dependent invalidation that registration performs as a side
// effect. Cascading keeps the second without the first: an uncacheable value must not be
// advertised as this key's current version.
if lock_is_volatile {
    // unchanged: a volatile asset is not a graph node
} else if stale_dependency {
    let key = { self.data.read().await.key.clone() };          // P7: keyed only
    if let Some(key) = key {
        let envref = self.get_envref().await;
        envref
            .get_asset_manager()
            .cascade_expire_dependents(&DependencyKey::from(&key))
            .await;                                             // P6: no data lock held
    }
} else {
    // unchanged: track_asset + expire_dependencies_result
}
```

- **P6** — the `key` read takes and releases its own short-lived read guard. No `data` lock may be
  held across `cascade_expire_dependents`, which takes the DM's `expiration_lock`.
- **P7** — a non-keyed asset does nothing. There is no key to cascade on.
- **P8** — `cascade_expire_dependents`, never `expire()` or `mark_expired_status()`. The latter
  writes metadata only `if store.contains(&key)`, which is false at this point in the run.

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core --lib
# Expected: green, including test_wait_for_retained_expired_dependency_labels_asset_expired_on_completion
```

**Rollback:** revert this hunk only; Steps 1–2 stand alone and already fix the persisted status.

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 2 §"The dependency-manager
branch in `evaluate`", `dependencies.rs:282` (`track_asset`'s status gate), `assets.rs:3960`
(`cascade_expire_dependents`).
*Rationale:* lock discipline and a deliberate behaviour change; not pattern-following.

---

### Step 4 — Unit tests U1–U7

**File:** `liquers-core/src/assets.rs`, existing `#[cfg(test)] mod tests`

**Action:** Add the seven tests from Phase 3's unit table.

**Binding setup rules** (Phase 3 §"Verified Setup Facts" — these are what three drafts got wrong):

- Construct with `AssetData::<SimpleEnvironment<Value>>::new(id, query.into(), None, envref).to_ref()`.
- **Install the value under the write lock** — `lock.data = Some(Arc::new(value))` — the way
  `evaluate` does. **Do not use `set_value`**: it sets `Ready`, notifies, *and persists*
  (`:3330`), which would make U3 assert "before persistence" after persisting.
- Read status back with `lock.metadata.status()` (`metadata.rs:1966`), not by matching `Metadata`.
- Log assertions compare `entry.kind == LogEntryKind::Warning`; there is no `level` field.
- No `_ =>` arms anywhere.

**Validation:**
```bash
cargo test -p liquers-core --lib finalize_status
cargo test -p liquers-core --lib
# Expected: seven new tests pass; nothing else changes.
```

**Rollback:** delete the added tests; Steps 1–3 are unaffected.

**Agent:** sonnet · skills: rust-best-practices, liquers-unittest · knowledge: Phase 3's unit table
and Verified Setup Facts, plus the neighbouring tests in the module for style.
*Rationale:* the setup traps are exactly what a fast agent walked into three times.

---

### Step 5 — `SharedMemoryStore`, the test-only shared store

**File:** `liquers-core/tests/expiration_integration.rs`

**Action:** Add a `#[derive(Clone)]` wrapper holding `inner: Arc<AsyncMemoryStore>` and delegating
`AsyncStore`. `AsyncMemoryStore` owns its `scc::HashMap` (`store.rs:609`) and is **not** shareable
by cloning, so this is what lets two environments see one store.

```rust
#[derive(Clone)]
struct SharedMemoryStore {
    inner: Arc<AsyncMemoryStore>,
}

#[async_trait]
impl AsyncStore for SharedMemoryStore {
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> { self.inner.get(key).await }
    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        self.inner.set_metadata(key, metadata).await
    }
    // Every other method has a default implementation. Delegate any that the tests exercise
    // (`set`, `contains`, `remove`) — the defaults route through `get`/`set_metadata` and are
    // not necessarily what a memory store should do.
}
```

`AsyncStore` has **two required methods** and twenty defaulted. `ToOverrideGateStore` (`:880`) is
the proven precedent for this exact shape.

**Validation:**
```bash
cargo test -p liquers-core --test expiration_integration
# Expected: compiles and the existing suite is unaffected (nothing uses the new type yet).
```

**Rollback:** delete the struct.

**Agent:** haiku · skills: rust-best-practices · knowledge: `ToOverrideGateStore` at
`expiration_integration.rs:880`, the `AsyncStore` trait.
*Rationale:* copying a proven local pattern; the compiler catches a missed method.

---

### Step 6 — Integration tests I1–I7

**File:** `liquers-core/tests/expiration_integration.rs`

**Action:** Add the seven scenarios from Phase 3, each generic over the environment with
`*_default` / `*_immediate` wrappers, per `manager_parametric.rs`.

**Two things decide whether these tests are worth anything:**

1. **The mid-evaluation window must be forced, not hoped for.** Copy
   `test_dependency_expiring_during_parent_evaluation_is_allowed` (`:749`): the parent holds a
   `tokio::sync::oneshot`, reads its dependency through `context.get_dependency_state()`, then
   blocks; the test **polls until the child is `Ready`** (bounded, 200 × 2 ms) before expiring it
   and releasing the gate. The bounded poll is positive proof the parent already took the value.
   A `sleep` instead will sometimes take the scheduling-time path and pass for the wrong reason.
   Assert the parent is `Expired` early in each scenario so a missed window fails there.
2. **I1 asserts an evaluation counter, not a value.** The recomputed value equals the stale value,
   so a value assertion passes whether or not the fix works. Use an `Arc<AtomicUsize>` incremented
   in the command, reset before the second environment's request.

Scenario bodies take `envref: EnvRef<E>` — `Environment` has no `new()`. `register_command!`
needs `type CommandEnvironment` in scope. Recipes are built with `RecipeList` + `Recipe::new` +
`serde_yaml::to_string`, as at `:1010`. Volatility for I7 is declared
`register_command!(cr, fn vol_cmd() -> result volatile: true)?`.

**Validation:**
```bash
cargo test -p liquers-core --test expiration_integration
# Expected: all 14 new integration tests pass, existing suite green.
```

**Checkpoint that proves the fix.** Before Step 6 is complete, run I2 against a build with Steps
2–3 reverted:
```bash
git stash && cargo test -p liquers-core --test expiration_integration scenario_keyed_stale_dependency_is_stored_expired
# Expected: FAILS — stored status is Ready. Then `git stash pop` and confirm it passes.
```
A test that passes both before and after is testing nothing; this is the cheapest way to find that
out.

**Rollback:** delete the added scenarios.

**Agent:** sonnet · skills: rust-best-practices, liquers-unittest · knowledge: Phase 3's
integration table, the gate test at `:749`, `manager_parametric.rs`'s parametric shape.
*Rationale:* the timing is subtle and the failure mode is a false pass.

---

### Step 7 — Documentation and issue bookkeeping

**Files:** `specs/reference/ASSET_LIFECYCLE.md`, `specs/reference/ASSETS.md`,
`specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`, `liquers-core/src/assets.rs`
(module rustdoc), `specs/issues/ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY.md`

**Action:** Per Phase 2's documentation plan.

- `ASSET_LIFECYCLE.md` step 6: name the four outcomes `finalize_status` decides between, including
  the stale-dependency one; add the DM branch to step 8.
- `ASSETS.md` §Expiry (`:241-244`): retarget from `finish_run_with_result` to `finalize_status`,
  and say such an asset is *born* expired rather than relabelled — which is what keeps the
  `*_any_status` / `to_override` sentence beside it correct.
- `DOC_03` (`:246-248`): add that the store agrees, since the next sentence is about what manager
  access does.
- Module rustdoc in `assets.rs`: the read-exposure table's `Expired` row names the old location.
- The issue: correct its four pre-consolidation citations.

Each reference gets a `## History` row and a `reviewed:` bump **in the same commit** (§9.2).

**Validation:**
```bash
python3 scripts/docs_index.py && python3 scripts/docs_index.py --check
# Expected: 0 errors. Warning count unchanged from before this work.
```

**Rollback:** `git checkout specs/`

**Agent:** sonnet · skills: none · knowledge: Phase 2 §"Documentation Architecture", the three
target sections, `DOCS_STRUCTURE_GUIDE.md` §9.2.
*Rationale:* prose that must be true; a wrong reference is worse than none.

---

### Step 8 — Full validation

```bash
cargo test -p liquers-core --lib --tests
cargo test -p liquers-lib --lib --tests
cargo check -p liquers-py
bash scripts/check-build-matrix.sh
cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

`liquers-lib` and `liquers-py` are compile-only confirmations that nothing public moved. The wasm
loop matters because `assets.rs` is shared with the inline path — the one way this change could
reach wasm is a `tokio::` primitive slipping into the new branch, which it must not.

**Agent:** haiku · skills: none · knowledge: `CLAUDE.md` §"Building and testing".

## Testing Plan

| When | Command | Expected |
|---|---|---|
| After Step 1 | `cargo check -p liquers-core` + the `try_to_set_ready` grep | Compiles; grep empty |
| After Step 2 | `cargo test -p liquers-core --lib test_wait_for_retained_expired_dependency` | **Passes unchanged** — the non-keyed regression guard |
| After Step 4 | `cargo test -p liquers-core --lib` | 7 new unit tests pass, no regressions |
| During Step 6 | I2 with Steps 2–3 stashed | **Fails**, then passes — proves the test tests the fix |
| After Step 6 | `cargo test -p liquers-core --test expiration_integration` | 14 new integration tests pass |
| After Step 7 | `docs_index.py --check` | 0 errors |
| After Step 8 | the full matrix above | Green, wasm included |

**No manual validation.** There is no binary to run and no query whose output changes: the entire
observable difference is a status byte in a stored sidecar, which I2 asserts directly. Saying so is
better than inventing a ritual command.

## Task Splitting (Agent Assignments)

| Step | Model | Skills | Rationale |
|---|---|---|---|
| 1 Rename | haiku | — | Mechanical; compiler-checked |
| 2 Move the rule | sonnet | rust-best-practices | Four silent failure modes (P1, P2, P5, P10) |
| 3 DM branch | sonnet | rust-best-practices | Lock discipline; deliberate behaviour change |
| 4 Unit tests | sonnet | rust-best-practices, liquers-unittest | The setup traps that defeated three drafts |
| 5 `SharedMemoryStore` | haiku | rust-best-practices | Copying a proven local pattern |
| 6 Integration tests | sonnet | rust-best-practices, liquers-unittest | Subtle timing; failure mode is a false pass |
| 7 Documentation | sonnet | — | Prose that must be true |
| 8 Validation | haiku | — | Running listed commands |

No step needs opus: there is no cross-crate reasoning and no open architectural question — Phase 2
closed all four.

## Rollback Plan

The steps are ordered so that each prefix is a coherent state:

- **After Steps 1–2** the defect is fixed and the store is correct. Step 3 is a separate concern
  (dependent invalidation) and can be reverted alone without reopening the bug.
- **After Step 4** the fix is proven in-process.
- **Steps 5–6** add no source behaviour; reverting them loses coverage, not correctness.

If Step 3 proves wrong in review — the broader cascade turns out to be too aggressive — revert that
hunk only and file the invalidation gap as an issue, per the Phase 2 alternative the owner
considered and rejected. That is the one place this plan can partially retreat without returning to
Phase 2.

If Step 2 cannot be made to work as specified, the design's premise is wrong and the correct move
is Phase 2, not a workaround in Phase 4.

## Phase 5 Entry Criteria

Phase 5 is **mandatory** for `workflow: liquers-project`. It starts when, and only when:

- Steps 1–8 are complete and Step 8's matrix is green;
- every review comment on the implementation is answered or incorporated — including anything the
  Claude Approvals check raises, if the repository runs it on the PR;
- nothing in the Definition of Done below is outstanding.

Phase 5 then owns four things this phase deliberately does not:

1. The one-to-three-page summary of what was actually implemented, and any deviation from this plan
   with its reason.
2. Closing `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY` with a resolution note (§4.3). **Step 7
   corrects the issue's stale citations but does not close it** — an issue is closed when the work
   is done and validated, not when the plan says it will be.
3. Re-reviewing the three references against the behaviour that actually shipped, rather than
   against this plan's description of it.
4. Deciding whether the Phase 3 testing knowledge — `set_value` persists; `AsyncMemoryStore` is not
   shareable by cloning; a cross-process test must count evaluations — belongs in
   `specs/guides/UNITTEST_GUIDE.md`. Phase 3 recommends it; Phase 5 decides, since by then it will
   be clear whether the knowledge generalizes or was specific to this change.

`EXPIRY-RECORDS-NO-REASON` stays open regardless: it is separate work, and this design's Step 2
only establishes the ordering precedent it must follow.

## Definition of Done

- [ ] `try_to_set_ready` appears nowhere in the workspace
- [ ] Exactly one reader of `stale_dependency`, in `finalize_status`
- [ ] I2 fails with Steps 2–3 reverted and passes with them
- [ ] `test_wait_for_retained_expired_dependency_labels_asset_expired_on_completion` passes
      **unchanged** — not edited to agree
- [ ] Step 8's full matrix green, wasm included
- [ ] Three references updated with `## History` rows and `reviewed:` bumps
- [ ] `docs_index.py --check` at 0 errors
- [ ] Phase 5 entered — it is mandatory for `workflow: liquers-project`, and the issue's status is
      settled there, not here
