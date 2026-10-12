---
id: DEPENDENCY-RESOLVED-ONCE-PER-EVALUATION
kind: design
title: Each dependency resolved once per evaluation
form: compact
workflow: liquers-project
status: in_review
phase: high-level
readiness: needs-decision
autofix: not-eligible
area: [core/assets]
issues: [FAILING-UPSTREAM-RERUNS-EXPONENTIALLY-AT-QUEUE-CAPACITY, REFUSED-DEPENDENCY-RECOMPUTED-TWICE-DURING-A-DEPENDENT-EVALUATION]
merged: 2026-10-12
created: 2026-10-12
---
# Each dependency resolved once per evaluation

## Phase 1: High-Level Design

### Purpose

One evaluation of an asset resolves the same dependency more than once: the interpreter's plan
pre-pass schedules it, then the plan step that reads it schedules it again. When the first
resolution has already finished in `Error`, the second evicts it and runs it again, and on a chain
the repeats multiply level by level. Make every dependency resolve to one asset per evaluation, so a
failing upstream runs once and its dependents fail at once, as the maintainer's rule of 2026-10-12
requires (`deep-chain-evaluation-stack-depth`, decision 1).

### Problem Example

`data/recipes.yaml` holds `l0.txt = fail` and `l{i}.txt = -R/data/l{i-1}.txt/-/upper`. Nothing has
been evaluated yet. On `SimpleEnvironment` (queued `DefaultAssetManager`, default capacity 4):

```rust
let asset = envref.evaluate("-R/data/l8.txt").await?;
asset.get().await?;          // a no-value state; asset.status() is Error
```

**Today:** the error comes back after `fail` has run many times. Reproduced at HEAD on 2026-10-12
with a throwaway probe test (`multi_thread` runtime; the issue's `current_thread` numbers differ a
little because a freed slot sometimes lets a link run as its own task):

| Links n | 3 | 4 | 5 | 6 | 8 |
|---|---|---|---|---|---|
| `fail` runs | 2 | 4 | 6 | 14 | 49 |

**Should:** `fail` runs once, at any chain length and queue capacity, and the caller gets
`leaf failed`, the error `fail` returned.

