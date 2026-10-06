# Phase 4: Implementation Plan

1. Remove the field and its uses (Phase 2 table, without the registry). Proof:
   `cargo check -p liquers-core -p liquers-lib -p liquers-py --all-features`. Agent: haiku tier.
2. T1, T2. Proof: `cargo test -p liquers-core --lib command_metadata`.
3. Regenerate the registry and add the CHANGELOG line. Proof: T3.
4. `bash scripts/check-build-matrix.sh`.
5. Docs, issue resolution quoting the decision, index. Diff review: the registry diff is only
   `cache` deletions and the CHANGELOG line.
