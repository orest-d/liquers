# Phase 4: Implementation Plan - expiration-safety

## Overview

**Feature:** expiration-safety (WP-3, `plan20260707.md`)

**Architecture:** Three targeted changes confined to `liquers-core/src/assets.rs` (per Phase 2):
weak-ref monitor tracking, a `poll_state()` fix paired with a new `*_any_status` read family, and
two new **shared default** `AssetManager<E>` trait methods (`get_any_status`, `to_override`) —
written once on the trait, requiring no per-manager implementation. Tests land in
`liquers-core/tests/expiration_integration.rs` (integration) and `assets.rs`'s own
`#[cfg(test)] mod tests` (unit, monitor internals). No new crates, no new public surface outside
`liquers-core`.

**`async-wasm-refactor` sync note (added after this plan's initial approval, before execution):**
`async-wasm-refactor` — an independent, parallel effort — merged into `main` while this branch was
open (`liquers-core/src/assets.rs`: +1153/-279 lines). Re-audit findings, folded into this
document:
- A second `AssetManager<E>` implementor, `ImmediateAssetManager<E>` (wasm/browser), now exists,
  confirming the concern Phase 1/2 anticipated. It has no monitor/timer — expiration is checked
  lazily on access — so **Step 1 stays exactly as scoped, `DefaultAssetManager`-only**.
- New trait primitives (`lookup_key_asset`, `get_envref`, `insert_key_asset`) let **Step 3** write
  `get_any_status`/`to_override` as **one shared default method each** instead of duplicating a
  required implementation across both managers — strictly less work than originally planned.
- **All line-number citations below are refreshed** against the post-merge file. The underlying
  algorithm in Step 3 is unchanged; only where things live in the file, and how many places need
  editing, changed (for the better).

**Estimated complexity:** Medium-Low (mechanical monitor change + one focused piece of new
business logic — the `PersistenceStatus` branching in `to_override`, written once — inside an
already-large, well-tested file). Lower than the original estimate: Step 3 no longer requires a
second, duplicate implementation for `ImmediateAssetManager`.

**Estimated time:** 3-5 hours for an experienced Rust developer familiar with this codebase.

**Prerequisites:**
- Phases 1, 2, 3 approved ✅
- Both Phase 1 open questions resolved (trait placement, override-persistence branching) ✅ —
  Question 1's answer was later refined (shared default, not required-per-manager) after
  `async-wasm-refactor` landed; see the sync note above and Phase 1/2's own sync notes.
- No new dependencies — `async-trait`, `tokio`, `chrono` already in `liquers-core`'s `Cargo.toml` ✅
- `rust-best-practices` skill **is now installed** in this environment (it was not when this plan
  was first drafted). It should be invoked directly for any further revision of this plan or its
  execution, superseding the manual idiom-checking noted in the original draft.

## Implementation Steps

### Step 1: Weak-ref expiration monitor

**File:** `liquers-core/src/assets.rs`

**Action:**
- Change `TimedAsset<E>.asset_ref` field type from `AssetRef<E>` to `WeakAssetRef<E>` (struct at
  `assets.rs:2969`, `#[cfg(not(target_arch = "wasm32"))]`-gated — native-only, unaffected by wasm
  concerns).
- At both `Track` message construction sites inside `run_expiration_monitor` (`:3112`, `:3217`),
  downgrade at heap-insertion time: `asset_ref: asset_ref.downgrade()`. The
  `ExpirationMonitorMessage::Track` message itself is unchanged (still carries a strong
  `AssetRef<E>` — the sender already owns one; only the heap entry downgrades).
- At the one fire site (inside the `while let Some(Reverse(timed)) = heap.peek()` loop in the
  timer-fire `select!` arm, `:3142`), replace the direct move `let asset_ref = timed.asset_ref;`
  with:
  ```rust
  let Some(asset_ref) = timed.asset_ref.upgrade() else {
      // Asset already dropped elsewhere (no strong refs remain) — nothing to expire or evict.
      continue;
  };
  ```
  (There is only one fire site, not two — the `Track`/`Untrack` handling is duplicated across the
  two `select!` arms, one per whether the heap is currently empty, but firing only happens in the
  timer-fire arm.)

**Code changes:**
```rust
// MODIFY (assets.rs:2969-2973):
struct TimedAsset<E: Environment> {
    expiration: chrono::DateTime<chrono::Utc>,
    asset_id: u64,
    asset_ref: WeakAssetRef<E>,   // was: AssetRef<E>
}

// MODIFY (both Track construction sites, :3112 and :3217):
heap.push(Reverse(TimedAsset {
    expiration: dt,
    asset_id,
    asset_ref: asset_ref.downgrade(),   // was: asset_ref
}));

// MODIFY (the one fire site, :3142, inside the timer-fire select! arm):
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
- **Skills:** `rust-best-practices` (now installed)
- **Knowledge:** Phase 2 architecture doc's "Expiration Monitor Fire Logic" section; the exact
  current code at `assets.rs:2969-3234` (`run_expiration_monitor`, both `select!` arms — note only
  the `Track`/`Untrack` handling is duplicated between arms; the actual firing logic lives only in
  the timer-fire arm, so there is exactly one fire site to change, not two)
- **Rationale:** Small, self-contained, mechanical substitution with a clearly specified before/
  after; no architectural judgment needed.

---

### Step 2: `poll_state` fix + new `*_any_status` read family

**File:** `liquers-core/src/assets.rs`

**Action:**
- In `AssetData::poll_state()` (`:619-659`), move `Status::Expired` out of the
  `Ready | Expired | Source | Override | Volatile` arm into its own arm returning `None`.
- Add `AssetData::poll_state_any_status(&self) -> Option<State<E::Value>>` immediately after
  `poll_state()`: for `Status::Expired`, build the state the same way the old `poll_state` arm did
  (the exact `metadata.with_type_identifier`/`with_type_name` + `State::from_parts` construction
  currently at `:648-653`); for every other status, delegate to `self.poll_state()`.
- Add `AssetRef::poll_state_any_status(&self) -> Option<State<E::Value>>` immediately after the
  existing async `poll_state()` wrapper (`:2174-2177`), mirroring its `self.data.read().await...`
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
// MODIFY (assets.rs:619-659), split Status::Expired out:
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

// NEW, placed directly after the async poll_state() wrapper on AssetRef (:2174-2177):
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
- **Skills:** `rust-best-practices`, `liquers-unittest`
- **Knowledge:** Phase 2 architecture doc's "New `AssetData`/`AssetRef` methods" section; current
  `assets.rs:619-659` (`poll_state`) and `:2174-2177` (async wrapper); `AssetRef::get()` at
  `:2084` onward, notification-wait `Expired` arm at `:2129` (read-only — confirm no change
  needed, do not edit)
- **Rationale:** Requires care to preserve the exact existing construction logic for the `Expired`
  arm verbatim inside the new method (a haiku model is more likely to subtly alter the metadata
  construction); also must NOT touch the `Error | Cancelled` arm (WP-2 territory) — this
  discipline needs judgment, not just pattern-copying.

---

### Step 3: `AssetManager` shared default methods

**File:** `liquers-core/src/assets.rs`

**Action:**
- Add two new methods to the `AssetManager<E>` trait (`:2383`) **with default bodies**, placed in
  the "shared default methods" section alongside `cascade_expire_dependents`/
  `expire_dependencies_result` (`:2919` onward) — not the "required primitives" section
  (`:2864-2918`). Built entirely from the existing required primitives (`lookup_key_asset`,
  `get_envref`, `insert_key_asset`) plus `AssetRef` methods, so neither `DefaultAssetManager` nor
  `ImmediateAssetManager` needs to override either method.
- Extract the existing inline deserialization logic inside `try_fast_track` (`:509-527`: the
  `is_binary_type_identifier` check + `E::Value::deserialize_from_bytes(...)` call + error
  handling) into a small `pub(crate) fn deserialize_stored_value(binary: &[u8], type_identifier:
  &str, data_format: &str) -> Result<E::Value, Error>` free function (or associated function on
  `AssetData<E>`), used by BOTH `try_fast_track` and the new `get_any_status` store-fallback below.
  This avoids duplicating the ~20-line deserialize-or-treat-as-corrupted logic.
- Write `get_any_status(key)` as a trait default:
  1. If `self.lookup_key_asset(key)` finds an entry, return
     `Ok(asset_ref.get_any_status().await)`.
  2. Else, `self.get_envref().get_async_store().get(key)` (the raw store, NOT through
     `try_fast_track`'s status allow-list); if `Err`, propagate via `?`; if `Ok((binary,
     metadata))` and `metadata.status().has_data()`, deserialize via the extracted helper and
     return `Ok(Some(State::from_parts(...)))`. Deliberately do **not** call
     `dependency_manager().register_version`/`load_from_records` and do **not** call
     `insert_key_asset` — no side effects on normal evaluation (Phase 2 constraint).
  3. Else `Ok(None)`.
- Write `to_override(key)` as a trait default:
  1. If `self.lookup_key_asset(key)` finds an entry:
     - `asset_ref.to_override().await?` (existing method, handles every data-bearing status).
     - `match asset_ref.persistence_status().await { Persisted => self.get_envref()
       .get_async_store().set_metadata(key, &metadata_with_override_status).await?,
       NonSerializable => {} (no store write), NotPersisted | None => { serialize + store.set(key,
       &binary, &metadata).await; record via the existing record_persistence_result-style
       tracking } }`.
     - The entry is already reachable via `lookup_key_asset` — no reinsertion needed unless a race
       with lazy-expiry/monitor eviction removed it in between (re-`insert_key_asset` defensively
       in that case).
  2. Else, load from the store exactly as `get_any_status`'s fallback does; if found, rewrite
     **only** the metadata `status` field to `Override` via `store.set_metadata` (bytes are
     provably already persisted, since we just deserialized them) — do not construct or insert an
     in-memory `AssetRef` via `insert_key_asset`.
  3. If neither exists, `Err(Error::key_not_found(key))`.

**Code changes:** (signatures only; full bodies per the algorithm above — this step is the one
requiring genuine implementation, not a mechanical copy)
```rust
// MODIFY trait (assets.rs:2383), add two methods with default bodies in the
// "shared default methods" section (:2919 onward, alongside cascade_expire_dependents):
async fn get_any_status(&self, key: &Key) -> Result<Option<State<E::Value>>, Error> {
    if let Some(asset_ref) = self.lookup_key_asset(key) {
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
    if let Some(asset_ref) = self.lookup_key_asset(key) {
        asset_ref.to_override().await?;
        // ... PersistenceStatus branch per algorithm above, using self.get_envref().get_async_store() ...
        return Ok(());
    }
    // ... store-fallback branch: load, rewrite metadata status only, per algorithm above ...
    Err(Error::key_not_found(key))
}
```

**No changes needed to `DefaultAssetManager` or `ImmediateAssetManager`'s own `impl
AssetManager<E> for ...` blocks** — neither overrides `get_any_status`/`to_override`, so both
compile against the trait default unchanged. (`DefaultAssetManager` does override several *other*
trait defaults — `get`, `remove`, `set_binary`, `set_state` — with its own `scc`-direct versions,
for reasons unrelated to this WP; this step does not touch those.)

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core --lib assets::
cargo check -p liquers-py   # public AssetManager trait gained new methods (CLAUDE.md rule) —
                            # expected to stay green since they're defaults, not required
```

**Rollback:**
```bash
git diff liquers-core/src/assets.rs
git checkout liquers-core/src/assets.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** `rust-best-practices`, `liquers-unittest`
- **Knowledge:** Phase 2 architecture doc in full (this step implements its central algorithm);
  `assets.rs:480-560` (`try_fast_track`, for the extraction and the exact store-read/allow-list
  pattern to bypass); `assets.rs:1932` onward (`AssetRef::to_override`); `assets.rs:1099-1146`
  (`persistence_status`/`record_persistence_result`/`classify_persistence_error`); `assets.rs:498`
  (fast-track's status allow-list, to confirm `Override` is already in it so the store-fallback
  promotion needs no fast-track changes); `assets.rs:2864-2918` (the primitive methods this step
  builds on: `lookup_key_asset`, `get_envref`, `insert_key_asset`, `dependency_manager`)
- **Rationale:** This is the one step with real architectural/business logic (the
  `PersistenceStatus` three-way branch, and the two-path in-memory-vs-store algorithm) — requires
  judgment, not pattern-following. **Do not delegate to haiku.**

---

### Step 4: Unit tests (monitor internals)

**File:** `liquers-core/src/assets.rs`, existing `#[cfg(test)] mod tests { ... }` block (end of
file)

**Action:** Add the three unit tests from Phase 3 ("Test Plan > Unit Tests"):
`test_untrack_releases_strong_ref`, `test_retrack_earlier_deadline_fires_once`,
`test_expire_failure_preserves_processing_asset`. Adapt Phase 3's sketches to whatever exact
imports/helpers already exist in this test module (it already has `use super::*;` plus
`command_metadata::CommandKey`, `AsyncMemoryStore`, etc. — confirm exact names in place rather than
re-guessing import paths, since Phase 3 wrote these assuming module-local access it could not
fully verify from outside `assets.rs`).
**Opus final review fix required:** as drafted, `test_untrack_releases_strong_ref` never calls
`schedule_expiration`, so the asset is never entered into the monitor's heap — `upgrade() -> None`
after dropping would hold true even before the WP-3 weak-ref change, making the assertion pass for
the wrong reason. Track the asset with a future deadline (e.g. `asset.schedule_expiration(&
ExpirationTime::At(chrono::Utc::now() + chrono::Duration::hours(1))).await;`) before capturing the
`WeakAssetRef` and dropping the strong ref, so the test actually exercises the monitor's heap entry.

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
- **Skills:** `rust-best-practices`, `liquers-unittest`
- **Knowledge:** Phase 3 examples doc's "Unit Tests" section; the actual current imports at the
  top of `assets.rs`'s test module; `CommandRegistry::register_async_command` signature (for the
  gate-command pattern in test 3 — re-verify its current line number, it has shifted along with
  everything else in this file)
