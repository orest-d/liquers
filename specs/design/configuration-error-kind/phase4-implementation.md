# Phase 4: Implementation plan

1. After the taxonomy decision, add `ConfigurationError` and the typed constructor in `liquers-core/src/error.rs`; update every exhaustive match, especially `AssetData::classify_persistence_error`. Proof: compile plus Phase 3 error tests; rollback is reverting the variant and callers as one atomic change.
2. In `store_config.rs` and applicable factory argument validation, replace only semantic configuration `General` errors. Preserve parser and support error variants. Proof: table-driven Phase 3 tests.
3. Add tests in `error.rs`, `store_config.rs`, `store_factory.rs`, and `assets.rs`; run focused tests then `cargo test -p liquers-core --lib`.
4. Update error/store/binding documentation and issue/design lifecycle records, regenerate the specs index, run `python3 scripts/docs_index.py --check`, format, and inspect the diff for accidental taxonomy broadening.
