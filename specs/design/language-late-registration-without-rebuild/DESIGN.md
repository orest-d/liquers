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

Bulk design, 2026-10-27: Phases 1-4 written, not approved, nothing implemented.

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

- `liquers-py/src/environment.rs`: `register`, `unregister`, `import_metadata` call `retire`,
  which moves the built `EnvRef` onto `Declarations::retired`; the next `envref()` builds anew.
- `liquers-web/src/environment.rs`: `register_commands_on` calls `rebuild_with` once the
  environment is shared, `unregister_command_on` calls `rebuild_without`; both replay
  `REGISTERED_SPECS` into a `new_environment` and `set_singleton`.
- `liquers-web/src/command/adapter.rs` `register_js_command` never sets `impl_version`, and
  `add_command` (:121) carries the old version over, so a replaced JavaScript command keeps `0`.

```python
env = lq.Environment()
env.evaluate("-R/data/big.csv/-/slow_summary")   # computed, held in the asset cache
@env.command
def extra(text: str) -> str: return text
env.evaluate("-R/data/big.csv/-/slow_summary")   # today: computed again, the rebuild dropped it
```

The second evaluation should come from the cache (checked with `liquers-validate --command
slow_summary`).

### Scope and Acceptance Criteria

- **AC-1** The cache survives a new command
  WHEN a command is registered after a cached evaluation
  THEN re-evaluating the same query does not run its command again, and the new command evaluates
- **AC-2** A replaced command invalidates its results
  WHEN a command is registered again under its key (Python: with different code)
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
  THEN the next use fails with the report, as today, and the registry and cache are unchanged
- **AC-7** No environment accumulates
  WHEN registrations follow evaluations repeatedly
  THEN the environment and its asset manager stay the same objects; Python keeps no retired list
- **AC-8** Replay tests stay green
  WHEN the existing replay tests run
  THEN their replay assertions pass unchanged; only the identity assertions listed in Phase 3
  (a new `Arc`, a retired list) are inverted

Non-goals: store configuration after sharing (still a rebuild in `liquers-web`); registration on
an explicit JavaScript `Environment`; Python's `async def` loop pool.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible: documented binding behaviour changes (rule 4), two crates
  (rule 6), readiness `needs-decision` (rule 5).
- **Leading issue:** **Open design - synchronous reads before the gate.**
- **Explanation:** both integrations already replay retained declarations into a registry; only
  the destination changes, to the core's `EnvRef::update_commands`.
- **Open questions:**
  1. **Open design - synchronous reads before the gate.** `update_commands` is async, and the
     publish happens when the spawned task first runs, so a synchronous read right after
     `registerCommands` (`existing_command_kind`, `has_command`, `commandNames`,
     `describeCommand`) would see the old registry. Recommended: `update_registry` keeps the
     replayed `fresh` metadata in a thread-local `REPLAYED` until the gate resolves, and those
     readers look there first. The alternative accepts a one-microtask stale window. Python has
     none: every reader goes through `envref()` and awaits the gate.
  2. **Proposed resolution - replay the whole registry.** The update closure replaces the
     registry with one built exactly as today's rebuild builds it (`configured_builder` in
     Python; `new_environment` plus replay in the web), rather than applying each change
     incrementally. One path, so replay and update cannot drift, and restoring a built-in (AC-3)
     needs no special case. The cost per update is today's rebuild minus the asset manager.
  3. **Proposed resolution - JavaScript versions.** Each accepted JavaScript declaration gets
     `Version::new_unique()` as its `impl_version`, retained with it and reapplied on replay.
     It is set after `add_command` through `get_mut`, as Python does. Without it a replaced
     command is invisible to the core's version refresh (AC-2).

### Design Dependencies

- `post-init-command-registration` (in review, `needs-decision`): **requires**
  `EnvRef::update_commands` as specified there; nothing is added to it. Under its question 1 (a)
  or (b) this design holds unchanged, since both keep `update_commands`; under (b) every update
  keeps one more registry alive, which a page registering per route pays. Under (c), not
  planned, this design is **blocked** and its issue closes with the rebuild kept.
  `DefaultEnvironment` is `GenericEnvironment`, whose `CommandExecutor` is `CommandRegistry<Self>`
  (`liquers-core/src/context.rs:1701`), so `*cr = fresh` type-checks.
