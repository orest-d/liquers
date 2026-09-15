# Phase 3: Examples & Use-cases - Stale-Dependency Status Finalization

> **Revision 2 (2026-09-15).** Rewritten against HEAD after `keyed-expiry-cascade-fix` and against
> Phase 2 Revision 2, which added the fast-track reading half and replaced the dependency-manager
> decision. Revision 1's test plan largely survives; what changed is recorded in §"What changed
> since Revision 1".

## High-Level Introduction

Phase 1's purpose: an asset that consumed a stale dependency must be **written** as `Expired`, so
the store and the runtime agree about the status whose only job is to force recomputation. Phase 2
added the other half — a later reader must **act** on that, so `try_fast_track` must decline to
build on a dependency it can see is expired.

Both halves only matter across a process boundary, which is why the examples are ordered the way
they are: the cross-process scenario first, because it is the one nothing exercises today and the
one where a green test is worth having.

This change has **no public API and no query-reachable surface**, so there is nothing to write a
conceptual usage example against. The examples are runnable tests — the same determination
`expired-binary-read-safety` made for the same reason, restated and open to correction at the gate.

## What changed since Revision 1

| Revision 1 | Now |
|---|---|
| "`save_to_store` has no status gate" — marked **wrong** at the Phase 4 review | **Resolved upstream.** `serialize_to_binary` reads `poll_state_any_status()`, and `prepare_version` installs the bytes beforehand. The row below states the current truth rather than the correction |
| Tests target `try_to_set_ready` | Target `finalize_status_with_version`; `try_to_set_ready` is now a wrapper |
| I4 asserted a cascade expired dependents | I4 asserts **`register_version` fired** — the Phase 2 decision changed, so the assertion changes with it |
| A `SharedMemoryStore` had to be invented | Still needed, and now also closes `CROSS-PROCESS-RELOAD-IS-UNTESTED`, filed by `keyed-expiry-cascade-fix` for the same missing fixture |
| No fast-track tests | Four (F1–F4), for Phase 2's reading half |
| Pitfall P4 withdrawn | Replaced: the live risk is now treating an **inconclusive** dependency as expired |

## Verified Setup Facts

Re-verified against HEAD on 2026-09-15. Binding on Phase 4 — every row is something a plausible
test would otherwise get wrong.

| Assumption | Reality |
|---|---|
| `set_value` is inert test setup | **No.** It sets `Ready`, notifies, **and persists**. A test asserting "the reason is recorded before persistence" that sets up with it has already persisted. Install the value under the write lock, as `evaluate` does |
| Two environments share a store by cloning it | **No.** `AsyncMemoryStore` owns its `scc::HashMap` and is not `Clone`. Needs a `#[derive(Clone)]` wrapper over `Arc<AsyncMemoryStore>`, delegating `AsyncStore`. `ToOverrideGateStore` (`expiration_integration.rs:880`) is the proven shape |
| Delegate only `AsyncStore`'s two required methods | **Not enough, and not for the reason it first appears.** The other twenty defaults are *not forwarding defaults*: `set`'s default is `Err(key_not_supported)`. A wrapper overriding only the required pair compiles and then fails every write. Size the wrapper by compiling, not by counting |
| A generic `scenario_*<E>` can call `E::new()` | **No.** `Environment` has no `new()`. The shape is `scenario_x<E>(envref: EnvRef<E>)` with the concrete environment built in the wrapper tests (`manager_parametric.rs:33`) |
| `manager.get_any_status(&key)` yields a `State` | **No.** `Result<Option<State>, Error>` at the manager (`assets.rs:4006`); `Option<State>` on the `AssetRef` |
| `LogEntry` has a `level` field | **No.** `kind: LogEntryKind`; compare `entry.kind == LogEntryKind::Warning` |
| Read status by matching `Metadata` | Use `Metadata::status()` (`metadata.rs:1966`). Matching needs both arms — no `_ =>` |
| Volatility is set by mutating the registry | **No.** `register_command!(cr, fn vol_cmd() -> result volatile: true)?` |
| `status()` can be read straight after `evaluate()` | **No.** `evaluate()` may return while still `Processing`. **Always `get().await` first** — reading status first reports a lie and makes a subsequent `expire()` fail. This discipline is stated at the top of `keyed_version_cascade.rs` and applies to every test here |
| `evaluate` leaves `lock.binary` unset, so persistence serializes | **No longer.** `evaluate` clears it, then `prepare_version` installs the serialized bytes. A stale-dependency asset therefore reaches persistence **with** a cached binary, and `save_to_store` writes those exact bytes |
| The persistence path consults the read gate | **No — fixed upstream.** `serialize_to_binary` reads `poll_state_any_status()`. This row said the opposite in Revision 1 and was the blocking Phase 4 finding |

