# Phase 2: Solution and architecture

## Chosen solution

Subject to the Phase 1 taxonomy decision, add `ConfigurationError` to `ErrorType` and `Error::configuration_error(message)`. Replace `General` only in `StoreConfig::require_config_string`, `expand_env_vars` for unset variables/unclosed interpolation where the input is semantically invalid, and configuration argument validation introduced by store factories. Keep parser conversion in `from_yaml`, `from_json`, and `from_toml` as `ParseError`; keep unsupported factory/store types as `NotSupported` because capability is more precise.

Extend `AssetData::classify_persistence_error` explicitly with `ConfigurationError => PersistenceStatus::NotPersisted`. This preserves exhaustive matching and means background persistence failure remains observable. Use typed constructors and `?`; no panic, erased error, or message parsing is introduced.

## Risks and validation

| Risk | Affected files/workflow | Validation and containment | Certainty |
|---|---|---|---|
| Binding compatibility | serialized `ErrorType` consumers | decide the public variant before merge; test enum serialization if supported | Medium |
| Over-classification | store document parsing/factory support | table-driven tests retain `ParseError` and `NotSupported` cases | High |
| Exhaustive match failure | `assets.rs` persistence status | compiler plus focused test | High |
| Message regression | user config troubleshooting | retain current context strings | Medium |

Rejected: make every setup failure configuration-related (loses parser/capability detail), and leave callers on `General` (does not solve binding classification). Existing boxed `Error` means the enum variant does not enlarge each `Result`; its payload remains behind one allocation.
