# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | test | `committed_registry_impl_versions_are_fresh` passes at HEAD (verified fresh on 2026-10-06) |
| T2 | manual negative | Add a comment inside the body of an `auto`-versioned command (e.g. `liquers-lib/src/commands.rs` `command_metadata`) → T1 fails naming `dep/command_metadata`; regenerate → passes; revert |
| T3 | manual negative | Temporarily switch one command to `version: now` → T1 fails with the `now` message; revert |

Run with default features: `cargo test -p liquers-lib --test registry_export`.
