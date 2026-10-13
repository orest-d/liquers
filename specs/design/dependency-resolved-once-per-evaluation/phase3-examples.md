# Phase 3: Examples & Use-cases — dependency-resolved-once-per-evaluation

All tests are runnable. **Fixture:** a generic `EnvironmentBuilder::<Value, (), K>` with
`K = Queued` or `Inline`, as in `liquers-core/tests/cache_strategy.rs`. It has an `Arc`
`AsyncMemoryStore` holding `data/recipes.yaml`, a `DefaultRecipeProvider`, and
`AssetManagerOptions::with_job_capacity(n)` where a test needs capacity 1.

**Counting commands** (`Arc<AtomicUsize>` each):
- `fail` returns `Error::general_error("leaf failed")`;
- `upper` and `join`;
- `tick`, registered as volatile;
- `gated_fail` / `gated_text`: async commands that wait on a latch, then fail or return text. The
  latch is an `AtomicBool` plus `Notify::notify_waiters`, or a `Semaphore`, so a second waiter is
  never stranded.

**Results:**
- The queued manager's `evaluate` returns `Ok(asset)` holding an `Error` state.
- The inline manager's `evaluate` returns the `Err`. Tests on it use `expect_err`.

Every test that can hang at HEAD runs under `tokio::time::timeout`. Deep tests run on a
`std::thread` with `stack_size(1 << 20)`; the queued ones build a `current_thread` runtime on that
thread, because a multi-thread runtime would run tasks on 2 MiB workers. Queries were checked with
`liquers-validate`.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Failing chain | root runs once, error propagates | AC-1 |
| 2 | Example | Shared root, volatile value | one asset per identity per evaluation | AC-2, AC-7 |
| 3 | Example | Deep chains, priming | depth independent of length | AC-9 to AC-11, AC-14 |
| 4 | Test | `tests/evaluation_session.rs` (11) | session rules on both managers | AC-1 to AC-3, AC-5 to AC-8, AC-15, AC-16 |
| 5 | Test | `tests/deep_chain_evaluation.rs` (8) | stack depth, warm upstream, cycles, cancellation | AC-1, AC-9 to AC-14 |
| 6 | Test | `tests/dependency_audit_integration.rs` | refused dependency recomputed once | AC-4 |
| 7 | Test | unit tests | resolver, lifetime, graph | AC-6, AC-5, AC-10, AC-11 |

## Example 1: A failing chain (queued)

`l0.txt = fail`, `l{i}.txt = -R/data/l{i-1}.txt/-/upper`. A cold evaluation of `l10`:

1. `l10`'s run begins session S.
2. The pre-pass resolves `-R/data/l9.txt` and pins it in S.
3. At capacity, `l9` goes on `l10`'s local queue with S adopted. The drain spawns it, and `l9`
   recurses the same way down to `l0`, which fails.
4. Each step's second resolution gets the pinned failed asset, and `wait_for_dependency` returns
   `leaf failed`.

```rust
let asset = envref.evaluate("-R/data/l10.txt").await?;   // queued: Ok(asset)
let state = asset.get().await?;                          // an Ok error-state
assert_eq!(state.status(), Status::Error);
assert!(state.value_state().err().expect("failed").to_string().contains("leaf failed"));
assert_eq!(fail_runs.load(SeqCst), 1);
```

## Example 2: A shared root and a volatile value (delta)

- **Shared root:** `a{i}`/`b{i} = -R/data/a{i-1}.txt/-/join-~X~-R/data/b{i-1}.txt~E` over
  `r.txt = fail`. Evaluating `a10`: `a9` and `b9` are separate runs in S, so the second one to
  resolve `a8` or `b8` gets the first one's pin, and `r` runs once.
