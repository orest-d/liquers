# Phase 4: Implementation Plan - Keyed Asset Registration Semantics

## Overview

**Feature:** Resolve `ASSET-MANAGER-INSERT-KEY-ASSET-NO-OVERWRITE`.

**Architecture:** Both built-in managers receive a crate-private atomic insert-if-absent helper and
a manager-level Tokio mutation lock. The lock serializes keyed external mutation, cache entry
installation, and keyed eviction with store I/O; registration remains reachability-only.

**Estimated complexity:** High. **Estimated time:** 6–9 hours for an experienced Rust developer.

**Prerequisites:** Phases 1–3 approved; no dependency or public API addition. Keep
`QUEUED-MANAGER-EVICTION-RACE` separate.

## Implementation Steps

### Step 1: Establish private registration and locking infrastructure

**File:** `liquers-core/src/assets.rs`

**Action:** Add `key_mutation_lock: tokio::sync::Mutex<()>` to both built-in manager structs and
constructors. Remove `AssetManager::insert_key_asset` without a compatibility wrapper. Add this
matching concrete inherent method to both managers:

```rust
pub(crate) async fn try_insert_key_asset(&self, key: &Key, asset: AssetRef<E>) -> bool;
```

Default uses `insert_async(...).await.is_ok()`; Immediate uses one `HashMap::entry` operation under
its existing short mutex guard. Add the Phase 2 private locked helpers as call paths migrate; never
hold Immediate's standard map mutex across an await.

**Validation:**

```powershell
rg -n "fn insert_key_asset|\.insert_key_asset\(" liquers-core/src
```

**Expected:** no trait declaration remains; reported direct callers are migrated in Step 4. Do not
require a successful compile until that migration is complete.

**Rollback:** apply the inverse patch (or later revert the dedicated implementation commit),
preserving unrelated dirty-worktree changes.

**Agent Specification:**

- **Model:** sonnet
- **Skills:** rust-best-practices (if available)
- **Knowledge:** Phase 2; AssetManager and both manager constructors in `assets.rs`.
- **Rationale:** private concurrency boundary plus trait-surface removal.

---

### Step 2: Serialize DefaultAssetManager keyed workflows

**File:** `liquers-core/src/assets.rs`

**Action:** Implement Default-local `set_binary`, `set_state`, `remove`, and `to_override`. Each
acquires `key_mutation_lock` before lookup and retains it through map, dependency, and store work.
`set_state` must cancel/untrack old ref, conditionally remove by id, claim new ref, and only then
persist; failure to claim is the existing keyed general error with no write. Preserve dependency
version registration as the only cascade source. `to_override` re-reads/promotes/persists current
ref under the guard and drops only the unsafe post-persistence reinsertion. Preserve legitimate
store-only recovery by loading the current ref under the guard. Coordinate Default cache miss only
after volatility/recipe awaits, then re-check under the guard; likewise coordinate stale keyed
get/dependency eviction, `remove_expired_from_maps`/monitor cleanup, and volatile
`owned_key_asset` cleanup using
the guard and an inside-guard re-check; leave unrelated query-map eviction unchanged. Do not call a
new lock-taking eviction helper from `set_state` while it holds the non-reentrant gate. Implement
`owned_key_asset` as an explicit `AssetManager` trait-method override, not merely an inherent helper.

**Validation:**

```powershell
cargo test -p liquers-core --lib assets::tests
```

**Expected:** Default asset tests pass.

**Rollback:** apply the inverse patch, preserving unrelated dirty-worktree changes.

**Agent Specification:**

- **Model:** sonnet
- **Skills:** rust-best-practices (if available)
- **Knowledge:** Default manager, dependency/expiration paths, Phase 2 lock inventory.
- **Rationale:** lifecycle and durable-ordering judgment.

---

### Step 3: Mirror the contract in ImmediateAssetManager

**File:** `liquers-core/src/assets.rs`

**Action:** Implement the same locked mutators and cache/eviction/volatile-owner coordination for
Immediate. Hold the Tokio guard through store awaits but its standard map mutex only for one map
operation. Use `HashMap::entry`, retain identity-safe removal, and do not convert to `scc`.

**Validation:**

```powershell
cargo test -p liquers-core --lib assets::tests
cargo check -p liquers-core --target wasm32-unknown-unknown
```

**Expected:** immediate tests and wasm compile pass.

**Rollback:** apply the inverse patch to Immediate portions and immediate-only tests.

**Agent Specification:**

- **Model:** sonnet
- **Skills:** rust-best-practices (if available)
- **Knowledge:** Immediate manager/no-runtime tests and Phase 2 async decision.
- **Rationale:** wasm and inline async re-entrancy require precise lock lifetime handling.

---

### Step 4: Migrate callers and add primitive/lifecycle unit coverage

**Files:** `liquers-core/src/assets.rs`, `liquers-core/src/context.rs`,
`liquers-core/src/interpreter.rs`