### Fixtures to reuse rather than build

**`chain_env` (`liquers-core/tests/keyed_version_cascade.rs:36`) is the fixture this design needs.**
It builds a three-link keyed chain — `a.txt` ← `hello`, `b.txt` ← `a.txt/world`, `c.txt` ←
`b.txt/world` — plus `n.bin`, a non-serializable case. Its three links are exactly what a
dependent-invalidation assertion needs, and its recipes are already in the tree and already valid,
so no new query strings are introduced by this design.

The one fixture that does not exist is the shared store. Building it closes
`CROSS-PROCESS-RELOAD-IS-UNTESTED` as well as serving I1 and F1.

## Overview Table

| # | Kind | Name | What it demonstrates or checks |
|---|---|---|---|
| 1 | Example | Cross-process recomputation | The writing half's payoff: a fresh environment declines the stored value and recomputes |
| 2 | Example | The finalization truth table | Four outcomes, their precedence, and that the status reaching the store is the final one |
| 3 | Example | Fast-track declines an expired dependency | The reading half, and the inconclusive case that must **not** decline |
| 4 | Example | Pitfalls | Ten ways to reintroduce the defect while believing it fixed |
| U1 | Unit | `finalize_without_stale_dependency_is_ready` | Guard: the branch does not over-trigger |
| U2 | Unit | `finalize_with_stale_dependency_is_expired_in_metadata_too` | Status **and** `metadata.status()` — the disagreement *is* the bug |
| U3 | Unit | `finalize_records_the_reason_before_persistence` | The warning is present when finalization returns |
| U4 | Unit | `finalize_volatile_wins_over_stale_dependency` | Precedence, in the one combination with two plausible answers |
| U5 | Unit | `finalize_without_data_is_error_regardless_of_flag` | The error arm is untouched |
| U6 | Unit | `finish_run_fallback_finalizes_with_the_same_rule` | The fallback call site gained the rule deliberately |
| U7 | Unit | `finalize_expiration_time_agrees_across_arms` | The `Expired` arm mirrors the `Ready` arm |
| U8 | Unit | `finalize_does_not_alter_the_prepared_version` | Staleness is about freshness, not content — the version is untouched |
| I1 | Integration | `cross_process_stale_dependency_recomputes` | Example 1, on both managers |
| I2 | Integration | `keyed_stale_dependency_is_stored_expired` | The store entry says `Expired`, not the memory copy |
| I3 | Integration | `non_keyed_stale_dependency_writes_nothing` | No write attempted, so no spurious "cannot determine key" warning |
| I4 | Integration | `stale_dependency_registers_version_and_invalidates_dependents` | The Phase 2 decision: `register_version` fires and `c.txt` is invalidated |
| I5 | Integration | `stale_dependency_never_observable_as_ready` | Deterministic, where B1 called it racy |
| I6 | Integration | `stale_dependency_recovery_from_store` | `get_any_status` / `to_override` still recover it |
| I7 | Integration | `volatile_keyed_stale_dependency_stays_volatile` | Volatile keyed assets keep being written, as `Volatile` |
| I8 | Integration | `delegating_stale_dependency_registers_nothing` | `bound_owner_key()` returns `None`, so no version is written under the owner's key |
| **F0** | Integration | `fast_track_succeeds_for_a_ready_stored_asset` | **The missing baseline.** No test today asserts `try_fast_track` can return `true` at all |
| F1 | Integration | `fast_track_declines_a_dependency_expired_in_the_store` | The reading half, restart case |
| F2 | Integration | `fast_track_declines_a_dependency_expired_in_memory` | The live-asset branch |
| F3 | Integration | `fast_track_proceeds_when_the_dependency_check_is_inconclusive` | A command-implementation dependency must not block fast-track |
| F4 | Integration | `fast_track_reads_no_metadata_when_dependencies_are_live` | The live-asset branch is not bypassed |
| — | Regression | `test_wait_for_retained_expired_dependency_labels_asset_expired_on_completion` (`assets.rs:7964`) | Must pass **unchanged**, not adjusted to agree |
| — | Regression | `keyed_expiry_cascades_to_keyed_dependents` (`keyed_version_cascade.rs:119`) | The versions work's own guard must stay green |

