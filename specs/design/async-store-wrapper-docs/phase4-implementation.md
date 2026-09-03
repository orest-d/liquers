# Phase 4: Implementation Plan

1. Inspect the current signatures and callers in CLAUDE.md, specs/guides/UNITTEST_GUIDE.md, specs/reference/STORE_CONFIG_FSD.md, .claude/skills/liquers-unittest/SKILL.md; stop if they differ from Phase 2. Proof: the focused Phase 3 test. Containment: revert only this source's files.
2. Implement Replace obsolete synchronous-store examples with AsyncMemoryStore and correct the unittest skill imports. Preserve existing ownership, async, serialization, and typed-error conventions. Proof: rg 'AsyncStoreWrapper|MemoryStore, Store' CLAUDE.md specs .claude/skills; python3 scripts/docs_index.py --check.
3. Add the Phase 3 regression tests and any current contract documentation updates. Proof: focused tests plus documentation review.
4. Update the source issue resolution/status only after evidence exists; regenerate `specs/index.csv` with `python3 scripts/docs_index.py`, run `python3 scripts/docs_index.py --check`, format, and review the diff for unrelated edits.

## Final Review

The plan is intentionally implementation-free. It must be rechecked against current signatures before execution and rolled back as a single scoped change if validation fails.

