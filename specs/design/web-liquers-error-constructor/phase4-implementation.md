# Phase 4: Implementation Plan

1. Inspect the current signatures and callers in liquers-web/src/error.rs, liquers-web/tests/error_ERROR.rs, liquers-web/src/typescript.rs; stop if they differ from Phase 2. Proof: the focused Phase 3 test. Containment: revert only this source's files.
2. Implement Expose a two-argument wasm constructor validated by error_type_from_name; leave key/query Liquers-populated. Preserve existing ownership, async, serialization, and typed-error conventions. Proof: cargo test -p liquers-web --target wasm32-unknown-unknown --test error_ERROR.
3. Add the Phase 3 regression tests and any current contract documentation updates. Proof: focused tests plus documentation review.
4. Update the source issue resolution/status only after evidence exists; regenerate `specs/index.csv` with `python3 scripts/docs_index.py`, run `python3 scripts/docs_index.py --check`, format, and review the diff for unrelated edits.

## Final Review

The plan is intentionally implementation-free. It must be rechecked against current signatures before execution and rolled back as a single scoped change if validation fails.

