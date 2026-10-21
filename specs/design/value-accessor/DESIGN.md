---
id: VALUE-ACCESSOR
kind: design
title: Value accessor abstraction
status: abandoned
area: [core/value]
gh_pr: []
issues: []
created: 2026-03-02
superseded_by:
---
# Value accessor abstraction

> **Abandoned on 2026-10-20** (bulk design review). Its read side now exists as `DataNode` (`liquers-core/src/data_node.rs`, shipped with `liquers-mustache`). Its `Value::Accessor(Arc<dyn …>)` with `Arc<Mutex>` mutation contradicts `specs/reference/VALUE_TYPE_SYSTEM.md` §"What a value variant may hold" and the content-versioned asset layer; its UI-binding motive belongs to `lui`, and converting on demand is answered by `value-conversion-registry` (conversion at binding). No source issue was ever attached.

> **Acceptance scenarios not defined.** This design predates acceptance scenarios
> (`specs/DOCS_STRUCTURE_GUIDE.md` §5.2.1). Consider updating it: state the Phase 1 acceptance
> criteria as `AC-<n>` WHEN/THEN scenarios and cite each from the Phase 3 test that proves it. Remove
> this note when you do; until then `docs_index.py --check` counts this design in its warning.

Design tracking for `value-accessor`. This folder predates the four-phase
skeleton; its findings and proposed solution are in the sibling documents.
