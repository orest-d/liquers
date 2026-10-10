# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit (`query.rs`) | `parse_query("/-R/a.txt").rooted == true`; `parse_query("-R/a.txt").rooted == false` |
| T2 | unit | `serde_json::to_value(&parse_query("/-R/a.txt")?)?["rooted"] == true`, and there is no `"absolute"` key |
| T3 | unit (`metadata.rs`) | A `MetadataRecord` with query `/-R/a.txt` round-trips through JSON as the text `"/-R/a.txt"` |
| T4 | regression | `query::tests::cwd_cursor_absolute_query_uses_private_root_without_fallback` (renamed to `cwd_cursor_rooted_query_uses_private_root_without_fallback`) and `context::tests::absolute_outer_query_keeps_relative_link_independent` (renamed `rooted_…`): behaviour unchanged |
| T5 | doc | `cargo doc -p liquers-core --no-deps` builds; the module doc no longer says "no semantic meaning" |

Queries used: `/-R/a.txt`, `-R/a.txt`, `-R/./data/x.csv/-/to_text`, `/-R/./data/x.csv/-/to_text`
(checked with `liquers-validate`).
