# Phase 3: Examples & Use-cases - Stale-Dependency Status Finalization

## High-Level Introduction

Phase 1's purpose is one sentence: an asset whose evaluation consumed a stale dependency must be
*written* as `Expired`, so the store and the runtime agree about the status whose whole job is to
force recomputation. Phase 2 decided where that is settled (`finalize_status`, before persistence)
and what happens to the dependency graph (`cascade_expire_dependents` for a keyed asset).

This change has **no public API and no query-reachable surface**, so there is nothing to write a
conceptual usage example against — the examples here are runnable tests, the same determination
`expired-binary-read-safety` made for the same reason. Stated rather than asked, and open to
correction at the gate.

The progression is:

- **Scenario 1** is the fix's payoff and the thing nothing exercises today: a *second* environment
  over the same store must recompute rather than serve the stored value. This is where the P1
  evidence becomes a test.
- **Scenario 2** goes down one level, to what `finalize_status` decides and in what order, where
  the interesting behaviour is the interaction with volatility and with the store write.
- **Scenario 3** is the pitfalls table: ten ways to implement this change and still ship the bug,
  most of them ways for the store and memory to disagree again in a new place.

## Example Type

**Runnable tests** — determined, not asked, for the reason above. No `examples/` binary: there is
no runnable *demonstration* of an internal ordering property, only assertions about it.

## Verified Setup Facts

The drafting pass produced code against APIs that **do not exist or do not behave as assumed**.
Each was checked against the source. Phase 4 should treat this list as binding — it is the most
valuable output of this phase, and every row was a draft that would not have compiled or would have
passed for the wrong reason.

| Assumption in the drafts | Reality |
|---|---|
| `LogEntry` has a `level` field compared to `"warning"` | **No.** It is `kind: LogEntryKind` (`metadata.rs:LogEntry`), so the comparison is `entry.kind == LogEntryKind::Warning` |
| Read status via `match &*lock.metadata { … _ => panic!() }` | **Two errors.** `Metadata` is a plain two-variant enum, not a smart pointer; and the `_ =>` arm violates the no-default-arm rule. Use the `Metadata::status()` accessor (`metadata.rs:1966`) and match nothing |
| `set_value(…)` is a fine way to install a value before calling `finalize_status` | **No — it invalidates the test.** `set_value` (`assets.rs:3330`) already sets `Ready`, sends `ValueProduced`/`JobFinishing`, **and persists**. A test asserting "the warning is in metadata *before* persistence" would have persisted in its own setup. Install the value the way `evaluate` does: `lock.data = Some(Arc::new(value))` under the write lock |
| Two environments share one store via `Box::new((*store).clone())` | **No.** `AsyncMemoryStore` (`store.rs:609`) owns its `scc::HashMap` directly — it is not `Clone` over shared state, so two environments built this way would not see each other's writes. **This is the crux of Scenario 1**; the working pattern is below |
| A generic `scenario_*<E>` body can call `E::new()` and `env.command_registry` | **No.** `Environment` has no `new()`. The established shape (`manager_parametric.rs:33`) is `async fn scenario_x<E>(envref: EnvRef<E>) -> Result<(), Error> where E: Environment<Value = Value>`, with the concrete `SimpleEnvironment` / `ImmediateEnvironment` built in the two thin wrapper tests |
| `manager.get_any_status(&key)` yields a `State` | **No.** `AssetManager::get_any_status` returns `Result<Option<State<E::Value>>, Error>` (`assets.rs:4006`); the `AssetRef` method returns `Option<State>` (`:3236`) |
| Volatility is set by mutating `cr.commands` after registration | **No.** It is declared in the macro: `register_command!(cr, fn vol_cmd() -> result volatile: true)?` (`payload_inheritance.rs:227`) |
| `save_to_store` might refuse a non-`Ready` status | ~~**Checked — it does not.**~~ **WRONG — corrected 2026-09-04 at the Phase 4 review.** `save_to_store` itself has no status gate, but on the evaluate path `lock.binary` is `None`, so it falls through to `serialize_to_binary` (`:2718`), which calls `poll_state()` — and that returns `None` for `Expired`. Writing `Expired` fails with "Failed to obtain binary value" and stores nothing. This row checked one call too few. See `DESIGN.md` §"Phase 4 review" |
| Recipes are set up from a raw `recipes:` YAML literal | Existing tests build a `RecipeList`, add `Recipe::new(query, title, description)`, and `serde_yaml::to_string` it (`expiration_integration.rs:1010-1020`). Follow that |

