# Phase 4: Implementation Plan

1. In `liquers-core/src/error.rs`, change `Error::with_key` from assigning `query` to assigning
   `key`; add field-separation, composition, serde, and dependency-constructor tests. This is the
   behavioural change and can be rolled back as one assignment plus tests.
2. Run a focused caller test in `recipes.rs` or `plan.rs` if existing coverage exposes the enriched
   error; do not broaden scope solely to manufacture an integration harness.
3. Check error/reference documents for an exhaustive context-field claim and update only if needed;
   update issue/design lifecycle records during implementation.
4. Run `cargo fmt --all -- --check`, focused and full core tests, clippy for core, and docs-index
   validation. Review the diff for accidental changes to dependency constructors, historical data
   migration, debug code, and unrelated errors.
