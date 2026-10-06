# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| E1 | example | `register_command!(cr, fn wrap(state, width: i32 = 80 (label: "Width", description: "Maximum line width in characters")) -> result)` → `metadata.arguments[0].description == "Maximum line width in characters"` |
| T1 | unit (core) | serde: `ArgumentInfo` with empty description serializes without the key; with text, round-trips JSON and YAML |
| T2 | unit (core) | Deserializing a registry entry without `description` gives empty |
| T3 | macro test (`liquers-macro` or `liquers-lib/tests`) | E1 compiles and stores the description |
| T4 | macro compile-fail (if the crate has trybuild tests) | `description: 3` (non-string) is rejected |
| T5 | regression | `cargo test -p liquers-lib --test registry_export` passes without regenerating |

Test names: `argument_description_omitted_when_empty`, `argument_description_round_trips`,
`register_command_sets_argument_description`.