### The shareable-store pattern Scenario 1 needs

Because `AsyncMemoryStore` cannot be shared by cloning, the second environment needs a wrapper that
holds it behind an `Arc` and delegates `AsyncStore` to it. `ToOverrideGateStore`
(`expiration_integration.rs:880`) is exactly this shape and is the precedent to copy:

```rust
#[derive(Clone)]
struct SharedMemoryStore {
    inner: Arc<AsyncMemoryStore>,
}
// #[async_trait] impl AsyncStore for SharedMemoryStore { … delegate every method to self.inner … }
```

Both environments are then built with `Box::new(shared.clone())`. Each gets its own
`AssetManager` and its own — empty — `DependencyManager`, which is the property that makes it a
faithful stand-in for a restarted process, while the store contents survive.

**The delegation is cheap, and that was checked rather than hoped.** `AsyncStore` has **two
required methods** — `get` and `set_metadata` — and twenty with default implementations. So the
wrapper is two forwarding bodies, not twenty-two. `ToOverrideGateStore` is the proof: it delegates
the two required methods and overrides exactly one defaulted method (`set`) to inject its gate.
`SharedMemoryStore` needs no override at all.

## Overview Table

| # | Kind | Name | What it demonstrates or checks |
|---|---|---|---|
| 1 | Example | Cross-process recomputation | The fix's payoff: a fresh environment declines the stored value and recomputes (Phase 2's P1 evidence, as a test) |
| 2 | Example | What `finalize_status` decides | The four outcomes, their precedence, and that the status reaching the store is the final one |
| 3 | Example | Pitfalls | Ten ways to reintroduce the defect while believing it is fixed |
| U1 | Unit | `finalize_status_without_stale_dependency_is_ready` | Guard: the branch does not over-trigger |
| U2 | Unit | `finalize_status_with_stale_dependency_is_expired_in_metadata_too` | Status **and** `metadata.status()` are `Expired` — the disagreement is what the bug was |
| U3 | Unit | `finalize_status_records_the_reason_before_persistence` | The warning is in metadata at the moment finalization returns |
| U4 | Unit | `finalize_status_volatile_wins_over_stale_dependency` | Precedence, in the one combination that has two right-looking answers |
| U5 | Unit | `finalize_status_without_data_is_error_regardless_of_flag` | The error arm is untouched by the new input |
| U6 | Unit | `finish_run_fallback_finalizes_with_the_same_rule` | The `:2224` call site gained the rule, and did so deliberately |
| U7 | Unit | `finalize_status_expiration_time_agrees_across_arms` | Phase 2 decision (d): the `Expired` arm mirrors the `Ready` arm's `expiration_time`, so the scheduling step behaves identically |
| I1 | Integration | `scenario_cross_process_stale_dependency_recomputes` | Scenario 1, on both managers |
| I2 | Integration | `scenario_keyed_stale_dependency_is_stored_expired` | The store entry says `Expired`, not the memory copy |
| I3 | Integration | `scenario_non_keyed_stale_dependency_writes_nothing` | No write is attempted, so no spurious "cannot determine key" warning |
| I4 | Integration | `scenario_stale_dependency_cascade_expires_dependents` | The Phase 2 branch: dependents are invalidated without registering the value as current |
| I5 | Integration | `scenario_stale_dependency_never_observable_as_ready` | The `expired-binary-read-safety` position, now deterministic |
| I6 | Integration | `scenario_stale_dependency_recovery_from_store` | `get_any_status` / `to_override` still recover the value once the store says `Expired` |
| I7 | Integration | `scenario_volatile_keyed_stale_dependency_stays_volatile` | Volatile keyed assets keep being written, with `Volatile` |
| — | Regression | `test_wait_for_retained_expired_dependency_labels_asset_expired_on_completion` (`liquers-core/src/assets.rs:7707`) | Must pass **unchanged** — not adjusted to fit. It uses a non-keyed asset, so it should; if it does not, the branch is wrong, and editing the test to agree would hide that |

