# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit | NDJSON `{"name":"a","age":1}\n{"name":"b","age":2}` → columns `["name","age"]` |
| T2 | unit | `records` shape `[{"b":1},{"b":2,"a":3}]` → `["b","a"]` |
| T3 | unit | `columns` shape `{"z":{"0":1},"a":{"0":2}}` → `["z","a"]` |
| T4 | unit | `index` shape `{"0":{"z":1,"a":2}}` → `["z","a"]` |
| T5 | unit | Declared schema `[a, z]` on T3 input → `["a","z"]` |
| T6 | regression | `cargo test -p liquers-lib --test registry_export` passes and `cargo tree -e features -i serde_json` shows no `preserve_order` |

Names: `ndjson_schema_less_keeps_key_order`, `records_shape_orders_by_first_appearance`,
`columns_shape_keeps_top_level_order`, `index_shape_keeps_inner_order`.
