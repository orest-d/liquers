---
id: ACTIVE-09
kind: design
title: Design for REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED
phase: implementation
readiness: ready
autofix: eligible
area: [lib/commands, build, docs]
issues: [REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED]
gh_pr: [94]
created: 2026-09-03
---

# Design Tracking

> **Acceptance scenarios not defined.** This design predates acceptance scenarios
> (`specs/DOCS_STRUCTURE_GUIDE.md` §5.2.1). Consider updating it: state the Phase 1 acceptance
> criteria as `AC-<n>` WHEN/THEN scenarios and cite each from the Phase 3 test that proves it. Remove
> this note when you do; until then `docs_index.py --check` counts this design in its warning.

- [x] Phase 1: High-Level Design
- [x] Phase 2: Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan

## Review 2026-10-06

Re-verified against HEAD after the latest merges. **Rewritten (the original phases were template text).**
Findings at HEAD:

1. Regenerating `specs/command_registry.yaml` now reproduces the committed file exactly (checked
   2026-10-06: `export-command-registry` to a scratch path, `diff` with comments stripped, no
   difference). The stale `impl_version`s the issue reported were fixed by a later regeneration.
   The **defect stands**: nothing would detect the next drift, because `registry_export`'s
   `signature_of` zeroes `impl_version`.
2. `register_command!` now has a `version:` statement (`auto`, `now`, string, integer). `auto` uses
   the `#[command_version]` token hash, and `now` is a build-time timestamp. A `now` command can
   never match a committed registry. No in-tree command uses `now` (`rg "version: now" liquers-lib/src`
   is empty), so the check can require that.
3. Version *kind* cannot identify `now` reliably (`Version::kind` reads a flag bit that a blake3
   hash may or may not set), so the test cannot skip "timestamp" versions. It fails with a message
   instead.

Readiness stays `needs-decision`: whether comment-only edits to an `auto`-versioned command must
force a regeneration (that is the cost of exact comparison).
