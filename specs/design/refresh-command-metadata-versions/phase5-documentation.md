# Phase 5: Documentation - Refresh Command Metadata Versions

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical

## Implementation Summary

Implemented the approved `Environment::to_ref` lifecycle fix for
`MACRO-LEAVES-STALE-METADATA-VERSION`. `CommandMetadataRegistry` now has
`refresh_metadata_versions(&mut self) -> &mut Self`, and the older
`update_all_metadata_versions` name remains as a deprecated delegating shim. The `Environment`
trait now exposes `get_mut_command_metadata_registry`, allowing the default `to_ref(mut self)`
implementation to refresh command metadata while the environment is still owned and before it is
wrapped in `EnvRef`.

The mutable accessor was added to all current environment implementors in scope:
`SimpleEnvironment`, `SimpleEnvironmentWithPayload`, `ImmediateEnvironment`,
`ImmediateEnvironmentWithPayload`, `liquers_lib::DefaultEnvironment`, and `liquers_py::Environment`.
This conforms to the approved design: the refresh happens after registration mutation, before
manager startup can load command versions, and without adding a runtime error path.

Tests now cover the registry operation directly, the `to_ref` lifecycle boundary across core
environment variants, the existing INT02 macro/declaration parity regression without manual
recomputation, and public `liquers-lib::DefaultEnvironment` behavior. The only planned validation
row not run to completion is default-feature `liquers-lib` validation, which is already blocked by
`LIB-POLARS-ETHNUM-RUST-1-98-BROKEN`; the no-default and wasm-webui rows passed.

## Documentation Delivered

### New Reference Documents

None. The capability is a lifecycle correction best anchored in existing command and environment
references.

### New Guide Documents

None. Existing command-registration and unit-testing guides now carry the needed task guidance.

### Existing Documents Reviewed or Updated

Authoritative `affects_docs`:

- `specs/reference/COMMAND_DECLARATION.md` - reviewed and updated to state that
  `metadata_version` is refreshed by `Environment::to_ref` after registration-time mutation.
- `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` - reviewed and updated to include
  `refresh_metadata_versions` in the environment initialization flow and to distinguish `EnvRef::new`
  from the full lifecycle.
- `specs/guides/COMMAND_REGISTRATION_GUIDE.md` - reviewed and updated to tell command authors to
  customize metadata before `env.to_ref()` and not treat pre-`to_ref` versions as final.
- `specs/guides/UNITTEST_GUIDE.md` - reviewed and updated because it had stale `to_ref()` setup
  guidance.

### Links and Capability Map

`specs/index.csv` and generated `specs/README.md` blocks were regenerated. No manual capability-map
entry was needed beyond the generated design and issue rows.

## Issues Filed

None. The default `liquers-lib` Polars validation failure is already tracked by
`LIB-POLARS-ETHNUM-RUST-1-98-BROKEN`.

## Important Learning

`Environment::to_ref` is the sound refresh point because it still owns the environment mutably and
is already the boundary that all normal code crosses before sharing an `EnvRef`. The future
`environment-builder` design only needs to preserve that invariant: delegate through refreshed
`to_ref`, or call the same registry lifecycle operation before manager startup if it bypasses
`to_ref`.

`liquers-py` has runtime-incomplete methods, but its crate compiles and is a default workspace
member, so compile compatibility is a real gate for trait changes. For `liquers-lib` wasm
validation, use `--no-default-features --features webui`: default features pull Polars, while
no-feature wasm omits the wasm UI dependencies used by the crate.

## Conformance and Remaining Work

The implementation matches the user request and approved design: the method is named
`refresh_metadata_versions`, it is called when the command metadata registry is finalized by
`Environment::to_ref`, and the builder design carries an explicit invariant note. No requested
scope remains in this design.

Post-share dynamic command registration remains out of scope and belongs to a future environment
builder or runtime-registration design. The existing Polars build failure remains tracked separately.

## Validation

- `cargo fmt` - passed
- `cargo check -p liquers-core` - passed
- `cargo check -p liquers-py` - passed
- `cargo check -p liquers-lib --no-default-features` - passed
- `cargo test -p liquers-core refresh_metadata_versions` - passed
- `cargo test -p liquers-core update_all_metadata_versions_delegates_to_refresh_metadata_versions` - passed
- `cargo test -p liquers-core to_ref_refreshes_metadata_versions` - passed
- `cargo test -p liquers-core --test command_declaration int02_declaration_and_macro_agree_including_metadata_version` - passed
- `cargo test -p liquers-core` - passed, 732 lib tests plus integration and doc tests
- `cargo test -p liquers-lib --no-default-features default_environment_to_ref_refreshes_macro_metadata_versions` - passed
- `cargo test -p liquers-lib --no-default-features --lib --tests` - passed
- `cargo check -p liquers-lib --target wasm32-unknown-unknown --no-default-features --features webui` - passed
- `cargo check -p liquers-lib` - blocked by existing `LIB-POLARS-ETHNUM-RUST-1-98-BROKEN`
- `python3 scripts/docs_index.py --check` - passed with existing repository warnings