- **Rationale:** Timing-sensitive tests (retrack-fires-once, gate-based Processing preservation)
  need judgment to keep deterministic per this repo's "no sleeps for correctness" convention —
  not simple pattern-copying.

---

### Step 5: Integration tests

**File:** `liquers-core/tests/expiration_integration.rs` (existing file, append)

**Action:** Add all Phase 3 integration-level tests: Example 1 (3 tests), Example 2 (4 tests,
including the shared `keyed_counter_env()` helper), Example 3 (3 tests, one marked as a sketch
needing gate-synchronization rework), and the "Integration Tests" section (`CountingStore` double,
`test_to_override_metadata_only_when_persisted`, the still-deferred `#[ignore]`d
`test_to_override_retries_persist_when_not_persisted`,
`test_to_override_skips_store_write_when_nonserializable` (un-deferred by the Phase 4 opus final
review — no test-only `Value` type needed, an unrecognized key extension makes `Value::as_bytes`
fail for any value), and `test_get_any_status_has_no_side_effects_on_normal_get`) — all as drafted
in `specs/expiration-safety/phase3-examples.md`, verified compilable against the real APIs added
in Steps 1-3.

**Code changes:** As drafted in Phase 3, appended to `expiration_integration.rs` with the shared
`keyed_counter_env()` helper and `CountingStore` struct defined once near the top of the new
additions (not duplicated per test).

