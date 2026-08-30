# Phase 1: High-Level Design - Refresh Command Metadata Versions

## Feature Name

Refresh Command Metadata Versions

## Purpose

Ensure every command's `metadata_version` reflects the completed command metadata before an
environment is shared for evaluation. This fixes `MACRO-LEAVES-STALE-METADATA-VERSION`, where
`register_command!` inserts a skeleton, mutates it later, and leaves the stored version stale.

## Core Interactions

### Query System
No syntax change. Planning continues to read `CommandMetadataRegistry` normally.

### Store System
No store I/O. Version refresh is an in-memory registry operation.

### Command System
No new query commands. Add or rename a registry lifecycle method, likely `refresh_metadata_versions`,
that recomputes all stored command metadata versions after registration mutation is complete.

### Asset System
Indirectly protects dependency invalidation: asset dependency records use command
`metadata_version`, so the version must be current before manager startup loads command versions.

### Value Types
No value type changes.

### Web/API (if applicable)
No HTTP API change.

### UI (if applicable)
No UI change.

## Crate Placement

`liquers-core` owns `CommandMetadataRegistry`, `Environment::to_ref`, and dependency-version loading,
so the fix belongs there. `liquers-macro` should not be the only fix point because manual or future
post-registration metadata edits can create the same stale-version state.

## Documentation Intent

**Reference:** Extend `specs/reference/COMMAND_DECLARATION.md` for computed metadata-version
semantics and `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` for the initialization
boundary.

**Guide:** Extend `specs/guides/COMMAND_REGISTRATION_GUIDE.md` only if implementation changes what
command authors should do; otherwise note that no guide change is needed.

**Other documents to create:** None beyond the liquers-project phase documents.

**Specific documents to update:** `specs/issues/MACRO-LEAVES-STALE-METADATA-VERSION.md`,
`specs/README.md` if this design reaches an indexed capability-map change, and tests/comments that
currently assert the stale-version bug.

Future command and environment authors should understand that registry finalization, not individual
macros, is the invariant boundary. Reconsider a new reference only if Phase 2 exposes broader
registry lifecycle semantics.

## Open Questions

1. Resolved: rename or expose the operation as `refresh_metadata_versions`.
2. Phase 2 must verify whether `Environment::to_ref(self)` can refresh through owned `self` without
   adding mutable access to the `Environment` trait, or whether a default lifecycle hook is needed.
3. Resolved: add a note to `specs/design/environment-builder/` that the builder design must verify
   metadata versions are refreshed. If the builder delegates to `to_ref` and `to_ref` performs the
   refresh, no separate builder work should be needed.

## References

- `specs/issues/MACRO-LEAVES-STALE-METADATA-VERSION.md`
- `specs/design/environment-builder/`
- `liquers-core/src/command_metadata.rs`
- `liquers-core/src/context.rs`
- `liquers-core/src/commands.rs`
