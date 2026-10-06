# Phase 4: Implementation Plan

1. Trait hook + `CombinedValue` delegation (Phase 2 §1–2). Proof: `cargo check -p liquers-lib`.
2. `ExtValue` implementation + helper (Phase 2 §3–4). Proof: T1–T3,
   `cargo test -p liquers-lib --test record_scalar_reading`. Agent: haiku tier; rust-best-practices.
3. `bash scripts/check-build-matrix.sh` (a match over `ExtValue` changed).
4. `cargo test -p liquers-lib --lib --tests`.
5. Reference sentence, issue resolution, index. Diff review.
