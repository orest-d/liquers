---
id: LANGUAGE-LATE-REGISTRATION-WITHOUT-REBUILD
kind: design
title: liquers-web and liquers-py apply late registrations to the running environment
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [web, py, core/commands]
issues: [LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION]
created: 2026-10-27
---
# liquers-web and liquers-py apply late registrations to the running environment

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written, not
approved, nothing implemented. Written against the unimplemented `post-init-command-registration`.

## Phase 1: High-Level Design

### Purpose

After the first evaluation, a command registration, removal or metadata import in `liquers-py`
or `liquers-web` builds a whole new environment, so every in-memory result is recomputed and the
old environment is kept on a retired list (Python) or dropped (web). Once
`post-init-command-registration` lets a shared environment publish a new command registry, both
integrations should publish their replayed registry into the running environment instead, so only
the results of changed commands are lost.

### Problem Example

Verified at HEAD (`04667a80`):

- `liquers-py/src/environment.rs`: `EnvCell::register`, `unregister` and `import_metadata` call
  `retire`, which moves `EnvState::Built(envref)` onto `Declarations::retired` and sets `Pending`;
  the next `envref()` calls `build` with a new asset manager.
- `liquers-web/src/environment.rs`: `register_commands_on` calls `rebuild_with` once the
  environment is shared, and `unregister_command_on` calls `rebuild_without`; both build a new
  environment with `new_environment`, replay `REGISTERED_SPECS`, and `set_singleton`.
- `liquers-web/src/command/adapter.rs` `register_js_command` never sets `impl_version`, and
  `CommandMetadataRegistry::add_command` carries an existing entry's version over, so a replaced
  JavaScript command keeps version `0`.

```python
env = lq.Environment()
env.evaluate("-R/data/big.csv/-/slow_summary")   # computed, held in the asset cache
@env.command
def extra(text: str) -> str: return text
env.evaluate("-R/data/big.csv/-/slow_summary")   # today: computed again, the rebuild dropped it
```

The second evaluation should be served from the cache, and `extra` should be available. Checked
with `liquers-validate --command slow_summary`.

### Scope and Acceptance Criteria

- **AC-1** The cache survives a new command
  WHEN a command is registered after a cached evaluation
  THEN re-evaluating the same query does not run its command again, and the new command evaluates
- **AC-2** A replaced command invalidates its results
  WHEN a command is registered again under the same key (Python: with different code; JavaScript:
  any re-registration)
  THEN the next evaluation of a query using it runs the new implementation
- **AC-3** A removal invalidates and restores
  WHEN a command is unregistered
  THEN queries using it fail as not registered; a Python command that replaced a built-in brings
  the built-in back, as today
- **AC-4** Metadata imports apply in place
  WHEN `import_metadata` (Python) or a registry document (web) changes a command's metadata
  THEN results of that command are recomputed, and results of other commands are kept
- **AC-5** No stale read right after an update
  WHEN an evaluation starts immediately after a registration that replaced a command
  THEN it waits until the replaced command's results are expired, and never returns them
- **AC-6** A failed update changes nothing
  WHEN the replayed declarations fail validation
  THEN the next use raises `ConfigurationError` / rejects with the report, as today, and the
  published registry and cache are unchanged
- **AC-7** No environment accumulates
  WHEN registrations follow evaluations repeatedly
  THEN the environment and its asset manager stay the same objects; Python keeps no retired list
- **AC-8** Replay tests stay green
  WHEN the existing replay tests run (all but the Python unit test that asserts retirement)
  THEN they pass unchanged in what they assert

Non-goals: store configuration after sharing (still a rebuild in `liquers-web`: assets of the old
store are stale); registration on an explicit JavaScript `Environment` instance; Python's
`async def` command loop pool.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It changes observable binding behaviour documented in
  `PYTHON_INTEGRATION.md` §3 (rule 4), spans two crates (rule 6), and readiness is
  `needs-decision` (rule 5).
- **Leading issue:** **Open design - where the asynchronous half runs.**
- **Explanation:** both integrations already rebuild a complete registry from retained
  declarations; this design keeps that replay and changes only its destination. Phases 3-4 hold
  under the core design's recommended trait shape.
