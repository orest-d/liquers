# Phase 4: Implementation Plan

1. Inspect the current signatures and callers in liquers-store/src/store_factory.rs, liquers-store/src/opendal_store.rs, liquers-store/Cargo.toml; stop if they differ from Phase 2. Proof: the focused Phase 3 test. Containment: revert only this source's files.
2. Implement Derive enabled service config fields from Default plus Serialize behind per-service cfg gates, then add offline S3 construction tests. Preserve existing ownership, async, serialization, and typed-error conventions. Proof: cargo test -p liquers-store store_factory; cargo test -p liquers-store --features opendal.
3. Add the Phase 3 regression tests and any current contract documentation updates. Proof: focused tests plus documentation review.
4. Update the source issue resolution/status only after evidence exists; regenerate `specs/index.csv` with `python3 scripts/docs_index.py`, run `python3 scripts/docs_index.py --check`, format, and review the diff for unrelated edits.

## Final Review

The plan is intentionally implementation-free. It must be rechecked against current signatures before execution and rolled back as a single scoped change if validation fails.

## Post-Phase-4 Review (2026-10-05)

- **Problem still valid:** yes. `OpendalStoreFactory::common_arguments`
  (`liquers-store/src/store_factory.rs` ≈103) is still hand-written, and the S3 offline tests do
  not exist.
- **Solution correct:** the direction is right, but it is not specified in the design. The source
  issue holds more of the design than Phases 1-4 do.
- **Detail:** **insufficient.** Phases 2-4 are generic template text. Missing: the signature of
  `derived_arguments<C>()`; the `store_type → config type` match and how each arm is `#[cfg]`-gated
  per `services-*` feature; how hand-written `doc` text merges onto derived entries; whether
  coverage stays `ArgumentCoverage::Partial`; the names and assertions of `derive01`, `s3_01`
  and `s3_02` (including the issue's warning to assert *presence*, never exhaustiveness).
- **Tests:** placeholder table. It must name the three tests above and include a
  `--no-default-features` build-matrix check.
- **Interactions:** `opendal-list-option-config` (implemented). A derived default for a list
  field (`endpoints`) is a JSON array, but configuration now expects a YAML list that gets
  comma-joined, so the derived description must say so.
- **Verdict:** not ready. Readiness should be lowered (`phase2-blocked` in spirit) until Phases 2-4
  are written from the issue's own content.
