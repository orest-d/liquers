---
id: RUNTIME-VOLATILITY
kind: design
title: Volatility discovered while a command runs
form: compact
workflow: liquers-project
status: in_review
phase: architecture
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
no longer needs `volatile: true` for its *inputs'* volatility. The template docs from PR #139
(merged), `specs/reference/TEMPLATE_LANGUAGE.md` §volatility and `specs/guides/TEMPLATE_GUIDE.md`
**Volatility**, are relaxed in this PR: a render over volatile queries becomes volatile by itself.

### Overlap

- `design/search-read-dependencies` (PR #141, merged 2026-10-17; this branch is rebased on it)
  made `Context::wait_for_dependency_recording` inherit a non-keyed dependency's records.
  Volatility inheritance is the same idea for a different fact and builds on that code. Not
  blocking.
- `EVALUATION-SESSION-VOLATILE-TEST-INTERMITTENT`: the known flaky test sits next to the code this
  changes; failures of it are triaged against that issue, not this design.
- `RESOURCE-STEP-VOLATILITY-IS-NEVER-CHECKED`: plan-time, separate.

### Open Questions

None. All four were answered by Orest on 2026-10-17:

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

## Phase 2: Architecture

### Solution

**The context collects what it resolved; the asset reads their metadata when the run ends.**
Every asset a `Context` hands to a command or the interpreter is noted with a short description.
When `AssetRef::compute` has the command's result, it asks the context whether any of them is
volatile by its metadata (`Metadata::is_volatile`, i.e. `is_volatile` or `Status::Volatile`, per
decision 2). If one is, `complete_evaluation` sets the asset volatile *before*
`finalize_status_with_version`, and the existing finish path does the rest: status `Volatile`, no
version prepared, the stored copy written with `Status::Volatile` (so `try_fast_track` refuses
it), no dependency-graph registration. Eviction needs no new code: `get`, `get_asset` and
`get_dependency_asset` already treat a mapped `Volatile` asset as a cache miss and rebuild it, and
`owned_key_asset` already drops one (`specs/reference/ASSETS.md` §Volatile assets are never owned).

Reading the metadata at the end, not when the dependency is scheduled, is what catches a
dependency that only turns volatile during its own run (AC-4's chain) and a command that waits
with `AssetRef::get` (AC-5). A dependency that is still running when the command returns is judged
by what its metadata says then: volatile if its plan was, otherwise not.

Rejected:
- *Only in `wait_for_dependency_recording`* (next to the record inheritance of
  `design/search-read-dependencies`): misses `AssetRef::get` after `evaluate`, `apply`, and
  `apply_ad_hoc`, against decision 2. It is still used, to update `Context::is_volatile()` early.
- *Re-plan or hold registration until the run ends*: rejected by decision 1.
- *Push from the child to its requesters*: a non-volatile child is shared, and the child does not
  know who read it.

**Known-issue preflight.** None blocks. `EVALUATION-SESSION-VOLATILE-TEST-INTERMITTENT` is next to
this code; a failure of `volatile_dependency_has_one_value_per_evaluation` is triaged against it.
`RESOURCE-STEP-VOLATILITY-IS-NEVER-CHECKED` is plan-time and separate; resource steps read the
store, not an asset, so they are outside this mechanism. `INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS`
is unaffected. **Command namespaces involved:** none; no command is added or changed.

### Changes

`liquers-core/src/context.rs`, `Context<E>`:

```rust
/// Assets this evaluation resolved, for the volatility check at finish (shared by clones).
resolved_dependencies: Arc<std::sync::Mutex<Vec<(String, AssetRef<E>)>>>,
/// Set once a resolved dependency is seen to be volatile; read by `is_volatile()`.
volatile_dependency_seen: Arc<AtomicBool>,

pub fn is_volatile(&self) -> bool          // unchanged signature: is_volatile || seen
pub(crate) fn note_resolved(&self, description: String, asset: &AssetRef<E>)
pub(crate) async fn volatile_dependency(&self) -> Option<String>   // first volatile, by metadata
```

- `note_resolved` is called in `schedule_dependency_asset_with_key` (so also `submit`,
  `evaluate`, `get_dependency_state` and the interpreter's scheduled steps),
  `schedule_payload_dependency_asset`, `apply` and `apply_ad_hoc` (on main since PR #139 merged
  on 2026-10-17; its doc comment that says volatility is not passed on is rewritten). The description is
  the dependency key's encoding, or `apply <query>` for an ad-hoc asset.
- `wait_for_dependency_recording` sets `volatile_dependency_seen` when the returned state's
  metadata is volatile, so `is_volatile()` turns true as soon as the command has the value (AC-8).
  `volatile_dependency` sets it too.
- `new` creates both; `with_volatile`, `clone_context` and the `Clone` impl share them, like
  `pending_dependencies`. A `std::sync::Mutex` suffices: it is never held across an `.await`
  (`volatile_dependency` clones the list out before reading each asset's metadata), and a
  poisoned lock is recovered with `unwrap_or_else(|e| e.into_inner())`, as `cwd_key` does.

`liquers-core/src/assets.rs`:
- `RecipeEvaluation` gains `volatile_dependency: Option<String>`. `compute` fills it from
  `context_for_deps.volatile_dependency()` next to `take_pending_dependencies()`; the delegation
  branch sets `None` (a key's registered owner is never volatile).
- `complete_evaluation`, in the same write lock that merges the dependencies: if `Some(cause)`,
  `lock.is_volatile = true`, `metadata.set_volatile()`, and one info log entry
  `"Volatile: dependency <cause> is volatile"` (AC-7). Everything after that is unchanged.
- `resolve_volatility_before_evaluation` keeps its plan-time role; its doc comment says the
  decision can still change at finish.

`liquers-core/src/interpreter.rs`, free function `evaluate` (temporary asset, not managed): if
`context.volatile_dependency()` is `Some`, the returned state's metadata is marked volatile, so a
caller of that path sees the same rule.

No public signature changes, no new error, no new trait. All changes are async where they touch
asset locks, as the surrounding code is; `note_resolved` and `is_volatile` stay synchronous.

**Documents.** `ASSETS.md` §Volatile assets are never owned (volatility can be decided at finish,
by a dependency) and `ASSET_LIFECYCLE.md` steps 1 and 6, each with a `## History` row and
`reviewed:` bump; `COMMAND_DESIGN_GUIDE.md` (a command that evaluates queries does not declare
`volatile` for its inputs); `TEMPLATE_LANGUAGE.md` and `TEMPLATE_GUIDE.md` (drop the "put `v`
before the render or declare it volatile" advice); the `assets.rs` module docs §Volatility; the
`Context` method docs, including `apply_ad_hoc`'s.
`design/volatility-system` is complete and frozen, so its decision 4 is not edited; this design is
the record of the revision.

### Risks

- **Existing tests that expect `Ready`** from a command reading a volatile value at run time will
  now see `Volatile`. Each is checked: a test pinning the old behaviour is updated, citing this
  design.
- **Cost.** One `Vec` push per resolved dependency and one metadata read lock per dependency at
  finish. Linear; a search catalog reading thousands of files pays thousands of uncontended read
  locks once. No measurement is planned unless the Phase 4 tests show a regression.
- **Memory.** The list holds each resolved `AssetRef` until the run ends. The evaluation session
  already pins them, so the lifetime does not grow in practice.
- **Recovery.** The change is confined to three files and one decision point in
  `complete_evaluation`; reverting the PR restores today's behaviour. Certainty: high, since the
  finish path it reuses is already tested (`runtime_volatile_asset_is_evicted_from_key_map`,
  `volatile_keyed_recipe_recomputes_every_time`).
