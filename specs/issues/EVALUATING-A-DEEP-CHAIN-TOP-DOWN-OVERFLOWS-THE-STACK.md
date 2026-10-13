---
id: EVALUATING-A-DEEP-CHAIN-TOP-DOWN-OVERFLOWS-THE-STACK
kind: issue
title: Evaluating the last link of a deep recipe chain overflows the thread stack
status: closed
priority: P2
complexity: M
area: [core/assets]
design: dependency-resolved-once-per-evaluation
created: 2026-10-08
github:
---

## Problem

Evaluating the last link of a chain of keyed recipes (`l{i} = -R/data/l{i-1}.txt/-/upper`), with
nothing evaluated before it, aborts with a stack overflow between 20 and 30 links. The run was a
debug build on the default 2 MB test-thread stack, with `SimpleEnvironment`. Each link's
evaluation waits on its dependency's evaluation inside the same task, so the poll recursion
depth grows with the chain. Evaluating the links one at a time, bottom-up, works for any length.

Re-measured at HEAD on 2026-10-12 (debug, `current_thread` runtime on a 2 MiB thread): the queued
`SimpleEnvironment` (job capacity 4) works up to 26 links and aborts at 27; `ImmediateEnvironment`
works up to 9 and aborts at 10. In a release build: 190 / 200 (queued) and 35 / 40 (inline). At job
capacity 1000, where every dependency runs as its own task, 200 links pass. Locally queued
dependencies are run with `Box::pin(dep.run(None)).await` inside the dependent
(`DefaultAssetManager::drain_dependencies`, `wait_for_dependency`), and the inline manager nests
`run_inline` per link (`ImmediateAssetManager::get`).

The dependency *analysis* is not involved: since `dependency-chain-analysis-cost` it is iterative
(a 2 000-link analysis runs on the default stack), and so is the stored-records walk.

## Problem Example

`data/recipes.yaml` holds `l0.txt = make_text` and `l{i}.txt = -R/data/l{i-1}.txt/-/upper`. With
nothing evaluated, `envref.evaluate("-R/data/l30.txt").await?.get().await` under `SimpleEnvironment`
aborts with `has overflowed its stack`. It should return `"TEXT"`, as evaluating `l0` to `l30` one
at a time does.

## Impact

- **Affected:** any deployment whose recipe chains are deeper than a few dozen links, on a cold
  evaluation of the end of the chain.
- **Severity:** the process aborts, which cannot be caught as an error.
- **Workaround:** evaluate upstream links first, or run with a larger stack (`RUST_MIN_STACK`,
  or a runtime with bigger worker stacks).

## Expected behaviour

Evaluation depth bounded independently of chain length. For example, a dependency is evaluated
as a separate task that the dependent awaits, rather than inline in the dependent's poll; or
evaluation is scheduled bottom-up.

## Discovery

Found on 2026-10-08 while implementing `dependency-chain-analysis-cost` (Phase 4, Step 5): a test
evaluating a 50-link chain top-down overflowed. A probe over a fresh chain environment passed at 10
and 20 links and overflowed before 30. The tests in `liquers-core/tests/dependency_audit_integration.rs`
evaluate link by link for this reason.

## Resolution

Fixed 2026-10-13 by `design/dependency-resolved-once-per-evaluation/` (orest-d/liquers#127), which
superseded `deep-chain-evaluation-stack-depth`. The queued manager now runs every locally queued
dependency in a task of its own (`DefaultAssetManager::run_claimed_in_own_task`), so each link
holds its own stack; the inline manager primes a cold top-level asset's static upstream bottom-up
(`ImmediateAssetManager::prime_upstream` over `plan.rs` `upstream_graph`), so the chain nests one
level instead of one per link.

Evidence: `liquers-core/tests/deep_chain_evaluation.rs` — 100-link cold chains on a 1 MiB stack:
`queued_cold_100_link_chain_on_1mib_stack`, `queued_capacity_one_cold_100_link_chain_on_1mib_stack`,
`immediate_cold_100_link_chain_without_runtime_on_1mib_stack` (no tokio runtime),
`immediate_warm_upstream_is_not_reloaded`, `immediate_failing_root_of_100_link_chain_runs_once`
(HEAD overflowed at 27 links queued and 10 inline on 2 MiB). Residual limit, filed:
`INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS` (inline chains through a dynamic or volatile link,
and `apply_queued`).
