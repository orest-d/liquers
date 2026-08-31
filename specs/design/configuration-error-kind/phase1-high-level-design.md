# Phase 1: High-level design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question - public error taxonomy:** bindings observe `ErrorType`, so adding `ConfigurationError` is a cross-language compatibility commitment.
- **Explanation:** Semantic configuration failures can be isolated and tested. The recommended taxonomy preserves `ParseError` for malformed documents and `NotSupported` for unavailable store types, using the new variant only where configuration is semantically incomplete or invalid.
- **Open questions:**
  - **Proposed resolution - taxonomy boundary:** add `ErrorType::ConfigurationError` and `Error::configuration_error`; migrate missing required keys, unset environment variables, and rejected keys only. Preserve parse and capability kinds.

## Problem and outcome

Store configuration errors currently appear as `General`, `ParseError`, or `NotSupported`, making semantic failures impossible for bindings to classify reliably. Create a typed configuration category for semantic configuration failures without replacing more precise parse or support errors.

Acceptance criteria: missing required config and unset `${VAR}` produce `ConfigurationError`; malformed YAML/JSON/TOML remains `ParseError`; unavailable or unknown support remains `NotSupported`; persistence classification handles the new exhaustive enum variant as `NotPersisted`.

## Scope and constraints

Affected crates are `liquers-core` error, store configuration, factory construction, assets persistence classification, and any exhaustive tests. `Error` stays boxed and fallible APIs retain `Result`; no string matching or panic path is introduced. Do not alter wire serialization names or collapse existing diagnostics into a less precise category.

## Design Dependencies

- `store-factories-in-core` - **overlaps**: it establishes store argument coverage and factory failures; this design gives only its semantic configuration failures a stable category.
- `environment-builder` - **required-by**: host-built environments need to distinguish setup faults from query evaluation failures.

## Documentation assessment

Review store factory and language integration guides plus error references after the public enum decision; document the taxonomy boundary, not every error message.

## Consolidated Findings

`AssetData::classify_persistence_error` is exhaustive, providing a compile-time review point. The new variant must join `NotPersisted`; classifying it as `NonSerializable` would hide a real configuration failure. The unresolved decision is intentionally narrow: whether the public bindings contract gains this category.
