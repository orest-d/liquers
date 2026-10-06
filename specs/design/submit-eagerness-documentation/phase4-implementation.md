# Phase 4: Implementation Plan

1. Edit the `Context::submit` doc comment (Phase 2 §1). Proof: `cargo doc -p liquers-core --no-deps`
   builds without warnings for this item.
2. Add T3 in `liquers-core/src/context.rs` tests. Proof:
   `cargo test -p liquers-core --lib submit_`. Agent: haiku tier; liquers-unittest. Containment:
   if T3 shows the status is `Processing` (the dependency started despite saturation), the
   reference sentence is wrong instead. Stop and correct the documentation to the observed
   behaviour.
3. Edit `DEPENDENCIES_STATUS.md` §2 with History and `reviewed:`.
4. Close the issue with a resolution noting that the frozen design text stays as is. Regenerate
   and check the index. Diff review.
