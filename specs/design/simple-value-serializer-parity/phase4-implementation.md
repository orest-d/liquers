# Phase 4: Implementation Plan

1. Extend `as_bytes` (Phase 2). Proof: T2, T3. Agent: haiku tier; rust-best-practices; knowledge:
   core `Value::as_bytes`/`deserialize_from_bytes` in `liquers-core/src/value.rs`.
2. Extend `deserialize_from_bytes`. Proof: T1 with `UNWRITABLE` emptied.
3. Delete `UNWRITABLE` and rename T1. Add T4.
4. `cargo test -p liquers-lib --lib --tests`; build matrix (default arms changed in a value type).
5. Issue resolution, index. Diff review.
