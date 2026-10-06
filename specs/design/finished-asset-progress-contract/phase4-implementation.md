# Phase 4: Implementation Plan

Precondition: the maintainer has decided the contract (Phase 1). The steps below implement the
recommended one and note the alternative.

1. **Reorder finalization.** `liquers-core/src/assets.rs`: in `run_with_future` and
   `run_inline_with_future`, move the `finalize_primary_progress` call after the service loop
   completes (Phase 2). Proof: `cargo test -p liquers-core --lib` is green with the current
   clear-everything body. This step alone makes the outcome deterministic. Containment: this step
   is self-contained and can merge alone.
2. **Status-aware finalization.** Change the signature to
   `finalize_primary_progress(&self, succeeded: bool)` (private method; two callers). Implement
   the cancelled/succeeded/else rule, and call `save_metadata_to_store()` afterwards. Check whether
   the harness can end `Partial` (search `Status::Partial` assignments). Proof: T1–T3. Agent:
   sonnet tier; skills rust-best-practices, liquers-unittest; knowledge: the service loop
   (`process_service_messages`), `finish_run_with_result`.
3. **Tests.** Add T1–T4 (Phase 3). Run each stress test locally with `--test-threads=1` and with
   the default. Proof: `cargo test -p liquers-core --lib finished_` and
   `cargo test -p liquers-core --lib interpreter`.
4. **Downstream check.** `cargo test -p liquers-lib --lib --tests`. UI code that hides a bar on
   `is_off()` would now show a full bar. Grep `primary_progress` in `liquers-lib` and
   `liquers-axum` and adjust any client that treats "done" as "in progress".
5. **Documentation.** `specs/reference/ASSETS.md`: "Progress after completion", History,
   `reviewed:`. Close the issue. Regenerate and check the docs index.
6. **Final diff review.** Confirm that the post-finish drop policy is unchanged and that no
   `println!` was added.
