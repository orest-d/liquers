# Phase 3: Examples & Use-cases — Store and asset search

> Written 2026-10-11, and revised the same day after two independent reviews. Every query is
> validated with `liquers-validate` against the real signatures:
> `--registry-file specs/command_registry.yaml --registry-file search-commands.registry.yaml`.
> [`search-commands.registry.yaml`](./search-commands.registry.yaml) declares the five proposed
> commands. (`--command` would accept any arguments, which hid wrong argument counts in the first
> draft.)

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Open issues about expiration safety | Folder, catalog with content, front-matter, ranked search | AC-1, AC-4, AC-5, AC-6, AC-19, AC-24, AC-26 |
| 2 | Example | Where is a function defined? | Lines with line numbers, by regex | AC-28, AC-29 |
| 3 | Example | Which commands handle records? | Command discovery with default fields | AC-10, AC-13 |
| 4 | Tests | `liquers-records` `search::syntax` | Grammar, `/` rule, positions, round-trip | AC-7, AC-21, AC-28 |
| 5 | Tests | `liquers-records` `search::{kernels,rank,structure,source}` | Matching, BM25, front-matter, lazy filter | AC-6–AC-10, AC-12, AC-14–AC-19, AC-21, AC-24, AC-25 |
| 6 | Tests | `liquers-lib/tests/search_commands.rs` | Commands through `evaluate` on the fixture store | AC-1–AC-6, AC-11–AC-14, AC-18, AC-20, AC-24–AC-26, AC-28–AC-30 |
| 7 | Checks | Registry tests; wasm32 build-matrix row | `search` is registered under `records`, with no target-specific cfg | AC-22 |

## The fixture store

There is one `AsyncMemoryStore` in a `DefaultEnvironment<Value>`, with
`type CommandEnvironment = DefaultEnvironment<Value>`, and `register_search_commands!` and
`register_records_commands!` applied. Each file is written as in `records_end_to_end.rs`:
`set_filename` (which seeds the format from the extension), then `set_status(Status::Source)`, then
`store.set`.

| Key | Content |
|---|---|
| `specs/issues/A-EXPIRY.md` | Front-matter: `id`, `status: draft`, `priority: P1`, `area: [core/assets]`, `links: {github: 12}`, `key: bogus`, and `title: Expired assets are served stale`, which avoids the search terms. The body says "expiration safety" twice |
| `specs/issues/B-CLOSED.md` | `status: closed`; the body says "expiration safety" once |
| `specs/issues/C-STORE.md` | `status: draft`, `area: [core/store]`; nothing about expiration |
| `specs/issues/D-TOML.md` | TOML front-matter: `status = "accepted"`, `created = 2026-10-01T00:00:00Z`; nothing about expiration |
| `specs/issues/notes.txt` | Plain text |
| `specs/issues/recipes.yaml` | Declares `derived.md` (`query: "ns-fixture/count_derived-<tag>/derived.md"`), whose command counts its runs per test tag, as `fixture_counts` does in `records_end_to_end.rs` |
| `specs/design/x/DESIGN.md` | `phase: architecture` |
| `specs/design/x/phase1-high-level-design.md` | No front-matter; `# Phase 1`, then `## Purpose` |
| `src/lib.rs` | `fn get_asset_info(` on one line, and a call to `get_asset_info` on another |
| `data/table.csv`, `data/config.json`, `data/settings.yaml`, `data/blob.bin` | Three CSV rows with a `city` column; a JSON map; a YAML map; binary bytes |

Row counts include `recipes.yaml` itself, which is an ordinary entry.

## Example 1: Open issues about expiration safety

```
-R-key/specs/issues/-/ns-search/catalog-t-t/field_extract/search-expiration~.safety~.~_status~ncolon~closed~nast~
```

Decoded: `expiration safety -status:closed*`.
1. `catalog` lists and reads the folder. `derived.md` is listed with status `Recipe` and no
   content, and is never started.
2. `field_extract` keeps the `.md` files with front-matter, so `notes.txt` and `derived.md` drop
   out. Front-matter `status` and `title` replace the catalog's.
3. `search` ranks the result. B-CLOSED contains both terms, so only `-status:closed*` removes it.
   C-STORE and D-TOML lack the terms.

**Expected:** one row, `A-EXPIRY`. `score > 0`. `matched` is exactly `content`, because the title
has neither term. `excerpt` contains "expiration safety" and no `---`.

The same filter as an agent tool sends it, without ranking or a limit:
`-R-key/specs/issues/-/ns-search/field_extract/search-area~ncolon~core~/assets~.status~ncolon~draft-f-f-0`.
Decoded: `area:core/assets status:draft`, `rank=false`, `case_sensitive=false`, `limit=0`. It
returns `A-EXPIRY`: `area` is a JSON list column, and `core/assets` matches one of its elements.

