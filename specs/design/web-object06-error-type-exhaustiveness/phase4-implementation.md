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
