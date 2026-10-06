# Phase 4: Implementation Plan

Precondition: question 1 accepted. Question 2 answered as (a); for (b), stop and design separately.

1. **Switch both lazy sites** to `expire_with_reason` (Phase 2). Proof: `cargo check -p liquers-core`.
   Agent: haiku tier; rust-best-practices.
2. **Lock check.** Read `cascade_expire_dependents` and every callee it reaches for
   `key_mutation_lock`, and record the finding in the PR description. Containment: if a re-entrant
   acquisition is found, move the call after the unmapping block and release the lock first.
3. **Scenario tests.** Add E1–E3 to `tests/common/manager_scenarios.rs` and run them on both
   managers. Update T1. Proof:
   `cargo test -p liquers-core --lib` plus the scenario test target.
4. **Web.** `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`
   (after `cargo clean`, per CLAUDE.md), because the browser runs this manager.
5. **Docs.** `DEPENDENCIES_STATUS.md` (+History, `reviewed:`), the code comments, the issue,
   `docs_index.py` and `--check`.
6. **Diff review.**
