# Phase 4: Implementation Plan

1. Inspect the current signatures and callers in specs/reference/STORE_SEMANTICS.md, liquers-core/src/store.rs, liquers-store/src/opendal_store.rs, liquers-web/src/store/local_storage.rs, liquers-core/tests/store_conformance_CONF.rs; stop if they differ from Phase 2. Proof: the focused Phase 3 test. Containment: revert only this source's files.
2. Implement Keep children populated, matching the trait default and seven implementations; document the cost and align AsyncOpenDALStore or explicitly scope its exception. Preserve existing ownership, async, serialization, and typed-error conventions. Proof: cargo test -p liquers-core --test store_conformance_CONF; cargo test -p liquers-store.
3. Add the Phase 3 regression tests and any current contract documentation updates. Proof: focused tests plus documentation review.
4. Update the source issue resolution/status only after evidence exists; regenerate `specs/index.csv` with `python3 scripts/docs_index.py`, run `python3 scripts/docs_index.py --check`, format, and review the diff for unrelated edits.

## Final Review

The plan is intentionally implementation-free. It must be rechecked against current signatures before execution and rolled back as a single scoped change if validation fails.

