# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| E1 | example | Header `a,b,c,d` and rows of 4, 2, 3, 4 cells → 4 rows; warning: "There has been 2 rows with number of cells between 2 and 3, which is less than number of columns in the header (4)." |
| T1 | unit | `"a,b\n1,\"x\ny\"\n2,z,extra\n"` → error contains `line 4 (record 2)` |
| T2 | unit | E1 with a declared nullable schema → padded cells are `Null`, report has the warning |
| T3 | unit | Non-nullable `Text` column missing → `""` |
| T4 | unit | Non-nullable `Int` column missing → error naming line and column |
| T5 | unit | Inferred reader on E1 → nulls + warning |
| T6 | unit | Trailing newline → no warning, no extra row |
| T7 | integration (`liquers-lib`, records-gated) | Store E1 as `data/short.csv`; evaluate `-R/data/short.csv/-/ns-rec/to_record-csv`; the asset's metadata log contains the warning |

T7's query is checked with `liquers-validate`. Names: `csv_error_names_physical_line_and_record`,
`csv_short_rows_are_padded_and_reported_once`, `csv_short_non_nullable_text_pads_empty`,
`csv_short_non_nullable_int_is_an_error`, `to_record_logs_padded_csv_rows`.
