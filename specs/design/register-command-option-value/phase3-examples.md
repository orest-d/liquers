# Phase 3: Examples and Tests

Integration tests in `liquers-core/tests/register_command_option.rs` (environment as in
`async_hellow_world.rs`), each command returning its argument formatted with `{:?}`:

| Test | Query | Expected | Proves |
|---|---|---|---|
| `option_i64_absent_and_present` | `opt_i`, `opt_i-5` | `None`, `Some(5)` | 1 |
| `option_i64_default_on_empty` | `opt_i_d` (declared `= 7`) | `Some(7)` | 1 |
| `option_widths_register` | registers `Option<i8…usize, f32, f64>` | each `ArgumentType` is `IntegerOption`/`FloatOption` | 1 |
| `option_bool_spellings` | `opt_b-t`, `opt_b-TRUE`, `opt_b-yes`, `opt_b-y`, `opt_b-1`, `opt_b-f`, `opt_b-No`, `opt_b-n`, `opt_b-0`, `opt_b-none`, `opt_b-NONE`, `opt_b` | `Some(true)`×5, `Some(false)`×4, `None`×3 | 2 |
| `option_bool_default_on_empty` | `opt_b_d` (declared `= true`) | `Some(true)` | 2 |
| `option_bool_rejects_other_text` | `opt_b-maybe` | conversion error | 2 |
| `option_from_link_none_and_value` | a link resolving to none / to `5` | `None` / `Some(5)` | 3 |
| `bool_opt_serializes` (unit, `command_metadata.rs`) | `ArgumentType::BooleanOption` ↔ `"bool_opt"` | round-trip | 2 |

Macro unit tests (`registration.rs`): `option_unsupported_inner_is_rejected` (`Option<Value>`,
`Option<String>`, `Option<Vec<u8>>`; message contains the type and `String = ""`) — 4;
`option_f64_maps_to_float_option` (replaces the `FloatOpt` expectation) — 1.

Existing registrations still compile: `cargo test -p liquers-lib --lib --tests --no-run` — 5.

Queries validated with `liquers-validate --command opt_i --command opt_b` (bare actions and
one-parameter actions).
