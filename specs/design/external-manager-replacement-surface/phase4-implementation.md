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

## Outcome (2026-10-11)

Steps 1–6 done. Difference from Phase 3: the unserializable-value scenario (T2) runs against the
external manager only. The built-in managers validate a supplied state's declared data format
against the type registry and refuse `Value::I32` declared as `bin` with `SerializationError`;
they accept a value with no byte form only when its type declares no formats (a UI element), and
core's `Value` has no such type. The replacement scenario (T3) runs against all three managers.
The built-in managers do not adopt `new_installed` (optional per Phase 1).

Review (Codex, PR #118): the contract now says to notify **after** unmapping, once the key holds
its replacement, as the built-in managers do; notifying before let a woken subscriber find the old
asset again. A metadata-only `set_state` removes the key's stored object first, so a replaced
value's bytes do not survive beside the new metadata (`scenario_unserializable_state_replaces_stored_bytes`).
