# Phase 4: Implementation Plan

1. Change `AsyncStoreRouter::get_metadata` (Phase 2). Proof: `cargo check -p liquers-core`.
2. Add T1–T3 in `store.rs` tests. Proof: `cargo test -p liquers-core --lib router`. Agent: haiku
   tier; liquers-unittest.
3. `cargo test -p liquers-core --test store_conformance_CONF` (router suite unchanged).
4. Guide §9 note (History, `reviewed:`), issue resolution, index. Diff review.
