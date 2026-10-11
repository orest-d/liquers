# Phase 3: Examples & Use-cases — Store and asset search

> Written 2026-10-11, after Phase 2 was approved. Every query below was checked with
> `liquers-validate` against `specs/command_registry.yaml`, with the five new commands declared
> through `--command`; its decoded expression is shown beside it.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Open issues about expiration | Folder → catalog with content → front-matter → ranked search | AC-1, AC-5, AC-6, AC-18, AC-19, AC-24, AC-26 |
| 2 | Example | Where is a function defined? | Lines with line numbers, by regex | AC-28, AC-29 |
| 3 | Example | Which command reads CSV? | Command discovery with default fields | AC-13 |
| 4 | Tests | `liquers-records` `search::syntax::tests` | Grammar, `/` rule, positions, round-trip | AC-7, AC-21, AC-28 |
| 5 | Tests | `liquers-records` `search::{kernels,rank,structure,source}::tests` | Matching, BM25, front-matter, lazy filter | AC-6–AC-12, AC-15–AC-17, AC-19 |
| 6 | Tests | `liquers-lib/tests/search_commands.rs` | Commands through `evaluate` on a fixture store | AC-1–AC-6, AC-11–AC-14, AC-18, AC-20, AC-24–AC-26, AC-29, AC-30 |
| 7 | Check | Build matrix, wasm32 row | Commands registered in the browser build | AC-22 |

The withdrawn and moved criteria of Phase 1 (explicit structure formats; the `ns-rec` filters) have no tests
here; the filters' tests belong to `record-filter-commands`.

### The fixture store

Integration tests share one `AsyncMemoryStore` inside a `DefaultEnvironment<Value>`, with
`register_search_commands!` and `register_records_commands!` applied:

| Key | Content |
|---|---|
| `specs/issues/A-EXPIRY.md` | front-matter `id`, `status: draft`, `priority: P1`, `area: [core/assets]`, `title`; the body mentions "expiration safety" twice |
| `specs/issues/B-CLOSED.md` | `status: closed`; the body mentions "expiration" once |
| `specs/issues/C-STORE.md` | `status: draft`, `area: [core/store]`; nothing about expiration |
| `specs/issues/D-TOML.md` | TOML front-matter (`+++`), with `created = 2026-10-01T00:00:00Z` |
| `specs/issues/notes.txt` | plain text, no front-matter |
| `specs/issues/recipes.yaml` | declares `derived.md`, a recipe-only key whose command counts its runs |
| `specs/design/x/DESIGN.md` | `phase: architecture` |
| `specs/design/x/phase1-high-level-design.md` | no front-matter; `# Phase 1` heading, then `## Purpose` |
| `data/table.csv` | three rows, a `city` column |
| `src/lib.rs` | contains `fn get_asset_info(` once, and a call to `get_asset_info` once |

## Example 1: Open issues about expiration

An agent wants the open issues about expiration safety, best first, with enough to judge each one.

```
-R-key/specs/issues/-/ns-search/catalog-t-t/field_extract/search-expiration~.safety~.~_status~ncolon~closed~nast~
```

The decoded expression is `expiration safety -status:closed*`. Step by step:
1. `catalog-t-t` lists `specs/issues/` recursively, with content (AC-1, AC-5). `derived.md` appears
   with status `Recipe`, no content, and its command does not run (AC-4).
2. `field_extract` keeps the `.md` files, parses their front-matter, and adds `id`, `priority`,
   `area` (a JSON list column) and `created`. Front-matter `status` and `title` replace the
   catalog's (AC-24, AC-26).
3. `search` ranks by BM25 (AC-6).

Expected result: one row, `A-EXPIRY`, with `score > 0`, `matched` containing `content`, and an
`excerpt` that contains "expiration safety" but not `---` (AC-19). Its description is
`1 of 1 matches shown` (AC-18).

