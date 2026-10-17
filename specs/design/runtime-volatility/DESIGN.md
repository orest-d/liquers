---
id: RUNTIME-VOLATILITY
kind: design
title: Volatility discovered while a command runs
form: compact
workflow: liquers-project
status: approved
phase: documentation
area: [core/assets, core/context]
issues: [RUNTIME-VOLATILE-DEPENDENCY-DOES-NOT-MAKE-PARENT-VOLATILE]
created: 2026-10-17
---
# Volatility discovered while a command runs

Pre-approved after Phase 2 on 2026-10-17.

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

- **AC-9** A command can declare its result volatile
  - WHEN a non-volatile command calls `Context::set_volatile("reason")` and returns
  - THEN its asset finishes `Volatile`, as in AC-3, and the reason is in its log
  - *(added at the Phase 2 review with `Context::set_volatile`)*

### Systems touched and crate placement

`liquers-core` only: `context.rs` (a new public `Context::set_volatile`, and the context notes the
assets it resolves) and `assets.rs` (the asset records its volatility during the run). No new
command, no new crate. *Revised 2026-10-17 at the Phase 2 review, at Orest's request: the asset's
metadata is the authority and the context records volatility on it through `set_volatile`.*

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

**The asset records its own volatility; the context is how a command tells it.** The asset is
the authority: `AssetData::is_volatile` and the metadata's `is_volatile` (which
`finalize_status_with_version` already reads). A new public `Context::set_volatile(reason)` records
on the asset, at once, that it is volatile, the way `Context::set_expires` and `set_title` already
write to the asset they produce. A command calls it when it decides its result is volatile, and the
context calls it itself when it sees that a dependency is volatile (decision 2):

- when `wait_for_dependency_recording` returns a state whose metadata is volatile;
- at the end of the run, for every asset the context resolved: `AssetRef::compute` asks the context
  to check them (`Metadata::is_volatile`, i.e. `is_volatile` or `Status::Volatile`) before it
  returns, which catches a command that waited with `AssetRef::get` instead (AC-5).

Because the flag is already on the asset when `complete_evaluation` runs, the existing finish path
does the rest unchanged: status `Volatile`, no version prepared, the stored copy written with
`Status::Volatile` (so `try_fast_track` refuses it), no dependency-graph registration. Eviction
needs no new code: `get`, `get_asset` and `get_dependency_asset` already treat a mapped `Volatile`
asset as a cache miss and rebuild it, and `owned_key_asset` already drops one
(`specs/reference/ASSETS.md` §Volatile assets are never owned).

**Why a direct write and not a service-channel message.** The asset's service loop
(`AssetRef::process_service_messages`) is drained only after the run has settled:
`complete_evaluation` finalizes and persists the status, and only then is `JobFinishing` sent. A
`SetVolatile` message queued by the command could therefore be applied after the asset was already
stored as `Ready`. A write under the asset's lock, which the context's other setters already use,
is in place before finalization by construction.

