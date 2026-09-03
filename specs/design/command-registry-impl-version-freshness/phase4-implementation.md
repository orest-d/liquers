# Phase 4: Implementation Plan

1. Inspect the current signatures and callers in liquers-lib/tests/registry_export.rs, liquers-lib/src/bin/export_command_registry.rs, specs/command_registry.yaml; stop if they differ from Phase 2. Proof: the focused Phase 3 test. Containment: revert only this source's files.
2. Implement Compare implementation versions as well as signatures and regenerate the checked-in registry, because impl_version is exported semantic data. Preserve existing ownership, async, serialization, and typed-error conventions. Proof: cargo test -p liquers-lib --test registry_export; cargo run -p liquers-lib --features cli --bin export-command-registry -- --format yaml -o specs/command_registry.yaml.
3. Add the Phase 3 regression tests and any current contract documentation updates. Proof: focused tests plus documentation review.
4. Update the source issue resolution/status only after evidence exists; regenerate `specs/index.csv` with `python3 scripts/docs_index.py`, run `python3 scripts/docs_index.py --check`, format, and review the diff for unrelated edits.

## Final Review

The plan is intentionally implementation-free. It must be rechecked against current signatures before execution and rolled back as a single scoped change if validation fails.