The same filter without ranking, as an agent tool would send it (`rank=false`, `limit=0`):
`…/field_extract/search-area~ncolon~core~/assets~.status~ncolon~draft-f-0`, decoded
`area:core/assets status:draft`. It returns `A-EXPIRY` only, because `area` is a list and
`core/assets` matches one of its elements (AC-25).

## Example 2: Where is a function defined?

```
-R-key/src/-/ns-search/md_split-line/search-~/fn~.get_asset_info~/-f-0-text
```

The decoded parameters are `/fn get_asset_info/`, `f`, `0`, `text`. `md_split-line` reads the
text files of `src/` and makes one row per line. The regex keeps only the definition: the call
does not match `fn get_asset_info`. The result row carries `key = src/lib.rs` and its 1-based
`line` (AC-28, AC-29).

## Example 3: Which command reads CSV?

```
ns-search/commands/search-csv
```

With no field list, `csv` is matched in the text-role fields (`name`, `title`, `description`,
`doc`, …), so `rec/to_record` and `pl/from_csv` are found (if polars is built). Each row's `query`
column is the fragment that calls it (AC-10, AC-13).

## Corner Cases

| Symptom | Expected | Test |
|---|---|---|
| `/a/ /b/` | two regex terms | `regex_body_ends_at_first_closing_slash` |
| `area:core/store`, `path:/usr/bin`, `/unclosed`, `f:` | a plain value, a value, a term, and a term | `slash_without_closing_delimiter_is_literal` |
| `(a` (unbalanced) | `TryFrom` fails with a `Position`; `parse_lenient` searches the words | `malformed_expression_reports_position` |
| `/[/` (invalid regex) | the same as above | `invalid_regex_reports_position` |
| `nosuch:x` | no match, one warning | `unknown_field_test_warns_and_matches_nothing` |
| `search-x-t-f-50-nosuch` | error naming `nosuch` | `explicit_unknown_search_field_is_error` |
| `size:1*` on an Int column | bind error | `wildcard_on_non_text_column_is_error` |
| `updated:>2026-10-01` on a Timestamp column | widened to midnight UTC | `date_only_compare_on_timestamp` |
| a nested field test on a non-JSON Text column | bind error | `nested_field_test_requires_json_column` |
| a JSON column with one bad cell | non-match, one warning | `invalid_json_cell_counts_warning` |
| a TOML datetime in front-matter | an ISO string, not a wrapper object | `toml_datetime_becomes_iso_string` |
| `limit = -1`; `limit = 0` | an error; no limit | `limit_zero_is_unlimited_negative_is_error` |
| a folder over `max_entries` | stops, with a description and a log entry | `catalog_stops_at_max_entries` |
| a stream over `max_rows` with `rank=true` | an error that advises `rank=false` | `rank_over_large_source_advises_rank_false` |

## Test Plan

**`liquers-records/src/search/syntax.rs` — `mod tests`**
- `parses_terms_phrases_negation_or_groups` (AC-7): AST shapes.
- `parses_field_glob_compare_nested` (AC-7, AC-17): `f:v`, `f:*`, `f:>v`, `f:(…)`.
- `regex_body_ends_at_first_closing_slash` (AC-28): also covers `/a/b/` and `title:/x/ | /y/`.
- `slash_without_closing_delimiter_is_literal` (AC-28): the cases in the table above.
- `malformed_expression_reports_position` (AC-7, AC-21): `TryFrom` returns a positioned error;
  `parse_lenient` falls back to words.
- `invalid_regex_reports_position` (AC-28).
- `to_syntax_round_trips_through_try_from` (AC-21): a property over generated ASTs.

**`liquers-records/src/search/kernels.rs` — `mod tests`**
- `terms_match_case_insensitively_by_default` (AC-8).
- `case_sensitive_flag_is_respected` (AC-8): terms, phrases and regex.
- `explicit_fields_restrict_terms` (AC-9).
- `default_fields_prefer_roles_then_names_then_text` (AC-10).
- `unknown_field_test_warns_and_matches_nothing` (AC-14).
- `explicit_unknown_search_field_is_error` (AC-9).
- `nested_text_predicate_is_field_scoped` (AC-16).
- `json_column_paths_and_any_element` (AC-17, AC-25).
- `nested_field_test_requires_json_column` (AC-17).
- `invalid_json_cell_counts_warning` (AC-17).
- `wildcard_on_non_text_column_is_error` (AC-7).
- `presence_test_on_non_text_column` (AC-7).
- `date_only_compare_on_timestamp` (AC-7).

