# Phase 3: Examples and Tests

## Behaviour Examples

An argument deserialized from `{"name":"count"}` and one built with
`ArgumentInfo::any_argument("count")` both yield `TextField(40)`. A macro parameter without `gui:`
also yields 40, while `gui: integer` or another explicit hint remains unchanged.

## Tests

1. In `liquers-core/src/command_metadata.rs`, deserialize minimal JSON and YAML `ArgumentInfo`
   values and assert 40; serialize and round-trip without changing it.
2. In `liquers-macro/src/registration.rs`, update scalar expansion assertions to 40 and add an
   explicit-GUI case proving the override still wins.
3. In `liquers-core/src/command_declaration.rs`, extend the representative macro/declaration parity
   test to compare `gui_info` and `metadata_version` without exclusions.
4. Regenerate the registry and run its freshness test to expose expected version churn.

Use ordinary `#[test]` modules; no environment or async harness is required. Validation commands:
`cargo test -p liquers-core --lib`, `cargo test -p liquers-macro`,
`cargo test -p liquers-lib --test registry_export`, and `bash scripts/check-build-matrix.sh`.
