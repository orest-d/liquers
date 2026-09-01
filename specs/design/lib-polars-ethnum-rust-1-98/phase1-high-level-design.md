# Phase 1: Polars Dependency Compatibility with Rust 1.98

## Design Readiness

- **Readiness:** phase2-blocked
- **Leading issue:** **Blocking - dependency/toolchain policy:** `ethnum 1.5.2` has no released
  Rust 1.98 fix, and the repository does not say whether to pin Rust <=1.96, carry an upstream fork,
  or replace/upgrade Polars.
- **Explanation:** The failure is reproducible and its dependency chain is known, but plausible
  remedies create incompatible support and supply-chain contracts.
- **Open questions:** **Blocking - supported remedy:** A Rust pin makes 1.98 unsupported; a git patch
  adds an unreleased dependency; a Polars change may alter APIs and data-format behaviour.

## Problem and Evidence

On `rustc 1.98.0`, `cargo check -p liquers-lib --no-default-features --features polars --tests`
fails in `ethnum 1.5.2::tfie` because it transmutes zero-sized `()` into the now one-byte
`TryFromIntError`. `cargo tree` shows `ethnum` is unconditional in `polars-arrow 0.53.0` and also
used by `polars-parquet`; removing one Liquers feature does not remove the dependency.

## Expected Behaviour and Acceptance Criteria

The supported toolchain and all Polars-enabled matrix rows compile without patching Cargo registry
sources. The chosen remedy is reproducible from committed manifests/lockfile, preserves required
Polars CSV/Parquet behaviour, and has an explicit update/removal path.

## Scope and Non-Goals

This design identifies the compatibility boundary; it does not vendor an unreviewed fork, rewrite
Polars commands, or silently lower the supported Rust version. Users affected are contributors and
Polars consumers. There is no Liquers runtime security exposure, but dependency provenance matters.

## Design Dependencies

None.

## Documentation Assessment

Once decided, update the build/toolchain guidance and dependency rationale in
`liquers-lib/Cargo.toml` or the workspace manifest. No reference behaviour document changes unless
a Polars upgrade changes supported formats.
