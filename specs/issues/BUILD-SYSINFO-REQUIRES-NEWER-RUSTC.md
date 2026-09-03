---
id: BUILD-SYSINFO-REQUIRES-NEWER-RUSTC
kind: issue
title: liquers-lib test builds fail on rustc 1.94 because a transitive sysinfo requires 1.95
status: draft
priority: P2
complexity: S
area: [build]
design:
created: 2026-09-02
github:
---
## Problem

`cargo check -p liquers-lib --tests` fails before compiling anything:

```
error: rustc 1.94.1 is not supported by the following package:
  sysinfo@0.39.6 requires rustc 1.95
```

`sysinfo` is a transitive dependency of the `liquers-lib` test targets. Two rows of
`scripts/check-build-matrix.sh` fail for this reason alone:

- `cargo check -p liquers-lib --tests`
- `cargo check -p liquers-lib --no-default-features --features polars --tests`

The library targets build; only the test targets pull it in.

## Impact

**Local only.** `.github/workflows/build-matrix.yml` pins `dtolnay/rust-toolchain@stable`, which
resolves to a rustc new enough for `sysinfo`, so CI is unaffected — this was checked rather than
assumed, after an earlier version of this issue claimed the matrix "cannot go green" without
qualifying where.

The cost is on a contributor's machine: `CLAUDE.md` names the matrix as the check to run after
touching a `#[cfg(feature = …)]`, and on a 1.94 toolchain two of its rows are always red, so a
genuine regression in them is indistinguishable from this. A check that is reliably red locally and
green in CI is the worst of both — it teaches people to ignore the local run and to discover
breakage only after pushing.

## Expected behaviour

Either pin the dependency to a version supporting the pinned toolchain —

```
cargo update sysinfo --precise <last 1.94-compatible version>
```

— or raise the project's minimum supported rustc deliberately, and say so where the toolchain is
declared. Pinning is the smaller change; raising the minimum is the honest one if the workspace
already wants 1.95 features.

## Discovery

Found on 2026-09-02 while adding the `store-conformance` rows to the build matrix, at Phase 4 step
16 of `design/store-conformance-suite/`. Pre-existing and unrelated to that work: the two failing
rows fail identically without any of its changes. The CI/local split was established afterwards, by
reading the workflow's toolchain pin rather than inferring from the local failure.
