# Phase 1: High-Level Design — dependency-resolved-once-per-evaluation

## Purpose

A top-level request for an asset (an *evaluation*) can run the same upstream asset many times,
exponentially often in the worst case, and on a long chain it can overflow the thread stack. Give
every evaluation an **evaluation session**: within it, each dependency resolves to one asset and is
evaluated at most once, a failure reaches its dependents without being re-run, and the depth of the
call stack does not grow with the length of the chain.

## Problem Example

All examples use `data/recipes.yaml` over a memory store and `SimpleEnvironment` (the queued
`DefaultAssetManager`, default capacity 4) unless they name another manager. They were measured at
HEAD on 2026-10-12 with throwaway probe tests that are not committed.

**1. A failing chain re-runs its root.** `l0.txt = fail`, `l{i}.txt = -R/data/l{i-1}.txt/-/upper`:

```rust
let asset = envref.evaluate("-R/data/l8.txt").await?;
asset.get().await?;      // a no-value state; asset.status() is Error
```

| Links n | 3 | 4 | 5 | 6 | 8 | 10 | 20 |
|---|---|---|---|---|---|---|---|
| `fail` runs | 2 | 4 | 6 | 14 | 49 | 240 | not finished in 60 s |

The 10 and 20 figures are the issue's (`current_thread`); the rest are from `multi_thread`. The
interpreter's pre-pass (`interpreter.rs` `schedule_plan_dependencies`) resolves `l_{k-1}`. At
capacity, the dependent's own task runs it to `Error` (`evaluate_local_queue`). Then the
`Step::GetAsset` step resolves it again (`Context::get_dependency_state`). The second resolution
(`DefaultAssetManager::get_dependency_asset`) treats `Error` as stale, evicts the asset and runs it
again, and every level repeats this for the level below. `REFUSED-DEPENDENCY-RECOMPUTED-TWICE-…` is
the same double resolution on the refusal path: the second one fast-tracks the not yet finished
asset again, is refused again, and schedules a second run.

**2. A shared failing root re-runs once per dependent.** `r.txt = fail`,
`a0`/`b0 = -R/data/r.txt/-/upper`, `a{i}`/`b{i} = -R/data/a{i-1}.txt/-/join-~X~-R/data/b{i-1}.txt~E`,
evaluating `-R/data/a{n}.txt`:

| Depth n | 1 | 2 | 3 | 4 | 10 |
|---|---|---|---|---|---|
| HEAD | 3 | 9 | 15 | not finished in 60 s | — |
| with a memo per asset run (the first proposal) | 2 | 2 | 2 | 3 | 3 |

A memo per asset run is not enough: `a_i` and `b_i` are separate runs, so the second to reach `r`
evicts and re-runs it. Only state spanning the whole request reaches "once".

**3. A deep cold chain overflows the stack** (`EVALUATING-A-DEEP-CHAIN-TOP-DOWN-OVERFLOWS-THE-STACK`).
With `l0.txt = make_text`, evaluating `-R/data/l30.txt` cold aborts with `has overflowed its stack`
(debug, 2 MiB stack). The queued manager works up to 26 links, or up to 200 at capacity 1000, where
every dependency is its own task. The inline `ImmediateAssetManager` works up to 9. A locally queued
dependency runs inside its dependent's future (`DefaultAssetManager::drain_dependencies`,
`wait_for_dependency`), and the inline manager nests `run_inline` per link.

**Should:** example 1 runs `fail` once and returns its error (`leaf failed`); example 2 runs it once
at any depth; example 3 returns `"TEXT"` at any length with each `upper` run once.

## Scope and Acceptance Criteria

- **AC-1** A failing chain runs its root once
  WHEN `l0 = fail` and `-R/data/l10.txt` is evaluated cold on the queued manager, at default capacity and at capacity 1
  THEN `fail` has run exactly once and the asset ends `Error` with `leaf failed`
- **AC-2** A shared failing root runs once
  WHEN `-R/data/a10.txt` of example 2 is evaluated cold at default capacity
  THEN `fail` has run exactly once and `a10` ends `Error` with `leaf failed`
