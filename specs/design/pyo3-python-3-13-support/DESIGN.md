---
id: PYO3-PYTHON-3-13-SUPPORT
kind: design
title: liquers-py builds against the current Python
form: compact
gh_pr: [110]
readiness: ready
autofix: eligible
area: [py, build]
issues: [PY-PYO3-REJECTS-PYTHON-3-13]
created: 2026-10-08
---
# liquers-py builds against the current Python

Produced under [`guides/autonomous_bulk_design.md`](../../guides/autonomous_bulk_design.md) by the
2026-10-08 backlog compaction: Phases 1-4, reviewed without phase approval. Not an approval and not
an implementation.

## Phase 1: High-Level Design

### Purpose

`cargo check -p liquers-py --lib` should not fail in a build script on a host whose `python3` is
3.13, or it should fail with a pointer to the documented workaround.

### Problem Example

On the cloud dev environment (`python3 --version` → 3.13.16, 2026-10-08), with `pyo3 = "0.21.2"`
in `liquers-py/Cargo.toml`, `cargo check -p liquers-py --lib` fails in `pyo3-ffi`'s build script
("the configured Python interpreter version (3.13) is newer than PyO3's maximum supported version
(3.12)"). `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 cargo check -p liquers-py --lib` builds. Nothing
in the repository names the variable.

### Scope and Acceptance Criteria

- **AC-1** A current Python builds
  - WHEN `cargo check -p liquers-py --lib` runs with Python 3.13 and no override
  - THEN it succeeds (option A), or the failure is the one `liquers-py/README.md` documents with
    its workaround (option B)
- **AC-2** The supported range is written down
  - WHEN a contributor reads `liquers-py/README.md` or `pyproject.toml`
  - THEN the supported Python range is stated, and matches what the pinned PyO3 supports

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — documentation and build-metadata fix (`liquers-py/pyproject.toml`,
  `liquers-py/README.md`, `CLAUDE.md`); no Rust change.
- **Leading issue:** None
- **Explanation:** Decided (maintainer, 2026-10-08, backlog compaction D4): option B now, and the
  PyO3 upgrade (option A) as its own `M` feature, filed when this design is implemented.
- **Open questions:**
  1. **Resolved — option B** (maintainer, 2026-10-08). Option A, kept for the follow-up: raise
     `pyo3` to the oldest release supporting 3.13 (0.22 or later) and migrate from the GIL-ref API.

### Design Dependencies

- `overlaps` `CORE-VALUE-INTERFACE-CAPABILITY-SPLIT` and `PY-MODULES-NOT-DECLARED-IN-LIB`
  (weak: same crate, independent changes). Option A should precede neither.

## Phase 2: Architecture

### Solution

Specified for the chosen option B; option A is outlined for the follow-up feature.

- **B:** `liquers-py/pyproject.toml` `requires-python = ">=3.8,<3.13"` (the existing lower bound is
  kept; only the upper bound PyO3 0.21 imposes is added); a "Building" section in
  `liquers-py/README.md` (the file did not exist; created) stating the range, the error and the override; one line in `CLAUDE.md`
  under "Building and testing".
- **A (follow-up feature):** `pyo3` bump in `liquers-py/Cargo.toml`; mechanical `&PyAny` → `Bound<'py,
  PyAny>` migration in `liquers-py/src/`; re-run `liquers-py/tests/`. Re-size to `M` and redesign.

### Changes

Option B touches no Rust. No commands, no registry change.

### Risks

B: none to the build; the risk is that 3.13 users keep relying on the ABI forward-compatibility
override. A: binding behaviour changes that the Python tests do not cover.

## Phase 3: Examples and Tests

### Examples

The Problem Example, with and without the override.

### Tests

- `pyo3-range-documented` (manual check, no test harness builds `liquers-py`): run
  `cargo check -p liquers-py --lib` with and without `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` and
  confirm the README states the observed outcome. Proves AC-1 (option B), AC-2. Conceptual because
  the outcome depends on the host's Python.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-py/pyproject.toml` — `requires-python` — `grep requires-python liquers-py/pyproject.toml`
- [x] 2. `liquers-py/README.md`, `CLAUDE.md` — supported range and override — manual check above
- [x] 3. File the PyO3 upgrade (option A) as an `M` feature; issue resolution and `status: closed`; `python3
  scripts/docs_index.py --check`

### Validation

The manual check in Phase 3. Rollback: revert the commit.
