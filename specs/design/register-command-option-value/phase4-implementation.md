# Phase 4: Implementation Plan

Preconditions: questions 1–2 decided.

1. Add the supported list and the parse-time check in `registration.rs`. Proof: T1
   (`cargo test -p liquers-macro`). Agent: sonnet tier; knowledge: REGISTER_COMMAND_FSD.
2. Confirm no in-tree registration is rejected: `cargo test -p liquers-lib --lib --tests --no-run`
   and `bash scripts/check-build-matrix.sh`.
3. (If accepted) Add the `Option<String>`/`Option<bool>` impls and the `ArgumentType` mapping. Proof: T3.
4. T2 if trybuild is available. Otherwise document E1 in the FSD only.
5. Docs, issue resolution (record that `Option<Value>` support was not pursued). If wanted, file a
   feature `REGISTER-COMMAND-OPTION-VALUE-SUPPORT` (§4.8). Index. Diff review.