## Example 1: A restarted process must not serve the stale value

### Connection to the high-level design

This is Phase 1's purpose stated as an observable: "so that the store and the runtime agree". The
disagreement is invisible in-process, which is why the issue originally read P2. It becomes visible
the moment a second `DependencyManager` — one that has never seen these keys — meets a store entry
written by the first.

### The mechanism being tested

`try_fast_track` (`assets.rs:1048`) accepts a stored asset whose status is
`Ready | Source | Override`, then validates recorded dependency versions:

```rust
if let Some(dm_version) = dm.get_version(&dep_record.key).await {
    if !dm_version.matches(&dep_record.version) { /* refuse */ }
}
```

A fresh dependency manager holds no version for any key, so the guard body never runs and every
recorded dependency passes. **Before the fix**, the asset is stored `Ready`, fast-track accepts it,
and the stale value is served with no recomputation. **After the fix** it is stored `Expired`,
fast-track refuses at the status check, and the asset is recomputed.

### Shape

```rust
async fn scenario_cross_process_stale_dependency_recomputes<E>(
    envref: EnvRef<E>,
    shared: SharedMemoryStore,
    second: EnvRef<E>,
    calls: Arc<AtomicUsize>,
) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    // 1. Evaluate `dep.txt`, then `parent.txt` while `dep.txt` expires mid-run, so
    //    `note_expired_dependency` fires on the production path.
    // 2. Assert the STORE entry for `parent.txt` reports Status::Expired.
    // 3. Reset `calls`, then request `parent.txt` through `second` — a different manager and
    //    an empty DependencyManager over the same store.
    // 4. Assert the command ran again: `calls` incremented, i.e. no fast-track.
    Ok(())
}
```

Both environments are constructed by the two concrete wrapper tests, per the parametric pattern.
The `calls` counter is the assertion that matters: a value-equality check would pass either way,
because the recomputed value and the stale value are the same string. **Count the evaluations, not
the result** — this is the single easiest way to write this test so that it passes without testing
anything.

### The one hard part: making the dependency expire *mid*-evaluation

`note_expired_dependency` fires only from `wait_for_dependency`, i.e. while the parent is already
evaluating. Expiring the dependency before the parent starts takes the *scheduling*-time path
instead (`get_dependency_asset` evicts and recomputes), and the flag is never set.

The precedent exists and Phase 4 should copy it rather than reinvent it:
`test_dependency_expiring_during_parent_evaluation_is_allowed`
(`liquers-core/tests/expiration_integration.rs:749`). Its mechanism, in order:

1. The parent command is registered holding a `tokio::sync::oneshot` receiver.
2. The parent reads its dependency through `context.get_dependency_state()`, then blocks on
   `gate_rx.await` — so it is provably *inside* evaluation, having already taken the value.
3. The test **polls until the child is `Ready`**, bounded (200 iterations × 2 ms) rather than
   sleeping. This is the step that makes the window deterministic: it is positive proof the parent
   read the dependency, not an assumption that it had time to.
4. Only then does the test expire the child, and release the gate.

The bounded poll is the part worth copying carefully. A test that expires the dependency on a
timer instead will sometimes take the scheduling-time path, pass for the wrong reason, and be
impossible to distinguish from a working one. Assert the parent ends `Expired` early in the
scenario, so a run that missed the window fails there rather than deep in a later assertion.

## Example 2: What `finalize_status` decides

### Connection to the high-level design

Phase 2 made `finalize_status` the single status authority. This example is the truth table of that
authority, and the reason the function was renamed: `Ready` is one of four answers.

| `data` | volatile | `stale_dependency` | Status | Written to store? |
|---|---|---|---|---|
| present | yes | either | `Volatile` | yes, if keyed |
| present | no | **yes** | **`Expired`** | yes, if keyed — **this row is the fix** |
| present | no | no | `Ready` | yes, if keyed |
| absent | — | either | `Error` | no |

