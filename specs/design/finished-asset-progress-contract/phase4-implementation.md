# Phase 4: Implementation Plan

1. **Reorder finalization** in `run_with_future` and `run_inline_with_future` (Phase 2). Proof:
   `cargo test -p liquers-core --lib` is green with today's clear-everything body. This step alone
   makes the outcome deterministic and can merge alone.
2. **Contract.** Implement `finished_progress` and the new `finalize_primary_progress` body
   (persisting via `save_metadata_to_store`). Proof: T1–T3. Agent: sonnet tier;
   rust-best-practices, liquers-unittest; knowledge: `process_service_messages`,
   `finish_run_with_result`.
3. **Tests.** E1–E4, T4. Run the stress tests with the default and with `--test-threads=1`.
   Proof: `cargo test -p liquers-core --lib finished_` and `--lib interpreter`.
4. **Downstream.** `cargo test -p liquers-lib --lib --tests`. Grep `primary_progress` in
   `liquers-lib` and `liquers-axum` for clients that treat "done" as in progress.
5. **Docs.** `ASSETS.md` paragraph (History, `reviewed:`). Close the issue with the decision quoted.
   Regenerate and check the index. Diff review.
