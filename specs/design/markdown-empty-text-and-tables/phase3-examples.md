# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| E1 | example | A view with Text column `[None, "", " ", "a"]` writes rows `\|  \|`, `\| <!----> \|`, `\| &#32; \|`, `\| a \|` |
| T1 | unit | `read(write(E1)) == E1` |
| T2 | unit | Hand-written empty cell → null |
| T3 | unit | Hand-written `<!---->` in an Int column → null |
| T4 | unit | Document with two tables: only the first is read (pins the documented rule) |

Names: `markdown_empty_text_round_trips`, `markdown_empty_cell_is_null`,
`markdown_reads_only_first_table`. Run with `cargo test -p liquers-records --all-features --lib markdown`.
