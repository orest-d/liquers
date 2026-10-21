# Phase 3: Examples & Use-cases — value-interface-capability-split

No query changes: the design is a Rust trait refactor. Examples are Rust; every test is runnable.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A new extension variant writes two methods | the implementor's view | AC-1, AC-5 |
| 2 | Example | A foreign value states its facts once | the integration's view | AC-2 |
| 3 | Test | `liquers_core::value::tests::value_type_defaults_come_from_the_description` | every core variant | AC-1 |
| 4 | Test | `liquers-lib/tests/value_type_system.rs` `ext_value_defaults_come_from_the_description` | every sampled `ExtValue` and its combined value | AC-1, AC-5 |
| 5 | Test | `liquers-lib/tests/foreign_value_registration.rs` `foreign_value_states_identity_and_description_once` | mock foreign value | AC-2 |
| 6 | Test | `liquers_core::state::tests::state_metadata_carries_type_identifier` | `State` writes `type_identifier()` | AC-3 |
| 7 | Test | existing suites (listed below) | no behaviour change | AC-4 |

## Example 1: a new extension variant

```rust
impl ValueType for ExtValue {
    fn type_identifier(&self) -> Cow<'static, str> {
        match self {
            ExtValue::Image { .. } => "Image".into(),
            // … one arm per variant; `Foreign { value } => value.type_identifier()`
        }
    }
    fn type_info(&self) -> TypeInfo {
        match self {
            ExtValue::Foreign { value } => value.type_info(),
            ExtValue::Image { .. } /* | every other variant */ => {
                TypeInfo::find(ext_descriptions(), &self.type_identifier())
            }
        }
    }
    fn type_descriptions() -> Vec<TypeInfo> { ext_descriptions().to_vec() }
}
```

`value.default_filename()` on an image is `"image.png"` because the `Image` description says so;
there is no `default_filename` arm to forget. Changing the description to
`.with_defaults("webp", "webp", "image/webp", "image.webp")` changes `default_extension()`,
`default_filename()`, and the same methods on `liquers_lib::value::Value::from(ext)` (AC-5).

## Example 2: a foreign value

```rust
impl ValueType for PyObjectValue {
    fn type_identifier(&self) -> Cow<'static, str> { PY_OBJECT_TYPE_IDENTIFIER.into() }
    fn type_info(&self) -> TypeInfo { py_type_info(self.pickle).with_type_name(self.type_name.clone()) }
    fn type_name(&self) -> Cow<'static, str> { self.type_name.clone().into() }
}
// `default_extension()` is "pickle", read from `py_type_info`; no second spelling exists.
```

## Edge and Error Cases

- **Undescribed identifier** (a downstream extension describing nothing, e.g. `MatrixExt`):
  `TypeInfo::find` returns the minimal description; `default_extension()` is `bin`. Covered by a
  unit test in `type_system.rs`, `find_falls_back_to_a_minimal_description` (AC-1).
- **Feature-gated variants:** the derived methods have no arms, so a gated variant is covered in
  every configuration; `scripts/check-build-matrix.sh` proves the remaining matches (AC-4).
- **Bindings:** `liquers-py`'s existing `py_type_info_agrees_with_instance` test (`liquers-py/src/value.rs`
  tests module) and `liquers-web`'s `second_value_type` suite must pass unchanged in assertion
  (AC-2, AC-4).

## Test Plan

- Unit: `liquers-core/src/value.rs` — `value_type_defaults_come_from_the_description` (AC-1): for
  one sample of each of the 15 variants, `type_name`/`default_*`/`supports_data_format("json")`
  equal `type_info()`'s fields and `type_info()` equals `TypeRegistry::from_value_type::<Value>()`'s entry.
- Unit: `liquers-core/src/type_system.rs` — `find_falls_back_to_a_minimal_description` (AC-1).
- Unit: `liquers-core/src/state.rs` — `state_metadata_carries_type_identifier` (AC-3):
  `State::new().with_data(Value::from(3i64))` has `metadata.type_identifier() == "I64"`, equal to
  `value.type_identifier()`.
- Integration: `liquers-lib/tests/value_type_system.rs` — `ext_value_defaults_come_from_the_description`
  (AC-1, AC-5): iterates `samples()`, checks each `ExtValue` and `Value::from(ext)` the same way.
- Integration: `liquers-lib/tests/foreign_value_registration.rs` —
  `foreign_value_states_identity_and_description_once` (AC-2): `MockForeign` implements only the
  three `ValueType` methods; its defaults equal the registered description.
- Regression (AC-4): `cargo test -p liquers-core`, `cargo test -p liquers-lib --lib --tests`,
  `bash scripts/test-liquers-py.sh`, `cargo test -p liquers-web --target wasm32-unknown-unknown
  --features debug-handles` (after `cargo clean`), `bash scripts/check-build-matrix.sh`.

## Learning Log

- The three traits grew their copies independently; `ForeignValue`'s doc already explains that a
  default body cannot call an associated function (`Self: ?Sized`) — which is why `type_info` is
  required rather than defaulted.
