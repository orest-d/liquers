# Phase 4: Implementation Plan

1. **Probe image.** `cargo check -p liquers-lib --no-default-features --tests` with a scratch
   call of `register_image_commands!`, to learn whether the image macro needs gating. Proof: the
   compiler result recorded in the PR.
2. **Twins** for egui, polars, records (+image if step 1 says so) in `commands.rs`. Proof:
   `cargo check -p liquers-lib --no-default-features`. Agent: haiku tier; rust-best-practices.
3. **T1** new test file. Proof: the per-feature commands in Phase 3.
4. **Matrix:** `bash scripts/check-build-matrix.sh`.
5. Comments in `export_command_registry.rs`/`registry_export.rs`, guide line, issue resolution,
   index. Diff review.
