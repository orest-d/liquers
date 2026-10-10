# Phase 4: Implementation Plan

1. In `liquers-core/src/command_metadata.rs`, introduce `default_state_argument`, reuse it in both
   constructors, apply it as the serde default, remove the skip-on-`None` serializer rule, and add
   omission/null tests. Prove with focused core tests; rollback is one attribute/helper change.
2. In `liquers-core/src/command_declaration.rs`, verify source-command conversion supplies explicit
   intent before deserialization and extend convention tests. Do not broaden declaration behaviour.
3. In `liquers-core/src/plan.rs`, add the behavioural consumption regression using the existing
   PlanBuilder fixture; this depends on step 1 and contains semantic risk in one test.
4. Regenerate `specs/command_registry.yaml` (source commands gain `state_argument: null`) with the
   exporter in `CLAUDE.md`, add a dated CHANGELOG line noting the one-time `metadata_version` change
   for source commands, and prove with `cargo test -p liquers-lib --test registry_export`.
5. Update `COMMAND_DECLARATION.md` if raw omission/null is part of its exposed format, plus source
   issue/design lifecycle records during implementation.
6. Run formatting, `cargo test -p liquers-core --lib`, the registry test,
   `bash scripts/check-build-matrix.sh`, and docs-index checks. Review for unintended source-command
   changes, generated churn, debug code, and edits outside the stated files.