**Opus final review fix required:** add one additional test not in Phase 3's original draft —
the store-fallback branches of `get_any_status` and `to_override` (no in-memory entry, load
directly from the store) are otherwise unexercised, since
`test_get_any_status_loads_persisted_expired_state` cannot deterministically force eviction of the
in-memory entry, and `to_override`'s no-in-memory-entry branch has no test at all. Add
`test_get_any_status_and_to_override_from_store_only` (or split into two tests): persist an
expired keyed state through one `envref`/manager, then construct a **second**, independent
`SimpleEnvironment`/`envref` pointed at the same underlying store bytes (no shared in-memory
`AssetRef` at all — e.g. reuse the same `AsyncMemoryStore` instance across both environments if it
supports that, otherwise persist via the first environment, drop it entirely, then build a fresh
second environment over a store re-hydrated from the same bytes), and call `get_any_status`/
`to_override` on the second manager. This deterministically exercises the store-only code path
that the rest of Phase 3 could only reach probabilistically.

**Optional, not required:** `liquers-core/tests/manager_parametric.rs` (new since
`async-wasm-refactor`) tests shared `AssetManager` behaviors generically across both
`DefaultAssetManager` (via `SimpleEnvironment`) and `ImmediateAssetManager` (via the new
`ImmediateEnvironment` test harness). Since `get_any_status`/`to_override` are now shared trait
defaults, they apply to both managers automatically without extra code — adding a
`get_any_status`/`to_override` scenario there would give explicit cross-manager test evidence, but
is not required for this WP's acceptance criteria (which are scoped to `DefaultAssetManager`
behavior via `SimpleEnvironment`, matching Phase 1's approved crate placement). Leave as a
follow-up suggestion, not a blocking step.

