# Phase 4: Implementation Plan

Question 1 decided by the maintainer on 2026-10-10. All steps done on branch `claude/fix-axum-store-upload-metadata`; T1–T3 are `store::handlers::tests`, T4 is `sar17`–`sar19` (plus a declared and an octet-stream case), T5–T6 are `sar20`.

1. Legacy metadata via `metadata_json` in both handlers. Proof: T5, T6.
2. `declared_media_type` and the upload change. Proof: T1–T4. Agent: haiku tier;
   rust-best-practices; knowledge: `MetadataRecord` setters in `liquers-core/src/metadata.rs`.
3. `cargo test -p liquers-axum`.
4. Spec rows, issue resolution, index. Diff review.
