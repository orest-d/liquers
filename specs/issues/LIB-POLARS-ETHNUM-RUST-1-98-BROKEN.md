---
id: LIB-POLARS-ETHNUM-RUST-1-98-BROKEN
kind: issue
title: liquers-lib polars builds fail in ethnum on Rust 1.98
status: draft
priority: P2
complexity: S
area: [lib/polars, build]
design: lib-polars-ethnum-rust-1-98
created: 2026-08-30
github:
---
## Problem

On Rust 1.98.0, `bash scripts/check-build-matrix.sh` fails in the `liquers-lib` rows that enable
Polars:

```text
error[E0512]: cannot transmute between types of different sizes, or dependently-sized types
  --> ethnum-1.5.2/src/error.rs:16:14
```

Observed failing rows:

- `cargo check -p liquers-lib --no-default-features --features polars --tests`
- `cargo check -p liquers-lib --tests`

## Impact

The broad build matrix is red before it can be used as a clean final workspace check. This is
independent of `CORE-NO-DEFAULT-FEATURES-BROKEN`: the new `liquers-core --no-default-features` row
passes, and the failure is inside a third-party transitive dependency of Polars.

## Expected Behaviour

Polars-enabled `liquers-lib` build rows should compile on the supported Rust toolchain, or the
workspace should pin/update/gate dependencies so the supported build matrix is truthful.

## Discovery

Found on 2026-08-30 while validating `CORE-NO-DEFAULT-FEATURES-BROKEN` with
`bash scripts/check-build-matrix.sh` under `rustc 1.98.0`.
