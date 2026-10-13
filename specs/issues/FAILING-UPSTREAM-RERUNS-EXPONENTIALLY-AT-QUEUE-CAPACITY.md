---
id: FAILING-UPSTREAM-RERUNS-EXPONENTIALLY-AT-QUEUE-CAPACITY
kind: issue
title: A failing upstream link re-runs exponentially often when the queue is at capacity
status: closed
priority: P1
complexity: M
area: [core/assets]
design: dependency-resolved-once-per-evaluation
created: 2026-10-12
github:
---
## Problem

On the queued `DefaultAssetManager` at its default capacity (4), a chain of keyed recipes whose
root fails (`l0` = a `fail` command, `l{i} = -R/data/l{i-1}.txt/-/upper`) runs the failing command
far more than once when the last link is evaluated cold. Measured on 2026-10-12 with a throwaway
probe (`SimpleEnvironment`, memory store):

| Links n | 3 | 4 | 5 | 6 | 7 | 8 | 10 | 20 |
|---|---|---|---|---|---|---|---|---|
| `fail` runs | 2 | 4 | 8 | 16 | 29 | 54 | 240 | did not finish in 60 s |

The same chain runs `fail` once with queue capacity 1000 and once under `ImmediateAssetManager`, so
the multiplication appears only once the queue is full and dependencies are drained inline.

Likely cause (not confirmed): the plan pre-pass drains the dependency to `Error`; the `GetAsset`
step then resolves it again, and `DefaultAssetManager::get_dependency_asset`
(`liquers-core/src/assets.rs`) discards the fresh `Error` asset and recomputes it. Each level does
this for the level below, which doubles the work per link.

## Problem Example

```text
recipes: data/l0.txt = fail ; data/l{i}.txt = -R/data/l{i-1}.txt/-/upper  (i = 1..10)
evaluate("-R/data/l10.txt") on the default (queued) manager
today:    the error is returned after `fail` has run 240 times
expected: the error is returned after `fail` has run once
```

## Impact

- **Affected:** any server (axum uses the queued manager) whose recipe chain deeper than four
  links has a failing upstream.
- **Severity:** the work grows exponentially with depth; at 20 links a request effectively hangs.
  A failing upstream is exactly when users retry, which multiplies it again.
- **Workaround:** none in configuration short of a very large queue capacity.

## Expected behaviour

The rule the maintainer set on 2026-10-12 (recorded in `deep-chain-evaluation-stack-depth`,
decision 1): within one evaluation of an asset, each dependency is evaluated **at most once**, and
when a dependency fails its dependents fail as quickly as possible without re-running it. An asset
in `Error` may be evaluated again by a *later* evaluation (non-deterministic commands such as an LLM
call, network errors), so the failure is not remembered across evaluations. Here: `fail` runs once
per top-level request, at any chain length and queue capacity.

## Discovery

Found on 2026-10-12 while verifying `EVALUATING-A-DEEP-CHAIN-TOP-DOWN-OVERFLOWS-THE-STACK` for the
`deep-chain-evaluation-stack-depth` bulk design, which does not fix it (its AC-4 keeps the queued
failure chain at two links because of it). Possibly the same root cause as
`REFUSED-DEPENDENCY-RECOMPUTED-TWICE-DURING-A-DEPENDENT-EVALUATION` (a dependency re-run inside one
dependent evaluation); not merged because that one was observed on the refusal path at any capacity.

**Rule refined on 2026-10-12** (`dependency-resolved-once-per-evaluation`, decision 1): the rule
quoted above covers the whole top-level evaluation, not one asset's run. Within it, each dependency,
direct or transitive, is evaluated at most once and has one version; it is carried by an
*evaluation session*.

## Resolution

Fixed 2026-10-13 by `design/dependency-resolved-once-per-evaluation/` (orest-d/liquers#127). The
cause was a double resolution: the plan pre-pass resolved a dependency, ran it to `Error` at queue
capacity, and the step that reads it resolved it again; `DefaultAssetManager::get_dependency_asset`
then evicted the failed asset and ran it again, doubling the work per link. An evaluation session
(`liquers-core/src/evaluation_session.rs`) now pins the asset of each dependency's first resolution
for the whole top-level evaluation, so a failed dependency fails its dependents without running
again; a later evaluation runs it afresh.

Evidence: `liquers-core/tests/evaluation_session.rs` — `failing_chain_runs_root_once_at_default_capacity`
and `…_at_capacity_one` (10 links: `fail` once; 240 and 1024 runs before),
`shared_failing_root_runs_once_in_lattice`, `long_failing_chain_finishes_promptly` (20 links),
`failure_reruns_in_a_later_evaluation`; `deep_chain_evaluation.rs`
`queued_failing_root_of_100_link_chain_runs_once`.
