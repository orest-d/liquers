---
id: ENVIRONMENT-MANAGER-REFERENCE-CYCLE-DESIGN
kind: design
title: Environment teardown without reference cycles
form: compact
status: complete
readiness: ready
gh_pr: [176]
autofix: not-eligible
area: [core/assets, core/context]
issues: [ENVIRONMENT-MANAGER-REFERENCE-CYCLE]
affects_docs: [reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md, reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md]
created: 2026-10-12
---
# Environment teardown without reference cycles

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written; implemented
2026-10-22 with the backlog plan's corrections (see Phase 5).

## Phase 1: High-Level Design

### Purpose

Dropping the last `EnvRef` must drop the environment, its asset manager, its cached assets and the
manager's background tasks. Today three strong paths keep all of them alive for the process.

### Problem Example

Probe at HEAD, both manager kinds:

```rust
let envref = SimpleEnvironment::<Value>::new().to_ref();     // also ImmediateEnvironment
assert_eq!(Arc::strong_count(&envref.0), 2);                  // caller + manager
// evaluate "hello" (a registered command), drop the AssetRef:      count 3 (cached asset)
// liquers_core::interpreter::evaluate(envref.clone(), "hello", None): count 4, and it stays
let weak = Arc::downgrade(&envref.0); drop(envref);
assert!(weak.upgrade().is_some());                            // leaked
```

Should be: count 1 throughout, and `weak.upgrade()` is `None` once the caller's `EnvRef` is gone.

The three paths, verified in `liquers-core/src/assets.rs`:

1. `DefaultAssetManager.envref` / `ImmediateAssetManager.envref` — strong `EnvRef<E>`, given by
   `AssetManagerKind::build` from `GenericEnvironment::init_with_envref` (`context.rs`).
2. `AssetData.envref` — strong; the manager's `assets` / `query_assets` maps hold the assets.
3. `AssetRef::new_temporary` spawns `process_service_messages` on a strong clone. The loop ends
   only on `JobFinishing` / `ErrorOccurred`, which no temporary-asset user sends, and the asset owns
   its own `service_tx`, so the task, the asset and (via 2) the environment live forever.

Also, `Drop for DefaultAssetManager` stops the expiration monitor but not `JobQueue::run`.

### Scope and Acceptance Criteria

- **AC-1** Fresh environment has one owner
  WHEN an environment is built with `to_ref` or `EnvironmentBuilder::build`, queued or inline
  THEN `Arc::strong_count(&envref.0)` is 1
- **AC-2** Caching does not own the environment
  WHEN a query and a keyed asset are evaluated, cached, and the caller drops the `AssetRef`s
  THEN the strong count is 1 again
- **AC-3** The last `EnvRef` drops everything
  WHEN the last `EnvRef` is dropped after evaluations
  THEN a `Weak` to the environment no longer upgrades, and a cached value's `Arc` is released
- **AC-4** Repeated environments retain nothing
  WHEN 100 environments are built, used and dropped in a loop
  THEN no `Weak` to any of them upgrades
- **AC-5** Work in flight keeps its environment
  WHEN the caller drops its last `EnvRef` while a returned asset is queued or running
  THEN `AssetRef::get` still yields the value, `Context::get_envref` works inside the command, and
  the environment drops once the work finishes
- **AC-6** Background tasks exit
  WHEN a queued environment is dropped
  THEN its job-queue and expiration-monitor tasks end (runtime alive-task count returns to baseline)
- **AC-7** A temporary asset's service loop exits
  WHEN `interpreter::evaluate` returns and its temporary asset is unreachable
  THEN no service-loop task for it remains
- **AC-8** Use after teardown is an error
  WHEN a retained `Arc` of the manager calls `get_asset` after the environment is gone
  THEN it returns `Err` (environment dropped), not a panic

Non-goals: a `shutdown()`/soft-reboot API; asset GC in a live environment (`CORE-ASSET-GC`);
values holding an `EnvRef` or `Context` (none are cached today).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — adds `pub` `try_get_envref` methods in `liquers-core`
  (rule 4).
- **Leading issue:** None
- **Explanation:** all three paths are confirmed and each has a local `liquers-core` fix with no
  existing signature change; every contract question was decided by the maintainer on 2026-10-12.
- **Open questions:** None.
- **Decided 2026-10-12 (maintainer):**
  1. **Accessor failure contract: panic.** The asset and manager `get_envref()` accessors stay
     infallible and panic with a message naming teardown, documented under `# Panics`;
     `try_get_envref() -> Result<EnvRef<E>, Error>` is added on `AssetData`, `AssetRef` and
     `AssetManager` (default method) and used at every site in `assets.rs` already inside a
     `Result` function.
  2. **Work in flight keeps the environment alive.** Queued and running work pins the environment
     until it finishes (AC-5; matches today), so `evaluate` → drop → `get` keeps working.
  3. **Temporary-asset loop fix in scope (AC-7),** as proposed.
  4. **No explicit `Environment::shutdown()` here.** Split off as the feature
     `ENVIRONMENT-ORDERLY-SHUTDOWN` (P3, M), which requires this design and settles its own contract
     (in-flight work, use after shutdown, idempotence, an async hook for services). Dropping the
     last `EnvRef` is the teardown this design delivers; `DefaultAssetManager::shutdown` stays.

