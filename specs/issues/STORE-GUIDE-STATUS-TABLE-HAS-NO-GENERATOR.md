---
id: STORE-GUIDE-STATUS-TABLE-HAS-NO-GENERATOR
kind: issue
title: The store guide's status table claims to be generated from the conformance reports, and no generator exists
status: draft
priority: P3
complexity: S
area: [docs, store/backends]
design: store-guide-status-table
created: 2026-09-30
github:
---

## Problem

`specs/guides/STORE_IMPLEMENTATION_GUIDE.md` §9 ("Status of the in-tree stores") ends:

> This table is **generated from the reports**, not maintained by hand — `ConformanceReport`
> derives serde for exactly this reason. Regenerate it rather than editing it.

Nothing generates it. No script in `scripts/`, no test, and no code in
`liquers-core/src/store_conformance/` writes the table; it has been edited by hand, most recently
by `design/store-conformance-backlog/` (step 4 and Phase 5).

## Impact

The table's rule counts and statuses drift whenever a rule is added or a store changes, and
the sentence tells a reader not to fix it by hand while offering no other way. Adding `sidecar04`
changed every count at once.

## Expected behaviour

Either a small generator (a test or a script that runs the suites and renders the table from
serialized `ConformanceReport`s), or the sentence is replaced by an honest "maintained by hand; update
it when a rule or a store changes".

## Discovery

Found 2026-09-29 while planning Phase 4 of `design/store-conformance-backlog/`, which had to
edit the table and looked for the generator it names.
