# Phase 2: Solution and architecture

## Chosen solution

Subject to the taxonomy decision:

| File | Change |
|---|---|
| `liquers-core/src/error.rs` | `ErrorType::ConfigurationError`; `Error::configuration_error(message: String) -> Error` |
| `liquers-core/src/store_config.rs` | `require_config_string` (≈170): `general_error` → `configuration_error`. `expand_env_vars` (≈271): unset variable → `configuration_error`; unclosed `${` stays `parse_error`. |
| `liquers-core/src/environment_config.rs` | `from_yaml/json/toml` (≈92–107): `general_error` → `parse_error` (malformed document). Check the two other `general_error`s (≈114, ≈123) and classify each: semantic → `configuration_error`. |
| `liquers-core/src/store_factory.rs` | Argument-validation failures that are semantic → `configuration_error`. Unavailable/unknown types stay `not_supported`. |
| `liquers-core/src/assets.rs` | `classify_persistence_error`: `ConfigurationError => NotPersisted` (explicit arm) |
| `liquers-web/src/error.rs` | `error_type_name`: `ConfigurationError => "configuration_error"`, and the reverse in `error_type_from_name` |
| `liquers-web/tests/objects_OBJECT.rs` | add to `ALL_ERROR_TYPES` (count handled per `web-object06-error-type-exhaustiveness`) |
| `liquers-py/src/error.rs` | explicit arm (mirror its existing mapping style) |
| `liquers-axum/src/api_core/error.rs` | explicit arm → `500 Internal Server Error` |
| `liquers-lib/src/polars/util.rs` | explicit arm if its match is exhaustive over `ErrorType` |

Use typed constructors and `?`. No panic or message parsing.

## Rejected alternatives

- Make every setup failure a configuration error. That loses parser and capability detail.
- Leave callers on `General`. That does not solve classification.

## Risks and validation

| Risk | Affected | Validation and containment | Certainty |
|---|---|---|---|
| Binding compatibility | serialized `ErrorType`, JS names, Python | decide before merge; round-trip tests in each binding | Medium |
| Over-classification | parsing / factory support | table-driven tests keep `ParseError` and `NotSupported` | High |
| Missed exhaustive match | 5 crates | `cargo check --workspace --exclude liquers-web` + web loop + build matrix | High |
| Message regression | troubleshooting | keep current message strings | High |
