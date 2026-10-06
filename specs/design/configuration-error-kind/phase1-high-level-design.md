# Phase 1: High-level design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question - public error taxonomy:** every binding observes
  `ErrorType` (JavaScript names, Python, HTTP status), so adding `ConfigurationError` is a
  cross-language compatibility commitment.
- **Explanation:** Semantic configuration failures can be isolated and tested. The recommended
  taxonomy keeps `ParseError` for malformed documents and `NotSupported` for unavailable store
  types, and uses the new variant only where configuration is semantically incomplete or invalid.
- **Open questions:**
  - **Proposed resolution - taxonomy boundary:** add `ErrorType::ConfigurationError` and
    `Error::configuration_error`. Migrate missing required keys, unset environment variables and
    rejected keys. Keep parse and capability kinds.
  - **Proposed resolution - environment documents:** `EnvironmentConfig::from_yaml/json/toml`
    report malformed documents as `ParseError` (today `General`), consistent with store documents.
  - **Proposed resolution - HTTP:** `ConfigurationError` maps to 500 (a server-side setup fault),
    not 400.

## Problem and outcome

Configuration errors appear as `General`, `ParseError` or `NotSupported`, so bindings and hosts
cannot classify a setup fault reliably. Create a typed category for semantic configuration
failures without replacing more precise parse or support errors.

Acceptance criteria:

- missing required store config and an unset `${VAR}` give `ConfigurationError`;
- malformed YAML/JSON/TOML gives `ParseError`, for store **and** environment documents;
- an unavailable or unknown store type stays `NotSupported`;
- persistence classification maps the new variant to `NotPersisted`;
- the JavaScript name is `configuration_error`, and it round-trips through `error_type_from_name`;
- Python and the axum HTTP mapping handle the variant explicitly.

## Scope and constraints

Affected: `liquers-core` (`error.rs`, `store_config.rs`, `environment_config.rs`,
`store_factory.rs` argument validation, `assets.rs` classification) and every exhaustive
`ErrorType` match in `liquers-web`, `liquers-py`, `liquers-axum` and `liquers-lib`. `Error` stays
boxed. No string matching or panic path is introduced. Wire names of existing variants do not change.

## Design Dependencies

- `web-object06-error-type-exhaustiveness` (P2, ready) - **overlaps**: it replaces the hard-coded
  OBJECT06 count with a derived check. Land it first, and this design then adds one name to the list.
- `web-liquers-error-constructor` - **overlaps**: its constructor validates names with
  `error_type_from_name`, so the new name becomes constructible automatically.
- `store-factories-in-core` (complete) - **overlaps**: owns factory argument validation.
- `environment-builder` (complete) - **overlaps**: hosts using `EnvironmentBuilder::build` are the
  main beneficiaries.

## Documentation assessment

Error reference (if any lists `ErrorType`), `STORE_FACTORY_GUIDE.md` and
`ENVIRONMENT_CONSTRUCTION_GUIDE.md` (error kinds of configuration failures),
`LANGUAGE-INTEGRATION_GUIDE.md` (error names), `WEB_API_SPECIFICATION.md` (status mapping).

## Consolidated Findings

`AssetData::classify_persistence_error` is exhaustive and gives a compile-time review point. The
new variant goes to `NotPersisted`. Five crates match `ErrorType` exhaustively, so one PR must
update them all, and the build matrix plus the web loop prove it. The decision stays narrow:
whether the public contract gains this category.
