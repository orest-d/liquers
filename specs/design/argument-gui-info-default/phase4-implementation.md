# Phase 4: Implementation Plan

1. In `liquers-core/src/command_metadata.rs`, add the shared `DEFAULT_GUI` 40-column constant,
   wire `ArgumentInfo.gui_info` to a clone of it, and add the Phase 3 omission/round-trip tests. Prove with the
   focused core tests; rollback is confined to the helper and attribute.
2. In `liquers-macro/src/registration.rs`, replace the implicit 20-column expression while leaving
   parsed `gui:` statements untouched; update and add expansion tests. This depends on step 1 and is
   contained by reverting the generated expression.
3. In `liquers-core/src/command_declaration.rs`, remove any parity exclusion and assert complete
   metadata/version equality. Change production code only if the test reveals a remaining path not
   using the shared default.
4. Regenerate `specs/command_registry.yaml` with the repository exporter if freshness reports
   expected version changes; review that only implicit GUI values and derived versions moved.
5. Update `COMMAND_DECLARATION.md` only if its current contract lists this default, then update the
   source issue resolution and design lifecycle during implementation.
6. Run `cargo fmt --all -- --check`, the focused tests, `bash scripts/check-build-matrix.sh`, and
   `python3 scripts/docs_index.py --check`. Review the final diff for unrelated metadata churn,
   debug output, generated-file edits, and explicit GUI regressions.
