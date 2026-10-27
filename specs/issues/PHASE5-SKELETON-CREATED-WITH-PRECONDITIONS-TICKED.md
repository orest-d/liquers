---
id: PHASE5-SKELETON-CREATED-WITH-PRECONDITIONS-TICKED
kind: issue
title: A new design's Phase 5 skeleton claims every completion precondition is already met
status: closed
priority: P3
complexity: S
area: [docs]
created: 2026-10-18
design: phase5-skeleton-unticked
github:
---

## Problem

`.claude/skills/liquers-project/scripts/init_feature.py` copies the Phase 5 skeleton from
`references/phase5-documentation.md`, whose example section lists its five completion
preconditions as ticked (`- [x] Implementation is finished and validated`, …). Every design created
with the script therefore starts with a Phase 5 document asserting that the implementation is
finished, reviewed and documented, which is false until Phase 5 is written.

## Problem Example

`python3 init_feature.py demo` (full form) creates `phase5-documentation.md` whose
`## Completion Preconditions` reads `- [x] Implementation is finished and validated` and four more
ticked items; they should be `- [ ]`. Still true at main 18732dd
(`references/phase5-documentation.md:102-106`); the untouched skeletons of the draft designs
`agent-memory-derived` and `external-index-sync` carry it today.

## Impact

A reader or an agent checking the entry gate sees it met before any work is done; a reviewer has to
notice the falsehood. Low cost, but it erodes the gate. Workaround: untick by hand on creation.

## Expected behaviour

The skeleton's preconditions start unticked (`- [ ]`), and the reference keeps the ticked form only
as an example of a finished document, not as the copied template.

## Discovery

A reviewer of `specs/design/liquers-py/` Phase 1 found the ticked skeleton on 2026-10-18.

## Resolution

Fixed 2026-10-27 by `design/phase5-skeleton-unticked/`. The Summary Template in `.claude/skills/liquers-project/references/phase5-documentation.md` lists its five Completion Preconditions as `- [ ]`, with a sentence saying to tick each when its evidence exists and to name the evidence, so `init_feature.py` no longer creates a Phase 5 that claims finished work. The two untouched generated skeletons (`agent-memory-derived`, `external-index-sync`; byte-identical to the old template) are unticked. Evidence: `.claude/skills/liquers-project/scripts/test_init_feature.py`.
