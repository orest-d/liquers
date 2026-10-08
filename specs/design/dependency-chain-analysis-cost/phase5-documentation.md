# Phase 5: Documentation - Direct dependency records and linear dependency analysis

## Completion Preconditions

- [x] Implementation is finished and validated (Steps 1–7, commits `8ceba60`..`5f5a5a4` on
  `claude/confident-cray-i81pyb`)
- [x] All user comments are answered or incorporated (Phase 1 Decisions, Phase 2 Decisions, B1,
  the startup audit, the example)
- [x] All review comments are answered or incorporated (Phase 2–4 reviews and the final review)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation branch

## Implementation Summary

**What was implemented** (all in `liquers-core`, plus call sites):

- **Direct dependency records.** `plan.dependencies`, the stored `DependencyRecord`s and the graph
  edges hold direct dependencies only.
- **One memoized walk.** `analyze_plan_dependencies` (`plan.rs`) replaces
  `has_volatile_dependencies` + `has_expirable_dependencies`:
  - one walk per analysis, each recipe once per (key, caller CWD), with an O(1) on-path cycle
    check;
  - a separate per-key `DependencySummary` for volatility and expiry, keeping today's rules (links
    contribute their own `volatile` flag only, `GetAssetRecipe` expiry only).
- **Iterative walks.** Both walks use an explicit stack. The first, recursive version overflowed
  the stack at about 150 links (a deviation from Phase 2's `BoxFuture` recursion, found while
  measuring).
- **Recipe cache.** `DefaultRecipeProvider` gained a per-directory cache of parsed `recipes.yaml`,
  checked against the stored bytes. It is no longer a unit struct: `DefaultRecipeProvider::new()`,
  with 100 construction sites updated across core, axum tests and records.
- **The stored-records walk**, `AssetManager::stored_dependency_state` and
  `StoredDependencyState`: iterative, metadata only, infallible. It registers what it confirms,
  except in `ReportOnly`. It is used by the `on_load` fast-track branch and by `audit_gaps`, where
  a gap stale upstream expires its dependents with `StaleDependency`.
- **B1.** `missing_versions_for` walks the dependency manager's upstream closure, so the per-key
  audit reaches past known intermediates.
- **The startup audit**, `AssetManager::trigger_dependency_audit_store(root, mode)`: optional,
  called by the application. Shown in the `liquers-axum` `basic_server` example with
  `LIQUERS_STARTUP_AUDIT=1`.
- **First registration (added during implementation).** `register_version` now expires, on a
  first registration, the dependents whose edge records a concrete, other version. Without it the
  trusting policy's promised cascade (Phase 1 Decision 1) did not happen after a restart: a
  recomputed upstream registered its first version and expired nothing. Edges recording
  `Version::unknown()` are still spared, which is what the evaluation path needs. This narrows
  the earlier decision in `AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION`, which rejected the
  broad form.

**Measured** (debug, cold, link by link): 40 links 76.5 s → 0.26 s; 200 links (hours) → 4.5 s.
That meets Decision 5 (40 < 1 s, 200 < 5 s), with a thin margin at 200; Phase 2 Decision 2 allows
8 s there. The full numbers are in the issue's resolution.

**Conformance.** The work matches the approved Phases 1–4 and the later additions (startup audit,
B1). The deviations are the iterative walks and the first-registration rule, both above, and two
test simplifications listed under Conformance.

## Documentation Delivered

### New Reference Documents
None as separate files. The reference material went into
`reference/DEPENDENCIES_STATUS.md` as two new sections:
- **§What a dependency record holds:** records are direct; what "direct" means; analysis summary
  versus record; what a fast-track load contributes.
- **§Consistency policies:** a detailed `Explicit` / `OnLoad` table, the stored-records walk, the
  startup audit, and a restart example across three regimes.

### New Guide Documents
`guides/DEPENDENCY_CONSISTENCY_GUIDE.md`: choosing how much inconsistency to tolerate, configuring
it, running the startup audit, audits while running, and diagnosing a stale value from its expiry
reason. Phase 1 had planned no guide; the maintainer asked for one with the startup audit.

### Existing Documents Reviewed or Updated
`affects_docs` lists the nine documents below. Every one was reviewed against the code and has
`reviewed: 2026-10-08` and a History row:

| Document | Change |
|---|---|
| `reference/DEPENDENCIES_STATUS.md` | the two new sections; current-contract bullets (`OnLoad`, audits, first registration); Flow C; glossary |
| `reference/ASSET_LIFECYCLE.md` | reuse-check table (`on_load` walk); `AuditPolicy` → `DependencyAuditPolicy` |
| `reference/ENVIRONMENT_CONFIG.md` | the `dependency_audit` row |
| `reference/ASSETS.md` | the provided dependency checks; `StaleDependency` routes |
| `reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` | fast-track step 2 |
| `reference/api/DOC_08_RECIPES_PLANS.md` | the `dependencies` field; finalization; the recipe cache |
| `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` | `DefaultRecipeProvider::new()`; the inherited walk |
| `guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` | a link to the new guide |
| `guides/DEPENDENCY_CONSISTENCY_GUIDE.md` | new |

Candidates sharing an area that were discarded, because nothing they say changed:
`reference/ASSET_SET_OPERATION.md`, `reference/PROJECT_OVERVIEW.md`,
`reference/api/DOC_01_ARCHITECTURE_REFERENCE.md`, `guides/LANGUAGE-INTEGRATION_GUIDE.md`,
`guides/UNITTEST_GUIDE.md`.

### Links and Capability Map
In `specs/README.md`:
- a task-table row for the new guide;
- a capability-map entry, "Dependency records and consistency policies", pointing to the
  reference sections and the guide.

The fast-track and configuration documents link §Consistency policies.

## Issues Filed

- `EVALUATING-A-DEEP-CHAIN-TOP-DOWN-OVERFLOWS-THE-STACK` (P2): evaluating the end of a cold chain
  overflows the stack between 20 and 30 links. This is evaluation recursion, not analysis.
- `REFUSED-DEPENDENCY-RECOMPUTED-TWICE-DURING-A-DEPENDENT-EVALUATION` (P3): a duplicate recipe run
  under `on_load`.
- `DEPENDENCY-ANALYSIS-REWALKS-UPSTREAM-PER-EVALUATION` (P3): the optional summary cache, deferred
  in Phase 1.
- `ASSET-MANAGER-CANNOT-BE-SYNCHRONIZED-WITH-THE-STORE` (P3, filed during Phase 4): a sync/reload
  operation for a running system.
- Closed: `EVALUATING-A-LONG-DEPENDENCY-CHAIN-GETS-SUPER-LINEARLY-SLOW`. Annotated:
  `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING`, whose code is now one call.

## Important Learning

- **Transitive records were silently doing a job.** They made a restarted process notice an
  upstream command change, and no test covered it. Removing them needed an explicit owner for
  transitive freshness: the dependency manager, by policy.
- **Async recursion has a depth limit.** Recursion over boxed futures is bounded by the thread
  stack, and a chain's length is unbounded. Walks over user data should be iterative.
- **First registration is ambiguous.** "Not a change" (the evaluation path) and "a change" (a
  write or audit) were the only two rules. The middle rule, which expires only on concrete
  contradicting evidence, is what makes the trusting policy eventually consistent.

## Conformance and Remaining Work

- **Requested, approved and implemented:** they agree, except for the deviations above
  (iterative walks, the first-registration rule).
- **Test simplifications.**
  - I4c (`on_load` refusing on a store error) is covered at unit level by
    `stored_state_store_error_is_unresolvable` together with the explicit `Unresolvable` refusal
    arm, not as an integration test.
  - I1, I2 and I3 assert "at least" the recomputations, because of the duplicate-run issue.
- **Remaining:** the four issues above; nothing else.

## Validation

- **`liquers-core`:** `cargo test -p liquers-core --lib --tests` passes, including 1 030 lib tests
  and `dependency_audit_integration`, which grew from 30 to 42.
- **Other crates:** `cargo test -p liquers-records --all-features --lib --tests`,
  `cargo test -p liquers-axum --tests` and `cargo test -p liquers-lib --lib --tests` pass.
- **wasm32:** `cargo check -p liquers-core --target wasm32-unknown-unknown` passes.
- **Acceptance:** `cargo test -p liquers-core --test dependency_chain_scaling -- --ignored` passes
  the acceptance bounds.
- **Documentation:** `python3 scripts/docs_index.py --check` reports 0 errors.