### Design Dependencies

- `environment-builder` — **requires** (complete; frozen, E4, so not extended): its `OnceLock`
  slot and single construction site `init_with_envref`. Its Phase 1 question 3 deferred this issue.
- `expiration-monitor-assetref` — **overlaps** (complete): the monitor already holds `WeakAssetRef`.
- `asset-cancellation-outcome` — **overlaps** (`in_review`): owns `run_with_future`; this design
  touches only the claims there. No order.
- `CORE-ASSET-GC`, `CORE-TOKIO-REMOVAL` (issues) — **overlaps**: live-environment retention, the
  same `tokio::spawn` sites. No order.

### Consolidated Findings

- Two cycles as filed, plus path 3 and the `JobQueue::run` task leak. Once `AssetData.envref` is
  weak, path 3 leaks a task per `interpreter::evaluate` but no longer the environment.
- No signature changes: constructors keep taking `EnvRef<E>`; `Context` stays strong (one
  evaluation). Only `try_get_envref` is added.
- Pins are tied to ownership of work (`RunClaim`, `InlineRunClaim`, job-queue entries), not to
  status writes: `AssetData.status` has ~20 direct writers, so a status hook would miss some.
- Tests that pass `env.to_ref()` straight into `AssetData::new` lose their environment at once
  (step 6).
- The environment may now drop inside a tokio task; every `Drop` on that path is sync and lock-free.

## Phase 2: Architecture

### Solution

Make both owning back-edges weak; give evaluation a scoped strong pin.

- **Managers** store `std::sync::Weak<E>`. `DefaultAssetManager::with_capacity` and
  `ImmediateAssetManager::new` keep their `EnvRef<E>` parameter and store `Arc::downgrade(&envref.0)`.
  `get_envref` / `envref` upgrade; `try_get_envref` returns
  `Error::general_error("the environment was dropped; …")` on failure.
- **Assets**: `AssetData.envref` becomes `Weak<E>`; `AssetData::get_envref`, `AssetRef::get_envref`
  upgrade, with `try_` siblings. All constructors keep their signatures.
- **Pins** (strong `EnvRef<E>`, dropped with their owner): a field in `RunClaim` and in
  `InlineRunClaim`, upgraded in `AssetRef::try_claim_for_run` / `try_claim_for_run_inline` (an
  upgrade failure there fails the asset with that error so waiters wake); and the entries of
  `JobQueue.jobs`, which pair each parked `AssetRef` with its pin, released by
  `cleanup_completed_internal`. `local_deps` needs none: its parent's claim pins.
- **Spawned tasks**: each of the eight `tokio::spawn` sites in `assets.rs` either captures an
  `EnvRef` upgraded before the spawn and bounded by the task, or holds none (step 4).
- **Task exit**: `Drop for DefaultAssetManager` also calls `self.job_queue.shutdown()`.
  `AssetRef::new_temporary` spawns a loop holding a `WeakAssetRef` and the receiver only; the
  per-message body of `process_service_messages` moves into a private `handle_service_message`, so
  the loop upgrades per message and ends when `recv()` returns `None` (the asset, and with it
  `service_tx`, is gone). `run_with_future` keeps the strong loop it already terminates.

Rejected: `Arc::new_cyclic` (the `OnceLock` slot already breaks construction); weakly held cached
assets (the cache empties itself); a status-driven `AssetData` pin (~20 status writers);
`Environment::shutdown()` alone (opt-in; AC-1 fails); `Result` everywhere (decision 1). A
`Weak::upgrade` is one CAS, the same order as the `Arc::clone` it replaces.

Known-issue preflight: `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS` touches `InlineRunClaim`
but not ownership; nothing open blocks.

### Changes

`liquers-core/src/assets.rs`: `AssetData` (`envref` field, `new_ext`, `get_envref`, new
`try_get_envref`), `AssetRef::get_envref` + new `try_get_envref`, `new_temporary`,
`process_service_messages` (split), `RunClaim`, `InlineRunClaim`, `try_claim_for_run*`, `JobQueue`
(`jobs`, `submit`, `try_to_start_immediately`, `repark_as_submitted`, `run`,
`cleanup_completed_internal`, `waiting_jobs_count`), `DefaultAssetManager` / `ImmediateAssetManager`
(`envref` field, constructors, `get_envref`, `envref`, `Drop`), trait `AssetManager` (new default
method below). Mechanical `get_envref()` → `try_get_envref()?` in `Result` functions (76 sites in
the file; 18 `self.envref()`).

