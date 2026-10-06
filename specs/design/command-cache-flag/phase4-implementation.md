# Phase 4: Implementation Plan

Precondition: questions 1–3 accepted.

1. Remove the field and its uses (Phase 2 table, excluding the registry). Proof:
   `cargo check -p liquers-core -p liquers-lib -p liquers-py --all-features` (the egui feature is
   needed for the widget). Agent: haiku tier; rust-best-practices.
2. Update and add T1, T2. Proof: `cargo test -p liquers-core --lib command_metadata`.
3. Regenerate `specs/command_registry.yaml` with the exporter, then add the CHANGELOG line inside
   the markers. Proof: T3.
4. `bash scripts/check-build-matrix.sh` (a struct changed under several features).
5. Docs (search `cache` in `specs/reference/*COMMAND*`), issue resolution with the upgrade note,
   index. Diff review: the registry diff contains only `cache` deletions and the CHANGELOG line.
