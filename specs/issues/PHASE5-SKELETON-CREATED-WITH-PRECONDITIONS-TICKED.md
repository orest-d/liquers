---
id: PHASE5-SKELETON-CREATED-WITH-PRECONDITIONS-TICKED
kind: issue
title: A new design's Phase 5 skeleton claims every completion precondition is already met
status: draft
priority: P3
complexity: S
area: [docs]
created: 2026-10-18
design:
github:
---

## Problem

`.claude/skills/liquers-project/scripts/init_feature.py` copies the Phase 5 skeleton from
`references/phase5-documentation.md`, whose example section lists its five completion
preconditions as ticked (`- [x] Implementation is finished and validated`, …). Every design created
with the script therefore starts with a Phase 5 document asserting that the implementation is
finished, reviewed and documented, which is false until Phase 5 is written.

## Impact

A reader or an agent checking the entry gate sees it met before any work is done; a reviewer has to
notice the falsehood. Low cost, but it erodes the gate. Workaround: untick by hand on creation.

## Expected behaviour

The skeleton's preconditions start unticked (`- [ ]`), and the reference keeps the ticked form only
as an example of a finished document, not as the copied template.

## Discovery

A reviewer of `specs/design/liquers-py/` Phase 1 found the ticked skeleton on 2026-10-18.
