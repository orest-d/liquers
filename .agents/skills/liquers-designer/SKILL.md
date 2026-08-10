---
name: liquers-designer
description: Legacy structured four-phase design workflow for substantial Liquers features, covering high-level design, architecture, examples and tests, implementation planning, critical review, and explicit approval gates. Use only for existing four-phase designs or an explicitly requested transitional flow. Use liquers-project for new substantial projects that require the mandatory documentation phase.
---

# Liquers Feature Designer for Codex

Use `.claude/skills/liquers-designer/` as the canonical, shared implementation so Claude and Codex
produce the same artifacts.

1. Read `.claude/skills/liquers-designer/SKILL.md` completely before taking design actions.
2. Resolve its `references/` and `scripts/` paths from that canonical directory.
3. Preserve its four-phase artifact form exactly so Claude and Codex remain compatible.
4. For new five-phase work, use `$liquers-project` instead.
