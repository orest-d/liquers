# Phase 4: Implementation Plan - Derived OpenDAL Store Arguments

1. **Re-verify OpenDAL.** Confirm the locked OpenDAL is still 0.55.x (`Cargo.lock`) and that each
   advertised service's config is `opendal::services::<Name>Config`, `Default + Serialize`, with no
   `skip_serializing_if`. Stop and revise Phase 2 on a difference.
2. **Derivation.** Add `derived_arguments<C>` and `service_arguments` (one gated arm per
   `OPENDAL_STORE_TYPES` entry). Proof: `cargo check -p liquers-store` and
   `cargo check -p liquers-store --no-default-features --features opendal`.
3. **Merge.** Rename `common_arguments` → `hand_written_arguments`; add `arguments` and `merge`;
   `type_info` uses `arguments`; rewrite the doc comment. Add `derive01`-`derive04`.
   Proof: `cargo test -p liquers-store --lib store_factory`.
4. **S3 tests.** Add `s3_01` and `s3_02` (with `disable_config_load: true`).
   Proof: run them with `AWS_REGION=eu-west-1` set in the environment as well; `s3_02` must still fail
   construction.
5. **Matrix.** `bash scripts/check-build-matrix.sh`.
6. **Phase 5.** Execute `phase5-documentation.md`.

Agent: Sonnet tier with rust-best-practices and liquers-unittest; knowledge: this design,
`liquers-store/Cargo.toml` feature table, `store-factories-in-core` Phase 3 §offline S3 test.
Rollback: `type_info` back to the hand-written list; delete the new functions and tests.

## Post-Phase-4 Review Resolution (2026-10-05)

Phases 1-4 rewritten from the source issue and an inspection of OpenDAL 0.55: signatures, per-service
gating (including the Unix `sftp` case), the merge rule, coverage, and named tests. One defect found
in the inherited `s3_02` specification (it depended on the machine's AWS configuration) is fixed.