**`liquers-records/src/search/rank.rs` — `mod tests`**
- `bm25_orders_by_relevance_with_field_weights` (AC-6).
- `matched_and_excerpt_columns` (AC-19): the excerpt skips front-matter.
- `filter_only_and_unranked_keep_input_order` (AC-15).
- `limit_zero_is_unlimited_negative_is_error` (AC-6, AC-18).
- `report_counts_matched_and_shown` (AC-18).

**`liquers-records/src/search/structure.rs` — `mod tests`**
- `front_matter_yaml_and_toml` (AC-24).
- `toml_datetime_becomes_iso_string` (AC-24).
- `no_front_matter_is_none` (AC-24).

**`liquers-records/src/search/source.rs` — `mod tests`**
- `filtered_source_is_lazy_and_stops_at_limit` (AC-12): a counting inner source; `truncated`
  after a limit stop.
- `filtered_source_rebinds_on_schema_change` (AC-12).
- `custom_predicate_type_filters_like_builtin` (AC-21): a hand-written `Predicate` implementation
  gives the same mask.

**`liquers-lib/tests/search_commands.rs`** (`#![cfg(feature = "records")]`), all through
`env.evaluate` on the fixture store:
- `catalog_lists_folder_recursively` (AC-1).
- `catalog_without_input_starts_at_root` (AC-2).
- `catalog_folder_only` (AC-3).
- `catalog_never_evaluates_recipe_keys` (AC-4): `derived.md`'s command counter stays at 0.
- `catalog_reads_text_content_only` (AC-5): `notes.txt` and `.md` have content; `table.csv` is
  text too; a binary entry has none.
- `catalog_markdown_title_from_heading` (AC-30).
- `catalog_stops_at_max_entries` (AC-20).
- `search_ranked_example_1` (AC-6, AC-18, AC-19): Example 1.
- `search_accepts_view_source_folder_key_query_csv` (AC-11): the six input kinds.
- `search_unranked_source_stays_source` (AC-12).
- `rank_over_large_source_advises_rank_false` (AC-12).
- `commands_catalog_and_discovery` (AC-13): Example 3.
- `field_extract_reads_folder_directly` (AC-24): no `catalog` in front.
- `field_extract_from_catalog_records` (AC-24).
- `field_extract_prefix_and_flatten` (AC-25).
- `field_extract_replaces_columns_keeps_id` (AC-26).
- `md_split_sections_and_lines` (AC-29): Example 2, plus the section headings of `phase1…md`.

**Build matrix:** `bash scripts/check-build-matrix.sh`, wasm32 row, with
`search_commands_registered_in_wasm_build` (AC-22): a `#[cfg(target_arch = "wasm32")]` check in
`liquers-lib` that the `search` namespace is in the registry.

**Commands to run:**

```bash
cargo test -p liquers-records --all-features --lib search
cargo test -p liquers-lib --test search_commands
cargo test -p liquers-lib --test registry_export
bash scripts/check-build-matrix.sh
```

## Documentation and Learning Log

- Query encodings are the trap: only alphanumerics, `_`, `+` and `.` survive unescaped. Every
  query in this document was checked with `liquers-validate`, and its decoded parameter compared
  with the intended expression. Two early drafts were wrong: `~_` is `-`, not `_`, and an unescaped
  `-` in a date splits the parameter.
- The default field order (roles first) only became visible through the command catalog: AC-13
  failed under the original AC-10.
- Front-matter is reliable only when the format comes from outside the text, which is why it moved
  from the syntax to `field_extract`.
