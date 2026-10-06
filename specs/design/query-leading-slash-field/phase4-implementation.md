# Phase 4: Implementation Plan

1. **Step A, documentation** (any time, no decision needed): module and field docs in
   `liquers-core/src/query.rs`. Proof: T5. This step alone closes the "no semantic meaning"
   inaccuracy.
2. **Decision gate.** If "keep the name", stop here: close the issue with the decision, and update
   `PROJECT_OVERVIEW.md` if it names the field.
3. **Rename** (Phase 2 Step B), with `#[serde(rename = "absolute")]`. Proof:
   `cargo check --workspace --exclude liquers-web`, then T1–T4
   (`cargo test -p liquers-core --lib query`, `--lib context`).
4. **Python:** `rooted` getter, `absolute` alias. Proof: `cargo check -p liquers-py`.
5. `PROJECT_OVERVIEW.md` sentence (History, `reviewed:`), issue resolution, index. Diff review: no
   serialized-form change (T2, T3).
