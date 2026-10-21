---
id: REGISTER-COMMAND-ENUM
kind: design
title: Enum parameters in register_command!
status: complete
area: [macro, core/commands]
gh_pr: []
issues: []
created: 2026-03-02
superseded_by:
---
# Enum parameters in register_command!

> **Complete on 2026-10-20** (bulk design review): everything this design describes shipped without a tracked PR — the `enum:` / `enum_ref:` DSL with typed maps and `others`, default checks and GUI defaults (`liquers-macro` `registration.rs` `EnumParameterSpec`), alias expansion at plan time (`liquers-core/src/plan.rs`), documented in `specs/reference/REGISTER_COMMAND_FSD.md` §Parameter Metadata. The remaining global-enum work is `COMMAND-METADATA-ENHANCEMENTS` (design `global-enum-registration`).

> **Acceptance scenarios not defined.** This design predates acceptance scenarios
> (`specs/DOCS_STRUCTURE_GUIDE.md` §5.2.1). Consider updating it: state the Phase 1 acceptance
> criteria as `AC-<n>` WHEN/THEN scenarios and cite each from the Phase 3 test that proves it. Remove
> this note when you do; until then `docs_index.py --check` counts this design in its warning.

Design tracking for `register-command-enum`. This folder predates the four-phase
skeleton; its findings and proposed solution are in the sibling documents.
