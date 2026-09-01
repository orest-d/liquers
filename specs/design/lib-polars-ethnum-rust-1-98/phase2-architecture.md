# Phase 2: Solution and Architecture

## Current Architecture and Reproduction

`liquers-lib/Cargo.toml` enables `polars 0.53.0` with `lazy`, `temporal`, `csv`, and `parquet`.
`polars-arrow 0.53.0` requires `ethnum ^1.3.2`, resolved to 1.5.2. The local upstream source uses an
unsafe layout transmute to synthesize `TryFromIntError`; Rust 1.98 rejects it. The focused check
reproduced E0512 on 2026-09-01.

## Candidate Solutions

1. Prefer a released `ethnum` or Polars update that removes the transmute, constrained in
   `Cargo.toml`/`Cargo.lock` and verified through the full Polars suite.
2. Temporarily pin the repository toolchain to Rust <=1.96 only if maintainers choose that support
   policy; this does not satisfy the issue's stated Rust 1.98 expectation.
3. Use `[patch.crates-io]` to a reviewed upstream commit only with an explicit provenance,
   maintenance, and removal decision. Do not patch cached registry source or invent a local fork.

No candidate is selected: upstream evidence reports no released fix, and the repository has no
toolchain file or policy authorizing the latter two tradeoffs.

## Feasibility and Compatibility

A released dependency update is mechanically feasible but its exact version/API cannot be named
truthfully yet. A git patch is technically feasible but creates ongoing external-source ownership.
A Rust pin is easy but contradicts the observed supported environment. These alternatives produce
incompatible CI and consumer outcomes, so examples and an executable plan would be speculative.

## Risk Assessment

| Concern | Assessment and control |
|---|---|
| Files/crates | Workspace or `liquers-lib` manifest, lockfile, CI/toolchain guidance. |
| Existing tests | All Polars value, command, CSV, and Parquet tests may be affected by an upgrade. |
| Required validation | Both failing matrix rows, full `liquers-lib` tests, matrix workflow, dependency tree. |
| Compatibility/data | Polars upgrade can change APIs or Parquet behaviour; Rust pin changes support policy. |
| Security/supply chain | Unreleased git patches require provenance and removal ownership. |
| Recovery | Restore manifest/lockfile together; a toolchain pin is independently reversible. |
| Certainty | High on cause, low on a supportable released remedy. |

## Continuation Blocker

Choose the dependency/toolchain policy after a compatible upstream release or maintainer decision.
Example: pinning Rust 1.96 makes CI green while a downstream Rust 1.98 build still fails; carrying a
git fork makes 1.98 green but adds an unreleased source dependency. Those are not equivalent
acceptance contracts, so Phases 3 and 4 are intentionally absent.
