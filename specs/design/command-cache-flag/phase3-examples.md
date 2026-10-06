# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit | `CommandMetadata` JSON no longer contains `"cache"` (update the existing serialized-form test) |
| T2 | unit | Deserializing JSON containing `"cache": false` succeeds (old declarations still load) |
| T3 | integration | `cargo test -p liquers-lib --test registry_export` passes after regeneration |
| T4 | build | `cargo check -p liquers-py`; `cargo check -p liquers-lib --features egui` |
| E1 | upgrade note | A stored computed asset from before the change is recomputed once after upgrade (stated in the issue resolution, not tested) |

Name for T2: `command_metadata_ignores_legacy_cache_field`.