**Validation:**
```bash
cargo check -p liquers-core --test expiration_integration
cargo test -p liquers-core --test expiration_integration
# Expected: all non-#[ignore]'d tests pass. One #[ignore]'d test remains
# (test_to_override_retries_persist_when_not_persisted) — see its doc comment for what unblocks
# it (a store double that fails set() for the target key only, while still serving recipes.yaml
# reads). test_dependency_expiring_during_parent_evaluation_is_allowed may need its gate
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
- **Skills:** `rust-best-practices`, `liquers-unittest`
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
- **Simplified by the shared-default-method design (Step 3):** since `get_any_status`/
  `to_override` are trait defaults, there is no need to hunt for every `impl AssetManager<E> for
  ...` in the codebase and update each one — both existing implementors
  (`DefaultAssetManager`, `ImmediateAssetManager`, confirmed via workspace-wide search during the
  Phase 4 review to be the only two) compile unchanged. Still run
  `cargo check --target wasm32-unknown-unknown -p liquers-core` once (matching
  `async-wasm-refactor`'s own checkpoint convention) to confirm the new default methods compile
  under the `MaybeSend`/`MaybeSync` wasm bound — expected to pass automatically since they're
  built purely from already-wasm-compatible primitives, but worth the one extra command given this
  is new territory for this WP.

**Validation:**
```bash
cargo test -p liquers-core
cargo test --workspace --exclude liquers-py
cargo clippy --workspace --exclude liquers-py --all-targets
cargo check -p liquers-py
cargo check --target wasm32-unknown-unknown -p liquers-core   # new: confirm wasm compat holds
```

**Rollback:** N/A (this step only fixes fallout from Steps 1-5; if fallout is extensive enough to
suggest a design flaw, stop and return to Phase 2/3 rather than patching around it).

**Agent Specification:**
- **Model:** sonnet
- **Skills:** `rust-best-practices`, `liquers-unittest`
- **Knowledge:** All files touched in Steps 1-5; `plan20260707.md`'s "Validation commands"
  convention; `specs/async-wasm-refactor/DESIGN.md` for the wasm-check convention
- **Rationale:** Diagnosing and fixing any fallout (e.g. a clippy lint on the new code, an
  unexpected wasm-compat gap) requires judgment about whether the fix is safe or indicates a need
  to revisit the design.

---

### Step 7: Close out specs

**File:** `specs/FEATURES/EXPIRATION-SAFETY.md`, `specs/FEATURES/EXPIRATION-SAFETY-IMPLEMENTATION-PLAN.md`,
`specs/expiration-safety/DESIGN.md`

**Action:**
- `EXPIRATION-SAFETY.md`: change `Status: Draft` to `Status: Closed`; add a short "Implemented via
  `specs/expiration-safety/` (WP-3)" pointer, noting the acceptance criteria this WP adds beyond
  the original doc (weak-ref monitor, `get_any_status`/`to_override` recovery API as shared
  `AssetManager` defaults — the original doc's own scope, e.g. earliest-deadline-wins tracking and
  status-aware eviction, was already implemented before this WP started, confirmed by code audit
  in Phase 1).
- `EXPIRATION-SAFETY-IMPLEMENTATION-PLAN.md`: mark phases as complete; note that Phases 1-2
  (duplicate-scheduling removal, monitor state normalization) were already done prior to this WP.
- `specs/expiration-safety/DESIGN.md`: mark all phases complete, add final implementation notes/
  links to the merged PR, and note the `async-wasm-refactor` sync that happened mid-Phase-4.

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

**Expected:** 13 of the 14 Phase 3 integration-file tests pass (17 total minus 3 unit tests in
`assets.rs` = 14 here; minus the 1 still-`#[ignore]`'d `NotPersisted`-retry test = 13 expected
green), including the previously-deferred `test_to_override_skips_store_write_when_nonserializable`
now that it's un-deferred. The remaining `test_dependency_expiring_during_parent_evaluation_is_allowed`
sketch is counted among the 13 but may need its gate finalized first if it proves flaky — see
Step 5's validation note. Every pre-existing test in this file (the `test_timed_expiration`,
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
# Expected: compiles — confirms the two new AssetManager trait default methods don't break the
# Python bindings (they don't wrap AssetManager directly as of this audit, but this is the
# CLAUDE.md-mandated gate whenever a public core trait changes)

