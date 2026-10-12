# Phase 3: Examples & Use-cases — dependency-resolved-once-per-evaluation

All tests are runnable. The recipe fixtures follow `liquers-core/tests/dependency_chain_scaling.rs`
`chain_env`: `data/recipes.yaml` in an `AsyncMemoryStore`. Counting commands use an
`Arc<AtomicUsize>`:
- `fail` increments and returns `Error::general_error("leaf failed")`;
- `upper` and `join` increment per test;
- `tick` is `volatile: true` and returns its count.

Queries were checked with `liquers-validate`.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Failing chain | root runs once, error propagates | AC-1 |
| 2 | Example | Shared root and volatile value | one asset per identity per evaluation | AC-2, AC-7 |
| 3 | Example | Deep chains | depth independent of length | AC-9, AC-10, AC-14 |
| 4 | Test | `tests/evaluation_session.rs` (10 tests) | session rules on both managers | AC-1 to AC-3, AC-5 to AC-8, AC-15 |
| 5 | Test | `tests/deep_chain_evaluation.rs` (8 tests) | stack depth, warm upstream, cycles, cancellation | AC-9 to AC-14 |
| 6 | Test | `tests/dependency_audit_integration.rs` (tightened) | refused dependency recomputed once | AC-4 |
| 7 | Test | unit tests in `evaluation_session.rs`, `assets.rs`, `plan.rs` | resolver, lifetime, graph | AC-6, AC-5, AC-10, AC-11 |

## Example 1: A failing chain

`l0.txt = fail`, `l{i}.txt = -R/data/l{i-1}.txt/-/upper`. A cold evaluation of `l10` on
`SimpleEnvironment`:

1. `l10`'s run begins session S.
2. The pre-pass resolves `-R/data/l9.txt` and pins it in S.
3. At capacity, `l9` goes on `l10`'s local queue and inherits S. The drain spawns it, and `l9`
   recurses the same way down to `l0`, which fails.
4. `l1`'s `GetAsset` step resolves `l0` again, gets the pinned failed asset, and
   `wait_for_dependency` returns `leaf failed`. The same happens at each level.

```rust
let asset = envref.evaluate("-R/data/l10.txt").await?;
let state = asset.get().await?;                 // a failed asset is an Ok error-state
assert_eq!(state.status(), Status::Error);
let error = state.value_state().err().expect("failed");
assert!(error.to_string().contains("leaf failed"));   // l0's own error, unchanged text
assert_eq!(fail_runs.load(SeqCst), 1);
```

## Example 2: A shared root and a volatile value (delta)

- **Shared root:** `a{i}`/`b{i} = -R/data/a{i-1}.txt/-/join-~X~-R/data/b{i-1}.txt~E` over
  `r.txt = fail`. Evaluating `a10`: `a9` and `b9` are separate runs in S, so each resolves `a8` and
  `b8` into S. The second one gets the first one's pin, so `r` runs once.
- **Volatile:** with `v.txt = tick`, `a.txt = -R/data/v.txt/-/upper` and the same for `b`, and
  `c.txt = -R/data/a.txt/-/join-~X~-R/data/b.txt~E`:
  - Evaluating `c` runs `tick` once, and `a` and `b` see the same value.
  - A second evaluation of `c` runs it again.
  - Volatility makes `a`, `b` and `c` volatile too, so none of them is in a map. Only the pin keeps
    them single.

## Example 3: Deep chains

`l0.txt = make_text`, 100 links, evaluated on a `std::thread` with `stack_size(1 << 20)`:
- **Queued** (default capacity and 1): every locally queued run is spawned, so each task holds one
  link.
- **Inline** (`ImmediateEnvironment` in `futures::executor::block_on`): `get` settles `l100`, begins
  S, primes `l0..l99` bottom-up (each pinned, each finding its dependency available), then runs
  `l100`.
- **AC-14:** with `l0 = fail`, priming stops at `l0` and returns `leaf failed`;
  `lookup_key_asset(l1)` is `None`.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| Two runs in one session resolve one identity at once | one resolver runs, the other waits for its asset | `resolve_runs_the_resolver_once` |
| The resolver fails (lookup error) | nothing pinned; the next call resolves again | `resolve_error_pins_nothing` |
| A cycle through a pinned identity | `Circular dependency detected`, no wait on its own cell | `cycle_is_reported_by_both_managers` |
| A session's last run ends | the session is dropped (`Weak::upgrade` is `None`) | `evaluation_session_ends_with_its_last_run` |
| A dependency has already failed during the pre-pass | the rest of the plan is not scheduled | `failed_dependency_stops_the_rest_of_the_plan_inline` |
| A dependent is cancelled while a spawned child runs | dependent `Cancelled`, child `Ready` | `queued_cancelled_dependent_lets_dependency_finish` |

