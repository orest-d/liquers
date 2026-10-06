# Phase 4: Implementation Plan - Account for `entities.rs` and `cache.rs`

1. **Re-audit.** Run the three `grep` checks of Phase 3. If `cache` gained a caller in
   `liquers-core`, or lost its `liquers-py` consumer, stop and revise Phase 1 (in the second case
   deletion becomes possible here).
2. **Module doc.** `liquers-core/src/cache.rs`: insert the Phase 2 doc comment; delete
   `use chrono::format;`. Proof: `cargo check -p liquers-core`,
   `cargo check -p liquers-core --target wasm32-unknown-unknown`, `cargo check -p liquers-py --lib`.
3. **Issue scope.** `specs/issues/CORE-SYNC-STORE-TRAIT-OBSOLETE.md` already carries the scope note
   (added with this design on 2026-10-05, §Related); confirm it still stands.
4. **Phase 5.** Execute `phase5-documentation.md` (PROJECT_OVERVIEW row, issue closure).
5. **Review.** The diff touches one Rust file (comment + one import), one reference row and two
   issue files.

Agent: one Haiku-tier agent (no skills needed beyond CLAUDE.md). Rollback: revert the commit.

## Post-Phase-4 Review Resolution (2026-10-05)

Phases 1-4 rewritten with the audit evidence; the `cache.rs` deletion is assigned to
`CORE-SYNC-STORE-TRAIT-OBSOLETE` and the orphan to `PY-MODULES-NOT-DECLARED-IN-LIB`; the placeholder
test plan is replaced by the evidence table.
