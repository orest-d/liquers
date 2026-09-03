# Phase 4: Implementation Plan

1. Inspect the current signatures and callers in Cargo.lock, Cargo.toml, scripts/check-build-matrix.sh, .github/workflows/build-matrix.yml; stop if they differ from Phase 2. Proof: the focused Phase 3 test. Containment: revert only this source's files.
2. Implement Pin the lockfile to the last sysinfo compatible with Rust 1.94; changing MSRV needs explicit maintainer approval. Preserve existing ownership, async, serialization, and typed-error conventions. Proof: cargo check -p liquers-lib --tests; cargo check -p liquers-lib --no-default-features --features polars --tests.
3. Add the Phase 3 regression tests and any current contract documentation updates. Proof: focused tests plus documentation review.
4. Update the source issue resolution/status only after evidence exists; regenerate `specs/index.csv` with `python3 scripts/docs_index.py`, run `python3 scripts/docs_index.py --check`, format, and review the diff for unrelated edits.

## Final Review

The plan is intentionally implementation-free. It must be rechecked against current signatures before execution and rolled back as a single scoped change if validation fails.