## Example 2: Where is a function defined?

```
-R-key/src/-/ns-search/md_split-line/search-~/fn~.get_asset_info~/-f-f-0-text
```

Decoded: `/fn get_asset_info/`, `rank=false`, `case_sensitive=false`, `limit=0`,
`fields=[text]`. `md_split-line` reads every text file. `.rs` is `text/x-rustsrc`; this
clarification of Phase 2 is to be confirmed at this gate. The regex keeps the definition and
rejects the call. The single row has `key = src/lib.rs` and the 1-based `line`.

## Example 3: Which commands handle records?

`ns-search/commands/search-record` finds `ns-rec/to_record` and `ns-rec/to_record_source` with no
field list. `name` and `title` are text-role fields, and roles come first. Each row's `query` is the
fragment that calls the command. Restricted to the namespace: `ns-search/commands-rec`.

With the `polars` feature, and `register_polars_commands!` added to that test's environment,
`ns-search/commands/search-csv` finds `ns-pl/from_csv` and `ns-pl/to_csv`.

## Corner Cases

Each case is a named test in the plan below. The plan gives the expected behaviour as its
assertion.

## Test Plan

**`liquers-records/src/search/syntax.rs`**
- `parses_terms_phrases_negation_or_groups` (AC-7).
- `parses_field_glob_compare_nested` (AC-7, AC-17).
- `regex_body_ends_at_first_closing_slash` (AC-28): `/a/ /b/` is two regexes; `/a/b/` is `a/b`;
  `title:/x/ | /y/`.
- `slash_without_closing_delimiter_is_literal` (AC-28): `area:core/store`, `path:/usr/bin`,
  `/unclosed`, `f:`.
- `malformed_expression_reports_position` (AC-7, AC-21): `(a` makes `TryFrom` fail with a
  `Position`, and `parse_lenient` falls back to plain words.
- `invalid_regex_reports_position` (AC-28): `/[/`.
- `to_syntax_round_trips` (AC-21): table-driven over the cases of `phase2-details.md` §A–B and a
  list of hand-built ASTs. It checks `to_syntax(parse(to_syntax(e))) == to_syntax(e)`, since
  positions change when the expression is re-printed. There is no `proptest` in the workspace.

**`liquers-records/src/search/kernels.rs`**
- `terms_match_case_insensitively_by_default` and `case_sensitive_flag_is_respected` (AC-8):
  terms, phrases and regexes.
- `explicit_fields_restrict_terms` and `explicit_unknown_search_field_is_error` (AC-9).
- `default_fields_prefer_roles_then_names_then_text` (AC-10).
- `unknown_field_test_warns_and_matches_nothing` (AC-14): the bind warning names the field.
- `nested_text_predicate_is_field_scoped` (AC-16).
- `json_column_paths_and_any_element` (AC-17, AC-25).
- `nested_field_test_requires_json_column` and `invalid_json_cell_counts_warning` (AC-17).
- `wildcard_on_non_text_column_is_error`, `presence_test_on_non_text_column` and
  `date_only_compare_on_timestamp` (AC-7).

**`liquers-records/src/search/rank.rs`**
- `bm25_orders_by_relevance_with_field_weights` (AC-6).
- `matched_and_excerpt_columns` (AC-19): the excerpt skips front-matter.
- `unranked_filter_only_and_empty_keep_input_order` (AC-15).
- `limit_zero_is_unlimited_negative_is_error` and `report_counts_matched_and_shown` (AC-18).

**`liquers-records/src/search/structure.rs`**
- `front_matter_yaml_and_toml`, `toml_datetime_becomes_iso_string`, `yaml_non_string_keys` and
  `no_front_matter_is_none` (AC-24).

**`liquers-records/src/search/source.rs`**: a test-local `RecordSource` with a counting resolver,
following `TestSource` and `FixtureResolver` in `liquers-records/src/value.rs`.
- `filtered_source_is_lazy_and_stops_at_limit` (AC-12): pulls are counted, and `truncated()` is
  true after the stop.
- `filtered_source_rebinds_on_schema_change` (AC-12).
- `custom_predicate_type_filters_and_ranks_like_builtin` (AC-21): a hand-written `Predicate`
  gives the same `search_view` output as the built-in one, scores included.

