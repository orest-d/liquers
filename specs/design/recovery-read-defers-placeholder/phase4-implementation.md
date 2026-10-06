# Phase 4: Implementation Plan

1. **Change the two default methods.** In `liquers-core/src/assets.rs`, `trait AssetManager`,
   `get_any_status` and `get_binary_any_status`, apply the Phase 2 guard with
   `live_status_defers_to_store`. Depends on nothing. Proof: `cargo check -p liquers-core`.
   Agent: sonnet tier; skills rust-best-practices; knowledge: this design, `remove` in the same
   trait.
2. **Add T1–T3** in the same file's `mod tests`. If `try_insert_key_asset` is not reachable from
   the test, use the manager's existing test helper for mapping (look for `insert_key_asset` users
   in the tests module). Proof: `cargo test -p liquers-core --lib recovery_`. Agent: haiku tier;
   skills liquers-unittest.
3. **Run the asset suites:** `cargo test -p liquers-core --lib assets` and
   `cargo test -p liquers-core --test external_change_integration`. Proof: green. Containment:
   revert step 1 if any existing recovery test changes outcome, then reassess.
4. **Documentation.** Edit `specs/reference/ASSETS.md` (recovery reads sentence, History row,
   `reviewed:`). Close the source issue with a resolution note (§4.3). Set this design's status
   per §5.1. Run `python3 scripts/docs_index.py` and `--check`.
5. **Final diff review.** Only `assets.rs`, `ASSETS.md`, the issue, this design and the index have
   changed, and no `println!` was added.
