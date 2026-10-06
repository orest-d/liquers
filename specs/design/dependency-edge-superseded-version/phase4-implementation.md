# Phase 4: Implementation Plan

1. Add the stale-at-birth block in `enter_dependencies` (Phase 2). Proof: `cargo check -p liquers-core`.
   Agent: sonnet tier; rust-best-practices; knowledge: `note_expired_dependency`,
   `stale-dependency-status-finalization` design.
2. T1–T3. Proof: `cargo test -p liquers-core --test stale_edge_at_birth` and `--lib`.
3. Full `cargo test -p liquers-core --lib --tests`. Fix any test that recorded mismatches on
   purpose (expected `Expired` now), with a comment.
4. `DEPENDENCIES_STATUS.md` sentence (History, `reviewed:`), issue resolution, index. Diff review.