## Example 1: A restarted process must not serve the stale value

### Connection to the design

This is Phase 1's purpose as an observable. The disagreement between store and runtime is invisible
in-process and becomes visible the moment a second dependency manager — one that has never seen
these keys — meets a store entry written by the first.

### The mechanism

`try_fast_track` accepts a stored asset whose status is `Ready | Source | Override` (`:1066`).
**Before the fix** a stale-dependency asset is stored `Ready`, so it is accepted and served with no
recomputation. **After**, it is stored `Expired` and fast-track refuses at the status check.

### Shape

```rust
async fn scenario_cross_process_stale_dependency_recomputes<E>(
    first: EnvRef<E>,
    second: EnvRef<E>,      // a different manager and DM over the same shared store
    calls: Arc<AtomicUsize>,
) -> Result<(), Error>
where E: Environment<Value = Value>
{
    // 1. Evaluate b.txt while a.txt expires mid-run, so note_expired_dependency fires on the
    //    production path. `get().await` before reading any status.
    // 2. Assert the STORE entry for b.txt reports Status::Expired.
    // 3. Reset `calls`, request b.txt through `second`.
    // 4. Assert the command ran again.
    Ok(())
}
```

**Assert an evaluation counter, never the value.** The recomputed value equals the stale one, so a
value assertion passes whether or not the fix works. `chain_env` already carries a counting command
for exactly this purpose.

### Forcing the mid-evaluation window

`note_expired_dependency` fires only from `wait_for_dependency`, i.e. while the dependent is already
evaluating. Expiring the dependency earlier takes the *scheduling*-time path instead and the flag is
never set. Copy `test_dependency_expiring_during_parent_evaluation_is_allowed`
(`expiration_integration.rs:748`): the parent holds a `oneshot`, reads its dependency through
`context.get_dependency_state()`, then blocks; the test **polls until the child is `Ready`**
(bounded, 200 × 2 ms) before expiring it and releasing the gate.

The bounded poll is the part to copy carefully — it is positive proof the parent already took the
value. A `sleep` instead will sometimes take the scheduling-time path, pass for the wrong reason,
and be indistinguishable from a working test. Assert the parent is `Expired` early so a missed
window fails there.

## Example 2: What finalization decides

| `data` | volatile | `stale_dependency` | Status | Written? | Version |
|---|---|---|---|---|---|
| present | yes | either | `Volatile` | yes, if keyed | none (volatile assets are not graph nodes) |
| present | no | **yes** | **`Expired`** | yes, if keyed — **the fix** | content hash, unchanged by the flag |
| present | no | no | `Ready` | yes, if keyed | content hash |
| absent | — | either | `Error` | no | none |

Three properties the table does not show, each asserted:

1. **Metadata moves with the field** — the decision goes through `AssetData::set_status`. Updating
   only the field reproduces the original defect one layer down.
2. **The reason is recorded before the write**, so it reaches the store with the value.
3. **The version is untouched** (U8). `prepare_version` derives it from content; staleness is about
   freshness. Two evaluations producing identical bytes must yield identical versions whether or not
   a dependency expired.

## Example 3: Fast-track declines a dependency it can see is expired

### Connection to the design

Phase 2's reading half. Writing an expiry to the store pays off only if something consults it.

### The three outcomes, and which one is dangerous to get wrong

