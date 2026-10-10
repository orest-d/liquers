# Phase 4: Implementation Plan

1. **Documentation**: module and field docs in `liquers-core/src/query.rs`. Proof: T5.
2. **Rename** (Phase 2 Step B), field and wire name, no `serde(rename)`; update the legacy plan JSON
   in `plan.rs` tests to `"rooted"`. Proof: `cargo check --workspace --exclude liquers-web`, then
   T1–T4 (`cargo test -p liquers-core --lib`).
3. **Python:** replace the `absolute` getter with `rooted`. Proof: `cargo check -p liquers-py`.
4. `PROJECT_OVERVIEW.md` sentence naming the field (History, `reviewed:`), issue resolution, index.
   Diff review: no change to query text encoding (T1, T3).
