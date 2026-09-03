# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** Whether the project pins sysinfo to its declared Rust 1.94 support window or raises its MSRV.
- **Explanation:** Pin the lockfile to the last sysinfo compatible with Rust 1.94; changing MSRV needs explicit maintainer approval.
- **Open questions:** **Proposed resolution:** Pin the lockfile to the last sysinfo compatible with Rust 1.94; changing MSRV needs explicit maintainer approval.

## Problem, Behaviour, and Scope

The source issue documents the observed failure and acceptance evidence. The desired behaviour is the source's expected behaviour, with compatibility preserved unless Phase 2 explicitly states otherwise. Affected systems are limited to the inspected files below; implementation, migrations, and test changes remain out of scope for this design-only work.

## Constraints and Documentation

Cargo resolution is reproducible through Cargo.lock; CI's stable toolchain remains a separate compatibility signal. Current documentation that names the affected contract must be updated with implementation, while historical design records remain frozen.

## Design Dependencies

None.

