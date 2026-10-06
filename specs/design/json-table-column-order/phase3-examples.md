# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit | NDJSON `{"b":1}\n{"a":2}` → columns `["a","b"]` |
| T2 | unit | Same rows in the other order → `["a","b"]` |
| T3 | unit | `records` shape `[{"z":1,"a":2},{"m":3}]` → `["a","m","z"]` |
| T4 | unit | `columns` shape `{"z":{"0":1},"a":{"0":2}}` → data columns `["a","z"]`, index column where it is today |
| T5 | unit | `index` shape `{"0":{"z":1},"1":{"a":2}}` → data columns `["a","z"]` |
| T6 | unit | Declared schema `[z, a]` → `["z","a"]` |

Names: `schema_less_json_columns_are_sorted`, `schema_less_json_column_order_ignores_row_order`,
`columns_shape_sorts_columns`, `index_shape_sorts_columns`.