Two properties the table does not show, both asserted:

1. **Metadata moves with the field.** The decision goes through `AssetData::set_status`
   (`:1183`), which writes `self.status` *and* `self.metadata.set_status(...)`. A row that updates
   only the field reproduces the original defect one layer down, which is pitfall 1.
2. **The reason is recorded before the write.** The "evaluated with an expired dependency value"
   warning is added inside the same locked decision, so it reaches the store with the status.
   Today it is added after persistence and the stored sidecar has neither.

### Volatility wins, and that is a decision

A volatile keyed asset with a stale dependency stays `Volatile`. Volatile results are never reused
— they are not registered in the manager's maps and `try_fast_track` refuses `Volatile` — so
`Expired` would buy nothing and would erase the fact that the asset was volatile. The warning is
still recorded, so the diagnostic survives the arm that does not change the status.

## Example 3: Common pitfalls and edge cases

## Corner Cases

Ten ways to make this change and still be wrong. Most are the same failure as the original bug,
relocated.

| # | Case | Symptom if wrong | Cause | Correction | The assertion that catches it |
|---|---|---|---|---|---|
| P1 | **Status set without `set_status`** | Store says `Ready`, memory says `Expired` — the original bug, one layer down | `lock.status = Status::Expired` updates the field but not `metadata`, and `save_to_store` writes `metadata` | Go through `AssetData::set_status` (`:1183`), as the `Ready` arm does | U2 asserts `metadata.status()`, not just `status()` |
| P2 | **Warning added after persistence again** | Stored sidecar says `Expired` with no reason; an operator cannot tell a stale-dependency completion from an ordinary expiry | The log entry is left in `finish_run_with_result`, or added after the lock is released | Add it inside the same locked decision as the status | U3, and I2 re-reads it from the store |
| P3 | **The `:2224` fallback call site is missed** | A run that finished without `evaluate` finalizing skips the rule | The rename is applied at the definition but not at the second call site | Rename both; the fallback *gains* the rule deliberately — it does not persist, so there is no ordering cost | Compile error if `try_to_set_ready` is gone; U6 asserts the behaviour |
| P4 | ~~Assuming the store refuses a non-`Ready` status~~ | — | — | **This row was wrong and is withdrawn.** There *is* an effective status gate, one call down in `serialize_to_binary`. It is now blocking finding B1, not a pitfall | I2 — which cannot pass until B1 is resolved |
| P5 | **`expiration_time` diverges between arms** | An `Expired` asset carries no expiration, so `finish_run_with_result`'s scheduling step behaves differently than for `Ready` | The new arm omits `set_expiration_time_from` / `lock.expiration_time` | Mirror the `Ready` arm exactly | A test asserting `expiration_time()` agrees for the two arms given identical metadata |
| P6 | **Cascade called while holding the `data` lock** | Deadlock or a hang under `#[tokio::test]` | The branch is written inside the existing locked read block instead of after it | Take the facts under the lock, release, then cascade — the shape `evaluate` already uses for `track_asset` | I4 with a test timeout; a hang is the failure |
| P7 | **Cascade fired for a non-keyed asset** | Work done against a key that does not exist, or a panic constructing `DependencyKey` | The branch checks `stale_dependency` but not keyedness | Three-way branch: volatile → nothing; stale **and keyed** → cascade; else → `track_asset` | I3, plus the existing non-keyed regression test staying green |
| P8 | **"Simplifying" to `expire()` / `mark_expired_status()`** | The status is never persisted, so the bug survives the fix | `mark_expired_status` (`:2920`) writes metadata only `if store.contains(&key)`, and at finalization time the entry does not exist yet — the guard is false and the write is skipped | Do not route through it. The status is decided in `finalize_status`; only the *cascade* is borrowed | I2 fails: the stored status is still `Ready` |
| P9 | **Dependency waits moved after finalization** | The flag is set after the decision reads it; the asset stays `Ready` | Restructuring `evaluate` so `apply_recipe` is not awaited to completion first | Keep `evaluate_recipe_outcome` fully awaited before finalization | I1/I5 fail — the parent is `Ready` where `Expired` is asserted |
| P10 | **Volatility checked after the stale-dependency branch** | A volatile asset becomes `Expired`, losing its volatility in the stored metadata | Branch order reversed or wrongly nested | `if volatile … else if stale_dependency … else …` | U4, I7 |

