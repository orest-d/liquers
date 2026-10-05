# Phase 4: Implementation Plan

1. Inspect the current signatures and callers in liquers-core/src/lib.rs, liquers-core/src/entities.rs, liquers-core/src/cache.rs, liquers-py/src/cache.rs, liquers-core/src/escape.rs, specs/issues/REPO-DEAD-CODE-HYGIENE.md; stop if they differ from Phase 2. Proof: the focused Phase 3 test. Containment: revert only this source's files.
2. Implement Re-audit the named modules, retain modules with callers, and close the stale issue with evidence rather than deleting live code. Preserve existing ownership, async, serialization, and typed-error conventions. Proof: cargo test -p liquers-core entities; cargo test -p liquers-core cache.
3. Add the Phase 3 regression tests and any current contract documentation updates. Proof: focused tests plus documentation review.
4. Update the source issue resolution/status only after evidence exists; regenerate `specs/index.csv` with `python3 scripts/docs_index.py`, run `python3 scripts/docs_index.py --check`, format, and review the diff for unrelated edits.

## Final Review

The plan is intentionally implementation-free. It must be rechecked against current signatures before execution and rolled back as a single scoped change if validation fails.

## Post-Phase-4 Review (2026-10-05)

- **Problem still valid:** half. `entities.rs` is live: it holds the named-entity tables used by
  `escape.rs` and `bin/generate_entities.rs`. `cache.rs` (`BinCache`, `Cache`, `NoCache`) has
  exactly one compiled consumer, `liquers-py/src/context.rs`, whose legacy `Environment` is
  obsolete in the same way as the sync `Store` trait. `liquers-py/src/cache.rs` is an undeclared
  orphan module (`PY-MODULES-NOT-DECLARED-IN-LIB`). Phase 1 says "entities and cache are live",
  which is only half true.
- **Solution correct:** incomplete. "Close the issue with evidence" fits `entities.rs`. For
  `cache.rs` the right outcome is to hand it to the issue that removes the obsolete sync layer.
- **Unnecessary abstractions:** none proposed.
- **Detail:** **insufficient.** Phases 2-4 are generic template text ("Implement Re-audit
  the named modules…"). They name no evidence, no decision per module and no files to change.
- **Tests:** the Phase 3 table is a placeholder. `cargo test -p liquers-core cache` proves
  nothing about dead code. A records-only change needs no tests; this one needs a `grep`/build
  evidence record instead.
- **Interactions:** overlaps `CORE-SYNC-STORE-TRAIT-OBSOLETE` (the sync `Cache` goes with the sync
  `Store`) and `PY-MODULES-NOT-DECLARED-IN-LIB` (the orphan `liquers-py/src/cache.rs`).
- **Verdict:** not ready as written. Recommend `readiness: covered`. Close the `entities.rs` half
  with the evidence above, and move the `cache.rs` half into `CORE-SYNC-STORE-TRAIT-OBSOLETE`.