```rust
// AssetManager<E>, default method; both built-in managers override it
fn try_get_envref(&self) -> Result<EnvRef<E>, Error> { Ok(self.get_envref()) }
// inherent, same shape on AssetData (sync) and AssetRef (async)
pub async fn try_get_envref(&self) -> Result<EnvRef<E>, Error>;
```

`liquers-core/src/context.rs`: doc comments only (`EnvRef`, `init_with_envref`, the manager slot).
No change in other crates, commands or `specs/command_registry.yaml`.

Documents: `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` (environment lifecycle:
ownership and teardown) and `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` (pins,
`try_get_envref`), each with a History row and `reviewed:` bump.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A path calls `get_envref` after its pin is gone → panic | step 4 audit of spawns; `try_` in `Result` paths; full core suite | medium |
| Existing tests drop their only `EnvRef` early | step 6 keeps a binding; failures are loud | high |
| Env dropped inside a task or under the `jobs` lock | all `Drop`s on the path are sync and lock-free (`shutdown` is an atomic + `notify`) | high |
| Splitting `process_service_messages` changes message handling | body moved verbatim; `asset_cancellation`, `asset_failure_contract` suites | medium |
| wasm (`ImmediateAssetManager`, no tasks) | same weak field; `scripts/check-build-matrix.sh` wasm rows | high |

Recovery: one commit per step; reverting 2-3 restores the strong edges.

## Phase 3: Examples and Tests

### Examples

The Problem Example, with the expected counts, is test 1-3. Usage that must keep working (AC-5):

```rust
let envref = env.to_ref();
let asset = envref.evaluate("hello").await?;
drop(envref);
assert_eq!(asset.get().await?.try_into_string()?, "hi");
```

### Tests

New file `liquers-core/tests/environment_teardown.rs`; each test runs on both manager kinds unless
stated, with a `hello` command (`CommandRegistry::register_command`).

- `teardown01_fresh_environment_has_one_strong_reference` — `to_ref` and `EnvironmentBuilder::build`: AC-1
- `teardown02_cached_query_and_key_do_not_own_environment` — `-R/` key from an `AsyncMemoryStore` plus a query: AC-2
- `teardown03_last_envref_drops_environment_and_values` — `Weak` to env and to the cached value `Arc`: AC-3
- `teardown04_repeated_build_and_drop_retains_nothing` — 100 iterations, all `Weak`s dead: AC-4
- `teardown05_queued_asset_survives_dropped_envref` — capacity-1 queue, second job parked: AC-5
- `teardown06_context_envref_usable_in_command` — command calls `context.get_envref()` after the caller dropped its `EnvRef`: AC-5
- `teardown07_queued_manager_tasks_exit` (queued only) — `Handle::current().metrics().num_alive_tasks()` back to baseline: AC-6
- `teardown08_temporary_asset_loop_exits` (queued only) — same metric around `interpreter::evaluate`: AC-7
- `teardown09_manager_after_teardown_returns_error` — retained `get_asset_manager()`; `get_asset` is `Err`: AC-8

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test environment_teardown`, then
`cargo test -p liquers-core --lib --tests` and `cargo test -p liquers-lib --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-core/tests/environment_teardown.rs` — add the nine tests; every one except `teardown06` fails at HEAD — `cargo test -p liquers-core --test environment_teardown`. Rollback: delete the file.
- [x] 2. `assets.rs` `DefaultAssetManager`, `ImmediateAssetManager` — `Weak<E>` field, downgrade in constructors, upgrade in `get_envref`/`envref`, `try_get_envref` (trait default + overrides), `Drop` shuts down `job_queue` — teardown01, teardown07. Rollback: revert the commit.
- [x] 3. `assets.rs` `AssetData`, `AssetRef`, `RunClaim`, `InlineRunClaim`, `try_claim_for_run*`, `JobQueue` entries — weak `envref`, `try_get_envref`, pins — teardown02-05, teardown09. Rollback: revert steps 3 and 2 together.
- [x] 4. `assets.rs` spawn audit (the eight `tokio::spawn` sites in Phase 2) and `get_envref()` → `try_get_envref()?` in `Result` functions — `cargo test -p liquers-core --lib --tests`.
- [x] 5. `assets.rs` `AssetRef::new_temporary`, `process_service_messages` → `handle_service_message` + weak loop — teardown08; `--test asset_cancellation --test asset_failure_contract`. Rollback: revert; path 3 then leaks a task but no environment.
- [x] 6. Tests that lose their only `EnvRef` (`assets.rs` / `context.rs` test modules, `liquers-core/tests/`) — keep a binding — `cargo test -p liquers-core --lib --tests` and `cargo test -p liquers-lib --lib --tests`.
- [x] 7. Docs: `context.rs` / `assets.rs` doc comments, `DOC_04_…`, `DOC_03_…` (History row, `reviewed:`); close `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
`bash scripts/check-build-matrix.sh` (wasm rows); after `cargo clean`,
`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`;
`python3 scripts/docs_index.py --check`; a diff review for debug output and edits outside
`liquers-core` and the two reference documents.