## Test Plan

**Integration, `liquers-core/tests/evaluation_session.rs` (new):**
- `failing_chain_runs_root_once_at_default_capacity` (AC-1): 10 links, `fail` once, `leaf failed`.
- `failing_chain_runs_root_once_at_capacity_one` (AC-1): the same with `with_job_capacity(1)`.
- `shared_failing_root_runs_once_in_lattice` (AC-2): depth 10, `fail` once, `a10` `Error`.
- `long_failing_chain_finishes_promptly` (AC-3): 20 links under `tokio::time::timeout(10 s)`, `fail`
  once.
- `failure_reruns_in_a_later_evaluation` (AC-5): `l3` evaluated twice; `fail` totals 1, then 2.
- `command_reading_dependency_twice_gets_one_asset` (AC-6): an async command calls
  `ctx.get_dependency_state` twice on `-R/data/l0.txt`; both `Err(leaf failed)`, `fail` once.
- `volatile_dependency_has_one_value_per_evaluation` (AC-7): queued; `tick`, `a`, `b` once each;
  `c`'s value is the same `tick` value twice; a second `c` evaluation raises `tick` to 2.
- `volatile_dependency_has_one_value_per_evaluation_inline` (AC-7): the same on
  `ImmediateEnvironment`.
- `concurrent_evaluations_join_the_first` (AC-8): `l0` is an async `gated_fail` awaiting a
  `tokio::sync::Notify`. Two `evaluate("-R/data/l5.txt")` calls are started and the gate is
  released; `fail` once, both `Error` with `leaf failed`.
- `failed_dependency_stops_the_rest_of_the_plan_inline` (AC-15): on `ImmediateEnvironment`,
  `-R/data/l0.txt/-/join-~X~-R/data/y.txt~E` fails with `leaf failed`, and `y` never runs.

**Integration, `liquers-core/tests/deep_chain_evaluation.rs` (new, own binary so an abort hides
nothing else):**
- `queued_cold_100_link_chain_on_1mib_stack` (AC-9) and
  `queued_capacity_one_cold_100_link_chain_on_1mib_stack` (AC-9): `"TEXT"`, `upper` 100 times.
- `immediate_cold_100_link_chain_without_runtime_on_1mib_stack` (AC-10).
- `immediate_warm_upstream_is_not_reloaded` (AC-11): `upper` once,
  `lookup_key_asset(l98)` `None`.
- `cycle_is_reported_by_both_managers` (AC-12): `l0` reads `l40`; queued under a 20 s timeout.
- `queued_cancelled_dependent_lets_dependency_finish` (AC-13): capacity 1, gated `l0`, `l2`
  cancelled once `l1` waits; `l1` `Ready`, `l2` `Cancelled`.
- `immediate_failing_root_of_100_link_chain_runs_once` (AC-14): `leaf failed`, `fail` once,
  `lookup_key_asset(l1)` `None`.
- `queued_failing_root_of_100_link_chain_runs_once` (AC-1, AC-9): the queued counterpart on a
  1 MiB stack.

**Integration, `liquers-core/tests/dependency_audit_integration.rs`:**
- `restart_upstream_command_change_on_load_refuses` (AC-4): `assert_eq!(upper_calls, 2)` instead of
  `>= 2`; the comment naming `REFUSED-DEPENDENCY-…` is removed.
  `restart_deep_expired_status_on_load_refuses` is tightened the same way if its count is then
  exact.

**Unit:**
- `liquers-core/src/evaluation_session.rs`: `resolve_runs_the_resolver_once` (AC-6),
  `resolve_error_pins_nothing` (AC-5), `pin_keeps_the_first_asset` (AC-10).
- `liquers-core/src/assets.rs`: `evaluation_session_ends_with_its_last_run` (AC-5).
- `liquers-core/src/plan.rs`: `upstream_graph_is_post_order_without_volatile_keys` (AC-10, AC-11).

**Run:**
- `cargo test -p liquers-core --test evaluation_session --test deep_chain_evaluation --test dependency_audit_integration`
- Regression: `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests`,
  and the `liquers-web` Node loop.

## Learning Log

- The double resolution is pre-pass plus step, not two commands. Any fix that dedupes only inside
  one command (`seen` in the pre-pass) misses it.
- A memo per asset run bounds the blow-up but leaves 2-3 runs on a diamond (Phase 1, example 2).
  The identity scope must be the request.
- `ImmediateAssetManager` relied on the trait-default `wait_for_dependency`. That default hands on
  `get()`'s `Ok` error-state for an `Error` dependency, which was invisible while the inline
  manager returned `Err` from `get_dependency_asset`.
- Probe-measured run counts differ between `current_thread` and `multi_thread` because a freed slot
  lets a link run as its own task; tests assert exact counts only where the session makes them
  deterministic.