- **AC-3** A long failing chain finishes promptly
  WHEN example 1 is run with 20 links at default capacity
  THEN it finishes within 10 s and `fail` has run exactly once
- **AC-4** A refused dependency is recomputed once
  WHEN `restart_upstream_command_change_on_load_refuses` evaluates `l2` after `l0`'s command changed (on-load audit)
  THEN `upper` has run exactly twice, and the test asserts equality instead of "at least"
- **AC-5** A failure is not remembered across evaluations
  WHEN an evaluation of `-R/data/l3.txt` has failed with `fail` run once, and the same query is then evaluated again
  THEN `fail` runs again, once (two in total)
- **AC-6** A command reading one dependency twice gets one asset
  WHEN a command calls `Context::get_dependency_state` twice for the same query in one evaluation, and that dependency fails
  THEN both calls return the dependency's error and the dependency's command has run once
- **AC-7** A volatile dependency has one value per evaluation
  WHEN `c = join(a, b)`, where `a` and `b` both read `v` and `v`'s recipe is a volatile command that counts its runs, and `c` is evaluated
  THEN `v`'s command has run once and `a` and `b` received the same value; a second evaluation of `c` runs it again
- **AC-8** A concurrent request joins the first request's work
  WHEN two evaluations of `-R/data/l5.txt` (with `l0 = fail`) start together on the queued manager
  THEN `fail` has run exactly once and both evaluations end `Error` with `leaf failed`
- **AC-9** A deep cold chain does not overflow, queued manager
  WHEN a 100-link chain's last link is evaluated cold on a 1 MiB stack, at default capacity and at capacity 1
  THEN it returns `"TEXT"`, each link's command runs once, and nothing overflows
- **AC-10** A deep cold chain does not overflow, inline manager without a runtime
  WHEN the same runs under `ImmediateEnvironment` in `futures::executor::block_on` (no tokio) on a 1 MiB stack
  THEN it returns `"TEXT"`, each link's command runs once, and nothing overflows
- **AC-11** A warm upstream is not reloaded
  WHEN `l0..l99` are persisted and a fresh `ImmediateEnvironment` over that store evaluates `-R/data/l100.txt`
  THEN only `l100`'s command runs, and no link before `l99` becomes a live asset
- **AC-12** Cycles are still reported
  WHEN a link of a 40-link recipe cycle is evaluated, under either manager
  THEN it fails with HEAD's `Circular dependency detected` error, without hanging or overflowing
- **AC-13** Cancellation keeps its contract
  WHEN, at job capacity 1, a dependent waiting for a locally queued dependency is cancelled
  THEN the dependent ends `Cancelled` and the dependency still finishes `Ready` (`DEPENDENCIES_STATUS.md` Flow D)

Non-goals: payload-path dependencies (`Context::schedule_payload_dependency_asset`), which are
volatile by construction, in no map, and evaluated inline once per call; remembering a failure
across evaluations (decision 1 forbids it); a public API for sessions (none is planned); analysis
cost (`DEPENDENCY-ANALYSIS-REWALKS-UPSTREAM-PER-EVALUATION`); inline chains built only from
dynamic or volatile dependencies, which still nest (filed during implementation as
`INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS`).

## The evaluation session (observable contract)

How a session is represented and carried is Phase 2. What callers can observe:

1. **Begins** at a top-level entry: `EnvRef::evaluate` / `evaluate_immediately`, `AssetManager::get`,
   `get_asset`, a top-level `get_query_asset`, and `apply`. A command's `Context::submit`,
   `evaluate` and `get_dependency_state` act inside the session of the evaluation that runs the
   command.
2. **Pins:** the first resolution of a dependency in the session (keyed or a non-keyed query, direct
   or transitive, static or dynamic) fixes the asset. Every later request for the same dependency in
   the session gets that asset, whatever its status: `Error`, `Cancelled`, `Volatile` or `Expired`.
