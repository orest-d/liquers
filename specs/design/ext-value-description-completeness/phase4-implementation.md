# Phase 4: Implementation Plan

1. Write `sampled` + `samples` and rewrite T1. Proof: `cargo test -p liquers-lib --test value_type_system`.
   Agent: sonnet tier; liquers-unittest; knowledge: `liquers-lib/src/value/mod.rs` variants,
   `tests/record_typeinfo.rs`.
2. Add T2.
3. T3 by hand (revert afterwards), then T4 with the per-feature commands and
   `bash scripts/check-build-matrix.sh`.
4. Guide sentence, issue resolution, index. Diff review.
