# Phase 3: Examples & Use-cases — a type registry that describes every realm

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A page learns the server's types | export, import, missing-type listing | AC-1, AC-3, AC-5 |
| 2 | Example | Declaring boundary behaviour | `with_transfer`, inference | AC-4 |
| 3 | Test | `liquers_core::type_system::tests::registry_round_trips_as_a_list` | JSON and YAML round trip; list shape | AC-1 |
| 4 | Test | `…::tests::duplicate_in_serialized_registry_is_refused` | `try_from` reuses `register` | AC-2 |
| 5 | Test | `…::tests::import_realm_relabels_default_entries` | relabel, default realm untouched, named entries skipped | AC-3 |
| 6 | Test | `…::tests::import_realm_clash_applies_nothing` | atomic failure; import into `""` refused | AC-3 |
| 7 | Test | `…::tests::transfer_mode_defaults_and_inference` | default, explicit, no-format inference | AC-4 |
| 8 | Test | `…::tests::types_not_in_lists_realm_gaps` | gap listing over three realms | AC-5 |
| 9 | Test | `…::tests::type_info_without_transfer_field_reads` | backward-compatible input | AC-6 |
| 10 | Test | `liquers_web::value::tests::js_value_is_untransferable` | the web entry | AC-7 |

## Example 1: a page learns the server's types

The server serializes its registry; the page imports it before building its environment.

```rust
// server side
let server = TypeRegistry::from_value_type::<liquers_lib::value::Value>();
let yaml = serde_yaml::to_string(&server)?;          // a YAML list of TypeInfo

// page side
let mut types = TypeRegistry::from_value_type::<Value>();
types.register(js_value_type_info())?;
types.import_realm("server", &serde_yaml::from_str::<TypeRegistry>(&yaml)?)?;

assert!(types.get_in_realm("server", "polars.DataFrame").is_some());
assert!(types.get("polars.DataFrame").is_none());     // the page itself still cannot hold one
let gaps: Vec<_> = types.types_not_in("").iter().map(|t| t.type_identifier.clone()).collect();
assert!(gaps.iter().any(|id| id == "polars.DataFrame"));
let env = EnvironmentBuilder::new().with_type_registry(types) /* … */;
```

## Example 2: declaring boundary behaviour

```rust
TypeInfo::new("polars.DataFrame")
    .with_defaults("parquet", "parquet", "application/vnd.apache.parquet", "data.parquet")
    .with_data_formats(["parquet", "csv"])
    .with_transfer(TransferPolicy::ConvertTo {
        type_identifier: "Text".into(), data_format: "csv".into(),
    });
// js.Value: no formats → transfer_mode() == Untransferable even without with_transfer
```

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| Serialized input lists `("", "Text")` twice | `Err`, message names `Text` | 4 |
| Import clashes with an existing `("server", "Text")` | `Err`; registry unchanged (len and entries) | 6 |
| `import_realm("", …)` | `Err` "cannot import into the default realm" | 6 |
| `other` holds `("browser", "js.Value")` | skipped, not relayed as `server` | 5 |
| `TypeInfo` JSON without `transfer` | reads, `transfer == Serialize` | 9 |
| `Serialize` with empty `supported_data_formats` | `transfer_mode() == Untransferable` | 7 |

## Test Plan

- Unit: `liquers-core/src/type_system.rs` `mod tests` — tests 3-9, each `-> Result<(), Box<dyn
  std::error::Error>>` like the existing `vts4.*` tests, using the existing `text_info()` helper
  and `serde_json` / `serde_yaml` (already dependencies of `liquers-core`).
- Unit (wasm): `liquers-web/src/value.rs` `mod tests` — test 10, a `#[wasm_bindgen_test]` beside
  `the_registered_type_has_no_byte_form`, asserting `js_value_type_info().transfer_mode() ==
  TransferPolicy::Untransferable` and `.transfer == TransferPolicy::Untransferable` (explicit, not
  only inferred).
- Commands: `cargo test -p liquers-core --lib type_system`;
  `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` (after
  `cargo clean`, per `CLAUDE.md`).

## Learning Log

- No `TypeInfo` struct literal exists outside `type_system.rs` (checked with `grep "TypeInfo {"`),
  so a new field is safe for every crate.
- `TypeRegistry`'s own derive was deliberately withheld by `foreign-value-type-registration`
  Phase 2 for this design; the list form was its stated expectation.
