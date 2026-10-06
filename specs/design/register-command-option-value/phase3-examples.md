# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| E1 | compile error | `register_command!(cr, fn f(state, schema: Option<Value>) -> result)` → error message contains `Option<Value>` and `String = ""` |
| E2 | ok | `fn g(state, n: Option<i32>) -> result` compiles (unchanged) |
| E3 | ok (if accepted) | `fn h(state, label: Option<String>) -> result`; query `h` → `None`; query `h-abc` → `Some("abc")` |
| T1 | unit (`liquers-macro`) | Parser rejects `Option<Value>`, `Option<Vec<u8>>`; accepts the numeric set |
| T2 | compile-fail (if `trybuild` present in dev-deps; check `liquers-macro/Cargo.toml`) | E1 |
| T3 | integration (if accepted) | E3 through `envref.evaluate(&parse_query("h-abc")?)` |

Validate E3's queries with `liquers-validate --command h`: `h` and `h-abc` are a bare action and
an action with one parameter.