- `language-global-enum-import` (this run): **overlaps, T3 holds** (same replay functions); the
  second to land carries the enum replay into `update_registry`.
- `language-store-type-registration` (this run): **overlaps**. Both edit
  `liquers-web/src/environment.rs`; store configuration keeps `rebuild_without` here, so its
  types still need to survive a store rebuild, while a command registration no longer rebuilds.
  Either order; the second rebases.
- `registry-publication` (in review): **overlaps**. Its `IMPORTED_REALMS` are applied in
  `new_environment`, which the update replay calls, so imported realms carry over unchanged.

### Consolidated Findings

- The core's version refresh can only expire what changed version; a JavaScript command never has
  a version of its own today (question 3).
- Python's declarations lock must not be held across a wait: `envref()` spawns the update
  task under it and returns; the wait happens after the lock is released.
- `liquers-web/README.md`'s rebuild limitation row goes; its explicit-instance row stays.

## Phase 2: Architecture

### Solution

**Shared shape.** `fresh` is the registry today's rebuild would build. The binding spawns
`envref.update_commands(move |cr| { *cr = fresh; Ok(()) })`; the task's `Shared` future is the
**refresh gate**, and each evaluation awaits the current gate before planning. An error is printed
(`eprintln!` / `console.warn`) and the gate resolves; the next update retries, as the core states.

**Python** (`liquers-py/src/environment.rs`). `EnvState::Built` gains `stale: bool`.
`register`, `unregister`, `import_metadata` set `stale` instead of calling `retire`. `envref()`, on
a stale `Built`, runs `configured_builder` and its validation (an error-level issue →
`ConfigurationError`, still stale, AC-6), spawns the update on `runtime()`, stores the gate in
`EnvCell`, clears `stale`. The evaluation entry points (`evaluate`, `evaluate_state`, `get_asset`,
their `_async` forms) and `command_metadata` await the gate. `retire` and `Declarations::retired`
are removed; `EnvCell::invalidate` (environment.rs:162), used only by tests, marks stale and still
fails after `shutdown`; `shutdown` stops one asset manager.

**Web** (`liquers-web/src/environment.rs`). For commands, `rebuild_with` / `rebuild_without`
become `update_registry(additional)`: `new_environment()`, replay, then the shared shape
(`spawn_local`), gate in thread-local `REFRESH_GATE`, `fresh` metadata in `REPLAYED`. The store
paths keep `rebuild_without`. `eval::evaluate_to_promise` and `asset::get_asset_for` await the gate.
`command::import::Retained` carries an `impl_version`, set at intake; `register_js_command` sets
it after `add_command` through `get_mut`.

Rejected: incremental closures (two code paths; question 2); awaiting `update_commands` inside
`registerCommands` (a `Promise` only after sharing); a core `expire_after_registry_update` split
(not needed: the spawned `update_commands` is the gate).

### Changes

- `liquers-py/src/environment.rs`: `EnvState`, `Declarations` (no `retired`), `EnvCell::{register,
  unregister, import_metadata, invalidate, envref, shutdown}`, the gate; evaluation and
  `command_metadata` await it.
- `liquers-py/src/aio.rs`: the awaitable evaluation futures await the gate.
- `liquers-web/src/environment.rs`: `update_registry`, `REFRESH_GATE`, `REPLAYED`,
  `existing_command_kind`, `register_commands_on`, `unregister_command_on`; `eval.rs`, `asset.rs`
  await the gate.
- `liquers-web/src/command/import.rs` `Retained`, `adapter.rs` `register_js_command`.
- No Python or JavaScript signature changes; registry file unchanged.
- Documents (History row, `reviewed:` bump): `reference/PYTHON_INTEGRATION.md` §3;
  `guides/PYTHON_GUIDE.md`, `guides/JAVASCRIPT_GUIDE.md` ("rebuilds" passages);
  `liquers-web/README.md`. Proposed `affects_docs`: the three under `specs/`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| An evaluation reads a replaced command's cached result between publish and expiry | The gate (AC-5); test pins it | medium |
