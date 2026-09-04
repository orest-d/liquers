# Phase 1: High-Level Design

## Purpose

Correct three independent, small current-documentation defects without changing Liquers behaviour:
the store-configuration reference incorrectly treats `AsyncFileStore` as future work, three public
rustdoc comments link to items that public rustdoc cannot resolve, and `CLAUDE.md` carries a stale
feature-matrix count.

## Scope and Acceptance Criteria

- `STORE_CONFIG_FSD.md` identifies the built-in filesystem implementation as `AsyncFileStore` and
  no longer says it must be implemented.
- `cargo doc -p liquers-core --no-deps` produces none of the three reported intra-doc-link warnings.
- `CLAUDE.md` does not duplicate the matrix configuration count; readers are directed to the
  script's computed result.
- The three source issues are closed only after the changes and checks succeed.

## Boundaries

No public API, runtime, configuration, or command behaviour changes. Historical documents stay
unchanged. The Polars namespace and command-payload documentation issues are independent and need
their own broader work.

## Documentation Assessment

Update the existing reference and `CLAUDE.md` only; no new guide or reference is needed. The Rust
source-doc changes are current public API documentation maintenance.

