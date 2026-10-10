# Phase 4: Implementation plan

1. After `web-object06-error-type-exhaustiveness` has landed, add
   the variant and constructor in `liquers-core/src/error.rs`. Fix every exhaustive match the
   compiler reports in core (`assets.rs` classification). Proof: `cargo check -p liquers-core`; T1, T7.
2. Migrate `store_config.rs` and `environment_config.rs` call sites (Phase 2 table), and classify
   the two remaining `general_error`s in `environment_config.rs`. Proof: T2–T6.
3. Bindings: `liquers-web/src/error.rs` names, `objects_OBJECT.rs` list, `liquers-py/src/error.rs`,
   `liquers-axum/src/api_core/error.rs`, and `liquers-lib/src/polars/util.rs`. Proof:
   `cargo check -p liquers-py -p liquers-axum -p liquers-lib --all-features`; T8 (web loop after
   `cargo clean`); T9.
4. `bash scripts/check-build-matrix.sh`.
5. Update the guides named in Phase 1, the issue status, and the index
   (`python3 scripts/docs_index.py` and `--check`). Format, and inspect the diff for accidental
   taxonomy broadening.