| Re-registering an identical JavaScript function invalidates its results | Safe direction | high |
| An expiry error leaves stale results | Printed; the next update retries | medium |
| `RUNTIME05` handle-release counts shift (no retired environments) | Re-run; counts can only drop | medium |
| Four existing tests assert a new environment `Arc` | Inverted (Phase 3), Steps 2 and 4 | high |
| A synchronous reader sees the old registry | `REPLAYED` (question 1); `has_command` tests pin it | medium |

Recovery: revert.

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
- `test_ENVIRON_failed_update_keeps_the_published_registry`: invalid, then valid import (AC-6)

JavaScript, `liquers-web/tests/environment_ENVIRON.rs`
(`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`):

- `environ_late_registration_keeps_the_cache` (AC-1)
- `environ_replaced_command_is_recomputed`: the Phase 3 example (AC-2, AC-5)
- `environ_unregister_after_sharing_expires` (AC-3)
- `environ_registry_document_after_sharing_expires_only_that_command` (AC-4)
- `environ_refused_late_batch_changes_nothing` (AC-6)

Changed (identity inverted to `Arc::ptr_eq`, replay assertions kept, AC-7/AC-8):

- `liquers-web/tests/environment_ENVIRON.rs`: `web_register_after_sharing_rebuilds` (:191),
  `fvt8_2_a_rebuild_keeps_the_javascript_type` (:254, premise "the same environment"),
  `environ08_valid_batch_registers_with_one_rebuild` (:533); `…rebuild…` → `…update…`
- Renamed only, no identity assertion: `web_unregister_after_sharing_rebuilds` (:288), whose
  synchronous `has_command` checks rely on `REPLAYED`; `commands_COMMAND.rs:675-684` keeps its
  assertion (the snapshot still replays), comment "forcing a rebuild" → "an update"
- `liquers-py/src/environment.rs` `tests`: `a_rebuild_keeps_the_store_and_retires_the_old_environment`
  (:886) becomes `invalidate_keeps_the_store_and_the_environment` (same `EnvRef` `Arc`);
  `shutdown_is_idempotent_and_later_use_fails` (:902) unchanged

Existing, unchanged: `environ08_registry_form_commands_survive_a_rebuild`,
`test_COMMAND_late_registration_keeps_store` (AC-8).

## Phase 4: Implementation Plan

### Steps

Prerequisite: `post-init-command-registration` implemented (`EnvRef::update_commands`).

- [ ] 1. `liquers-web/src/command/import.rs` `Retained`, `adapter.rs` `register_js_command` — retained `impl_version` — `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles environ08`
- [ ] 2. `liquers-web/src/environment.rs` `update_registry`, `REFRESH_GATE`, `REPLAYED`; `eval.rs`, `asset.rs` await it — the web tests and the four changed ones — same command (no filter), also with `,records`
- [ ] 3. `liquers-py/src/environment.rs` stale state, publish, gate; `aio.rs` — the Python tests — `bash scripts/test-liquers-py.sh`
- [ ] 4. `liquers-py/src/environment.rs` remove `retired` / `retire`; `invalidate`, `shutdown`; the rewritten unit test — `bash scripts/test-liquers-py.sh` (`RUNTIME05`, shutdown tests)
- [ ] 5. Documents of Phase 2 with History rows — `./liquers-web/scripts/test-guide-examples.sh` and `python3 scripts/docs_index.py --check`
- [ ] 6. `specs/issues/LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION.md` closed with a resolution note; this design `complete` — `python3 scripts/docs_index.py` then `--check`

Rollback: web (1-2) and Python (3-4) revert independently.

### Validation

Both bindings' loops as in `CLAUDE.md`; `cargo fmt --check -p liquers-web -p liquers-py`. Diff:
two crates and the documents; no core change.