3. **Refreshes only at first resolution:** an `Error`, `Cancelled`, `Volatile` or `Expired` asset
   found in the manager is replaced by a fresh one only when the session first resolves that
   dependency. A dependency that expires later in the session is used stale, and its dependent is
   marked expired (today's execution-time policy in `wait_for_dependency`).
4. **Joining:** an asset already being evaluated for another session is waited for, not restarted.
   Its own upstream was resolved in the session that started it.
5. **Ends** when the top-level evaluation finishes. Nothing about it outlives the request, so the
   next evaluation may re-run a failed or volatile dependency (AC-5, AC-7).

## Core Interactions

- **Assets** (`assets.rs`): dependency resolution and local queues of both managers; the top-level entries.
- **Context** (`context.rs`): carries the session, so `submit` and `get_dependency_state` use it.
- **Interpreter and plan** (`interpreter.rs` pre-pass, `plan.rs` `DependencyWalk` for inline priming).
- Commands, query syntax, store and formats: untouched.

## Crate Placement

`liquers-core` only: sessions belong to the asset lifecycle and the `Context`, both core. No
dependency on `liquers-lib`. `liquers-web` (inline manager on wasm) and `liquers-py` inherit the
behaviour unchanged; the Phase 2 preflight confirms that no `pub` signature they use changes.

## Documentation Intent

- Reference: extend `specs/reference/DEPENDENCIES_STATUS.md` with the evaluation session (the five
  rules above), and correct its description of locally queued runs (step 5 "invokes `B.run()`
  inline", Flow D item 2).
- Reference: extend `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`. Beside the
  `submit` / `get_dependency_state` table, say that a repeated request in one evaluation returns the
  first asset, and drop "may inline-run locally queued work".
- Guide: extend `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` §11. A manager must honour the
  session and must not nest per chain link. Record the residual limit.
- Other documents: none.
- Documents to update: the three above (`affects_docs`); `specs/README.md` when the design completes.

## Open Questions

None open. The maintainer's decisions, which the later phases build on:

1. **Evaluation rule** (2026-10-12, from `deep-chain-evaluation-stack-depth` decision 1, refined the
   same day). Within one evaluation, each dependency is evaluated at most once, and a failed
   dependency fails its dependents without re-running. The scope is the **whole top-level
   evaluation**, not one asset's run (example 2). A failed or volatile asset may be evaluated again
   by a *later* evaluation: a command can be acceptably non-deterministic without being volatile,
   such as an LLM call, or fail on a network error. The same dependency never has two versions
   within one evaluation.
2. **Mechanism: an evaluation session** (2026-10-12, option A). Rejected: a memo per asset run
   (example 2); a request id stamped on assets (cannot reach volatile assets, which are in no map,
   and an asset shared by two requests carries one stamp); settling once up front and never evicting
   afterwards (volatile and dynamic dependencies still get two versions). That up-front walk survives
   as the way the inline manager fills the session (decision 4).
3. **Joining** (2026-10-12, part of option A): rule 4 above.
4. **Stack depth, from `deep-chain-evaluation-stack-depth`** (decisions 2-3, 2026-10-12). The queued
   manager runs every locally queued dependency as its own task, which the dependent awaits. The
   inline manager cannot spawn: it primes the static upstream bottom-up before the top-level run.
   When priming fails at `l_k`, the evaluation returns `l_k`'s error at once, and
   `l_{k+1}..l_n` stay unevaluated.
5. **Merge** (2026-10-12): this design absorbs `deep-chain-evaluation-stack-depth` (superseded),
   because the session is the mechanism both halves share.

## Design Dependencies

- `deep-chain-evaluation-stack-depth`: **superseded** by this design. Its Phase 1 scenarios are
  AC-9 to AC-13 here; its AC-4 is replaced by AC-1 to AC-3. Its Phase 2 to 4 are input to this
  design's Phases 2 to 4, not a plan to follow.
- `dependency-chain-analysis-cost` (complete): **requires**. Priming reuses its iterative
  `DependencyWalk`. AC-4 tightens its `restart_upstream_command_change_on_load_refuses`.
- `dependency-scheduling` (complete): **overlaps**. The pre-pass and local queues stay; the session
  removes the second resolution they made possible, and spawning changes where a local run executes.
- `asset-cancellation-outcome` (`in_review`): **overlaps**. AC-13 re-checks its Flow D promise.
- `core-value-enum-oversized` (draft): **overlaps**. Smaller futures raise the depth limit only by a
  constant.
