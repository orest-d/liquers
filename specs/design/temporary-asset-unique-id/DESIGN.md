---
id: TEMPORARY-ASSET-UNIQUE-ID
kind: design
title: A temporary asset takes a fresh id from its manager
form: compact
status: complete
gh_pr: [200]
readiness: ready
autofix: eligible
area: [core/assets]
issues: [TEMPORARY-ASSETS-SHARE-ID-ZERO]
created: 2026-10-20
---
# A temporary asset takes a fresh id from its manager

> **Complete on 2026-10-27**: implemented by automatic fix in #200.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

`AssetData::new_temporary` builds every temporary asset with id `0`, so two temporary assets are
indistinguishable to any code that keys on `AssetRef::id()`. Give each one a fresh id from the
environment's asset manager, the counter every other asset already uses.

### Problem Example

Verified at HEAD (`liquers-core/src/assets.rs` `AssetData::new_temporary`, `new_ext(0, …)`):

```rust
let a = AssetRef::new_temporary(envref.clone());
let b = envref.get_asset_manager().create_temporary_asset();
assert_ne!(a.id(), b.id());   // today: both are 0
```

Where it matters today: `interpreter::evaluate` runs its plan under a temporary asset, and on the
queued manager a dependency that cannot get a job slot is parked on its parent's local queue,
keyed by the parent's id (`DefaultAssetManager::get_dependency_asset` →
`JobQueue::push_local_dependency(parent.id(), …)`; `drain_dependencies` pops by `parent.id()`).
Two concurrent `interpreter::evaluate` calls therefore share local queue `0`: one drains and
`enter_dependencies`-logs the other's dependencies. Expiration tracking (`untrack_expiration(id)`)
and log lines that name an asset by id have the same ambiguity.

Should be: every temporary asset has its own id, distinct from every other asset of that
environment.

### Scope and Acceptance Criteria

- **AC-1** Distinct ids
  WHEN two temporary assets are created, through `AssetRef::new_temporary`, `AssetData::new_temporary` or `AssetManager::create_temporary_asset`
  THEN their ids differ from each other and from every id the manager handed out before
- **AC-2** No id 0
  WHEN a temporary asset is created on either built-in manager
  THEN its id is not `0` and comes from `AssetManager::next_id_for_asset`
- **AC-3** Nothing else changes
  WHEN a temporary asset is used as today (no key, never stored, never registered)
  THEN the existing core suites pass unchanged

Non-goals: the temporary asset's service-loop leak (`TEMPORARY-ASSET-NEVER-RUN-IS-KEPT-ALIVE-BY-ITS-SERVICE-LOOP`,
covered by `environment-manager-reference-cycle`); replacing id comparisons with pointer
comparisons (the audit below finds none that a unique id does not fix).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a bug fix inside one function of `liquers-core/src/assets.rs`
  plus doc comments; no `pub` signature, type, command or format changes (rules 1-6 hold).
- **Leading issue:** None
- **Explanation:** the manager's id counter already exists behind a required trait method, and
  `AssetData::new_temporary` already receives the `EnvRef` that reaches it.
- **Open questions:** None.

### Design Dependencies

- `environment-manager-reference-cycle` — **overlaps** (in_review): it rewrites
  `AssetRef::new_temporary`'s service loop; this design changes only `AssetData::new_temporary`.
  Either order; the second rebases trivially.
- `temporary-asset-never-run-leak` — **overlaps**: the sibling temporary-asset issue, covered by
  the design above. No order.

### Consolidated Findings

- Audit of `id()` used as identity (`assets.rs`, `dependencies.rs`, `context.rs`,
  `evaluation_session.rs`): the job queue's `local_deps`, `jobs` de-duplication and expiration
  tracking key on ids; ownership checks compare a keyed asset's id with a map entry's. A temporary
  asset is never keyed or mapped, so only the job-queue and expiration paths can see it, and a
  unique id fixes both. `DEPENDENT-ASSET-WEAK-REFERENCES-ARE-NEVER-PRUNED` already compares weak
  dependents by allocation, which stays correct.
- `AssetData::new_temporary` is reached only after the environment is built (`EnvRef` exists, the
  manager is installed), so drawing the id from the manager cannot run before the manager exists.

## Phase 2: Architecture

### Solution

In `AssetData::new_temporary(envref)`, replace the literal `0` with
`envref.get_asset_manager().next_id_for_asset()` (sync; `AssetManager::next_id_for_asset` is a
required trait method implemented by both built-in managers and by every external manager).
`AssetRef::new_temporary` and both `create_temporary_asset` implementations call it and need no
change. Update the doc comment ("id `0`" → "a fresh id from the environment's asset manager").

Rejected: a separate static counter for temporary assets (ids could collide with managed ones);
`create_temporary_asset` passing `self.next_id()` only (misses `interpreter::evaluate` and tests,
which call `new_temporary` directly).

Known-issue preflight: none blocks. `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` makes the manager's
`EnvRef` weak; `get_asset_manager` on an `EnvRef` stays infallible.

### Changes

- `liquers-core/src/assets.rs`: `AssetData::new_temporary` body and doc; `AssetRef::new_temporary`
  doc (drop "id `0`" if present). No `pub` signature changes, no new items, no command or
  registry change. Sync.
- Documents: none state the id; `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` is
  checked for a mention during step 3 and left unchanged if it has none.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A test asserts `id() == 0` | grep found none (`id() == 0`, `asset 0`) | high |
| An external manager's `next_id_for_asset` panics or is slow | it is a required method already called per asset | high |
| Log output changes (`asset 0` → a real id) | stderr only, not asserted | high |

Recovery: one-line revert.

## Phase 3: Examples and Tests

### Examples

The Problem Example is test `temporary_assets_have_distinct_ids`.

### Tests

In `liquers-core/src/assets.rs` `mod tests` (beside the existing `AssetRef::new_temporary` test at
the `new_temporary(envref)` call site), on `SimpleEnvironment<Value>` and
`ImmediateEnvironment<Value>`:

- `temporary_assets_have_distinct_ids` — one asset from each of the three constructors plus one
  `manager.next_id_for_asset()` before them; all four ids distinct and none `0`. Covers AC-1, AC-2.
- Existing suites `cargo test -p liquers-core --lib --tests` stay green. Covers AC-3.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib temporary_assets_have_distinct_ids`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-core/src/assets.rs` `mod tests` — add `temporary_assets_have_distinct_ids`; it fails at HEAD — `cargo test -p liquers-core --lib temporary_assets_have_distinct_ids`. Rollback: delete the test.
- [x] 2. `assets.rs` `AssetData::new_temporary` — id from `envref.get_asset_manager().next_id_for_asset()`, doc comments — the test of step 1 and `cargo test -p liquers-core --lib --tests`. Rollback: revert the commit.
- [x] 3. Records — close `TEMPORARY-ASSETS-SHARE-ID-ZERO` with a resolution note; check DOC_03 for an id mention — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `bash scripts/check-build-matrix.sh` (wasm rows: the
function is target-independent); diff review for edits outside `assets.rs` and the issue file.
