# Phase 3: Examples & Use-cases — value-type-definition-macro

No query is involved: the macro changes how `ExtValue` is written, not what queries do.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | `polars.DataFrame` declared | a gated variant with codecs and accessor | AC-1, AC-3 |
| 2 | Example | `Foreign` declared | the delegating variant | AC-4 |
| 3 | Test | `liquers_macro::value_types::tests::parses_full_variant` | parser builds the model | AC-1 |
| 4 | Test | `liquers_macro::value_types::tests::rejects_bad_declarations` | duplicate id, missing id, unknown key, non-`Arc` accessor | AC-6 |
| 5 | Test | `liquers-lib/tests/value_type_system.rs` (existing suite) | descriptions complete, no stale entries, routing | AC-2, AC-3 |
| 6 | Test | `liquers-lib/tests/value_type_definition.rs` `foreign_json_hook_is_delegated` | mock foreign value returning JSON | AC-4 |
| 7 | Test | `liquers-lib/tests/value_type_definition.rs` `rust_types_name_their_identifiers` | `to_type_identifier` per `rust_type` | AC-5 |
| 8 | Check | `grep -c 'match self' liquers-lib/src/value/mod.rs` is 0 | no hand-written `ExtValue` match left | AC-1 |
| 9 | Check | `bash scripts/check-build-matrix.sh` | every feature row compiles and its tests run | AC-2, AC-3 |

## Example 1: a gated variant

```rust
#[cfg(feature = "polars")]
PolarsDataFrame(Arc<polars::frame::DataFrame>) {
    cfg: feature = "polars",
    identifier: "polars.DataFrame", type_name: "polars_dataframe",
    defaults: ("csv", "csv", "text/csv", "data.csv"),
    formats: ["csv", "csv:comma", "csv_comma", "parquet"],
    rust_type: polars::frame::DataFrame,
    accessor: polars_dataframe,
    encode: hooks::encode_polars, decode: hooks::decode_polars,
    data_node: hooks::polars_node,
},
```

With `--no-default-features --features records` the variant, its arms, its `TypeInfo` and its
`TypeIdentifiedIn` impl are absent; with `polars` they are present, and
`to_type_identifier::<ExtValue, polars::frame::DataFrame>()` is `"polars.DataFrame"`.

## Example 2: the foreign variant

```rust
Foreign(Arc<dyn crate::value::foreign::ForeignValue>) { foreign },
```

Every identity, description, string, JSON and encode arm calls the payload; decoding goes through
`foreign::foreign_decoder`. A mock whose `try_into_json_value` returns `{"a": 1}` is now returned
by `Value::from(ExtValue::Foreign { value }).try_into_json_value()` (today: a conversion error).

## Edge and Error Cases

- **Declaration errors** → `compile_error!` naming variant and key (test 4, AC-6).
- **Undeclared hooks** → today's refusal text, so error-message assertions in existing tests hold
  (test 5, AC-2).
- **`deserialize_from_bytes` with an unknown identifier** → foreign decoder, then the existing
  "Unsupported type identifier" error (existing `extended.rs` routing tests, AC-2).
- **wasm32** → `check-build-matrix.sh` wasm rows (AC-3).

## Test Plan

- Unit: `liquers-macro/src/value_types.rs` — `parses_full_variant` (AC-1): the model holds every key
  of Example 1; `rejects_bad_declarations` (AC-6): four inputs, each `parse` returns an error whose
  message names the variant and key.
- Integration: `liquers-lib/tests/value_type_definition.rs` (new) — `foreign_json_hook_is_delegated`
  (AC-4); `rust_types_name_their_identifiers` (AC-5): `Image` and, under `polars`,
  `polars.DataFrame`, both for `ExtValue` and for `liquers_lib::value::Value`.
- Regression: `cargo test -p liquers-macro`; `cargo test -p liquers-lib --lib --tests` (all value,
  record, polars, image and foreign suites, AC-2); `bash scripts/check-build-matrix.sh` (AC-3).

## Learning Log

- At HEAD every non-trivial `ExtValue` arm is either a codec, "read as the base value" (record
  views) or delegation (foreign); nothing needs arbitrary code in the macro input.
