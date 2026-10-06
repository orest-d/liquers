# Phase 4: Implementation Plan

1. Add `committed_registry_impl_versions_are_fresh` (Phase 2). Proof: T1. Agent: sonnet tier;
   liquers-unittest.
2. T2 and T3 by hand, with the outputs recorded in the PR, then revert both.
3. Edit CLAUDE.md's registry section.
4. Close the issue with a resolution noting that the original drift was fixed by a regeneration
   and that detection now exists. Regenerate and check the index. Diff review.
