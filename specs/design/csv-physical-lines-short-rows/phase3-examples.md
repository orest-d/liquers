# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit | `"a,b\n1,\"x\ny\"\n2,z,extra\n"` (record 2 starts on line 4) → error contains `line 4 (record 2)` |
| T2 | unit | Header `a,b,c`, row `1,2` → error "line 2 has 2 fields, but the table has 3 columns" (declared and inferred readers) |
| T3 | unit | Trailing newline at EOF → no error |
| T4 | unit | Equal-width file with a multi-line quoted cell reads correctly (regression) |
| T5 | unit | Non-nullable column error names the physical line |

Names: `csv_error_names_physical_line_and_record`, `csv_short_row_is_refused_declared`,
`csv_short_row_is_refused_inferred`, `csv_trailing_newline_is_not_a_row`.
Run with `cargo test -p liquers-records --all-features --lib csv`.
