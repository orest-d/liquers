# Phase 4: Implementation Plan

Same PR as `memory-store-metadata-only-entry`. This design's step 1 goes before that design's
step 2.

1. Change `try_fast_track` (Phase 2). Proof: `cargo check -p liquers-core`.
2. Add T2 (file store) first. It reproduces today's failure without the fix, so confirm it fails on
   a stash and passes with step 1. Then add T1 and T4. Proof: `cargo test -p liquers-core --lib fast_track`.
   Agent: sonnet tier; liquers-unittest.
3. Check acceptance 3 (no recipe): add an assertion to T2's variant without a recipe that the error
   type is `KeyNotFound`.
4. `cargo test -p liquers-core --lib`, `cargo test -p liquers-lib --lib --tests`.
5. `ASSETS.md` sentence, issue resolution, index. Diff review.
