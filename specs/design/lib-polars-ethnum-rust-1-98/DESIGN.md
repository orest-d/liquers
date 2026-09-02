---
id: LIB-POLARS-ETHNUM-RUST-1-98
kind: design
title: Polars dependency compatibility with Rust 1.98
status: complete
readiness: ready
area: [lib/polars, build]
issues: [LIB-POLARS-ETHNUM-RUST-1-98-BROKEN]
gh_pr: []
created: 2026-09-01
superseded_by:
---
# lib-polars-ethnum-rust-1-98 Design Tracking

Simplified autonomous design stopped after Phase 2 because no released upstream fix or approved
toolchain/dependency policy currently supports a safe implementation plan.

## Phase Status

- [x] [Phase 1: High-Level Design](./phase1-high-level-design.md)
- [x] [Phase 2: Solution and Architecture](./phase2-architecture.md)
- [ ] Phase 3: Examples and Tests - intentionally not produced
- [ ] Phase 4: Implementation Plan - intentionally not produced

## Resolution Update, 2026-09-02

The Phase 2 blocker is resolved by the released `ethnum 1.5.3`: it removes the
Rust 1.98 incompatibility in `ethnum 1.5.2`. The current lockfile resolves
that release for Polars 0.55.2, so no Rust toolchain pin, git patch, or local
fork is required. The two failing Polars build rows and the complete build
matrix pass on Rust 1.98.0. The previously omitted planning phases are not
needed because the selected solution is a compatible released dependency
resolution rather than a Liquers API or architecture change.
