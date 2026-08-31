# Phase 4: Implementation plan

1. Correct the argument order in `CommandRegistryIssue::warning` and `::error` in `liquers-core/src/command_metadata.rs`. Proof: constructor tests; rollback is the two-line reversal.
2. Add the three Phase 3 inline tests, with distinct identifier values. Proof: `cargo test -p liquers-core command_registry_issue`.
3. Update the issue as resolved, regenerate specs with `python3 scripts/docs_index.py`, run `--check`, format, run `cargo test -p liquers-core --lib`, and review the diff for unrelated metadata changes.