Reading the metadata at the end, not when the dependency is scheduled, is what catches a
dependency that only turns volatile during its own run (AC-4's chain) and a command that waits
with `AssetRef::get` (AC-5). A dependency that is still running when the command returns is judged
by what its metadata says then: volatile if its plan was, otherwise not.

Rejected:
- *Only in `wait_for_dependency_recording`* (next to the record inheritance of
  `design/search-read-dependencies`): misses `AssetRef::get` after `evaluate`, `apply`, and
  `apply_ad_hoc`, against decision 2. It is still used, so the asset is marked as early as possible.
- *Carry a flag back in `RecipeEvaluation` and set it in `complete_evaluation`*: works, but puts the
  decision in the run harness instead of on the asset, and gives a command no way to declare it.
- *Re-plan or hold registration until the run ends*: rejected by decision 1.
- *Push from the child to its requesters*: a non-volatile child is shared, and the child does not
  know who read it.

**Known-issue preflight.** None blocks. `EVALUATION-SESSION-VOLATILE-TEST-INTERMITTENT` is next to
this code; a failure of `volatile_dependency_has_one_value_per_evaluation` is triaged against it.
`RESOURCE-STEP-VOLATILITY-IS-NEVER-CHECKED` is plan-time and separate; resource steps read the
store, not an asset, so they are outside this mechanism. `INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS`
is unaffected. **Command namespaces involved:** none; no command is added or changed.

### Changes

`liquers-core/src/assets.rs`, `AssetRef<E>`:

```rust
/// Marks the asset being evaluated volatile, with the reason as an info log entry. Idempotent:
/// the first reason is logged, later calls change nothing. A no-op on a finished asset.
pub(crate) async fn set_volatile_during_run(&self, reason: &str) -> Result<(), Error>
```

It sets `AssetData::is_volatile` and `metadata.set_volatile()` under one write lock and adds
`"Volatile: <reason>"` (AC-7). `resolve_volatility_before_evaluation` keeps its plan-time role;
its doc comment says the decision can still be made during the run. `compute` calls
`context_for_deps.check_resolved_volatility().await` next to `take_pending_dependencies()`.

`liquers-core/src/context.rs`, `Context<E>`:

```rust
/// Assets this evaluation resolved, checked at the end of the run (shared by clones).
resolved_dependencies: Arc<std::sync::Mutex<Vec<(String, AssetRef<E>)>>>,
/// Mirrors the asset's flag once `set_volatile` has run, so `is_volatile()` stays synchronous.
volatile_set: Arc<AtomicBool>,

/// Records on the asset being produced that it is volatile: not reused, not stored as loadable.
pub async fn set_volatile(&self, reason: &str) -> Result<(), Error>
pub fn is_volatile(&self) -> bool                 // unchanged signature: is_volatile || volatile_set
pub(crate) fn note_resolved(&self, description: String, asset: &AssetRef<E>)
pub(crate) async fn check_resolved_volatility(&self) -> Result<(), Error>
```

- `set_volatile` calls `AssetRef::set_volatile_during_run` and sets `volatile_set` (AC-8). The
  authority stays the asset; the mirror only answers the synchronous `is_volatile()`.
- `note_resolved` is called in `schedule_dependency_asset_with_key` (so also `submit`,
  `evaluate`, `get_dependency_state` and the interpreter's scheduled steps),
  `schedule_payload_dependency_asset`, `apply` and `apply_ad_hoc` (on main since PR #139 merged;
  its doc comment that says volatility is not passed on is rewritten). The description is the
  dependency key's encoding, or `apply <query>` for an ad-hoc asset.
- `wait_for_dependency_recording` calls `set_volatile("dependency <key> is volatile")` when the
  returned state's metadata is volatile. `check_resolved_volatility` does the same for the first
  resolved asset whose metadata is volatile, then stops.
- `new` creates both fields; `with_volatile`, `clone_context` and the `Clone` impl share them,
  like `pending_dependencies`. The `std::sync::Mutex` is never held across an `.await` (the list is
  cloned out first), and a poisoned lock is recovered with `unwrap_or_else(|e| e.into_inner())`,
  as `cwd_key` does.

`liquers-core/src/interpreter.rs`: the free function `evaluate` runs on a temporary asset and
calls `check_resolved_volatility` before reading `context.get_metadata()`, as `compute` does.
*(Built: see Phase 5 for a second interpreter change.)*

One new public method (`Context::set_volatile`), no new error, no new trait. `set_volatile` and
`check_resolved_volatility` are async because they take the asset's lock; `note_resolved` and
`is_volatile` stay synchronous.

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
  finish. Linear; no measurement unless the Phase 4 tests show a regression.
- **Memory.** Resolved `AssetRef`s are held until the run ends; the evaluation session already
  pins them.
- **Recovery.** Two files; reverting the PR restores today's behaviour. Certainty: high, since
  the finish path it reuses is already tested (`runtime_volatile_asset_is_evicted_from_key_map`,
  `volatile_keyed_recipe_recomputes_every_time`).

## Phase 3: Examples and Tests

### Examples

The commands live in a new integration test, `liquers-core/tests/runtime_volatility.rs`, built
over both managers (`SimpleEnvironment`, queued, and `ImmediateEnvironment`, inline) with one
generic environment builder:

- `count` — volatile, returns `n0`, `n1`, … from a shared counter;
- `depend_on(query)` — `get_dependency_state(query)`, as in `tests/apply_ad_hoc.rs`;
- `read_counted` — `get_dependency_state(-R/n.txt)`, where `recipes.yaml` maps
  `depend_on-count/n.txt` to the key `n.txt`;
- `submit_wait(query)`, `evaluate_get(query)` (`evaluate` then `AssetRef::get`, bypassing
  `wait_for_dependency`), `apply_to(query)` (`Context::apply` then `AssetRef::get`);
- `probe_volatile(query)` — returns `"<before>/<after>"` of `Context::is_volatile()` around a
  `get_dependency_state(query)`;
- `declare_volatile` — calls `set_volatile("declared by the command")`, returns `"declared"`;
- `plain` — non-volatile, returns `"plain"`.

Primary example (AC-1): `depend_on-count` evaluated twice gives `n0` then `n1`, both `Volatile`.
Secondary (AC-3): the key `n.txt` requested twice gives two asset ids, the store's metadata for
`n.txt` has `Status::Volatile`, and the dependency manager has no version for `n.txt`.

Edge and error cases: a dependency submitted and never waited for that is not volatile by plan
leaves the parent `Ready` (decision 2's "judged by its metadata at the end"); a volatile dependency
whose evaluation fails fails the parent as today (the error wins, no volatility question arises);
`set_volatile` called twice logs one entry.

All queries above validate with `liquers-validate --command …` (2026-10-17).

### Tests

`liquers-core/tests/runtime_volatility.rs`, each scenario as `<name>_default` and `<name>_inline`:

| Test | Scenarios |
|---|---|
| `rv01_runtime_dependency_makes_query_volatile` | AC-1 |
| `rv02_trailing_v_recomputes_the_prefix` | AC-2 |
| `rv03_keyed_runtime_volatile_is_stored_volatile_and_recomputed` | AC-3 |
| `rv04_runtime_volatility_is_transitive` | AC-4 |
| `rv05_every_context_entry_point_passes_volatility` (`submit_wait`, `evaluate_get`, `apply_to`) | AC-5 |
| `rv06_non_volatile_dependency_stays_ready` (`depend_on-plain`: `Ready`, same asset twice, dependency record present) | AC-6 |
| `rv07_volatile_cause_is_logged` (exactly one info entry naming `count`) | AC-7 |
| `rv08_context_is_volatile_after_volatile_dependency` (`probe_volatile-count` returns `false/true`; `probe_volatile-plain` returns `false/false`) | AC-8 |
| `rv09_command_can_declare_volatile` (called twice: one log entry) | AC-9 |
| `rv10_unwaited_non_volatile_dependency_keeps_parent_ready` | AC-6 |

Changed: `tests/apply_ad_hoc.rs` `apply_ad_hoc_volatility_is_static`, which pins today's
behaviour, becomes `apply_ad_hoc_volatility_is_inherited`: `apply_with-rnd` is now volatile
(AC-5). Unit test in `liquers-core/src/assets.rs`: `set_volatile_during_run_marks_metadata_once`
(AC-7, AC-9).

```bash
cargo test -p liquers-core --test runtime_volatility --test apply_ad_hoc
cargo test -p liquers-core --lib set_volatile_during_run
```

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-core/src/assets.rs` `AssetRef::set_volatile_during_run` (+ unit test); doc of
  `resolve_volatility_before_evaluation` — `cargo test -p liquers-core --lib set_volatile_during_run` — done (b87faa1)
- [x] 2. `liquers-core/src/context.rs` — fields `resolved_dependencies`, `volatile_set` in `new`,
  `with_volatile`, `clone_context`, `Clone`; `set_volatile`, `is_volatile`, `note_resolved`,
  `check_resolved_volatility`; `note_resolved` in the two schedule functions, `apply`,
  `apply_ad_hoc`; the volatile check in `wait_for_dependency_recording` — `cargo check -p liquers-core` — done (b87faa1)
- [x] 3. `liquers-core/src/assets.rs` `AssetRef::compute` calls `check_resolved_volatility` before
  `take_pending_dependencies` — `cargo check -p liquers-core` — done (b87faa1)
- [x] 4. `liquers-core/tests/runtime_volatility.rs` (rv01–rv10); rename and invert
  `apply_ad_hoc_volatility_is_static` — the Phase 3 commands — done (08ba134)
- [x] 5. Regression: every test that pinned the old behaviour is updated with a reference to this
  design, or the implementation is fixed — `cargo test -p liquers-core --lib --tests` and
  `cargo test -p liquers-lib --lib --tests` (template and search commands read queries at run time) — done (caf1359, 5271046)
- [x] 6. Documents: `ASSETS.md`, `ASSET_LIFECYCLE.md`, `COMMAND_DESIGN_GUIDE.md`,
  `TEMPLATE_LANGUAGE.md`, `TEMPLATE_GUIDE.md` (History row and `reviewed:` each), the `assets.rs`
  module docs §Volatility, `Context` docs — `python3 scripts/docs_index.py --check` — done (a0dff45)

Rollback: each step is one commit on the PR branch; reverting the branch restores today's
behaviour. No stored format changes, so no data needs rollback.

### Validation

`cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests`, and
`cargo check -p liquers-core --target wasm32-unknown-unknown` (the new fields use `std::sync` only).
No `#[cfg]`, optional dependency or `ExtValue` match changes, so the build matrix is not needed.
No command signature changes, so `specs/command_registry.yaml` is unchanged.

## Phase 5: Documentation

### Built versus approved

Built as approved in Phase 2 (revised), with two additions found by the tests:

1. **An ad-hoc asset records its plan's volatility.** `rv05` failed for `apply_to-count`: an
   asset made by `AssetManager::apply` / `apply_for` is created without a plan, so nothing ever
   marked it volatile, and the context of its parent saw a non-volatile dependency. `apply_plan`
   (`liquers-core/src/interpreter.rs`) now calls `Context::set_volatile("its plan is volatile")`
   when the plan is volatile and the context is not. A managed asset was already marked at
   creation, so for it this is a no-op. This implements AC-5; it changes no approved contract.
2. **Consequence: a payload-requiring evaluation finishes `Volatile`.** `register_command!`
   declares a `payload: required` command `volatile`, but because of the gap above
   `evaluate_immediately` of such a query finished `Ready`, and
   `payload_requirement_does_not_imply_volatility` pinned that. It is now
   `payload_requirement_is_recorded_with_volatility`; `MetadataRecord::payload_required`,
   `AssetInfo::payload_required` and `PAYLOAD_GUIDE.md` say so. Reuse is unchanged (such an asset
   was already ad hoc and in no map); only the reported status and flag differ.

The free function `interpreter::evaluate` also calls `check_resolved_volatility`, a one-line
change Phase 2 had judged unnecessary (it would have missed a dependency waited with
`AssetRef::get`).

Tests: `liquers-core/tests/runtime_volatility.rs` rv01–rv10 on both managers (20 tests),
`set_volatile_during_run_marks_metadata_once`; inverted `apply_ad_hoc_volatility_is_inherited`
(`liquers-core`) and `volatile_query_makes_render_volatile` (`liquers-lib`, the template
commands). `cargo test -p liquers-core --lib --tests` and `cargo test -p liquers-lib --lib
--tests` pass; `cargo check -p liquers-core --target wasm32-unknown-unknown` passes.

### Documents

Reviewed and updated, each with a History row and `reviewed:` bump: `reference/ASSETS.md`
(§Volatile assets are never owned), `reference/ASSET_LIFECYCLE.md` (steps 1 and 6),
`reference/PAYLOAD_GUIDE.md`, `reference/TEMPLATE_LANGUAGE.md`, `guides/TEMPLATE_GUIDE.md` (the
`v/` advice is removed), `guides/COMMAND_DESIGN_GUIDE.md` (new paragraph on the volatility of what
a command reads). Code docs: `assets.rs` module docs §Volatility, `Context::is_volatile`,
`set_volatile`, `apply_ad_hoc`, `AssetRef::resolve_volatility_before_evaluation`. No new reference
or guide document was planned. `design/volatility-system` is frozen; this design records the
revision of its decision 4.

### Issues

- Closes `RUNTIME-VOLATILE-DEPENDENCY-DOES-NOT-MAKE-PARENT-VOLATILE` when this PR merges.
- Filed: `CORE-WASM-BUILD-EMITS-UNUSED-WARNINGS` (pre-existing, found while checking wasm32;
  eligible for an automatic fix, not done here to keep this PR to its subject) and
  `DEPENDENCY-MANAGER-VERSION-NOT-OBSERVABLE-FROM-TESTS` (why `rv03` checks the version on the
  asset instead of the graph).
- The ad-hoc plan-volatility gap was fixed here (triage case 1: belongs to this design).

### Learning

The asset's service channel is drained only after the run has settled, so anything a command must
record *before* finalization has to be written on the asset directly, as `set_expires` and now
`set_volatile` do.
