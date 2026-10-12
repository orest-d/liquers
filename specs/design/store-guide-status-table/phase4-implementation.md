# Phase 4: Implementation Plan

"Build" chosen by the maintainer on 2026-10-10.

1. Refactor suites into report-returning functions. Proof: T2. Agent: sonnet tier; knowledge:
   STORE_IMPLEMENTATION_GUIDE §5–§9.
2. Add `status_table` with the notes map in both crates' conformance test files. Proof: T1, T1b.
3. Update guide §9 from the output, plus the regeneration instruction. Proof: T3.
4. Issue resolution, index. Diff review.

## Outcome (2026-10-11)

Steps 1–4 done. T1 and T1b print rows equal to the guide's (44 rules; every native and OpenDAL
store `CONFORMANT`); T2 passes unchanged. The generator needs `--features store-conformance`
for `liquers-core` too, since the whole test file is gated on it, so the guide's commands carry
the feature. The browser rows stay hand-merged, as Phase 2 allowed.
