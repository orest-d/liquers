# Phase 4: Implementation Plan

Precondition: question 1 accepted.

1. Legacy metadata via `metadata_json` in both handlers. Proof: T5, T6.
2. `declared_media_type` and the upload change. Proof: T1–T4. Agent: haiku tier;
   rust-best-practices; knowledge: `MetadataRecord` setters in `liquers-core/src/metadata.rs`.
3. `cargo test -p liquers-axum`.
4. Spec rows, issue resolution, index. Diff review.
