# Phase 4: Implementation Plan

Precondition: the maintainer chooses "expose". For "keep closed", run only step 4 (guide) and
step 5.

1. **Public notify.** `notify_removed` becomes `pub`, with the contract doc. Proof:
   `cargo check -p liquers-core`.
2. **`AssetRef::new_installed`** as in Phase 2, with an explicit status match. Proof: T1
   (`cargo test -p liquers-core --lib new_installed`). Agent: sonnet tier; rust-best-practices.
3. **Minimal manager + scenarios.** Update `tests/common/minimal_manager.rs` `set_state` and its
   replacement path. Add the two shared scenarios to `tests/common/manager_scenarios.rs` and wire
   them into each manager's scenario runner. Proof:
   `cargo test -p liquers-core --test asset_manager_scenarios` (use the actual test target that
   includes `manager_scenarios`; check `liquers-core/tests/*.rs` for `mod common`).
4. **Guide.** Update `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` (rows, §11 row, History,
   `reviewed:`).
5. **Records.** Close the issue, set this design's lifecycle, then run `docs_index.py` and
   `--check`.
6. **Diff review.** Confirm that the surface widened by exactly two items.
