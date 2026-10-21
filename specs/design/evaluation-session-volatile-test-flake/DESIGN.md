---
id: EVALUATION-SESSION-VOLATILE-TEST-FLAKE
kind: design
title: volatile_dependency_has_one_value_per_evaluation passes deterministically
form: compact
status: abandoned
area: [core/assets]
issues: [EVALUATION-SESSION-VOLATILE-TEST-INTERMITTENT]
created: 2026-10-17
---
# volatile_dependency_has_one_value_per_evaluation passes deterministically

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-17 backlog compaction. Phases 1-2
only: the cause is not established, so no test or plan is written yet.

> **Abandoned 2026-10-21 (bulk design, resumed under guide §11).** The source issue no longer
> reproduces at HEAD (`18732dd`) and is closed. The cause was hypothesis H1 below, and runtime
> volatility (`design/runtime-volatility/`, commit `b87faa1`, PR #142) removed the condition that
> exposed it: `a`, `b` and `c` now become volatile at run time because they read the volatile `v`,
> so their stored copies carry status `Volatile` and fast track refuses them ("Asset 1004 has stored
> status Volatile, fast-track disabled"). Evidence: the test binary built from `2bc4228` (where the
> flake was reported) failed 16/20 and 21/30 runs on this machine, every failure logging "Fast track
> successful for asset with key data/c.txt" in the second evaluation — the stored `Ready` copy of `c`
> written by a background save that landed after `manager.remove`. The HEAD binary passed 280 of
> 280 runs (queued, including under 4× parallel load) and 20 of 20 (inline). The product race H1
> describes (a late background save reviving a removed key) still exists for non-volatile keys and
> is filed as `REMOVED-OR-REPLACED-KEY-REVIVED-BY-A-LATE-BACKGROUND-SAVE`. The phases below are kept
> as the record of the diagnosis; the Consolidated Findings' first bullet ("`a`, `b` and `c` are
> volatile at plan time") was wrong — the log shows "Getting non-volatile asset for key data/c.txt".

## Phase 1: High-Level Design

### Purpose

`liquers-core/tests/evaluation_session.rs` `volatile_dependency_has_one_value_per_evaluation`
(`volatile_scenario::<Queued>`) fails in about half the runs at
`assert_eq!(count(&counters.tick), 2, "a later evaluation runs v again")`: the second evaluation of
`-R/data/c.txt` does not run the volatile `v` again. The `Inline` variant is not reported failing.
A red CI run without cause, and a real regression of the once-per-evaluation contract looking like
the same flake, are both costs.

### Problem Example

```bash
for i in $(seq 20); do
  cargo test -p liquers-core --test evaluation_session volatile_dependency_has_one_value_per_evaluation
done
```

Today: about half the runs fail with `left: 1, right: 2`. Should be: 20 of 20 pass, and a fault
that really reuses `v` across evaluations fails 20 of 20.

### Scope and Acceptance Criteria

- **AC-1** Deterministic
  - WHEN the test runs 50 times on the `Queued` manager
  - THEN it passes every time
- **AC-2** Still a real check
  - WHEN `v` is deliberately reused across evaluations (for example, the session is kept alive by
    a held `Context`)
  - THEN the test fails

### Design Readiness

- **Readiness:** phase2-blocked
- **Automatic fixing:** not-eligible — readiness is not `ready` (rule 5).
- **Leading issue:** **Blocking - cause unknown.** Reading the code does not identify which race
  produces `tick == 1`, and the fix differs per cause: a test-only wait, or a fix in the queued
  manager. Example: if a late background save restores a removed value (H1 below), the right fix
  is in `remove` or `save_to_store`, and a test-only wait would hide a product bug; if the first
  evaluation's finalization overlaps the second request (H2), only the test needs to wait.
- **Explanation:** Needs one instrumented run, which this design pass could not do. Under every
  hypothesis the expected behaviour is clear, so Phases 3-4 follow quickly once the cause is known.
- **Open questions:** the blocking question above.

### Design Dependencies

- `overlaps` open PR #142 (runtime volatility: a volatile value read at run time makes the result
  volatile). It changes which of `a`, `b`, `c` are volatile at run time; re-run the diagnosis on
  top of it.

### Consolidated Findings

Evidence gathered at HEAD (2026-10-17):

- `a`, `b` and `c` are volatile at plan time: each reads `data/v.txt` as a state, and
  `DependencySummary::merge` carries `v`'s `declared_volatile` (`liquers-core/src/plan.rs`,
  `merge_read`, "Volatile due to dependency"). So `get_resource_asset` builds a fresh volatile
  asset for each on every request (`get_volatile_resource_asset`), and each fresh asset begins a
  fresh session (`evaluation_session_or_begin`); no path re-joins the first session.
- The queued manager persists keyed results in a spawned task (`persist_with_status_tracking`,
  `save_in_background && !inline`), and `AssetRef` completion persists any keyed `Computed` result
  without checking `is_volatile`. The inline manager persists synchronously, which matches only the
  queued variant failing.
- `fast_track` only accepts a stored `Ready`/`Source`/`Override`, so a stored `Volatile` entry
  should not be served.

## Phase 2: Architecture

### Solution

Not chosen until the cause is known. Hypotheses to separate, in order:

- **H1 Late background save.** A spawned save of the first evaluation's `a`, `b` or `c` lands after
  `manager.remove` rewrote the entry as `Recipe`, and the second evaluation is served from it.
  Check: `--nocapture` output shows "Fast track successful" for one of the keys in the second
  evaluation; or wait for each first-evaluation asset's `persistence_status()` before `remove`
  and see the flake vanish. If confirmed, the defect is in `liquers-core/src/assets.rs`
  (`remove` versus an in-flight `save_to_store`, and possibly volatile keyed results being saved at
  all), and gets its own issue; the test then waits for persistence only if the contract allows it.
- **H2 First evaluation still finishing.** The second request reaches an asset or pinned value of
  the first evaluation that is finished but not yet released (a running `Context` keeps the session
  alive). Check: log the session pointer per `Context` (a temporary `eprintln!`); a shared pointer
  across the two evaluations confirms it. Fix: the test awaits the first evaluation's
  completion signal, or the manager releases the session before notifying `ValueProduced`.
- **H3 Something else** (for example the volatile join cache of the top-level query). Found by the
  same logs.

Diagnosis command: `cargo test -p liquers-core --test evaluation_session
volatile_dependency_has_one_value_per_evaluation -- --nocapture 2>&1 | grep -E
"Fast track|volatile asset|Submitting"`, repeated until one failing run is captured.

### Changes

To be decided by the diagnosis: the test only (`liquers-core/tests/evaluation_session.rs`), or
`liquers-core/src/assets.rs` plus the test.

### Risks

Fixing the test before the cause is known could hide a product race (H1). Certainty about the
cause: low; about the expected behaviour: high.
