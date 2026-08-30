---
id: REFRESH-COMMAND-METADATA-VERSIONS
kind: design
title: Refresh command metadata versions before environment sharing
workflow: liquers-project
status: complete
area: [core/commands, macro, core/context, core/assets]
gh_pr: []
issues: [MACRO-LEAVES-STALE-METADATA-VERSION]
affects_docs:
  - specs/reference/COMMAND_DECLARATION.md
  - specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md
  - specs/guides/COMMAND_REGISTRATION_GUIDE.md
  - specs/guides/UNITTEST_GUIDE.md
created: 2026-08-30
superseded_by:
---
# Refresh Command Metadata Versions Design Tracking

**Created:** 2026-08-30

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples & Testing
- [x] Phase 4: Implementation Plan
- [x] Phase 5: Documentation
- [x] Implementation Complete

## Notes

Resolves `MACRO-LEAVES-STALE-METADATA-VERSION`: macro registration mutates command metadata after
the registry first computes `metadata_version`. The design verifies whether finalizing the command
metadata registry at `Environment::to_ref` is sound now, and how that maps to the future
`environment-builder` construction path.

Phase 1 approved with three decisions: expose the operation as `refresh_metadata_versions`; verify
the `to_ref(self)` mutation path in Phase 2; add an `environment-builder` note requiring that design
to preserve the refresh invariant, with no extra work if it delegates through refreshed `to_ref`.

Phase 2 approved: `to_ref(mut self)` can refresh before `EnvRef::new(self)` because the environment
is still owned. Add `Environment::get_mut_command_metadata_registry`; keep the fix synchronous and
pre-share; update `liquers-py` only for trait compatibility, with `cargo check -p liquers-py` as a
compile gate and unrelated runtime completion out of scope.

Phase 3 approved test style: runnable tests/prototypes. The test plan covers registry refresh in
isolation, `Environment::to_ref` lifecycle behavior across core environment variants, and the
existing macro/declaration regression without manual recomputation.

Phase 4 implemented: registry API, environment trait/accessors, cross-crate accessors, focused
tests, references, and final validation. `liquers-py` compile compatibility passed; unrelated
runtime completion remains out of scope. The default `liquers-lib` Polars validation row remains
blocked by existing `LIB-POLARS-ETHNUM-RUST-1-98-BROKEN`; no-default and wasm-webui rows pass.

Phase 5 approved: documentation, issue status, and generated spec indexes are current. The design is
complete.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
