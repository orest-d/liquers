---
id: REGISTER-COMMAND-PAYLOAD-DOCS
kind: design
title: Documenting the payload, expires and version metadata statements of register_command!
workflow: liquers-project
status: in_review
phase: documentation
readiness: ready
autofix: eligible
area: [docs, core/commands, macro]
issues: [REGISTER-COMMAND-PAYLOAD-STATEMENT-UNDOCUMENTED, REGISTER-COMMAND-EXPIRES-AND-VERSION-STATEMENTS-UNDOCUMENTED]
merged: 2026-10-05
affects_docs: [reference/REGISTER_COMMAND_FSD.md, guides/COMMAND_REGISTRATION_GUIDE.md]
created: 2026-10-04
---
# register-command-payload-docs Design Tracking

> **Acceptance scenarios not defined.** This design predates acceptance scenarios
> (`specs/DOCS_STRUCTURE_GUIDE.md` §5.2.1). Consider updating it: state the Phase 1 acceptance
> criteria as `AC-<n>` WHEN/THEN scenarios and cite each from the Phase 3 test that proves it. Remove
> this note when you do; until then `docs_index.py --check` counts this design in its warning.

Autonomous bulk design (`guides/autonomous_bulk_design.md`) for
`REGISTER-COMMAND-PAYLOAD-STATEMENT-UNDOCUMENTED`.

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation (executed 2026-10-07; awaiting approval)

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

## Scope Change (2026-10-05)

After the post-Phase-4 review the maintainer merged
`REGISTER-COMMAND-EXPIRES-AND-VERSION-STATEMENTS-UNDOCUMENTED` into this design: the same FSD table,
the same `CLAUDE.md` line and the same guide bullet, so one coherent edit instead of two History rows
per document. The payload issue stays the leading source. The design now follows the five-phase
`liquers-project` contract; Phase 5 holds the documentation plan.

- [Phase 5](./phase5-documentation.md) (executed 2026-10-07)

## Review 2026-10-06

Re-verified against HEAD after the latest merges. **Still valid; no change.** The macro arms (`"payload"`,
`"expires"`, `"version"` with `auto`/`now`/string/integer in `liquers-macro/src/registration.rs`
≈839–915) and emitters (`payload_required_code`, `expires_code`, `impl_version_code`) match
Phase 2. The three statements are still undocumented in `REGISTER_COMMAND_FSD.md`,
`COMMAND_REGISTRATION_GUIDE.md` (changed by `context-title-description`, but not in this area) and
`CLAUDE.md`. Readiness stays `ready`.
