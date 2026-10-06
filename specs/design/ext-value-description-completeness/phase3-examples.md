# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | test (rewritten) | Every sampled variant's identifier is described |
| T2 | test (new, same file) | Every described identifier belongs to a sampled variant |
| T3 | negative check (manual, recorded in PR) | Temporarily removing `RecordView`'s `TypeInfo` makes T1 fail under `records` |
| T4 | matrix | Passes under each `--no-default-features --features X` from CLAUDE.md |

Names: `ext_value_type_descriptions_complete` (kept),
`ext_value_type_descriptions_have_no_stale_entries`.
