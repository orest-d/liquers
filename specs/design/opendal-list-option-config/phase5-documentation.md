# Phase 5: Documentation - OpenDAL List Option Configuration Encoding

## Completion Preconditions

- [x] Implementation and focused validation are complete.
- [x] The existing design's requirements are reflected in code and tests.
- [x] Current reference documentation matches the implemented behavior.

## Implementation Summary

`StoreConfig::config_as_string_map` now converts a non-empty list of non-null scalar values into
OpenDAL's comma-separated configuration form. It omits a top-level null and rejects empty,
nested, object-containing, null-containing, or comma-bearing list values with the option name.
Scalar strings, booleans, and numbers retain their existing text formatting; environment expansion
occurs before list elements are checked for comma ambiguity. This conforms to the approved design.

## Documentation Delivered

No new reference or guide was needed. [`STORE_CONFIG_FSD.md`](../../reference/STORE_CONFIG_FSD.md)
is the authoritative `affects_docs` document and now specifies the list encoding, null, ambiguity,
and scalar compatibility rules. `specs/README.md` needs no capability-map change because its
existing design entry remains appropriate for this legacy simplified design.

## Issues Filed

None. `STORE-OPENDAL-LIST-OPTION-MISPARSED` is closed with the implementation and test evidence.

## Important Learning

OpenDAL's string deserializer uses commas for sequences, so JSON array serialization cannot cross
the `Operator::via_iter` boundary. List encoding belongs at the fallible Liquers conversion seam,
where invalid structured input can name its source option before it reaches a backend.

## Conformance and Remaining Work

The requested defect and all approved design scope are complete. No deferred work remains.

## Validation

Focused `liquers-core` tests, the applicable `liquers-store` test suite, `liquers-core` checks with
default, no-default, and `toml` features, and the documentation index check passed. Strict Clippy
is blocked by pre-existing `liquers-macro` warnings elevated by `-D warnings`; no diagnostic names
the changed module. The repository-wide formatting check also reports pre-existing formatting drift
outside this change, while `store_config.rs` was formatted directly.
