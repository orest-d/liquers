# Phase 4: Implementation Plan - keyed-delegation-hand-off

## Overview

Every step is small and local; the whole change is one crate. Steps run in order — step 2's tests
fail until step 1 lands.

## Implementation Steps

### Step 1 — Same-node guard in `record_dependency_on_asset`

**File:** `liquers-core/src/assets.rs` (`AssetRef::record_dependency_on_asset`, ~line 1107)

Derive `current_dep_key` immediately after `dep_key`, before the version read and the metadata
upsert. Return `Ok(())` when `current_dep_key.as_ref() == Some(&dep_key)`, with a comment naming
the rule ("two assets holding the same key are one dependency-graph node — a hand-off, not an
edge") and citing `ASSET-KEYED-DELEGATION-ALWAYS-CYCLES`. Reuse `current_dep_key` in the existing
`if let Some(current_key)` block instead of re-reading the lock.

Signature unchanged. Update the doc comment: it currently says "Record a direct dependency on
another asset in metadata and the dependency manager" with no mention of the exemption.

**Validation:** `cargo check -p liquers-core`

### Step 2 — Call-site comment in `evaluate_recipe`

**File:** `liquers-core/src/assets.rs` (delegation branch, ~line 1885)

Replace "Record delegation as a dependency wait, then delegate the F-1 inline guard onto the shared
… wait primitive" with a comment that states the hand-off semantics and why
`record_dependency_on_asset` is still called. Keep the F-1 explanation — it is still true and still
the reason `wait_for_dependency` is used rather than a bare `get()`. No code change.

**Validation:** `cargo check -p liquers-core`

### Step 3 — Unit tests T1 and T2

**File:** `liquers-core/src/assets.rs`, `#[cfg(test)] mod tests`, next to
`test_record_dependency_on_asset_does_not_downgrade_known_metadata_version_to_unknown`

Add `record_dependency_on_asset_skips_same_node_hand_off` and
`record_dependency_on_asset_records_distinct_key` per Phase 3. Follow the local convention: build
assets with `AssetData::<SimpleEnvironment<Value>>::new(id, key.into(), envref).to_ref()` and
unique ids (2240, 2241, 2242 are free).

**Validation:** `cargo test -p liquers-core --lib assets::`

### Step 4 — Invert `scenario_keyed_delegation`

**File:** `liquers-core/tests/manager_parametric.rs`

Replace the error-expecting body with the value-and-counter assertions from Phase 3, keeping both
preconditions (`assert_ne!` on ids, counter `1` before). Rewrite the doc comment: it currently
explains at length why the branch cannot succeed and instructs the reader to invert the test, all
of which becomes obsolete. It should instead state the contract being pinned — branch selection
*and* hand-off — and note that the counter is the assertion that catches a regression to
self-evaluation (Phase 3 S3).

**Validation:** `cargo test -p liquers-core --test manager_parametric`

### Step 5 — Full core regression

**Validation:**
```bash
cargo test -p liquers-core --lib --tests
cargo test -p liquers-lib --lib --tests
```

`liquers-lib` is the standard loop from `CLAUDE.md` and transitively covers core. Watch
specifically for the dependency-scheduling and expiration suites, which are the ones that exercise
`wait_for_dependency`.

### Step 6 — File `DELEGATED-VALUE-REPERSISTED`

**File:** `specs/issues/DELEGATED-VALUE-REPERSISTED.md` (new)

`status: draft`, `priority: P3`, `complexity: S`, `area: [core/assets]`. The delegating asset
re-writes the owner's bytes and metadata to the store under the same key. Required by `CLAUDE.md`:
noticed and not fixed ⇒ filed.

### Step 7 — Documentation (Phase 5)

Per the Phase 2 table: `DEPENDENCIES_STATUS.md` (content + `## History` + `reviewed:`), close
`ASSET-KEYED-DELEGATION-ALWAYS-CYCLES`, correct the two issues that reference it, write
`phase5-documentation.md`, update `specs/README.md` and `specs/index.csv`.

**Validation:** `python3 .claude/skills/liquers-project/scripts/validate_phase.py keyed-delegation-hand-off 5`

## Testing Plan

| When | Command |
|---|---|
| After steps 1-2 | `cargo check -p liquers-core` |
| After step 3 | `cargo test -p liquers-core --lib assets::` |
| After step 4 | `cargo test -p liquers-core --test manager_parametric` |
| After step 5 | `cargo test -p liquers-core --lib --tests` and `cargo test -p liquers-lib --lib --tests` |

Not run: the `liquers-web` wasm loops. They need a `cargo clean` between them and the native loop
(disk allowance, `CLAUDE.md`), and this change adds no wasm-specific behaviour — `liquers-web` uses
`ImmediateAssetManager`, whose path is covered natively by `keyed_delegation_immediate`. Recorded
in Phase 5 as untested-here rather than claimed.

## Rollback Plan

| Step | Rollback |
|---|---|
| 1 | Revert the guard. Delegation returns to failing with `dependency_cycle`; nothing else regresses, since no other caller reaches the same-node case. |
| 2 | Comment only. |
| 3-4 | Revert the tests to their error-expecting form (they are self-describing about which outcome they pin). |
| 6-7 | Documentation only. |

Steps 1 and 4 are the coupled pair: rolling back 1 without 4 leaves the suite red, which is the
intended signal rather than a hazard.

## Agent Assignment

Single-threaded, no delegation: the whole change is ~40 lines across two files in one crate, and
splitting it across agents would cost more context than it saves. The `rust-best-practices`
conventions applied throughout — no `unwrap`/`expect` outside tests, no `println!`, no `Error::new`,
no default match arm, typed error constructors, `#[tokio::test]` for async tests.

## Phase 5 Entry Criteria

Phase 5 starts when all of the following hold:

- Steps 1-6 are complete and `cargo test -p liquers-core --lib --tests` plus
  `cargo test -p liquers-lib --lib --tests` are green.
- `keyed_delegation_{default,immediate}` assert the hand-off outcome, not the cycle error.
- `DELEGATED-VALUE-REPERSISTED` is filed, so the omitted scope is recorded rather than forgotten.
- Any review comments on the change are answered or incorporated.
