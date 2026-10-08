# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — needs-decision (rule 5), and the chosen solution cannot be
  carried out as written: `Cargo.lock` is git-ignored and no `rust-version` is declared, so there is
  no lockfile to pin (compaction 2026-10-08)
- **Leading issue:** Whether the project pins sysinfo to its declared Rust 1.94 support window or raises its MSRV.
- **Explanation:** Pin the lockfile to the last sysinfo compatible with Rust 1.94; changing MSRV needs explicit maintainer approval.
- **Open questions:** **Proposed resolution:** Pin the lockfile to the last sysinfo compatible with Rust 1.94; changing MSRV needs explicit maintainer approval.
  - **Open design question (compaction 2026-10-08):** the proposed pin is not possible as written.
    `.gitignore` excludes `Cargo.lock`, and no `rust-version` or `rust-toolchain` file exists, so
    the repository declares no 1.94 window to stay inside. The cloud toolchain is now rustc 1.97.
    Recommended: declare `rust-version = "1.95"` (or whatever the current dependency set needs) in
    the workspace `Cargo.toml` and close the issue; pinning would first require committing
    `Cargo.lock`, which is a separate maintainer decision.

## Problem, Behaviour, and Scope

The source issue documents the observed failure and acceptance evidence. The desired behaviour is the source's expected behaviour, with compatibility preserved unless Phase 2 explicitly states otherwise. Affected systems are limited to the inspected files below; implementation, migrations, and test changes remain out of scope for this design-only work.

## Constraints and Documentation

Cargo resolution is reproducible through Cargo.lock; CI's stable toolchain remains a separate compatibility signal. Current documentation that names the affected contract must be updated with implementation, while historical design records remain frozen.

## Design Dependencies

None.

