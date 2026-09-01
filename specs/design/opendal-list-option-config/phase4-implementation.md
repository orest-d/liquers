# Phase 4: Implementation Plan

1. In `liquers-core/src/store_config.rs`, extract a small fallible scalar renderer, implement
   top-level null omission and safe comma-joined arrays, and include the option key in typed errors.
   Add all converter and JSON/YAML tests from Phase 3.
2. In `liquers-store/src/store_factory.rs`, add the narrow factory/pair integration regression if
   it can avoid external service I/O; production call signatures remain unchanged.
3. Update `specs/reference/STORE_CONFIG_FSD.md` with list encoding, top-level null, ambiguity, and
   error rules plus its History row. Update source issue/design lifecycle records in implementation.
4. Run formatting, focused core tests, full store tests in applicable feature sets, the build
   matrix, clippy for touched crates, and docs-index checks.
5. Review the diff for accidental service-schema logic, secrets in error messages, non-OpenDAL
   config changes, generated-file edits, debug output, and unrelated refactors. Rollback restores
   the converter and reference contract together.