| Dependency state | Source | Fast-track |
|---|---|---|
| Expired, live in the manager | `lookup_key_asset().status()` | **decline** (F2) |
| Expired, only in the store | `store.get_metadata()` | **decline** (F1) |
| Not determinable — a command-implementation node, or no stored metadata | — | **proceed** (F3) |

**There is no baseline today, and that is why this is dangerous.** Searching the suite for
`try_fast_track` finds exactly one test — `test_expired_keyed_asset_does_not_fast_track_back`
(`expiration_integration.rs:1557`) — and it asserts the function returns **`false`**. Nothing
asserts it can return `true`. Every other store-path test either works from an in-memory asset or
goes through the `get_any_status` / `to_override` recovery branch, not through fast-track.

So a Step 4 that fails closed would make fast-track stop working entirely, and **the suite would
stay green**: results would still be correct, just recomputed every time. F0 exists to give that
failure somewhere to land, and it should be written *before* the check in Step 4 so it is known to
pass beforehand.

**F3 is the test that matters most** among the refusal cases. Nearly every asset has a command-implementation dependency
(`ns-dep/command_impl---world`), which `Key::try_from` cannot address. Treating that as expired
would disable fast-tracking almost everywhere — and it would present as a severe performance
regression, not as a failing assertion, which is why it needs a test rather than a code review.

F4 pins the ordering: with every dependency live in memory, the check must perform **no** metadata
reads. Use the shared-store wrapper with a read counter — the same wrapper I1 needs.

## Corner Cases

| # | Case | Symptom if wrong | Cause | Correction | Caught by |
|---|---|---|---|---|---|
| P1 | Status set without `set_status` | Store says `Ready`, memory says `Expired` — the original bug, relocated | Field updated but not metadata | Go through `AssetData::set_status` | U2 |
| P2 | Warning added after persistence again | Stored metadata says `Expired` with no reason | Entry left in the harness, or added after the lock drops | Same locked decision as the status | U3, I2 |
| P3 | The fallback call site is missed | A run that finished without `evaluate` finalizing skips the rule | Rule added to one call site only | Both; the fallback gains it deliberately | U6 |
| P4 | **Inconclusive treated as expired** | Fast-track collapses for nearly every asset — reads as a performance bug, not a correctness one | Failing closed on a dependency that cannot be addressed | Fail open on absence, closed only on positive evidence | **F3** |
| P5 | `expiration_time` diverges between arms | Scheduling behaves differently for `Expired` than `Ready` | The new arm omits the two lines | Mirror the `Ready` arm | U7 |
| P6 | The version is recomputed or cleared by the branch | Dependents invalidated on every recomputation of identical content | Treating staleness as a content change | Touch status and metadata only | U8 |
| P7 | Registering under `lock.key` rather than `bound_owner_key()` | A delegating asset overwrites the real owner's version | Ownership-blind key derivation | `bound_owner_key()`, as `track_asset` uses | I8 |
| P8 | The DM step skipped entirely for `Expired` | Dependents of the key are never invalidated | Letting `track_asset`'s gate refuse it | Register directly for this case | I4 |
| P9 | Dependency waits moved after finalization | The flag arrives after the decision; the asset stays `Ready` | Restructuring `evaluate` so `apply_recipe` is not awaited first | Keep it awaited to completion | I1, I5 |
| P10 | Volatility checked after the stale branch | A volatile asset becomes `Expired`, losing its volatility | Branch order reversed | `if volatile … else if stale …` | U4, I7 |

### Pitfall-to-test map

Every row is claimed by a named test above; none relies on inference. P4 is the only row whose
failure mode is a performance collapse rather than an assertion, which is why it is called out
separately in Example 3.

## Test Plan

Conventions per `.claude/skills/liquers-unittest/`: `#[tokio::test]` for async,
`-> Result<(), Box<dyn std::error::Error>>` where `?` is used, no `unwrap`/`expect` outside tests,
typed error constructors, no default match arms, `type CommandEnvironment` before any
`register_command!`.

### Unit tests — `liquers-core/src/assets.rs`

Construct with `AssetData::<SimpleEnvironment<Value>>::new(id, query.into(), None, envref).to_ref()`
and **install the value under the write lock**, never via `set_value`.

### Integration tests

