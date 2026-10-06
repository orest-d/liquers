# Phase 3: Examples and tests

| # | Test | Location | Asserts |
|---|---|---|---|
| T1 | `configuration_error_has_its_type` | `error.rs` | constructor sets `ErrorType::ConfigurationError` |
| T2 | `missing_required_config_is_configuration_error` | `store_config.rs` | `require_config_string("bucket")` on an empty config |
| T3 | `unset_env_var_is_configuration_error` | `store_config.rs` | `expand_env_vars("${LIQUERS_TEST_MISSING_CONFIG}")` |
| T4 | `unclosed_env_var_is_parse_error` | `store_config.rs` | `expand_env_vars("${X")` stays `ParseError` |
| T5 | `malformed_environment_document_is_parse_error` | `environment_config.rs` | `from_yaml(":::")`, `from_json("{")`, `from_toml("=")` |
| T6 | existing unknown-store-type tests | `store_factory.rs` | still `NotSupported` |
| T7 | `configuration_error_is_not_persisted` | `assets.rs` | classification → `NotPersisted` |
| T8 | OBJECT06 | `liquers-web/tests/objects_OBJECT.rs` | `configuration_error` round-trips by name |
| T9 | axum error mapping test | `liquers-axum` | `ConfigurationError` → 500 |

Run `cargo test -p liquers-core --lib`, `cargo test -p liquers-axum`, `cargo check -p liquers-py`,
and the web Node loop after `cargo clean`.
