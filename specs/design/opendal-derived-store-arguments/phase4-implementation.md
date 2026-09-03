# Phase 4: Implementation Plan

1. Inspect the current signatures and callers in liquers-store/src/store_factory.rs, liquers-store/src/opendal_store.rs, liquers-store/Cargo.toml; stop if they differ from Phase 2. Proof: the focused Phase 3 test. Containment: revert only this source's files.
2. Implement Derive enabled service config fields from Default plus Serialize behind per-service cfg gates, then add offline S3 construction tests. Preserve existing ownership, async, serialization, and typed-error conventions. Proof: cargo test -p liquers-store store_factory; cargo test -p liquers-store --features opendal.
3. Add the Phase 3 regression tests and any current contract documentation updates. Proof: focused tests plus documentation review.
4. Update the source issue resolution/status only after evidence exists; regenerate `specs/index.csv` with `python3 scripts/docs_index.py`, run `python3 scripts/docs_index.py --check`, format, and review the diff for unrelated edits.

## Final Review

The plan is intentionally implementation-free. It must be rechecked against current signatures before execution and rolled back as a single scoped change if validation fails.