cargo check --target wasm32-unknown-unknown -p liquers-core
# Expected: compiles — confirms the new default methods respect the MaybeSend/MaybeSync bound
# async-wasm-refactor introduced; this is a new check this WP didn't originally plan for, added
# after the async-wasm-refactor sync
```

**Success criteria:** All commands above exit 0; the specific test names listed in "Integration
Tests" above are individually confirmed passing (not just "suite green"), since several encode the
exact WP-3 acceptance criteria (cache-miss semantics, no-double-serialization, dependency
freshness).

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|------|-------|--------|-----------|
| 1 | haiku | rust-best-practices | Mechanical strong-ref → weak-ref substitution, fully specified |
| 2 | sonnet | rust-best-practices, liquers-unittest | Must preserve exact construction logic while splitting one match arm; must NOT touch Error/Cancelled (scope discipline) |
| 3 | sonnet | rust-best-practices, liquers-unittest | Core new business logic — `PersistenceStatus` branching, written once as a shared default. **Not haiku-safe.** |
| 4 | sonnet | rust-best-practices, liquers-unittest | Timing-sensitive unit tests need deterministic-test judgment |
| 5 | sonnet | rust-best-practices, liquers-unittest | Reconciling Phase 3 sketches against Steps 1-3's real signatures |
| 6 | sonnet | rust-best-practices, liquers-unittest | Fallout triage requires judging safe-fix vs. design-flaw; new wasm-check |
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
`AssetRef::get_any_status` existing), the smallest safely-pausable checkpoint is after Step 3, not
mid-step. Step 3 is now strictly simpler than originally planned (one default method each, not two
per-manager implementations), so this checkpoint is reached faster than the original estimate.

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
`PROJECT_OVERVIEW.md`'s current content and decide if a note fits. Note: `PROJECT_OVERVIEW.md` was
also touched by `async-wasm-refactor`'s own M-F docs step (`#[async_trait(?Send)]` on wasm) — check
for merge-adjacent content before adding.

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
- Known gaps carried forward honestly rather than hidden: the `NotPersisted` retry-branch test
  remains `#[ignore]`d pending a store double not resolved in this design (fails `set()` for one
  key only, while still serving `recipes.yaml` reads); the in-flight dependency-race test's gate
  timing is a sketch, not a finished mechanism. (The `NonSerializable` test was un-deferred by the
  opus final review — see Step 5.) **These are execution risks, not design risks** — the
  underlying `AssetManager`/`AssetRef` code changes (Steps 1-3) do not depend on this test existing
  to be correct; it exists to prove correctness, and its absence-for-now is a testing-coverage gap
  explicitly flagged for whoever executes Step 5, not a silent one.
