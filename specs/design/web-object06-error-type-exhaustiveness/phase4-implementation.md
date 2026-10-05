# Phase 4: Implementation Plan - Compiler-Checked `ErrorType` List for OBJECT06

1. **Re-verify.** Count `ErrorType` variants in `liquers-core/src/error.rs`; if it is no longer 24,
   use the current set in step 2.
2. **Macro list.** `liquers-web/tests/objects_OBJECT.rs`: replace the literal `ALL_ERROR_TYPES`
   and its doc comment with the `error_types!` macro and invocation from Phase 2 (all variants, in
   enum order).
3. **Remove the count.** Delete the `assert_eq!(ALL_ERROR_TYPES.len(), 22, …)` line from
   `object06_every_enum_variant_roundtrips`.
4. **Prove.** `cargo clean`, then the Phase 3 run command; then the manual compile-failure check
   (remove one entry, `--no-run` fails, restore). If the wasm toolchain or Node runner is
   unavailable, report that rather than claiming the test passes.
5. **Records.** Close `specs/issues/WEB-OBJECT06-EXPECTS-A-STALE-ERROR-TYPE-COUNT.md` with a
   resolution noting the missing `KeyNotAbsolute`; regenerate and check the docs index.
6. **Review.** Diff limited to the test file and records; `cargo fmt -p liquers-web`; no library
   change.

## Final Review

Consistent across phases; test-only. Rollback restores the literal list.

## Post-Phase-4 Review (2026-10-05)

- **Problem still valid:** yes. The test still asserts `len() == 22`
  (`liquers-web/tests/objects_OBJECT.rs` ≈133), and `ErrorType` has 24 variants, including
  `KeyNotAbsolute`.
- **Solution correct:** yes. The macro gives a compile-time check with no core dependency.
- **Unnecessary:** none. `strum` was correctly rejected.
- **Detail / tests:** sufficient. The guarantee fires only in the wasm loop, which is where the
  test runs.
- **Interactions:** none.
- **Verdict:** ready.

**Resolution (2026-10-05):** the findings above are incorporated into Phases 1-4.
`phase5-documentation.md` holds the documentation plan; where a Phase 4 step names documentation
work, that plan is the authoritative list.
