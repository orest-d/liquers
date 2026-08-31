# Phase 3: Examples and tests

Add focused inline tests in `error.rs` for `Error::configuration_error` and its type. In `store_config.rs`, test a missing required string and an unset `${LIQUERS_TEST_MISSING_CONFIG}` independently, asserting `ConfigurationError` and preserving useful messages. Keep malformed YAML/JSON/TOML tests asserting `ParseError`; keep unknown/unavailable factory type tests in `store_factory.rs` asserting `NotSupported`.

Add or extend the `assets.rs` persistence classification test so `ConfigurationError` becomes `NotPersisted`, never `NonSerializable`. Tests may return `Result` where setup is fallible; production changes use no `unwrap`/`expect`. Run `cargo test -p liquers-core error`, `store_config`, `store_factory`, and then `cargo test -p liquers-core --lib`.
