---
id: CORE-WASM-WARNING-FREE
kind: design
title: A wasm32 build of liquers-core is warning-free
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/assets, build]
issues: [CORE-WASM-BUILD-EMITS-UNUSED-WARNINGS]
created: 2026-10-20
---
# A wasm32 build of liquers-core is warning-free

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20. Phases 1-4 are written but not
approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

The wasm32 rows of `scripts/check-build-matrix.sh` should print no warning for `liquers-core`, so
that a new one is seen. Today five warnings come from items whose only users are native-only.

### Problem Example

Read at HEAD (18732dd); the wasm32 target is not installed in the design environment, so the
evidence is the code, matching the issue's 2026-10-17 build output:

- `liquers-core/src/assets.rs:313-321` imports `VecDeque`, `AtomicUsize`, `scc` and `Notify`
  unconditionally. Their only uses are in `#[cfg(not(target_arch = "wasm32"))]` items:
  `DefaultAssetManager` (`scc::HashMap` fields, `:7773`) and the job queue (`running_count`,
  `notify`, `local_deps`, `:9460-9480`, `:9594`). `MetadataSaver` uses `tokio::sync::Notify` fully
  qualified, not the import.
- `MetadataSaver::interval` (`assets.rs:438`) is read only in `save_task`, which is native-only
  (`:530`); on wasm32 the saver writes inline and the field is never read.
- `FileLockGuard` (`liquers-core/src/store.rs:1459-1466`) has no `#[cfg]`, but its only
  constructor, `AsyncFileStore::acquire_lock`, sits in the native-only `impl AsyncFileStore`
  (`:1322`).

`cargo check -p liquers-core --target wasm32-unknown-unknown` prints these five warnings today; it
should print none.

### Scope and Acceptance Criteria

- **AC-1** wasm32 build warning-free
  WHEN `cargo check -p liquers-core --target wasm32-unknown-unknown` runs (and the matrix's other
  core wasm32 rows, if any are added)
  THEN it prints no warning attributed to `liquers-core`
- **AC-2** Native unchanged
  WHEN `cargo check -p liquers-core` and `cargo test -p liquers-core --lib` run natively
  THEN they print no new warning and every test passes, with no behaviour change

Non-goals: warnings of other crates on wasm32 (`liquers-lib` is `lib-build-warning-cleanup`);
`-D warnings` in the matrix; removing tokio from core (`CORE-TOKIO-REMOVAL`).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — `#[cfg]` gating of private imports and a private struct in one
  crate; no `pub` item, signature or behaviour changes.
- **Leading issue:** None
- **Explanation:** Every warning comes from an item whose users are already gated; gating it the
  same way is the plainly intended state.
- **Open questions:** None. *Resolved implementation detail:* `MetadataSaver::interval` keeps the
  field on both targets with `#[cfg_attr(target_arch = "wasm32", allow(dead_code))]`, because
  `MetadataSaver::new(interval)` is `pub` and gating the field would leave the parameter unused on
  wasm32 instead.

### Design Dependencies

- `lib-build-warning-cleanup` — overlaps (same kind of work, other crate); independent.
- `workspace-rustfmt-check` — overlaps: if its bulk reformat lands first, this diff rebases
  trivially; no order required.
- `CORE-TOKIO-REMOVAL` (issue, no design) — overlaps: it would rewrite the same import block; this
  fix does not constrain it.

### Consolidated Findings

The fix is three small `#[cfg]` changes. The only risk is an import that some other wasm32 code
path does use; the wasm32 check is the proof, and it needs `rustup target add
wasm32-unknown-unknown`. The issue's list is the 2026-10-17 output: the implementer fixes every
`liquers-core` wasm32 warning the check prints at the time, not only these five.

## Phase 2: Architecture

### Solution

Gate each item like its users:

1. `assets.rs` imports: split `VecDeque` out of the `std::collections` list, `AtomicUsize` out of
   the `std::sync::atomic` list, and `Notify` out of the `tokio::sync` list, each re-imported under
   `#[cfg(not(target_arch = "wasm32"))]`; put the same attribute on `use scc;`.
2. `MetadataSaver::interval`: `#[cfg_attr(target_arch = "wasm32", allow(dead_code))]` with a
   one-line comment (read only by the native debounce).
3. `store.rs` `FileLockGuard` and its `impl Drop`: `#[cfg(not(target_arch = "wasm32"))]`.

Rejected: `#![cfg_attr(target_arch = "wasm32", allow(unused))]` at crate level (hides future real
warnings, the opposite of the goal); `#[allow(unused_imports)]` on the import lines (same).

### Changes

`liquers-core/src/assets.rs` (import block `:313-321`, field `MetadataSaver::interval`),
`liquers-core/src/store.rs` (`FileLockGuard`). No signature, no new item, no command, no
documentation change.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A gated import is used by ungated code | Native and wasm32 checks both fail loudly | high |
| Test module relies on the parent's `AtomicUsize` | The test module imports its own (`assets.rs:10664`); native `cargo test` proves it | high |
| More wasm32 warnings exist than the issue lists | Plan step 4 fixes whatever the check prints | medium |

Recovery: revert the commit; nothing persisted or observable changes.

## Phase 3: Examples and Tests

### Examples

`cargo check -p liquers-core --target wasm32-unknown-unknown 2>&1 | grep -c '^warning'` prints `0`
(today `5` plus a summary line).

### Tests

No new test: the property is a build property.

- `cargo check -p liquers-core --target wasm32-unknown-unknown` with zero warnings — AC-1
- `cargo check -p liquers-core` with no new warning, and `cargo test -p liquers-core --lib` passes —
  AC-2

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/assets.rs` import block — gate `VecDeque`, `AtomicUsize`, `scc`, `Notify` — `cargo check -p liquers-core`
- [ ] 2. `assets.rs` `MetadataSaver::interval` — `cfg_attr(..., allow(dead_code))` — `cargo check -p liquers-core`
- [ ] 3. `liquers-core/src/store.rs` `FileLockGuard` + `impl Drop` — `#[cfg(not(target_arch = "wasm32"))]` — `cargo check -p liquers-core`
- [ ] 4. Run `cargo check -p liquers-core --target wasm32-unknown-unknown` (after `rustup target add wasm32-unknown-unknown`) and gate any further `liquers-core` warning the same way — zero warnings (AC-1)
- [ ] 5. `cargo test -p liquers-core --lib` and the core rows of `bash scripts/check-build-matrix.sh` — AC-2
- [ ] 6. Close `CORE-WASM-BUILD-EMITS-UNUSED-WARNINGS` with a resolution note — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib`; the `CORE_CONFIGS` rows of the build matrix. Run `cargo fmt -p
liquers-core` only on the touched lines (see `workspace-rustfmt-check`). Diff review: only `#[cfg]`
attributes and import splits.
