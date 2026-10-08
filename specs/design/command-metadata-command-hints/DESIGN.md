---
id: COMMAND-METADATA-COMMAND-HINTS
kind: design
title: Command-level metadata hints
status: in_review
phase: implementation
readiness: needs-decision
area: [core/commands, macro, lib/ui]
issues: [COMMAND-METADATA-HAS-NO-COMMAND-LEVEL-HINTS]
created: 2026-08-31
---
# Command-level metadata hints

> **Acceptance scenarios not defined.** This design predates acceptance scenarios
> (`specs/DOCS_STRUCTURE_GUIDE.md` §5.2.1). Consider updating it: state the Phase 1 acceptance
> criteria as `AC-<n>` WHEN/THEN scenarios and cite each from the Phase 3 test that proves it. Remove
> this note when you do; until then `docs_index.py --check` counts this design in its warning.

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution and Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

## Review 2026-10-06

Re-verified against HEAD after the latest merges. **Updated.** Three corrections:

1. The design relied on a "byte-identical export test". `liquers-lib/tests/registry_export.rs`
   compares **signatures** (`signature_of`: the serialized command with `impl_version` zeroed),
   not file bytes. The acceptance criterion is restated: an empty `hints` map is omitted from
   serialization, so `signature_of` and `metadata_version` of every existing command are
   unchanged.
2. New dependency: `argument-info-description` (`ARGUMENT-INFO-HAS-NO-DESCRIPTION`) is the mirror
   gap. **Recommended maintainer merge (M3)**, leading source this design's feature.
3. New dependency: `command-cache-flag` (decided 2026-10-06: remove `cache`) changes the same
   struct and regenerates the same registry, so ship them in one release.

The open question (macro spelling `hint key: "value"`) is unchanged. Readiness stays `needs-decision`.