**`liquers-lib/tests/search_commands.rs`** (`#![cfg(feature = "records")]`), all through
`env.evaluate`:
- `catalog_lists_folder_recursively` (AC-1): on `specs/`, rows from `issues/` and `design/x/`.
- `catalog_without_input_starts_at_root` (AC-2): `ns-search/catalog` lists `data/` and `src/` too.
- `catalog_folder_only` (AC-3): `catalog-f` on `specs/` gives `issues` and `design` with
  `is_dir = true`, and no file below them.
- `catalog_never_evaluates_recipe_keys` (AC-4): `derived.md` has status `Recipe` and null
  content, its counter is 0, and `lookup_key_asset(&derived)` is `None`. Volatile and unfinished
  assets are covered by the same code path, and are not fixtured separately.
- `catalog_reads_text_content_only` (AC-5): `.md`, `.txt`, `.csv`, `.json` and `.yaml` have
  content; `blob.bin` has none; with `max_bytes` set to 16, the content is cut at a UTF-8 boundary.
- `catalog_markdown_title_from_heading` (AC-30): `phase1-high-level-design.md` gets the title
  `Phase 1`.
- `catalog_stops_at_max_entries` (AC-20): asserts the description and the log entry.
- `search_ranked_example_1` (AC-6, AC-19): Example 1.
- `search_limit_reports_n_of_m` (AC-18): `search-expiration-t-f-1` over the catalog with content
  gives the description `1 of 2 matches shown` and an Info log entry. Through a source, the
  description is `first 1 matches; more may exist`.
- `search_accepts_all_input_kinds` (AC-11), one assertion each:

  | Input | Query |
  |---|---|
  | no input | `ns-search/search-x` |
  | view | `-R/data/table.csv/-/ns-rec/to_record/ns-search/search-x` |
  | source | `-R/data/table.csv/-/ns-rec/to_record_source/ns-search/search-x` |
  | folder | `-R-key/specs/issues/-/ns-search/search-x` |
  | `dir` state | `-R-dir/specs/issues/-/ns-search/search-x` |
  | key | `-R-key/data/table.csv/-/ns-search/search-x` |
  | query value | `-R/data/table.csv/-/q/ns-search/search-x` |
  | CSV with format metadata | `-R/data/table.csv/-/ns-search/search-x` |
- `search_unranked_source_stays_source` (AC-12).
- `rank_over_large_source_advises_rank_false` (AC-12): a fixture command returns
  `InMemorySource::new(vec![view; 1001])` over one 1 000-row view. `materialize` fails as soon as
  the count passes the limit, and the message mentions `rank=false`.
- `search_malformed_expression_warns_and_searches_words` (AC-7, AC-28).
- `unknown_field_logs_warning` (AC-14).
- `commands_catalog_and_discovery` (AC-13): Example 3, plus `commands-rec`. The `csv` case is under
  `#[cfg(feature = "polars")]`.
- `field_extract_reads_folder_directly` and `field_extract_from_catalog_records` (AC-24).
- `field_extract_json_and_yaml` (AC-24): `field_extract-json` and `field_extract-yaml` on `data/`.
- `field_extract_prefix_and_flatten` (AC-25): `prefix`; `links_github` with `flatten`;
  `links.github:12` without it.
- `field_extract_replaces_columns_keeps_id` (AC-26): `status` and `title` are replaced and
  `title` keeps the text role. The front-matter `key` is skipped with a warning. The log names the
  replaced columns.
- `md_split_sections_and_lines` (AC-28, AC-29): Example 2, plus the section headings of
  `phase1-high-level-design.md`.

**Registration (AC-22):**
- In `liquers-lib/tests/register_all_commands.rs` and
  `registry_export.rs::exported_registry_is_nonempty`, assert that
  `registry.find_command("", "search", "catalog")` is `Some`, under `#[cfg(feature = "records")]`.
- `registry_export.rs::full_registry` calls `register_search_commands!`.
- The wasm32 rows of `scripts/check-build-matrix.sh` compile it, and the registration carries no
  `target_arch` cfg.

**Commands to run:**

```bash
cargo test -p liquers-records --all-features --lib search
cargo test -p liquers-lib --test search_commands
cargo test -p liquers-lib --no-default-features --features records --test search_commands
cargo test -p liquers-lib --test register_all_commands --test registry_export
bash scripts/check-build-matrix.sh
```

## Documentation and Learning Log

- Validate example queries against real signatures (`--registry-file`), not `--command`.
  `search-x-f-0` sets `case_sensitive`, not `limit`; only the signature-aware check caught it.
- Only alphanumerics, `_`, `+` and `.` survive unescaped in a query. `~_` is `-`, not `_`.
- Fixture files need their filename (which gives the format) and a `Source` status, or `read_text`
  refuses them.