- **Volatile:** the recipes are:
  - `v.txt = tick`, with recipe-level `volatile: true`, because volatility comes from the recipe
    flag, not from the command (`DependencyWalk`, plan.rs);
  - `a.txt = -R/data/v.txt/-/upper`, and the same for `b`;
  - `c.txt = -R/data/a.txt/-/join-~X~-R/data/b.txt~E`.

  Evaluating `c` gives `tick` 1, `upper` 2, `join` 1; `a` and `b` read the same `v`.
- **Why the pin matters for `v`:** `v`'s volatile key gets a fresh asset on every manager request.
  `a`, `b` and `c` are not volatile: they enter the key map and are reused normally. Only the pin
  keeps `v` single.
- **Re-evaluation:** a second evaluation of `c` must re-read `v`. `c` is then served from the map,
  so the test evaluates `-R/data/c.txt` after removing `c`, `a` and `b` (`AssetManager::remove`).
  `tick` then reaches 2.

## Example 3: Deep chains and priming

`l0.txt = make_text`, 100 links:
- **Queued** (default capacity and 1): every locally queued run is spawned, so each task holds one
  link.
- **Inline** (`ImmediateEnvironment` under `futures::executor::block_on`, no tokio): `get` settles
  `l100` (not finished, not fast-trackable), begins S and primes.
  - Priming walks `upstream_graph` **top-down** from `l100`'s children, settling each key.
  - It stops at a key that is finished or fast-tracked (available) and collects the others.
  - It then runs the collected keys in post-order, each adopted and pinned, so each finds its
    dependency available and nests one level.
  - Then it runs `l100`.
- **AC-11:** with `l0..l99` persisted, the walk settles `l99`, which is fast-tracked, and stops.
  `l0..l98` are never touched.
- **AC-14:** with `l0 = fail`, the walk settles `l99..l0`. That creates assets but does not run
  them. `l0` fails, and priming returns `leaf failed`; `l1..l100` stay unevaluated (neither `Ready`
  nor `Error`), and `upper` is 0.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| Two runs in one session resolve one identity at once | one resolver runs; the other gets its asset | `resolve_runs_the_resolver_once` |
| The resolver fails (lookup error) | nothing pinned; the next call resolves again | `resolve_error_pins_nothing` |
| A recipe cycle | rejected while planning (`Circular dependency detected`, `analyze_plan_dependencies`) | `recipe_cycle_is_reported_by_both_managers` |
| A dynamic cycle (a command requests its own upstream key) | `ErrorType::DependencyCycle` from `register_scheduled_dependency`, before the cell; no hang | `dynamic_cycle_is_reported_without_hanging` |
| A session's last run ends | the session is dropped | `evaluation_session_ends_with_its_last_run` |
| A dependent is cancelled while its spawned child runs | dependent `Cancelled`, child `Ready`, the gated command entered once | `queued_cancelled_dependent_lets_dependency_finish` |

## Test Plan

**`liquers-core/tests/evaluation_session.rs` (new):**
- `failing_chain_runs_root_once_at_default_capacity` (AC-1): 10 links; `fail` once, `leaf failed`.
- `failing_chain_runs_root_once_at_capacity_one` (AC-1).
- `shared_failing_root_runs_once_in_lattice` (AC-2): depth 10 under a 20 s timeout; `fail` once.
- `long_failing_chain_finishes_promptly` (AC-3): 20 links within 10 s; `fail` once.
- `failure_reruns_in_a_later_evaluation` (AC-5): `l3` twice; `fail` 1, then 2.
- `command_reading_dependency_twice_gets_one_asset` (AC-6): an async command calls
  `ctx.get_dependency_state` twice on `-R/data/l0.txt`; both `Err(leaf failed)`; `fail` once.
- `volatile_dependency_has_one_value_per_evaluation` (AC-7): queued; the counts of Example 2, both
  `join` inputs equal, `tick` 2 after the re-evaluation.