- **Opus final review additionally found** (both folded into Step 5's scope): (1) the store-fallback
  branches of `get_any_status` and `to_override` (no in-memory entry, load from store directly) are
  not exercised by any test as originally drafted — `test_get_any_status_loads_persisted_expired_state`
  admits it can't deterministically force eviction, and `to_override`'s no-in-memory-entry branch had
  no test at all; Step 5 should add a variant that opens a second, independent manager/envref over
  the same persisted store bytes (no shared in-memory `AssetRef`) to force this path deterministically.
  (2) `test_untrack_releases_strong_ref` (Step 4) as drafted never calls `schedule_expiration`, so the
  asset is never entered into the monitor's heap at all — `upgrade() -> None` after dropping would
  hold even before the WP-3 weak-ref change, making it a weak regression guard; Step 4's agent should
  ensure the test actually tracks the asset (a future deadline) before dropping it, so the assertion
  is meaningful.
- **Post-approval `async-wasm-refactor` sync (this update):** re-verified every file:line citation
  against the current `main` after merging; Step 3 revised from "required, implement twice" to
  "shared default, implement once" (strict simplification); added one new validation command
  (`cargo check --target wasm32-unknown-unknown`) since the new default methods now compile as
  part of the wasm-facing trait surface; confirmed exactly two `AssetManager<E>` implementors exist
  workspace-wide (`DefaultAssetManager`, `ImmediateAssetManager`), both unaffected by Step 3's
  default-method approach.
- Confidence: **90%**, unchanged by the `async-wasm-refactor` sync — the sync simplified Step 3 and
  refreshed line numbers, but didn't remove or add any of the risks the opus reviewer identified
  (the one deferred test, the one sketch test, the two coverage gaps). Recommendation is still to
  **proceed to "Execute now"**, folding all noted coverage improvements into Steps 3-5.