**Action:** Migrate every direct `insert_key_asset` use: former trait-default paths, assets helpers
and removal tests, all three Immediate context tests, and the Immediate interpreter test. Helpers
assert successful `try_insert_key_asset` claims. Add Default/Immediate tests: first claim true,
second false, lookup stays first. Assert rejection preserves ref status/value and makes no store or
dependency change; do not infer notification absence from a watch channel. Update
`remove_key_asset_if_respects_id` and `test_set_state_replacement_untracks_old_timer`; retain
volatile owner, expiry, nonserializable recovery, and retry coverage.

**Validation:**

```powershell
cargo test -p liquers-core --lib assets::tests
cargo test -p liquers-core --lib context::tests
cargo test -p liquers-core --lib interpreter::tests
rg -n "\.insert_key_asset\(" liquers-core
```

**Expected:** tests pass and final search finds no removed-method caller.

**Rollback:** apply inverse patches to all three files together.

**Agent Specification:**

- **Model:** sonnet
- **Skills:** liquers-unittest, rust-best-practices (if available)
- **Knowledge:** Phase 3 plan and concrete-manager test fixtures.
- **Rationale:** private API migration and behavioral tests must stay aligned.

---

### Step 5: Add deterministic durable-ordering integration coverage

**File:** `liquers-core/tests/expiration_integration.rs`

**Action:** Add a cloneable test-only `AsyncMemoryStore` wrapper beside `WP3CountingStore`: after
arming, only the next `race.txt` persisted `set_metadata` signals entry and waits for release, while
a separate probe records value `set`; never hold a standard mutex across await. For recipe-backed serializable `race.txt` in
Default and Immediate, evaluate/persist old ref, expire, arm, and begin `to_override`. While it is
paused, first-poll pinned `set_state(new)` and require `Poll::Pending`; value write has not begun.
Release and await both. Assert public get and the wrapped inner store's bytes and metadata are the
distinct new `Override`/`new`; old held handle is detached Override. Preserve normal dependency cascade coverage
rather than expecting registration to cascade.

**Validation:**

```powershell
cargo test -p liquers-core --test expiration_integration
cargo test -p liquers-core --test manager_parametric
```

**Expected:** paused old promotion cannot become final durable state in either manager.

**Rollback:** apply the inverse test patch; no public test hook remains.

**Agent Specification:**

- **Model:** sonnet
- **Skills:** liquers-unittest, rust-best-practices (if available)
- **Knowledge:** Phase 3 race example, `WP3CountingStore`, existing to_override tests.
- **Rationale:** controlled async sequencing and durable-state assertions.

---

### Step 6: Final validation and Phase 5 evidence capture

**Files:** all files above and design records.

**Action:** Run formatting, targeted/crate tests, wasm compile, and record implemented-versus-planned
scope, retained boundaries, and results for Phase 5. Do not update behavior docs before code is
verified.

**Validation:**

```powershell
cargo fmt --all -- --check
cargo test -p liquers-core --lib
cargo test -p liquers-core --test expiration_integration
cargo test -p liquers-core --test manager_parametric
cargo check -p liquers-core --target wasm32-unknown-unknown
git diff --check
```

**Expected:** all pass; both maps preserve first claimant and gated ordering leaves newer state in
memory and store.

**Rollback:** use preceding targeted restores.

**Agent Specification:**

- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest (if available)
- **Knowledge:** all phase docs, modified source, and test output.
- **Rationale:** final integration and scope judgment.

## Testing Plan

After Step 4 run its three targeted inline suites. After Step 5 run `expiration_integration` and
`manager_parametric`. Step 6 is full crate, formatting, diff, and wasm validation. Success requires
no removed-method callers, uniform duplicate semantics, and deterministic final `new` bytes and
metadata in the gated race.

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|---|---|---|---|
| 1 | sonnet | rust-best-practices | private API and lock boundary |
| 2 | sonnet | rust-best-practices | Default lifecycle/store integration |
| 3 | sonnet | rust-best-practices | Immediate/wasm lock lifetime |
| 4 | sonnet | liquers-unittest | callers and behavior tests |
| 5 | sonnet | liquers-unittest | deterministic async integration |
| 6 | sonnet | both | holistic verification |

## Rollback Plan

Each step is limited to named files and reversible by an inverse patch or a revert of dedicated
implementation commits; never discard unrelated working-tree
changes. No files or dependencies are added. If pausing, record the completed step and failing
command in `DESIGN.md` rather than entering Phase 5.

## Documentation Updates

Phase 5 updates `specs/reference/ASSETS.md` with insert-if-absent and reachability/lifecycle
semantics and `specs/reference/ASSET_SET_OPERATION.md` with keyed serialization/conflict behavior.
It reviews `ASSET_LIFECYCLE.md` and `DEPENDENCIES_STATUS.md`, updates `reviewed:`/History, moves
the `specs/README.md` capability link, and closes the issue with evidence. No CLAUDE.md,
PROJECT_OVERVIEW.md, README, or guide update is expected: no user-facing API or pattern is added.

## Phase 5 Entry Criteria

- [ ] Implementation is finished and validated.
- [ ] All user and review comments are incorporated.
- [ ] Documentation claims are verified against implementation.
- [ ] Phase 5 records final scope and evidence.

## Execution Options

After approval: execute now, create a task list, revise, or exit for manual implementation.