- **Open questions:**
  1. **Open design - the asynchronous half.** The core `EnvRef::update_commands` is async (version
     refresh, cascade), but `registerCommands` is synchronous and must stay so, and Python's
     `envref()` must not block on a loop. Recommended: publish synchronously with
     `Environment::update_command_registry`, run the expiry as a spawned task, and make every
     evaluation entry point await that task first (AC-5). This needs the core to expose the async
     half on its own, for example `EnvRef::expire_after_registry_update(&RegistryUpdate)`, which
     `update_commands` would call. Without it, both integrations repeat its three calls.
  2. **Proposed resolution - replay the whole registry.** The update closure replaces the
     registry with one built exactly as today's rebuild builds it (`configured_builder` in
     Python; `new_environment` plus replay in the web), rather than applying each change
     incrementally. One path, so replay and update cannot drift; restoring a built-in (AC-3) needs
     no special case. The cost, building the command library's registry once per update, is what
     a rebuild pays today.
  3. **Proposed resolution - JavaScript versions.** Each accepted JavaScript declaration gets
     `Version::new_unique()` as its `impl_version`, retained with it and reapplied on replay.
     Without it a replaced command is invisible to the core's version refresh (AC-2). Python
     already hashes the code object.
  4. **Assumption - core design.** `post-init-command-registration` question 1 (a), snapshot
     handles, and its `update_command_registry` / `RegistryUpdate` as specified.

### Design Dependencies

- `post-init-command-registration` (in review, `needs-decision`): **requires**. Question 1 asks
  it for a small addition.
- `language-global-enum-import` (this run): **overlaps**. It adds enums to the same replay; this
  design reuses the replay, so either order works.
- `registry-publication` (in review): **overlaps**. Its `IMPORTED_REALMS` are applied in
  `new_environment`, which the update replay calls, so imported realms carry over unchanged.

### Consolidated Findings

- The core's version refresh can only expire what changed version; a JavaScript command never has
  a version of its own today (question 3).
- Python's declarations lock must not be held across a wait. With the synchronous publish there
  is none: the registry is swapped under the core's own lock, and the expiry runs as a task.
- `liquers-web/README.md` lists two limitations under `POST-INIT-COMMAND-REGISTRATION`: the
  rebuild row goes; the explicit-instance row stays and needs a tracker of its own once that
  issue closes.

## Phase 2: Architecture

### Solution

**Shared shape.** `fresh` is the registry today's rebuild would build. Then:
`let update = envref.0.update_command_registry(Box::new(move |cr| { *cr = fresh; Ok(()) }))?;`
publishes it; `spawn(envref.expire_after_registry_update(update))` runs the expiry; the spawned
task's `Shared` future is the **refresh gate**, and each evaluation awaits the current gate before
planning. An expiry error is printed (`eprintln!` / `console.warn`) and the gate resolves; the
next update retries, as the core design states.

**Python** (`liquers-py/src/environment.rs`). `EnvState::Built` gains `stale: bool`.
`register`, `unregister`, `import_metadata` set `stale` instead of calling `retire`. `envref()`, on
a stale `Built`, runs `configured_builder` and its validation (an error-level issue →
`ConfigurationError`, still stale, AC-6), publishes `builder.command_registry`, spawns the expiry
on `runtime()`, stores the gate in `EnvCell`, clears `stale`. The evaluation entry points
(`evaluate`, `evaluate_state`, `get_asset` and their `_async` forms) await the gate first. `Declarations::retired` and `retire` are removed; `shutdown` stops one asset
manager.

**Web** (`liquers-web/src/environment.rs`). `rebuild_with` / `rebuild_without` as used by
commands become `update_registry(additional)`: `new_environment()`, replay, then the shared shape
on the singleton (`wasm_bindgen_futures::spawn_local`), gate in a thread-local
`REFRESH_GATE`. `configure_store_on` and `register_store_object_on` keep `rebuild_without`.
`crate::eval::evaluate_to_promise` and `crate::asset::get_asset_for` await the gate.
`command::import::Retained` carries an `impl_version`, set at intake and reapplied in
`register_js_command`.

