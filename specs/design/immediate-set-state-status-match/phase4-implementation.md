# Phase 4: Implementation Plan

1. Add `written_status` and `written_status_with_recipe` (Phase 2). Proof: `cargo check -p liquers-core`.
2. Replace the four `final_status` matches (`DefaultAssetManager::set_binary`, `::set_state`,
   `ImmediateAssetManager::set_binary`, `::set_state`; search `let final_status = match`).
   Proof: `cargo test -p liquers-core --lib`. Agent: haiku tier; rust-best-practices.
3. Add T1, T2.
4. Run `rg "_ if self.recipe_opt" liquers-core/src`, which must return nothing.
5. Close the issue, set this design's lifecycle, regenerate and check the index. Diff review.
