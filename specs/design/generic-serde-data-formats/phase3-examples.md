# Phase 3: Examples & Use-cases — generic serde data formats

Queries were checked with `./target/debug/liquers-validate` on 2026-10-26. Each one plans as
`GetAsset[data, params.json]` followed by a `Filename` step.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A JSON resource saved as YAML and as CBOR | query-level use | AC-1, AC-3 |
| 2 | Example | A hand-written YAML file with `.nan` | shape read keeps NaN | AC-2 |
| 3 | Test | `liquers_core::serde_format::tests::from_data_format_maps_spellings` | `yml`, refinements refused | AC-4 |
| 4 | Test | `liquers_core::serde_format::tests::disabled_format_names_its_feature` (`#[cfg(not(feature = "cbor"))]`) | the refusal message | AC-6 |
| 5 | Test | `liquers_core::value::tests::generic_formats_round_trip_every_core_identifier` | every identifier × every enabled generic format | AC-3, AC-4 |
| 6 | Test | `liquers_core::value::tests::nested_nan_survives_binary_and_yaml` | `[1.0, NaN, None]` in `cbor`, `msgpack`, `yaml` | AC-3 |
| 7 | Test | `liquers_core::value::tests::every_declared_generic_pair_is_writable` | declarations equal codec capability | AC-4 |
| 8 | Test | `liquers_core::value::tests::cbor_is_compact` (`cbor`) | sizes against JSON | AC-7 |
| 9 | Test | `liquers_core::value::tests::untyped_cbor_reads_by_shape` (`cbor`) | byte string → `Bytes`, map → `Object` | AC-8 |
| 10 | Test | `liquers_core::value::tests::json_f64_is_native_in_binary_formats` | CBOR holds a float, JSON still holds the token | AC-3, AC-9 |
| 11 | Test | `liquers_lib::value::simple::tests::generic_formats_round_trip_every_identifier` | `SimpleValue`, including tagged nested values | AC-3, AC-4 |
| 12 | Test | `liquers_lib::value::simple::tests::yaml_shape_read_keeps_nan` | replaces the `serde_json` route; YAML written by `ShapeWrite` is plain and reads back by shape | AC-1, AC-2 |
| 13 | Test | `liquers-lib/tests/generic_serde_formats.rs` `yaml_and_cbor_written_through_a_query` | store round trip through the asset layer | AC-1, AC-3 |
| 14 | Test | `liquers-lib/tests/generic_serde_formats.rs` `opted_out_types_refuse_generic_formats` | Image, DataFrame, RecordView | AC-5 |
| 15 | Test | existing JSON, `json_f64_*`, `vts7.*` and `SimpleValue` YAML tests | unchanged | AC-9 |

## Example 1: a JSON resource saved as YAML and CBOR

Store `data/params.json` = `{"ratio": 0.5, "names": ["a", "b"]}` with no type identifier, then
evaluate:

```
-R/data/params.json/-/params.yaml
-R/data/params.json/-/params.cbor
```

The first result has metadata `data_format: yaml`, `type_identifier: Object`, and its bytes are
`names:\n- a\n- b\nratio: 0.5\n`, which is plain YAML. The second result has
`data_format: cbor`, holds the tagged payload, and reading it back through the store gives the
same `Object`, including element types. With `liquers-lib` built `--no-default-features`, the second query is refused when it
is written: `Object` does not declare `cbor` in that build.

## Example 2: YAML with a non-finite float

`config/limits.yaml` holds:

```yaml
ratio: .nan
max: .inf
```

`-R/config/limits.yaml` gives `Object { ratio: F64(NaN), max: F64(inf) }`. At HEAD it gives
`ratio: None`, `max: None`.

## Edge and Error Cases

- A refined spelling such as `yaml:x` is not a generic format and is refused, as an unknown
  format is today.
- `Text` written as `msgpack` is a MessagePack string, and reading it with identifier `Bytes`
  fails as a payload mismatch, so the asset is treated as corrupted (test 5 includes a mismatched
  case).
- A `cbor` file whose identifier is `polars.DataFrame` is refused by the extension decoder
  (test 14).
- `Value::None` as YAML is `null`, and as CBOR it is `null` (`0xf6`).
- `AssetInfo` with 1 000 entries: CBOR round-trips equal (test 5). Size is not asserted, because
  the content is mostly strings.

## Test Plan

```bash
cargo test -p liquers-core --lib serde_format value::tests
cargo test -p liquers-core --features cbor,msgpack --lib serde_format value::tests
cargo test -p liquers-lib --lib value::simple
cargo test -p liquers-lib --test generic_serde_formats
cargo test -p liquers-lib --no-default-features --lib --tests      # binary formats absent
bash scripts/check-build-matrix.sh
```

Tests 4 and 8-10 carry the feature `cfg` they need. Test 5 iterates over
`generic_serde_formats()`, so it covers whatever the build enables.
