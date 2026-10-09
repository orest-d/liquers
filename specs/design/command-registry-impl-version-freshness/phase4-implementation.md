# Phase 4: Implementation Plan

## Progress

- [x] Step 1: add `committed_registry_impl_versions_are_fresh` (T1 passes at HEAD) — 31cbbd4
- [x] Step 2: T2 and T3 by hand, reverted — no commit (outputs in the PR description)
- [x] Step 3: CLAUDE.md registry section — adc30ab
- [x] Step 4: close the issue, regenerate the index — 55e5ad5

**Finding during Step 2.** T2 as written does not fail: a `//` comment is not a token, so the
`#[command_version]` hash of the function's token stream ignores it. A code edit
(`let _t2_probe = 0;` in `command_metadata`) does fail the test, naming `/dep/command_metadata`.
The "comment edits" friction in Phase 2's risk table is therefore smaller than assumed: only
doc comments (`///`, which are attributes) and code changes require a regeneration.

## Steps


1. Add `committed_registry_impl_versions_are_fresh` (Phase 2). Proof: T1. Agent: sonnet tier;
   liquers-unittest.
2. T2 and T3 by hand, with the outputs recorded in the PR, then revert both.
3. Edit CLAUDE.md's registry section.
4. Close the issue with a resolution noting that the original drift was fixed by a regeneration
   and that detection now exists. Regenerate and check the index. Diff review.
