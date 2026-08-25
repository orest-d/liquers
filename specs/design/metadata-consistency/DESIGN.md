---
id: METADATA-CONSISTENCY
kind: design
title: Metadata format and type consistency
status: superseded
area: [core/value]
gh_pr: []
issues: []
created: 2026-03-02
superseded_by: value-type-system
---
# Metadata format and type consistency

Design tracking for `metadata-consistency`. This folder predates the four-phase
skeleton; its findings and proposed solution are in the sibling documents.

## Superseded

Superseded on 2026-08-18 by `specs/design/value-type-system/`, which treated the same P0 as a
missing type model rather than a metadata-validation gap. The findings in `FINDINGS.md` remain
accurate and were used throughout; `PROPOSED_PLAN.md` was not followed — its hybrid
normalize-and-warn option was dropped in favour of a hard/soft tier split, and its `type_name`
proposal had already landed.