- `volatile_dependency_has_one_value_per_evaluation_inline` (AC-7): the same on `Inline`.
- `concurrent_evaluations_join_the_first` (AC-8): `l0 = gated_fail`; two `evaluate("-R/data/l5.txt")`
  started, then the latch opens; `fail` once, both `Error` with `leaf failed`, under a timeout.
- `failed_dependency_stops_the_rest_of_the_plan_inline` (AC-15): on `Inline`,
  `-R/data/l0.txt/-/join-~X~-R/data/y.txt~E` returns `Err(leaf failed)`; `y` never runs.
- `dynamic_cycle_is_reported_without_hanging` (AC-16): both managers; `k.txt = loop_cmd`, whose
  command calls `ctx.get_dependency_state("-R/data/k.txt")`; `ErrorType::DependencyCycle`
  within 10 s.

**`liquers-core/tests/deep_chain_evaluation.rs` (new, own binary so an abort hides nothing else):**
- `queued_cold_100_link_chain_on_1mib_stack` and `queued_capacity_one_cold_100_link_chain_on_1mib_stack`
  (AC-9): `"TEXT"`, `upper` 100.
- `queued_failing_root_of_100_link_chain_runs_once` (AC-1, AC-9).
- `immediate_cold_100_link_chain_without_runtime_on_1mib_stack` (AC-10).
- `immediate_warm_upstream_is_not_reloaded` (AC-11): `l0..l99` persisted by a first environment,
  replayed into a fresh `ImmediateEnvironment` over the same store; `upper` 1;
  `lookup_key_asset(l_i)` `None` for every `i` in `0..=98`.
- `recipe_cycle_is_reported_by_both_managers` (AC-12): `l0` reads `l40`; queued under 20 s.
- `queued_cancelled_dependent_lets_dependency_finish` (AC-13): capacity 1, `l0 = gated_text`, `l2`
  cancelled once `l1` waits, then the latch opens; `l1` `Ready`, `l2` `Cancelled`, `gated_text`
  entered once (the re-park at HEAD runs it twice).
- `immediate_failing_root_of_100_link_chain_runs_once` (AC-14): `Err(leaf failed)`, `fail` once,
  `upper` 0, `l1`'s status neither `Ready` nor `Error`.

**`liquers-core/tests/dependency_audit_integration.rs`:**
- `restart_upstream_command_change_on_load_refuses` (AC-4): `assert_eq!(upper_calls, 2)`.
- `restart_deep_expired_status_on_load_refuses`: tightened the same way if its count is then exact.

**Unit tests:**
- `evaluation_session.rs`: `resolve_runs_the_resolver_once` (AC-6), `resolve_error_pins_nothing`
  (supports AC-6), `pin_keeps_the_first_asset` (AC-10).
- `assets.rs`: `evaluation_session_ends_with_its_last_run` (AC-5).
- `plan.rs`: `upstream_graph_is_post_order_without_volatile_keys` (AC-10, AC-11).

**Run:** `cargo test -p liquers-core --test evaluation_session --test deep_chain_evaluation --test dependency_audit_integration`,
then the regression set in Phase 4, step 11.

## Learning Log

- The double resolution is pre-pass plus step, not two commands. Dedupe inside the pre-pass
  (`seen`) does not see it.
- A memo per asset run bounds the blow-up but leaves 2-3 runs on a diamond; the scope has to be
  the request.
- `ImmediateAssetManager` relied on the trait-default `wait_for_dependency`, which hands on `get()`'s
  `Ok` error-state. That was invisible while `get_dependency_asset` returned `Err`.
- Volatility is a recipe flag: a volatile *command* inside a keyed recipe does not make the key
  volatile. That is how `RESOURCE-STEP-VOLATILITY-IS-NEVER-CHECKED` shows up.
- A recipe cycle never reaches evaluation; the plan walk rejects it. Only a dynamic cycle exercises
  the runtime check.
- Run counts differ between `current_thread` and `multi_thread` at HEAD; tests assert exact counts
  only where the session makes them deterministic.