**Root cause (confirmed).** For a link `l_k`, `interpreter.rs` `apply_plan` first runs
`schedule_plan_dependencies`, which calls `Context::schedule_dependency_asset` for `l_{k-1}`. At
capacity, `DefaultAssetManager::get_dependency_asset` queues it on `l_k`'s local queue, and
`Context::evaluate_local_queue` runs it inline to `Error`. Then the `Step::GetAsset` arm of
`do_step` calls `Context::get_dependency_state` → `submit` → `schedule_dependency_asset` again.
`get_dependency_asset` sees `Status::Error`, treats it as a stale terminal (its comment: "a failure
may be transient"), evicts it and schedules a fresh asset, so `l_{k-1}` runs a second time. Each
of those runs does the same to `l_{k-2}`, which doubles the work per link. Below capacity the
pre-pass spawns `l_{k-1}` as a job that is still `Processing` when the step re-resolves it, so the
eviction does not happen; the inline manager runs the dependency to completion in a different way
and also runs it once. That is why only a full queue shows it.

The same double resolution explains `REFUSED-DEPENDENCY-RECOMPUTED-TWICE-DURING-A-DEPENDENT-EVALUATION`:
the second resolution finds the refused asset not yet finished and calls `fast_track` again, which
refuses it again and schedules a second run.

**Fix verified as a probe.** A per-evaluation map in `Context` (dependency key → the `AssetRef` the
first resolution returned), consulted by `schedule_dependency_asset_with_key` before it calls
`get_dependency_asset`, brought `fail` to **1 run for every n** above, and brought
`dependency_audit_integration.rs` `restart_upstream_command_change_on_load_refuses` from 3 `upper`
runs to 2. With it applied, `cargo test -p liquers-core --lib --tests` passed in full. The probe was
reverted; Phase 2 designs the change properly.

**A per-asset memo is not enough (found 2026-10-12, after the draft).** A second probe built a
diamond lattice: `r = fail`, `a0`/`b0 = -R/data/r.txt/-/upper`, and
`a{i}`/`b{i} = -R/data/a{i-1}.txt/-/join-~X~-R/data/b{i-1}.txt~E`, then evaluated `a{n}` at default
capacity. At HEAD `fail` ran 3, 9 and 15 times for n = 1..3, and n = 4 did not finish in 60 s.
With the per-`Context` memo, it ran 2-3 times for every n up to 10. That bounds the growth, but it
does not reach "once": sibling dependents (`a_i`, `b_i`) each have their own `Context`, so the
second one to resolve a shared failed upstream evicts it and runs it again. Satisfying the rule for
shared upstreams needs state that spans the whole top-level request, not one asset's run. This
reopens the Phase 2 approach and the relation to `deep-chain-evaluation-stack-depth`; see the
discussion of 2026-10-12, pending a maintainer decision.

### Scope and Acceptance Criteria

- **AC-1** A failing upstream runs once at queue capacity
  WHEN `l0 = fail` and `-R/data/l10.txt` is evaluated cold on the queued manager, at default capacity and at capacity 1
  THEN `fail` has run exactly once and the asset ends `Error` with `leaf failed`
- **AC-2** A long failing chain finishes promptly
  WHEN the same is done with a 20-link chain at default capacity
  THEN it finishes well within 10 s (the issue saw no finish within 60 s) and `fail` has run exactly once
- **AC-3** A refused dependency is recomputed once
  WHEN `restart_upstream_command_change_on_load_refuses` evaluates `l2` after `l0`'s command changed (on-load audit)
  THEN `upper` has run exactly twice (`l1` and `l2`), and the test asserts equality, not "at least"
- **AC-4** A failure is not remembered across evaluations
  WHEN a top-level evaluation of `-R/data/l3.txt` has failed with `fail` run once, and the same query is evaluated again
  THEN `fail` runs again, once (two in total)
- **AC-5** A command reading one dependency twice gets one asset
  WHEN a command calls `Context::get_dependency_state` twice for the same query during one evaluation, and the dependency fails
  THEN both calls return the dependency's error and the dependency's command has run once
- **AC-6** Successful and below-capacity chains are unchanged
  WHEN a 10-link chain of `make_text` and `upper` is evaluated cold at default capacity and under `ImmediateEnvironment`
  THEN it returns `"TEXT"` and each link's command runs once, as at HEAD

Non-goals: payload-path dependencies (`schedule_payload_dependency_asset`): such an asset is
volatile by construction, in no map, and evaluated inline once per call; the stack depth of deep
chains (`deep-chain-evaluation-stack-depth`); remembering failures across evaluations (decision 1
forbids it).

Systems: `liquers-core` only (`context.rs`, with comments in `assets.rs` and `interpreter.rs`). No
command, query syntax, format or `pub` signature changes. `liquers-web` and `liquers-py` inherit
the fix and need no changes.

**Documentation intent.** Reference: `specs/reference/DEPENDENCIES_STATUS.md` gains the rule "one
asset per dependency per evaluation" next to its scheduling section, and
`specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` says, beside the `submit` /
`get_dependency_state` table, that a repeated request in one evaluation returns the asset from the
first one. No new guide. The `get_dependency_asset` comment in `assets.rs` ("the ONLY point where
such a dependency is refreshed") becomes true per evaluation and is reworded.

### Design Readiness

- **Readiness:** needs-decision (the diamond finding above: per-asset or per-request scope)
- **Automatic fixing:** not-eligible. The change is small and verified, but it sets a contract
  (what a second request for a dependency returns within one evaluation) that AC-5 makes visible to
  commands, so it needs this review.
- **Leading issue:** `FAILING-UPSTREAM-RERUNS-EXPONENTIALLY-AT-QUEUE-CAPACITY` (P1). Merged with
  `REFUSED-DEPENDENCY-RECOMPUTED-TWICE-DURING-A-DEPENDENT-EVALUATION` (P3) by overlap test **T1**
  (same root cause, shown by the probe). The issue had kept them apart because the refusal was seen
  at any capacity; the double resolution happens at any capacity, and only the `Error` eviction
  needs a full queue.
- **Explanation:** The root cause is confirmed and the fix is proven on the reproducer and the
  whole core suite. The remaining points have recommended answers that follow from decision 1.
- **Open questions (proposed resolutions, not blocking):**
  1. **Volatile dependencies.** With the map, a volatile dependency read twice in one evaluation
     (pre-pass, then step; or a command asking twice) is evaluated once and both readers see the same
     value. At HEAD it is evaluated once below capacity and twice at capacity. *Recommended:* once,
     by decision 1 ("each dependency is evaluated at most once"). One evaluation then sees one
     consistent value of a volatile input.
  2. **A command that retries a failed dependency in the same evaluation** (`get_dependency_state`
     again after handling the error) gets the same error back instead of a new run. *Recommended:*
     accept. Decision 1 puts retrying in a later evaluation, and at HEAD the retry ran only at
     capacity, so no command can rely on it.
  3. **Expired between pre-pass and step.** At HEAD a dependency that expires between the two
     resolutions is recomputed by the second one. With the map, the step gets the expired asset and
     `wait_for_dependency` uses its stale value and marks the dependent expired. That is the
     execution-time expiry policy that `wait_for_dependency` already documents. *Recommended:* accept.
- **Implementation detail (for Phase 2):** where the map lives (`Context`, shared across context
  clones like `submitted_dependencies`; one `Context` per run, created in `AssetRef` run and
  `interpreter.rs`), and whether a concurrent second request races the first (no lock held across
  `get_dependency_asset`; the race at worst repeats HEAD's behaviour).

### Design Dependencies

- `deep-chain-evaluation-stack-depth` (`in_review`): **required-by** for its AC-4 only. That design
  keeps its queued failure chain at 2 links and without a run count because of the leading issue.
  Once this design lands, the chain can be lengthened and the count asserted. Overlap **T2** (shared
  contract: decision 1). Not merged: that design changes where a claimed run executes
  (`drain_dependencies`, `wait_for_dependency`, `ImmediateAssetManager`), and this one changes
  how often a dependency is resolved (`Context::schedule_dependency_asset_with_key`). The change
  sites differ and either order works.
- `dependency-scheduling` (complete): **overlaps**. The pre-pass and local queues it introduced
  stay; this design removes the second resolution they made possible.
- `dependency-chain-analysis-cost` (complete): **overlaps**. Its
  `restart_upstream_command_change_on_load_refuses` assertion is tightened by AC-3.

## Phase 2: Architecture
### Solution
### Changes
### Risks

## Phase 3: Examples and Tests
### Examples
### Tests

## Phase 4: Implementation Plan
### Steps
### Validation
