# Phase 4: Implementation Plan - `Context::set_title` and `Context::set_description`

0. **Decision gate.** Confirm the precedence answer (Phase 1, question 1). If "recipe wins" or
   the split is chosen, revise Phase 2 before step 1; the steps below implement "command wins".
1. **Methods.** `liquers-core/src/context.rs`: add `set_title` and `set_description` after
   `set_filename`, with the Phase 2 doc comments. Proof: `cargo check -p liquers-core`.
2. **Doc comment.** `liquers-core/src/assets.rs`, `AssetRef::set_description_fields`: name the
   new callers. No code change.
3. **Tests.** Add `liquers-core/tests/context_title_description.rs` (four tests) and the
   legacy-refusal unit test. Validate the test queries with
   `cargo run -p liquers-core --features cli --bin liquers-validate -- --command titled --command plain -- titled plain`.
   Proof: `cargo test -p liquers-core --test context_title_description` and
   `cargo test -p liquers-core --lib set_description_fields`.
4. **Docs.** `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`: add the two methods
   where metadata-writing context methods are listed (≈423, ≈469) with the precedence sentence;
   History row + `reviewed:`. `specs/guides/COMMAND_REGISTRATION_GUIDE.md`: a short example in the
   context section; History row + `reviewed:`. Close the issue with a resolution recording the
   precedence decision. Regenerate and check the docs index.
5. **Checks.** `cargo fmt`, `cargo test -p liquers-core --lib --tests`,
   `cargo test -p liquers-lib --lib --tests`, `cargo check -p liquers-core --target
   wasm32-unknown-unknown`. Diff review: no `liquers-py`/`liquers-web` edits, no status checks
   added to the setters, no `println!`.

## Final Review

The plan is safe to execute once the precedence decision is confirmed; under the recommended
answer, steps 1-5 are complete. Rollback: remove the two methods and the test file.
