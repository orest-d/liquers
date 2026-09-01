# Phase 1: Build Matrix CI

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The implemented workflow directly runs the existing matrix on relevant pushes
  and pull requests; the source issue records the selected tradeoff and completion.
- **Open questions:** None

## Problem, Behaviour, and Acceptance

The repository had an 11-row feature/target script but CI never invoked it. Relevant changes must
trigger a job that installs wasm and native prerequisites, caches Rust builds, and runs the script
unchanged. A failing row must fail CI; unrelated documentation-only changes need not run it.

## Scope and Compatibility

Scope is `.github/workflows/build-matrix.yml` and trigger paths. No crate API, runtime data, or
security boundary changes. CI minutes and cold-build duration are operational risks; splitting the
job remains a future optimization.

## Design Dependencies

- `overlaps` `core-no-default-features`: that completed design added a protected matrix row; this
  workflow executes it but was implemented independently.

## Documentation Assessment

The workflow and `scripts/check-build-matrix.sh` comments own the operational contract. No runtime
reference document changes were required.

## Consolidated Findings

Reuse the canonical script rather than duplicating rows in YAML. Install
`wasm32-unknown-unknown` and `libssl-dev`, cache Cargo work, and path-filter crate, Cargo, script,
workflow, and `.cargo` changes. Validate YAML, trigger coverage, and one complete script run.