Rejected: incremental closures per call (two code paths; question 2); `update_commands` awaited
inside `registerCommands` (it would return a `Promise` only after sharing); blocking Python's
`envref()` on the expiry (blocks a caller's event loop).

### Changes

- `liquers-py/src/environment.rs`: `EnvState`, `Declarations` (no `retired`), `EnvCell::{register,
  unregister, import_metadata, envref, shutdown}`, the gate field; the evaluation methods await it.
- `liquers-py/src/aio.rs`: the awaitable evaluation futures await the gate.
- `liquers-web/src/environment.rs`: `update_registry`, `REFRESH_GATE`, `register_commands_on`,
  `unregister_command_on`; `liquers-web/src/eval.rs`, `asset.rs`: await the gate.
- `liquers-web/src/command/import.rs` `Retained` and `liquers-web/src/command/adapter.rs`
  `register_js_command`: the retained `impl_version`.
- No signature visible to Python or JavaScript changes. No command; registry file unchanged.
- Documents (History row, `reviewed:` bump): `reference/PYTHON_INTEGRATION.md` §3 Rebuild and
  Shutdown; `guides/PYTHON_GUIDE.md` (late registration); `guides/JAVASCRIPT_GUIDE.md` (three
  "rebuilds" passages); `liquers-web/README.md` limitations. Proposed `affects_docs`: the three
  under `specs/`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| An evaluation reads a replaced command's cached result between publish and expiry | The gate (AC-5); test pins it | medium |
| JavaScript re-registration of an identical function now invalidates its results | Safe direction; documented | high |
| An expiry error leaves stale results | Printed; the next update retries the refresh | medium |
| Per-update cost of rebuilding the library registry | Equal to today's rebuild, minus the asset manager | high |
| `RUNTIME05` handle-release counts shift (no retired environments) | Re-run; counts can only drop | medium |

Recovery: revert both crates; the rebuild path is intact in history.

## Phase 3: Examples and Tests

### Examples

```javascript
liquers.registerCommand({ name: "slow", run: () => { calls += 1; return calls; } });
await liquers.evaluate("slow");            // 1
liquers.registerCommand({ name: "extra", run: () => "x" });
await liquers.evaluate("slow");            // 1 again: cached (AC-1)
liquers.registerCommand({ name: "slow", run: () => 42 });
await liquers.evaluate("slow");            // 42: the gate waited for the expiry (AC-2, AC-5)
```

### Tests

Python, `liquers-py/tests/test_ENVIRON_environment.py` (`bash scripts/test-liquers-py.sh`):

- `test_ENVIRON_late_registration_keeps_the_cache`: a counting command, then `extra` (AC-1)
- `test_ENVIRON_replaced_command_is_recomputed`: new code under the same name (AC-2, AC-5)
- `test_ENVIRON_unregister_restores_the_builtin`: override `to_text`, unregister (AC-3)
- `test_ENVIRON_import_metadata_expires_only_that_command` (AC-4)
- `test_ENVIRON_failed_update_keeps_the_published_registry`: invalid import, `ConfigurationError`,
  then a valid import (AC-6)

JavaScript, `liquers-web/tests/environment_ENVIRON.rs`
(`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`):

- `environ_late_registration_keeps_the_cache` (AC-1)
- `environ_replaced_command_is_recomputed`: the Phase 3 example (AC-2, AC-5)
- `environ_unregister_after_sharing_expires` (AC-3)
- `environ_registry_document_after_sharing_expires_only_that_command` (AC-4)
- `environ_refused_late_batch_changes_nothing` (AC-6)
- `environ_late_registration_keeps_the_singleton`: `shared_env()` before and after point at the
  same environment (`Arc::ptr_eq`) (AC-7)

Rust unit, `liquers-py/src/environment.rs` `tests`: `a_rebuild_keeps_the_store_and_retires_the_old_environment`
becomes `late_registration_keeps_the_store_and_the_environment`: the `EnvRef` before and after
is the same `Arc` (AC-7).

Existing, unchanged: `environ08_registry_form_commands_survive_a_rebuild` and
`test_COMMAND_late_registration_keeps_store` (AC-8).

## Phase 4: Implementation Plan

### Steps

Prerequisite: `post-init-command-registration` implemented, with the async half of question 1.

- [ ] 1. `liquers-web/src/command/import.rs` `Retained`, `adapter.rs` `register_js_command` — retained `impl_version` — `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles environ08`
- [ ] 2. `liquers-web/src/environment.rs` `update_registry`, `REFRESH_GATE`; `eval.rs`, `asset.rs` await it — the web tests — same command, also with `,records`
- [ ] 3. `liquers-py/src/environment.rs` stale state, publish, gate; `aio.rs` — the Python tests — `bash scripts/test-liquers-py.sh`
- [ ] 4. `liquers-py/src/environment.rs` remove `retired` / `retire`; `shutdown`; the rewritten unit test — `bash scripts/test-liquers-py.sh` (`RUNTIME05`, shutdown tests)
- [ ] 5. Documents of Phase 2 with History rows — `./liquers-web/scripts/test-guide-examples.sh` and `python3 scripts/docs_index.py --check`
- [ ] 6. `specs/issues/LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION.md` closed with a resolution note; this design `complete` — `python3 scripts/docs_index.py` then `--check`

Rollback: web (1-2) and Python (3-4) revert independently.

### Validation

Both bindings' loops as in `CLAUDE.md`; `cargo fmt --check -p liquers-web -p liquers-py`. Diff
review: two crates and the documents; no core change beyond what the core design delivers.