I1–I8 in `liquers-core/tests/expiration_integration.rs`, generic over the environment with
`*_default` / `*_immediate` wrappers. F1–F4 belong beside them; they reuse `chain_env`'s recipe
shape, so consider whether `keyed_version_cascade.rs` is the better home for the fast-track four
given that file already owns the chain fixture.

### Building the second environment: two options, one already proven

Revision 1 assumed a shareable store was required. It is not, for the reload scenario.
`test_get_any_status_and_to_override_from_store_only` (`expiration_integration.rs:1336`) already
does it by **re-hydration**: read the bytes and metadata out of the first store, drop the whole
first environment, then `set` those bytes into a *fresh* `AsyncMemoryStore` behind a second
environment. That is an independent manager and dependency manager over equivalent store contents,
with no wrapper at all, and it is in the tree and working.

Re-hydration is a snapshot, not a share — which is exactly right for I1 and F1, where the first
environment is finished before the second starts. A counting wrapper is still needed for F4, but a
counting wrapper over a plain store is much less than a full shared store.

**Phase 4 should prefer re-hydration and build only the counting wrapper**, and should note that
whether this still closes `CROSS-PROCESS-RELOAD-IS-UNTESTED` depends on whether that issue wants a
snapshot or genuine sharing — re-hydration cannot exercise concurrent access to one store.

### Optional fixture: the shared store

```rust
#[derive(Clone)]
struct SharedMemoryStore {
    inner: Arc<AsyncMemoryStore>,
    metadata_reads: Arc<AtomicUsize>,   // F4 counts through this
}
```

Delegate `get`, `set_metadata` (required) **and** `set`, `contains`, `remove` (overridden by
`AsyncMemoryStore`). Count reads in `get_metadata`. This fixture also closes
`CROSS-PROCESS-RELOAD-IS-UNTESTED`.

### Guide-worthy material

**None**, and Phase 2 agrees. Every test here asserts an internal ordering property; none shows a
developer how to do anything. What Phase 5 should consider taking is the **Verified Setup Facts**
table — testing knowledge about this codebase that independent drafts got wrong repeatedly, and
`specs/guides/UNITTEST_GUIDE.md` is where the `set_value`-persists trap, the shared-store pattern
and the `get()`-before-`status()` discipline would stop the next person losing an afternoon.

## Documentation and Learning Log

### Learning recorded during Phase 3

1. **`set_value` persists.** Still true at this HEAD. A test for "the reason is recorded before
   persistence" that sets up with it has already persisted, and passes while proving nothing.
2. **The value is the same either way.** A cross-process test asserting on the returned value passes
   whether or not the fix works, because the recomputed value equals the stale one. Only an
   evaluation counter distinguishes them.
3. **The dangerous failure of the reading half is a performance collapse, not a wrong answer.**
   Failing closed on an undeterminable dependency would be caught by nobody's assertion. F3 exists
   because the bug would be reported as "everything got slow".
4. **The positive path of `try_fast_track` has never been tested.** One test names the function and
   it asserts a refusal. A change that broke fast-tracking outright would leave the suite green, and
   arrive as a performance report rather than a bug. A baseline for the success path of a
   cache is worth as much as the tests for its refusals.
5. **Two designs needed the same missing fixture.** `keyed-expiry-cascade-fix` filed
   `CROSS-PROCESS-RELOAD-IS-UNTESTED` for the shared store this design also needs. A fixture that
   two designs defer is worth building on the first ask.
6. **Reuse beat invention.** `chain_env` already builds the three-link keyed chain, the counting
   command and the non-serializable case. Revision 1 planned to build all of that from scratch.

## References

- Phase 1: `./phase1-high-level-design.md` · Phase 2: `./phase2-architecture.md`
- `liquers-core/tests/keyed_version_cascade.rs` — `chain_env` (`:36`) and the cascade guard (`:119`)
- `liquers-core/tests/expiration_integration.rs` — `ToOverrideGateStore` (`:880`), the
  mid-evaluation gate (`:748`), and the recovery tests I6 must keep green
- `liquers-core/tests/manager_parametric.rs` — the `scenario_*` + `*_default` / `*_immediate` shape
