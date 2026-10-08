---
name: liquers-project
description: The Liquers design and backlog workflow. Five gated phases for substantial projects, plus issue triage and filing with design overlap, spin-off of problems found while fixing an issue, automatic-fix eligibility, bulk design and backlog compaction. Use for new value types, command libraries, storage backends, UI components, API endpoints, cross-crate changes, explicit Liquers project-phase requests, filing or triaging an issue, a problem discovered while fixing another issue, "bulk-design" / "bulk design", and "compaction" / "backlog compaction". Supersedes the removed liquers-designer skill.
---

# Liquers Project for Codex

Use `.claude/skills/liquers-project/` as the canonical shared implementation so Claude and Codex
produce artifacts with exactly the same form.

1. Read `.claude/skills/liquers-project/SKILL.md` completely before taking project actions.
2. Resolve its `references/` and `scripts/` paths from that canonical directory.
3. Follow its Host Compatibility and Artifact Contract section.
4. Do not duplicate, translate, or independently modify its templates when producing artifacts.
5. Route by its Modes table: issue triage, fixing with spin-offs, bulk design and compaction each
   have a reference under `.claude/skills/liquers-project/references/`. For spin-offs, use the
   mechanism Codex supports (a delegated task, or a second branch) per `references/spin-off.md`.
