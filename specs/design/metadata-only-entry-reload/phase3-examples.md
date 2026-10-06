# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit | Memory store (after companion change): metadata-only `Ready` entry → `try_fast_track()` is `Ok(false)`, payload cleared |
| T2 | unit | File store (`AsyncFileStore` on a temp dir): same, and a keyed `get` through the manager recomputes from the recipe |
| T3 | unit | Corrupt non-empty bytes → still `Ok(false)` via the corruption branch (existing test stays) |
| T4 | integration | `set_state` of a non-serializable value with a recipe, drop the live asset, `get` → recipe evaluated once (command call counter), no error |

T4 setup: a recipe for `ui/panel` whose command returns a value with no byte form. In
`liquers-core`, use a test value type whose `as_bytes` errors. In `liquers-lib`, a `UIElement`
works. Prefer core so no feature gate is needed. The query is the bare action name of the
registered test command, validated with `liquers-validate --command <name>`.

Names: `fast_track_skips_metadata_only_entry_memory`,
`fast_track_skips_metadata_only_entry_file`, `metadata_only_value_reloads_by_recomputation`.