## Phase 5: Implementation Notes (2026-10-22)

Implemented as planned, steps 1-3 and 5 in `9cc22b3`, steps 4 and 6 in `dd9781c`, step 7 with
the records. Managers and `AssetData` hold `Weak<E>`; `get_envref` panics on teardown and
`try_get_envref` is on `AssetData`, `AssetRef` and `AssetManager` (default `Ok(get_envref())`,
overridden by both built-in managers); `RunClaim`, `InlineRunClaim` and each `JobQueue.jobs` entry
carry a pin; `Drop for DefaultAssetManager` also shuts the job queue down; a temporary asset's
loop holds a `WeakAssetRef` and the receiver and calls the extracted `handle_service_message`.

Backlog plan §9 corrections applied: the audit covered **nine** spawn sites, including
`MetadataSaver::save_immediately`, whose coalescing task now takes the store
(`Arc<dyn AsyncStore>`) rather than the environment, so a pending save waiting out its interval does
not keep the environment alive. The other sites: the background `save_to_store` task carries a pin;
the expiration monitor and `AssetData::record_expiry` skip an asset whose environment is gone; the
two job tasks hold their claim's pin; the `RunClaim` repair moves the pin into its task; the
job-queue and monitor tasks hold none; `run_with_future`'s loop is awaited by its run. The sweep
changed 93 `get_envref()` / `envref()` sites in `Result` functions to the `try_` forms.

Deviations, none changing a contract:

- `claim.complete()` moved after the re-park of undrained local dependencies in both job tasks, so
  the claim's pin keeps the environment alive for them (the asset is finished, so the repair it
  disarms has nothing to do there).
- `JobQueue::shutdown` stores a permit (`notify_one`) besides waking waiters: the environment can
  now be dropped by the queue loop's own cleanup, between its shutdown check and its wait, where
  `notify_waiters` alone would leave it parked forever. `cleanup_completed_internal` releases the
  finished entries' pins after the `jobs` lock is dropped.
- A claim that finds the environment gone fails the asset in memory only (no store to write to).
- `teardown05` is two tests (queued with a capacity-1 gate; inline), because an inline evaluation
  finishes before `evaluate` returns. `teardown10_never_run_temporary_asset_is_freed` was added for
  the absorbed issue's own example (AC-7). The assertions on counts and `Weak`s poll briefly: a
  pin is released when its task finishes, which can be just after the value is observable.
- Step 6 changed seven core test files: a binding kept (`envref.clone()`) where a test handed its
  only `EnvRef` to an asset, and one fixture (`stale_dep_fixture`) that returns only the asset
  deliberately keeps its environment for the test process.

- PR #176 review: the sweep missed `get_envref` calls inside closures of `Result` functions
  (`mark_expired_status`, so `expire()` on a retained keyed asset panicked; `set_volatile_during_run`;
  the two map insertions of `get_nonvolatile_*_asset`). They use `try_get_envref` now, and three
  store reads in non-`Result` helpers answer "inconclusive" / skip on teardown.
  `teardown11_expire_after_teardown_returns_error` covers it.

`TEMPORARY-ASSET-NEVER-RUN-IS-KEPT-ALIVE-BY-ITS-SERVICE-LOOP` (covered by step 5, per the backlog
plan) is closed with this design, and its coverage record `temporary-asset-never-run-leak` is
complete; it stays that issue's design.

Validation: `cargo test -p liquers-core --lib --tests` and `cargo test -p liquers-lib --lib --tests`
pass; `cargo check -p liquers-core --target wasm32-unknown-unknown` builds (its warnings predate
this change, `CORE-WASM-WARNING-FREE`). Reviewed documents: DOC-04 (new §Ownership and teardown)
and DOC-03 (§Manager lifecycle), plus the four that described the leak as open
(`PYTHON_INTEGRATION.md`, `PYTHON_GUIDE.md`, `LANGUAGE-INTEGRATION_GUIDE.md`,
`ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`), each with a History row.

liquers-py: `RUNTIME05` now passes (`bash scripts/test-liquers-py.sh` reported a strict XPASS), so
its `xfail` is removed and `liquers-py/README.md` marks `RUNTIME` and `ENVIRON` `COMPLETE`; `EVAL`
and `COMMAND` stay `PARTIAL` on `VALUE`. No liquers-py code changed. A replaced environment is now
freed by `shutdown()`, not never (noted on `LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION`).

