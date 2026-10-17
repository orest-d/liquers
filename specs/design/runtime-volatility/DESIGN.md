---
id: RUNTIME-VOLATILITY
kind: design
title: Volatility discovered while a command runs
form: compact
workflow: liquers-project
status: in_review
phase: high-level
area: [core/assets, core/context]
issues: [RUNTIME-VOLATILE-DEPENDENCY-DOES-NOT-MAKE-PARENT-VOLATILE]
created: 2026-10-17
---
# Volatility discovered while a command runs

## Phase 1: High-Level Design

### Purpose

Volatility is meant to be contagious: a value built from a volatile value is itself single-use.
Today that holds only for what the planner can see. A command that evaluates a query through its
`Context` at run time (`get_dependency_state`, `submit` + `wait_for_dependency`, `evaluate`,
`apply`, and `apply_ad_hoc` from PR #139) passes nothing on, so its result is cached, served again
and, when keyed, stored as `Ready`. Every command that evaluates queries it builds is affected:
`template/*` (PR #139), `ns-search`, and the agent-memory context packs. Today the caller has to
know to declare such a command `volatile` or to start the query with `v/`.

This design makes an asset that *used* a volatile value during its run finish `Volatile`. It
revises `design/volatility-system` decision 4 ("volatility determined upfront, not during
execution") and its Phase 2 decision 5 ("no transitions to Volatile after creation"). The second is
already not true: an expiry a command sets during evaluation makes its asset finish `Volatile`
(`AssetRef::finalize_status_with_version` in `liquers-core/src/assets.rs`; test `runtime_volatile_asset_is_evicted_from_key_map`).
This design generalises that "volatile at finish" path rather than adding a new one.

### Problem Example

`count` is a volatile command returning `n0`, `n1`, …; `depend_on(q)` is a non-volatile command
that returns `context.get_dependency_state(q)` (the commands of `liquers-core/tests/apply_ad_hoc.rs`).
Probe of 2026-10-17, evaluating each query in turn:

| Query | Today | Wanted |
|---|---|---|
| `depend_on-count` | `Ready n0` | `Volatile n0` |
| `depend_on-count` again | `Ready n0` (served from cache) | `Volatile n1` |
| `depend_on-count/v` | `Volatile n0` (trailing `v` serves the cached prefix) | `Volatile n2` |
| `v/depend_on-count` | `Volatile n1` | `Volatile n3` |

All four queries validate (`liquers-validate --command depend_on --command count`).

### Scope and Acceptance Criteria

In scope: assets evaluated by both asset managers (`DefaultAssetManager` and the inline manager),
keyed, query and ad-hoc. Out of scope: plan-time volatility (unchanged), and resource steps whose
volatility is never checked (`RESOURCE-STEP-VOLATILITY-IS-NEVER-CHECKED`, separate).

- **AC-1** Runtime dependency makes the query volatile
  - WHEN `depend_on-count` is evaluated twice
  - THEN both results are `Volatile`, the second is `n1`, and neither is served from the query map
- **AC-2** Trailing `v` no longer serves a stale prefix
  - WHEN `depend_on-count` is evaluated, then `depend_on-count/v`
  - THEN the second evaluation recomputes `depend_on-count` and returns a new value
- **AC-3** Keyed asset is not stored as reusable and is evicted
  - WHEN a key's recipe is `depend_on-count` and the key is requested twice
  - THEN the first result is `Volatile`, its stored copy carries `Status::Volatile` (so
    `try_fast_track` refuses it), the first asset is no longer in the key map, the second request
    recomputes, and the key registers no version in the dependency graph
- **AC-4** Runtime volatility is transitive
  - WHEN a non-volatile command reads AC-3's key through its context (`depend_on` given the key's
    resource query)
  - THEN that result is `Volatile` too
- **AC-5** Every context entry point passes volatility on
  - WHEN a command reaches a volatile query through `submit` + `wait_for_dependency`, `evaluate`
    + `AssetRef::get`, `apply`, or `apply_ad_hoc`
  - THEN its result is `Volatile` in each case
- **AC-6** Non-volatile run-time dependencies change nothing
  - WHEN `depend_on` reads a non-volatile query
  - THEN the result is `Ready`, cached, stored when keyed, and its dependency records are as today
- **AC-7** The cause is recorded
  - WHEN an asset finishes `Volatile` because of a run-time dependency
  - THEN its metadata log has one info entry naming that dependency, as the planner's
    `PlanBuilder::mark_volatile` reason does for plan-time volatility
- **AC-8** The command can see it
  - WHEN a command has resolved a volatile dependency through its context
  - THEN `Context::is_volatile()` returns `true` for the rest of that command's run

### Systems touched and crate placement

`liquers-core` only: `context.rs` (`Context` notes the volatile values it resolves),
`assets.rs` (`RecipeEvaluation` carries the result into `complete_evaluation`, which sets the
asset volatile before `finalize_status_with_version`; eviction from the maps at finish if the
on-entry eviction does not already cover the query map), `interpreter.rs` (the
temporary-asset path of the free function `interpreter::evaluate`). No new command, no new crate, no public signature change
expected beyond possibly `Context::is_volatile` reading shared state.

### Documentation intent

Update `specs/reference/ASSETS.md` §Volatile assets are never owned and
`specs/reference/ASSET_LIFECYCLE.md` (steps 1 and 6), the volatility section of the
`liquers-core/src/assets.rs` module docs, and the `Context` method docs that point users at the
issue. `specs/guides/COMMAND_DESIGN_GUIDE.md` gets one paragraph: a command that evaluates queries
no longer needs `volatile: true` for its *inputs'* volatility. After this lands PR #139's template
docs ("use `v/` or declare the command volatile") can be relaxed.

### Overlap

- `design/search-read-dependencies` (PR #141, merged 2026-10-17; this branch is rebased on it)
  made `Context::wait_for_dependency_recording` inherit a non-keyed dependency's records.
  Volatility inheritance is the same idea for a different fact and builds on that code. Not
  blocking.
- `EVALUATION-SESSION-VOLATILE-TEST-INTERMITTENT`: the known flaky test sits next to the code this
  changes; failures of it are triaged against that issue, not this design.
- `RESOURCE-STEP-VOLATILITY-IS-NEVER-CHECKED`: plan-time, separate.

### Decisions

Answered by Orest on 2026-10-17. No open questions remain.

1. **Volatile at finish.** `design/volatility-system` decision 4 is revised: an asset starts
   non-volatile when nothing at plan time says otherwise, and finishes `Volatile` if its run had a
   volatile dependency. Entries that were already registered are evicted.
2. **Any volatile dependency makes the result volatile, as read from that dependency's
   metadata.** Every asset the context resolved counts (`submit`, `evaluate`,
   `get_dependency_state`, interpreter-scheduled dependencies, `apply`, `apply_ad_hoc`, payload
   dependencies), whether the command waited through `wait_for_dependency` or `AssetRef::get`. The
   test is the dependency's metadata (`Metadata::is_volatile`, which covers `is_volatile` and
   `Status::Volatile`), not a separate flag.
3. **`Context::is_volatile()` reflects the discovery** (AC-8).
4. **No migration.** Backward compatibility is not a concern yet. Copies stored as `Ready` before
   this change are left as they are.
