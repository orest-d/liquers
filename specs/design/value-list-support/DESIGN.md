---
id: VALUE-LIST-SUPPORT
kind: design
title: Typed list values for lui/children and lui/roots
status: complete
area: [lib/ui]
gh_pr: []
issues: []
created: 2026-03-02
superseded_by:
---
# Typed list values for lui/children and lui/roots

> **Complete on 2026-10-20** (bulk design review): the typed list results shipped without a tracked PR — `lui/children` and `lui/roots` build their lists through `handles_to_value` in `liquers-lib/src/ui/commands.rs`, backed by the `From<Vec<i64>>` value conversions. No phase is outstanding.

> **Acceptance scenarios not defined.** This design predates acceptance scenarios
> (`specs/DOCS_STRUCTURE_GUIDE.md` §5.2.1). Consider updating it: state the Phase 1 acceptance
> criteria as `AC-<n>` WHEN/THEN scenarios and cite each from the Phase 3 test that proves it. Remove
> this note when you do; until then `docs_index.py --check` counts this design in its warning.

Design tracking for `value-list-support`. This folder predates the four-phase
skeleton; its findings and proposed solution are in the sibling documents.
