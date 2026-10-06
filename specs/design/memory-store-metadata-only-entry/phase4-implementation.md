# Phase 4: Implementation Plan

`metadata-only-entry-reload` steps 1–2 are implemented (2026-10-06), so this design can be
implemented alone. Add T7 first, and confirm it returns `""` before step 2.

1. **Rule first.** Add `sidecar05` and run the suite before changing the store. Record which
   stores fail (expected: memory, possibly JS stub/OpenDAL). Proof: the failing list. Agent:
   sonnet tier; skills rust-best-practices; knowledge: STORE_SEMANTICS §2, existing sidecar rules.
2. **Memory store** `Option` data (Phase 2). Update T1–T3 and the old assertion. Proof:
   `cargo test -p liquers-core --lib store` and the conformance suite (memory now passes).
3. **Other failing stores.** Fix each trivially (same pattern), or list it as allowed failure +
   file an issue (§4.8).
4. **Part G.** If every store passes `sidecar05`, delete `no_bytes_by_design` and rework T5/T6.
   Otherwise keep it with a comment naming the failing store's issue. Proof:
   `cargo test -p liquers-core --lib`.
5. **Downstream.** `cargo test -p liquers-lib --lib --tests`; browser suites (after `cargo clean`).
6. **Docs.** STORE_SEMANTICS, guide §9 table, issue resolution, index. Diff review.
