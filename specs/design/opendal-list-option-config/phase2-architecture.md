# Phase 2: Solution and Architecture - OpenDAL List Option Configuration Encoding

## Overview

Encode JSON arrays for OpenDAL by joining scalar string representations with commas, reject nested
arrays/objects with `Error::not_supported`, and omit `null` entries so absent optional OpenDAL
fields stay absent. Keep scalar string, bool and number behaviour unchanged except for documenting
that non-integral floats may still be rejected by OpenDAL integer fields.

## Known-Issue Preflight

| Issue | Status | Priority | Relevance and action | Blocking? |
|---|---|---|---|---|
| `STORE-OPENDAL-SLASH-HANDLING` | accepted | P1 | Same backend but path mapping, not configuration flattening. No dependency. | no |
| `STORE-OPENDAL-ARGUMENTS-NOT-DERIVED` | draft | P3 | Future argument metadata should describe list-as-string conventions; not required for this conversion fix. | no |

## Files and Symbols

Primary source file: `liquers-core/src/store_config.rs`, method
`StoreConfig::config_as_string_map`. Integration call site:
`liquers-store/src/store_factory.rs`, `create_opendal_operator(..., config.config_as_string_map()?)`.
Reference file: `specs/reference/STORE_CONFIG_FSD.md`.

## Data, Ownership, Serialization and Errors

The input remains `HashMap<String, serde_json::Value>` and output remains
`HashMap<String, String>`. Array elements are borrowed while formatting and inserted as one owned
comma-separated `String`. Reject unsupported element types with `Error::not_supported` including
the config key.

## Sync, Async and API Effects

The conversion is synchronous and I/O-free. Public signature can remain
`Result<HashMap<String, String>, Error>`, so no caller API changes are expected.

## Alternatives

Rejected: keep documenting comma-separated strings only; that leaves natural YAML arrays as a trap.
Rejected: introspect every OpenDAL service schema before flattening; too large and brittle for an
`S` issue. Rejected: always reject arrays; less ergonomic and contradicts Liquers list syntax.

## Risk Assessment

| Assessment | Record |
|---|---|
| Files | 1 source/test file, 1 reference document, issue/design/index specs. |
| Impact area | OpenDAL store config flattening. |
| Module/crate reach | Source change in `liquers-core`, call site in `liquers-store`; API unchanged. |
| Existing-test breakage | Low; tests expecting JSON array text would change, none expected. |
| New validation | Unit tests for string, bool, integer, array of strings, null omission and nested rejection. |
| Behavioural risk | Arrays containing commas become ambiguous; document scalar element restriction. No concurrency/security concern. |
| Recovery | Revert conversion branch and reference text. |
| Certainty | High on OpenDAL comma convention; medium on whether null should omit or error, chosen as omit for optional fields. |

## Rust Review

Use existing `Error` constructors and `?` from `expand_env_vars`; no unwraps. Keep borrowing local,
return owned strings, and do not add dependencies or trait bounds.
