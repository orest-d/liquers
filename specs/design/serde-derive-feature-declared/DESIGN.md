---
id: SERDE-DERIVE-FEATURE-DECLARED
kind: design
title: Every crate that uses serde's derive macros declares where they come from
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: not-eligible
area: [build, lib/ui, axum]
issues: [WORKSPACE-SERDE-DERIVE-UNDECLARED]
created: 2026-10-20
---
# Every crate that uses serde's derive macros declares where they come from

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20. Phases 1-4 are written but not
approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

A crate that writes `#[derive(Serialize, Deserialize)]` must compile because of its own manifest,
not because some other package in the graph happens to turn on `serde/derive`.

### Problem Example

Checked at HEAD (18732dd) with `cargo tree -e features -i serde` (read-only):

- `liquers-lib` declares `serde = "1.0.229"` without `derive`, derives in 13 source files (e.g.
  `src/ui/action.rs`, `src/mem/layout.rs`), and lists `liquers-store` only as a
  **dev-dependency**. In `cargo tree -p liquers-lib --no-default-features`, the only enabler of
  `serde feature "derive"` is `typetag`. Removing or replacing `typetag` would break 13 files that
  never mention it.
- `liquers-axum` declares the same, derives in 6 files, and gets the feature from `liquers-store`,
  `opendal`, `jsonwebtoken` and `ciborium`.

The issue's other two crates are not exposed: `liquers-core` (`src/lib.rs:117`) and `liquers-py`
(`src/lib.rs:2`) have `#[macro_use] extern crate serde_derive;` with `serde_derive` declared. There
the derive macros come from the declared crate whether or not `serde/derive` is on. `liquers-py`
was missing from the issue's list for the same reason.

Two dead declarations sit next to the gap: `serde_derive` in `liquers-lib`, `liquers-axum` and
`liquers-store` (no `extern crate`, no `serde_derive::` path), and `serde` itself in
`liquers-store`, whose source names neither. Its manifest comment still says `config.rs` needs
derive, but that file moved to `liquers-core` (design `store-factories-in-core`).

Should be: `liquers-lib` and `liquers-axum` declare `serde = { version = "1.0.229", features =
["derive"] }`, and the dead declarations go.

### Scope and Acceptance Criteria

- **AC-1** liquers-lib declares derive
  WHEN `cargo tree -p liquers-lib --no-default-features -e features -i serde` runs
  THEN `liquers-lib` itself is listed under `serde feature "derive"`
- **AC-2** liquers-axum declares derive
  WHEN `cargo tree -p liquers-axum -e features -i serde` runs
  THEN `liquers-axum` itself is listed under `serde feature "derive"`
- **AC-3** No dead serde declarations
  WHEN the workspace manifests are read
  THEN `serde_derive` is declared only by crates with `extern crate serde_derive` (`liquers-core`,
  `liquers-py`), and `liquers-store` declares no `serde`
- **AC-4** Every configuration still builds
  WHEN `bash scripts/check-build-matrix.sh` and `cargo test -p liquers-lib --lib --tests` run
  THEN both pass

Non-goals: converting `liquers-core`/`liquers-py` from `#[macro_use] extern crate serde_derive` to
imported derives (about 25 files gain an import for no build-safety gain); a lint for this.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — the change spans three crates' manifests (`liquers-lib`,
  `liquers-axum`, `liquers-store`), so rule 6 (one crate) fails; it is otherwise a mechanical
  build-settings fix. P2, so it needs the user's go-ahead in any case.
- **Leading issue:** None
- **Explanation:** The repository answers every question: the derives and their sources are
  enumerable, and the matrix proves each configuration.
- **Open questions:** None. *Resolved implementation detail:* `liquers-core` and `liquers-py` stay
  as they are, each with a one-line manifest comment naming the `macro_use` mechanism.

### Scope Changes

Re-sized from **L** to **S**. Two crates need the declaration, not three. Three dead
declarations are removed and one stale comment is fixed.

### Design Dependencies

- `workspace-rustfmt-check` — overlaps: no Rust source changes here, so no conflict.
- `core-tracing-migration` — overlaps (also edits manifests of several crates); textual merge only.

### Consolidated Findings

The hazard is real for `liquers-lib` (one transitive enabler, `typetag`) and `liquers-axum`. It is
absent for `liquers-core` and `liquers-py`. A one-line declaration fixes each exposed crate. The
build matrix is the proof, because each configuration resolves features separately.

## Phase 2: Architecture

### Solution

Manifest-only change:

1. `liquers-lib/Cargo.toml`, `liquers-axum/Cargo.toml`: `serde = { version = "1.0.229", features =
   ["derive"] }`; delete `serde_derive = "1.0.229"`.
2. `liquers-store/Cargo.toml`: delete `serde` and `serde_derive` and the stale four-line comment.
3. `liquers-core/Cargo.toml`, `liquers-py/Cargo.toml`: a comment above `serde_derive` saying the
   derives come from `#[macro_use] extern crate serde_derive` in `src/lib.rs`.

Rejected: workspace-level `[workspace.dependencies] serde = { features = ["derive"] }` (a larger
manifest migration than this issue warrants; worth doing only with all shared versions).

### Changes

Five `Cargo.toml` files and `Cargo.lock` (dependency edges only, no new package). No source, no
command, no documentation outside this design and the issue.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Something in `liquers-store` needs `serde` after all | `STORE_CONFIGS` matrix rows, incl. wasm32 | high |
| A consumer relied on `liquers-store` enabling `serde/derive` | Only `liquers-axum` did; it declares it now | high |

Recovery: revert the manifests.

## Phase 3: Examples and Tests

### Examples

`cargo tree -p liquers-lib --no-default-features -e features -i serde | grep -A4 'feature "derive"'`
lists `liquers-lib` (today only `typetag`).

### Tests

No new test: the property is in the manifests and the dependency graph.

- the `cargo tree` command above — AC-1; the same with `-p liquers-axum` — AC-2
- `grep -n serde_derive */Cargo.toml` shows only `liquers-core` and `liquers-py`, and
  `grep -n '^serde' liquers-store/Cargo.toml` shows only `serde_json` and `serde_yaml` — AC-3
- `bash scripts/check-build-matrix.sh`; `cargo test -p liquers-lib --lib --tests` — AC-4

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-lib/Cargo.toml` — `derive` on, `serde_derive` removed — `cargo check -p liquers-lib --no-default-features` and the AC-1 `cargo tree`
- [ ] 2. `liquers-axum/Cargo.toml` — same — `cargo check -p liquers-axum` and the AC-2 `cargo tree`
- [ ] 3. `liquers-store/Cargo.toml` — remove `serde`, `serde_derive`, the stale comment — `cargo check -p liquers-store` (default and `--no-default-features --features async_store`)
- [ ] 4. `liquers-core/Cargo.toml`, `liquers-py/Cargo.toml` — explanatory comment — the AC-3 greps
- [ ] 5. `bash scripts/check-build-matrix.sh`, `cargo test -p liquers-lib --lib --tests` — AC-4
- [ ] 6. Close the issue with a resolution note (complexity S) — `python3 scripts/docs_index.py --check`

### Validation

The full matrix (every feature set resolves separately). Diff review: manifests and `Cargo.lock`
only.
