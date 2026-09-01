# Phase 4: Reproducible Implementation Record

1. Add file-level `polars` gates to `polars_commands.rs` and `polars_value_serde.rs`. Dependency:
   Cargo feature names. Proof: no-default and Polars-only test builds. Containment: each target gate
   can be reverted independently.
2. Add an item-level `egui` gate in `ui_shortcuts_integration.rs`. Dependency: mixed-suite audit.
   Proof: six non-egui tests still run. Containment: do not gate the full file.
3. Make `registry_export.rs` assertions feature-aware. Dependency: known exported default registry.
   Proof: default freshness plus reduced-build anchors. Containment: preserve default comparison.
4. Extend `scripts/check-build-matrix.sh` native rows with `--tests` and image support; update
   `CLAUDE.md`. Dependency: steps 1-3. Proof: 11/11 matrix. Containment: keep wasm library-only.
5. Run all six configurations, matrix, formatting, and docs-index checks; update the source
   resolution and review for hidden tests, duplicated feature logic, output noise, or runtime edits.

This plan records commit `6b95eff`; it was not executed in this design run.
