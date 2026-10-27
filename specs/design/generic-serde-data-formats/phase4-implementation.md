# Phase 4: Implementation Plan — generic serde data formats

## Overview

Build the core helper first, so it can be tested alone. Then add the declarations and the core
codec, then `SimpleValue`, then the feature plumbing and the matrix, then the documents. This
assumes Phase 1's recommended answers, and that `data-format-vocabulary` has landed. If it has
not, step 8 adds the two vocabulary rows to that design's table when it does land.
`core-value-json-identifier-dispatch` should land first. If it has not, step 5 writes the
identifier table it specifies and that design rebases.

## Progress

- [ ] Step 1: core dependencies and features
- [ ] Step 2: `serde_format` module
- [ ] Step 3: `json_f64` native floats in binary formats
- [ ] Step 4: `TypeInfo::with_serde_formats` and core declarations
- [ ] Step 5: core `Value` codec branches
- [ ] Step 6: `SimpleValue` codec branches, `ShapeRead`, `ShapeWrite`
- [ ] Step 7: `liquers-lib` feature and `RecordSource` manifest arms
- [ ] Step 8: vocabulary rows and the build matrix
- [ ] Step 9: integration tests
- [ ] Step 10: documents and records
- [ ] Step 11: full checks and diff review

## Implementation Steps

### Step 1: core dependencies and features
- Files / symbols: `liquers-core/Cargo.toml`. Add `ciborium` (optional), `rmp-serde` (optional)
  and `serde_bytes`. Add features `cbor = ["dep:ciborium"]` and `msgpack = ["dep:rmp-serde"]`,
  each with a comment explaining why it is not in `default`.
- Depends on: none.
- Proof: `cargo check -p liquers-core` and `cargo check -p liquers-core --features cbor,msgpack`.
- Rollback: revert the manifest.

### Step 2: `serde_format` module
- Files / symbols: `liquers-core/src/serde_format.rs` (new): `SerdeFormat` and its methods,
  `generic_serde_formats`, `generic_serde_format`, `to_bytes`, `from_bytes`, `bytes_to_bytes`,
  `bytes_from_bytes`. Add `pub mod serde_format;` to `lib.rs`. Matches over `SerdeFormat` are
  exhaustive. A disabled format returns the feature error built with `Error::from_error`.
- Depends on: step 1.
- Proof: Phase 3 tests 3 and 4.
- Rollback: delete the module.

### Step 3: native floats in binary formats
- Files / symbols: `liquers-core/src/value.rs` `json_f64::serialize`, which branches on
  `serializer.is_human_readable()`. Update its doc comment.
- Depends on: none.
- Proof: Phase 3 test 10. The existing `json_f64_*` tests stay unchanged.
- Rollback: revert the function.

### Step 4: declarations
- Files / symbols: `liquers-core/src/type_system.rs` `TypeInfo::with_serde_formats`. In
  `liquers-core/src/value.rs` `Value::type_descriptions`, every core identifier gets
  `.with_serde_formats()`, and the comment there is updated.
- Depends on: step 2.
- Proof: Phase 3 test 7 (it fails until step 5 lands, so land steps 4 and 5 in one commit).
- Rollback: revert.

### Step 5: core codec
- Files / symbols: in `liquers-core/src/value.rs`, add private `Value::serde_payload` and
  `Value::serde_payload_read`, plus the early branch in both `DefaultValueSerializer` methods.
  `Bytes` uses `bytes_to_bytes` / `bytes_from_bytes`.
- Depends on: steps 2 and 4.
- Proof: Phase 3 tests 5-10, run with and without `--features cbor,msgpack`.
- Rollback: revert the branches. Nothing stored depends on them before release.

### Step 6: `SimpleValue`
- Files / symbols: in `liquers-lib/src/value/simple.rs`, add the early branches, private
  `ShapeRead` (a `Deserialize` with a `deserialize_any` visitor) and `ShapeWrite` (a `Serialize`
  used for YAML `Array`/`Object`), and remove the `"yaml" | "yml"` arm. Adjust the existing YAML
  tests only where they assert the route rather than the value.
- Depends on: step 5.
- Proof: Phase 3 tests 11 and 12; `cargo test -p liquers-lib --lib value::simple`.
- Rollback: revert the file.

### Step 7: library feature and manifest arms
- Files / symbols: in `liquers-lib/Cargo.toml`, add `serde-binary` and put it in `default`. In
  `liquers-lib/src/value/mod.rs`, `ExtValue::as_bytes` / `deserialize_from_bytes` route the
  `RecordSource` arms through `serde_format::{to_bytes, from_bytes}` for `yaml` / `json`. No other
  variant changes.
- Depends on: step 2.
- Proof: the existing `RecordSource` manifest tests; `cargo test -p liquers-lib --no-default-features --features records --lib --tests`.
- Rollback: revert.

### Step 8: vocabulary rows and matrix
- Files / symbols: `liquers-core/src/data_format.rs` (`CBOR`, `MSGPACK`, two table rows), and
  `serde_format` uses those constants. `scripts/check-build-matrix.sh` gets a `CORE_CONFIGS` row
  with `--features cbor,msgpack --tests` and a `LIB_CONFIGS` wasm32 row with
  `--features webui,serde-binary`.
- Depends on: steps 2 and 7, and `data-format-vocabulary`.
- Proof: that design's `every_declared_core_format_is_known` and
  `every_declared_lib_format_is_known`; `bash scripts/check-build-matrix.sh`.
- Rollback: revert the rows.

### Step 9: integration tests
- Files / symbols: `liquers-lib/tests/generic_serde_formats.rs` (new). It holds Phase 3 tests 13
  and 14, and uses an `AsyncMemoryStore` environment as `liquers-core/tests/async_hellow_world.rs`
  does.
- Depends on: steps 5-7.
- Proof: `cargo test -p liquers-lib --test generic_serde_formats`.
- Rollback: delete the file.

### Step 10: documents and records
- Files / symbols: the three documents in the Phase 2 documentation table, each with a History
  row and a new `reviewed:` date. Close both source issues under `DOCS_STRUCTURE_GUIDE.md` §4.3.
  Set the design's `status`. Regenerate `specs/index.csv` with `python3 scripts/docs_index.py`.
- Depends on: steps 1-9.
- Proof: `python3 scripts/docs_index.py --check`.
- Rollback: revert the documents.

### Step 11: full checks
- Run `cargo test -p liquers-core --lib --tests`, the same with `--features cbor,msgpack`,
  `cargo test -p liquers-lib --lib --tests` and `bash scripts/check-build-matrix.sh`. Then review
  the diff: no `println!`, `unwrap` or `_ =>` over Liquers enums, and no change to JSON bytes.

## Testing Plan

Unit tests live in `serde_format.rs`, `value.rs` and `simple.rs`, and one integration file covers
the asset path (Phase 3 table). The feature matrix proves AC-6. The existing JSON suites prove
AC-9.

## Rollback Plan

Each step is its own commit. Turning `serde-binary` off removes the binary formats from
`liquers-lib` builds without a code change, and reverting steps 4-6 removes YAML. Nothing
persisted before release depends on the new formats.

## Documentation Updates

`specs/guides/SERIALIZATION_GUIDE.md`, `specs/reference/VALUE_TYPE_SYSTEM.md` and
`specs/guides/TYPE_SYSTEM_GUIDE.md`, each with a History row and `reviewed:`. Both source issues
are closed.

## Phase 5 Entry Criteria

All Progress items are ticked, the Phase 3 tests pass in both core feature states, the build
matrix (including wasm32) passes, `docs_index.py --check` is clean, and the decisions in Phase 1
are recorded as made.
