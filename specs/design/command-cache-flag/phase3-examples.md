# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit | `CommandMetadata` JSON contains no `"cache"` (updated serialized-form test) |
| T2 | unit | JSON containing `"cache": false` deserializes (`command_metadata_ignores_legacy_cache_field`) |
| T3 | integration | `cargo test -p liquers-lib --test registry_export` after regeneration |
| T4 | build | `cargo check -p liquers-py`; `cargo check -p liquers-lib --features egui`; build matrix |
