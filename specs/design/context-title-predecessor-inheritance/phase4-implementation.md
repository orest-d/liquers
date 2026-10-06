# Phase 4: Implementation Plan

Precondition: "inherit" chosen. For "keep", close the issue with the decision and stop.

1. Flags + `command_set_fields` (Phase 2). Proof: `cargo check -p liquers-core`.
2. `Context::inherit_description_fields` and the `Step::Evaluate` change. Proof: T1–T4
   (`cargo test -p liquers-core --test context_title_description`).
3. `cargo test -p liquers-core --lib --tests` (predecessor-cut tests in `interpreter.rs`).
4. DOC_04 paragraph (History, `reviewed:`), issue resolution, index. Diff review.
