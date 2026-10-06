# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit (`query.rs`) | `parse_query("/-R/a.txt").rooted == true`; `parse_query("-R/a.txt").rooted == false` |
| T2 | unit | `serde_json::to_value(&parse_query("/-R/a.txt")?)?["absolute"] == true` (wire name kept) |
| T3 | unit | Deserializing `{"segments":[],"absolute":true,"source":"Unspecified"}` gives `rooted == true` |
| T4 | regression | `context::tests::absolute_outer_query_keeps_relative_link_independent` (rename the assertions, keep the behaviour) |
| T5 | doc | `cargo doc -p liquers-core --no-deps` builds; the module doc no longer says "no semantic meaning" |

Queries used: `/-R/a.txt` and `-R/a.txt` (checked with `liquers-validate --no-registry`; the first
encodes with a leading `/`).
