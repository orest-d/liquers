# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | integration (`liquers-lib/tests/register_all_commands.rs`, new, ungated) | Defines `type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;` and calls `liquers_lib::register_all_commands!(cr)?`. It passes in every configuration. Asserts `core` commands exist; asserts `ns-pl` commands exist only `#[cfg(feature = "polars")]`, and so on per feature. |
| T2 | matrix | `bash scripts/check-build-matrix.sh` compiles T1's target in each configuration |
| T3 | optional | `registry_export.rs` uses the macro and still matches the committed registry (default features) |

Run: `cargo test -p liquers-lib --no-default-features --test register_all_commands` and the same
with each `--features X` from CLAUDE.md's list.