### Pitfall-to-test map

Every row above is claimed by at least one test; no pitfall relies on inference.

| Pitfall | Caught by |
|---|---|
| P1 status without `set_status` | U2 |
| P2 warning after persistence | U3 (in memory at finalization), I2 (read back from the store) |
| P3 fallback call site missed | Compile error, plus U6 |
| P4 assuming a status gate on writes | I2 — and the assumption is already disproved above |
| P5 `expiration_time` diverges | **U7** |
| P6 cascade under the lock | I4, which fails by timing out rather than asserting |
| P7 cascade for a non-keyed asset | I3 (non-keyed arm) and I4 (keyed arm), plus the existing regression test |
| P8 routing through `expire()` | I2 — the stored status stays `Ready` and the test fails |
| P9 waits moved after finalization | I1 and I5 |
| P10 volatility checked second | U4, I7 |

**One drafted pitfall was rejected.** A draft proposed guarding the ordering with
`debug_assert!(!lock.stale_dependency)` inside `finalize_status`. That asserts the negation of the
case the design exists to handle: it would fire on every stale-dependency evaluation in a debug
build, which is every test run. The ordering property is real but belongs in a test (P9's row), not
in an assertion that contradicts the feature.

## Test Plan

Conventions per `.claude/skills/liquers-unittest/`: `#[tokio::test]` for async,
`-> Result<(), Box<dyn std::error::Error>>` where `?` is used, no `unwrap`/`expect` outside tests,
typed error constructors, no default match arms, `type CommandEnvironment` aliased before any
`register_command!`.

### Unit tests — `liquers-core/src/assets.rs`

All six construct the asset the way the module's existing tests do —
`AssetData::<SimpleEnvironment<Value>>::new(id, query.into(), None, envref).to_ref()` — and install
the value **directly under the write lock**, not through `set_value` (see Verified Setup Facts).

| Test | Asserts |
|---|---|
| `finalize_status_without_stale_dependency_is_ready` | `status()` and `metadata.status()` are both `Ready` |
| `finalize_status_with_stale_dependency_is_expired_in_metadata_too` | Both are `Expired`. The metadata half is the regression that matters |
| `finalize_status_records_the_reason_before_persistence` | A `LogEntryKind::Warning` entry naming the expired dependency is present when finalization returns, with nothing persisted yet |
| `finalize_status_volatile_wins_over_stale_dependency` | `is_volatile` **and** `stale_dependency` set → `Volatile`, and the warning is still recorded |
| `finalize_status_without_data_is_error_regardless_of_flag` | `data = None` + flag set → `Error` |
| `finish_run_fallback_finalizes_with_the_same_rule` | An asset finishing through the `:2224` fallback with the flag set ends `Expired` |
| `finalize_status_expiration_time_agrees_across_arms` | Two assets with identical metadata, one with the flag and one without, report the same `expiration_time()` — the `Expired` arm mirrors the `Ready` arm rather than dropping it (P5) |

### Integration tests — `liquers-core/tests/expiration_integration.rs`

Scenario bodies are generic over the environment and run against both managers via `*_default` /
`*_immediate` wrappers, per `manager_parametric.rs`. I1 and I6 additionally need the
`SharedMemoryStore` wrapper.

| Test | Asserts | Status today |
|---|---|---|
| `scenario_cross_process_stale_dependency_recomputes` | A second environment over the same store re-runs the command — asserted by an evaluation **counter**, not by the value | New; the fix's payoff, untested today |
| `scenario_keyed_stale_dependency_is_stored_expired` | `store.get(key)` metadata reports `Expired` | New; **fails at HEAD** — this is the bug |
| `scenario_non_keyed_stale_dependency_writes_nothing` | No store entry, and no "cannot determine key" warning in the asset's log | New |
| `scenario_stale_dependency_cascade_expires_dependents` | A dependent of the key ends `Expired` | New; pins the Phase 2 branch |
| `scenario_stale_dependency_never_observable_as_ready` | On receipt of `ValueProduced`, the status is already `Expired` | New; the deterministic form of the window B1 called racy |
| `scenario_stale_dependency_recovery_from_store` | Normal reads decline; `get_any_status` returns the value (`Result<Option<State>>`); `to_override` promotes it | New; guards `expired-binary-read-safety` |
| `scenario_volatile_keyed_stale_dependency_stays_volatile` | Status `Volatile`, and the key is present in the store | New; declared `volatile: true` in the macro |

### Decision (g) — "no `Expired` notification" — and why it gets no test

Phase 2 decided this path must **not** send `AssetNotificationMessage::Expired`. A review asked for
a test asserting that absence, and it cannot be written honestly: notifications go through a
**`tokio::sync::watch` channel** (`assets.rs:518`, `subscribe_to_notifications` at `:1177`), which
retains only the latest value. A subscriber that polls is not guaranteed to observe every message
sent, so draining one and finding no `Expired` proves nothing — the message could have been sent
and coalesced away. A test built that way would pass on a broken implementation.

What is verifiable stands in its place, and is enough:

- **Positively**, I5 asserts the property the decision exists to protect: on receipt of
  `ValueProduced`, the status is already `Expired`. A subscriber never sees a value withdrawn
  because it never sees one offered.
- **Structurally**, the absence is a property of the code, not of a run: the new arm in
  `finalize_status` contains no `notification_tx.send(...)` call, which Phase 4 checks by reading
  the diff. `mark_expired_status` (`:2920`) remains the only sender of `Expired`.

This is recorded rather than quietly dropped: a decision with no test is worth naming as such, so
that the next reader does not assume it was forgotten.

### Guide-worthy material

**None.** Phase 1 and Phase 2 both concluded no guide, and Phase 3 does not change that: every test
here asserts an internal ordering property, and none of them shows a developer how to *do*
anything. The one workflow a caller performs — recovering a technically-expired result — is already
documented with the `*_any_status` family in `ASSETS.md`, and I6 guards that documentation rather
than extending it.

What Phase 5 should take from here instead is the **Verified Setup Facts** table: it is testing
knowledge about this codebase that three independent drafts got wrong, and `UNITTEST_GUIDE.md` is
where the `set_value`-persists trap and the shared-store pattern would stop the next person losing
an afternoon.

## Documentation and Learning Log

### Learning recorded during Phase 3

1. **`set_value` persists.** Three drafts used it as inert test setup. A test for "the reason is
   recorded before persistence" that sets up with `set_value` has already persisted, and passes
   while proving nothing. Setup helpers with side effects are worth checking before reuse.
2. **The obvious cross-process test cannot be written the obvious way.** `AsyncMemoryStore` owns its
   map, so two environments do not share a store by cloning it. The draft asserted this was
   "verified feasible against existing tests"; the test it cited builds **one** environment. A
   feasibility claim is worth checking against the cited evidence, not just against plausibility.
3. **The value is the same either way.** A cross-process fast-track test that asserts on the
   returned value passes whether or not the fix works, because the recomputed value equals the
   stale one. Only an evaluation counter distinguishes them. The assertion has to be able to fail.
4. **A pitfall table can propose an assertion that contradicts the feature** (the rejected
   `debug_assert!`). Generated review material needs the same reading as generated code.

## References

- Phase 1: `./phase1-high-level-design.md` · Phase 2: `./phase2-architecture.md`
- `liquers-core/src/assets.rs` — `finalize_status` (`:1818`), `evaluate` (`:2528`),
  `try_fast_track` (`:1048`), `save_to_store` (`:2604`), `mark_expired_status` (`:2920`)
- `liquers-core/tests/expiration_integration.rs` — `ToOverrideGateStore` (`:880`), the
  mid-evaluation expiry gate, and the recovery tests I6 must keep green
- `liquers-core/tests/manager_parametric.rs` — the `scenario_*` + `*_default` / `*_immediate` shape
