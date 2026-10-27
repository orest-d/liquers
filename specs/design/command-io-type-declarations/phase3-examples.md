# Phase 3: Examples & Use-cases — commands declare the value types they accept and produce

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Declaring with the macro | statements, attribute | AC-1, AC-6 |
| 2 | Example | Validating an ill-typed pipeline | plan warning | AC-3 |
| 3 | Test | `liquers_core::type_system::tests::type_set_serializes_as_string_or_list` | shapes, round trip, refusal | AC-1 |
| 4 | Test | `liquers_core::command_metadata::tests::declared_types_round_trip` | JSON and YAML | AC-1, AC-6 |
| 5 | Test | `…::command_metadata::tests::undeclared_metadata_is_unchanged` | bytes and version | AC-2 |
| 6 | Test | `liquers_core::plan::tests::type_mismatch_is_a_plan_warning` | warning text, plan still built | AC-3 |
| 7 | Test | `…::plan::tests::resource_input_adds_no_type_warning` | unknown type | AC-4 |
| 8 | Test | `…::plan::tests::matching_or_aliased_types_add_no_warning` | overlap, alias copies `produces` | AC-5 |
| 9 | Test | `…::command_metadata::tests::check_declared_types_reports_unknown` | error issue | AC-7 |
| 10 | Test | `…::command_metadata::tests::commands_accepting_skips_undeclared` | listing | AC-8 |
| 11 | Test | `liquers_macro::registration::tests::accepts_and_produces_statements` | expansion, compile errors | AC-1, AC-6 |
| 12 | Test | `liquers-lib/tests/declared_command_types.rs::lib_declared_types_are_registered` | real commands | AC-7 |
| 13 | Test | `…::declaration_path_reads_accepts` | `CommandDeclaration` | AC-6 |
| 14 | Test | `liquers-lib/tests/registry_export.rs` (existing) | regenerated file fresh | AC-1 |

## Example 1: declaring with the macro

```rust
fn tag_rows(state: &State<Value>, lookup: Value) -> Result<Value, Error> { /* … */ }

register_command!(cr,
    fn tag_rows(state, lookup: Value (accepts: "RecordView")) -> result
    accepts: ["polars.DataFrame", "RecordView"]
    produces: "polars.DataFrame"
)?;
let cm = cr.command_metadata_registry.find_command("", "", "tag_rows").unwrap();
assert_eq!(cm.state_argument.as_ref().unwrap().accepts.principal(), Some("polars.DataFrame"));
assert_eq!(cm.arguments[0].accepts, TypeSet::of("RecordView"));
```

Serialized (YAML): `state_argument: {name: state, …, accepts: [polars.DataFrame, RecordView]}`,
`arguments: [{name: lookup, …, accepts: RecordView}]`, `produces: polars.DataFrame`.

## Example 2: validating an ill-typed pipeline

```
$ liquers-validate --detail summary -- '-R/data/sales.csv/-/ns-pl/head-5/ns-img/resize-100-100'
status: Warning
warnings: ["Type mismatch: 'img/resize' accepts Image, but 'pl/head' produces polars.DataFrame"]
```

(The query validates `Ok` today; checked with the prebuilt `liquers-validate`.) The same query with
`ns-pl/head-5` removed (`-R/data/photo.png/-/ns-img/resize-100-100`) stays `Ok`: AC-4.

## Edge and Error Cases

- `accepts: []` or `accepts: ["Image", "Image"]` in the macro: compile error / registration error.
- A deserialized `accepts: ""`: deserialization error naming the field.
- A source command (`state_argument: null`) with `accepts:` in the macro: compile error;
  `with_accepts` on it is a no-op (documented), and `accepts_input` returns `None`.
- `produces` declared, next command undeclared: no warning, and `flowing` becomes `None` after it.
- A `q`-terminated query or a `-R` header (`meta`, `dir`): `flowing` reset to `None`; no warning.
- A link argument whose query produces a disjoint type: no warning in this design (non-goal).
- Undeclared metadata read by an older binary: unchanged bytes (AC-2).

## Test Plan

- `cargo test -p liquers-core --lib type_system command_metadata plan` — tests 3-10.
- `cargo test -p liquers-macro` — test 11 (expansion and `compile_fail` cases the crate already uses).
- `cargo test -p liquers-lib --test declared_command_types --test registry_export` — tests 12-14.
- `bash scripts/check-build-matrix.sh` — the declarations under `polars` / `records` / `image-support`.
- `./target/debug/liquers-validate --detail summary -- '<Example 2 query>'` after rebuilding.
