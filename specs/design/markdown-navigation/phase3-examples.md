# Phase 3: Examples & Use-cases — Markdown navigation on a Liquers block parser

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Outline, then one section | an agent reads §8 of a reference document only | AC-1, AC-2, AC-12 |
| 2 | Example | Links of a section, the History table | following links; a table as records | AC-5, AC-6 |
| 3 | Example | Addresses by text | `phase~.2.solution`, `3..2`, `_` | AC-11, AC-3 |
| 4 | Test | `liquers-markdown` unit tests | blocks, sections, addresses, links | AC-1–AC-3, AC-5, AC-7, AC-8, AC-11, AC-12 |
| 5 | Test | `liquers-markdown/tests/specs_round_trip.rs` | every `specs/**/*.md` | AC-7 |
| 6 | Test | `liquers-lib/tests/markdown_commands.rs` | the five commands through queries | AC-1–AC-6, AC-10, AC-12 |
| 7 | Test | existing `md_split`, `mem` and `store_config` tests | unchanged on the new parser | AC-4, AC-9 |

## Example 1: Outline, then one section

An agent was pointed at `specs/reference/AGENT_MEMORY_SERVICE.md` and needs its storage part.
It reads the outline first (a few hundred bytes), then one section.

```
-R/specs/reference/AGENT_MEMORY_SERVICE.md/-/ns-md/outline
```

| n | level | heading | address | number | line | end_line | words |
|---|---|---|---|---|---|---|---|
| 0 | 1 | Agent Memory Service | 1 | | 8 | 330 | … |
| 1 | 2 | 1. Layers | 1.1 | 1 | 17 | 27 | … |
| 8 | 2 | 8. Storage | 1.8 | 8 | 192 | 219 | … |
| 11 | 2 | History | 1.11 | | 305 | 330 | … |

(`path` and `slug` columns omitted here.) The agent picks row 8; any of these read the same text:

```
-R/specs/reference/AGENT_MEMORY_SERVICE.md/-/ns-md/section-1.8
-R/specs/reference/AGENT_MEMORY_SERVICE.md/-/ns-md/section-_.8
-R/specs/reference/AGENT_MEMORY_SERVICE.md/-/ns-md/section-_.storage
```

The result is the text of lines 192–219, heading line first. `section-_.8-f` would drop the
subsections (`children=false`; `f` reads as false), `section-_.8-t-f` the heading line.

## Example 2: Links of a section, the History table

```
-R/specs/reference/AGENT_MEMORY_SERVICE.md/-/ns-md/links-_.8
-R/specs/reference/ASSETS.md/-/ns-md/table-_.history
```

`links` returns one row per link inside §8: `text`, `target` (`../guides/STORE_CONFIG_GUIDE.md`),
`kind` (`relative`), `key` (`specs/guides/STORE_CONFIG_GUIDE.md`, resolved against the input's
key), `section` (`1.8`), `line`. A target like `-R/data/a~~b.csv/-/to_text` has `kind = query`
and is kept whole. `table` returns the History table as records with the columns `Date`,
`Change`, `Source`, read by the records Markdown table reader; `n=2` would take the second table.

## Example 3: Addresses by text

```
-R/specs/design/runtime-volatility/DESIGN.md/-/ns-md/section-phase~.2.solution
```

Step `phase 2` (`~.` is a space) matches the top-level-or-child heading whose words start with
"phase 2", exact before word-prefix before contained, so "Phase 20" is not a candidate when
"Phase 2" exists; `solution` then matches among its direct subsections. Other forms:

| Address | Meaning |
|---|---|
| `3..2` | the text "3.2": matches `## Section 3.2` among the top-level sections |
| `1.3..2` | under section 1, the subsection whose heading contains "3.2" |
| `_._.history` | a `History` section two levels down, under any sections |
| `my__var` | the text "my_var" |
| `my_var` | error: `_` is reserved inside a step |
| `1.` / `.1` / empty | error: empty step |

## Edge and Error Cases

| Case | Expected | Test |
|---|---|---|
| Two `### Notes` under different parents, address `_._.notes` | error listing both addresses, headings and lines | `address_ambiguous_lists_candidates` |
| `storage` when §8 *Storage* is one level deeper than asked | error naming `1.8` as a deeper match | `address_missing_names_deeper_match` |
| Section number beyond the children | error naming the section and its child count | `address_number_out_of_range` |
| Setext heading (`Title\n=====`) | a level-1 section | `setext_headings_are_sections` |
| `# x` inside a fence, an indented block, an HTML block, front matter | not a heading | `hash_lines_in_code_html_and_front_matter_are_not_headings` |
| Unclosed fence | runs to the end of the text, no failure | `unclosed_fence_runs_to_end` |
| Empty text, text with no heading, CRLF line ends, no final newline | parses; round trip exact | `degenerate_inputs_round_trip` |
| `table` in a section with no table | error naming the address and "0 tables" | `table_missing_is_an_error` |
| Link inside a code span | not a link | `links_inside_code_spans_are_ignored` |
| Reference-style link | reported with its definition's target | `reference_links_resolve_definitions` |
| `markdown` feature off | no `md` namespace; build passes | build-matrix row; `markdown_feature_off_has_no_md_commands` |
| 100 KB document | parses in linear time | `parses_a_large_document_quickly` |

## Test Plan

- Unit, `liquers-markdown/src/block.rs`: `blocks_tile_the_input` (AC-7), `setext_headings_are_sections` (AC-1),
  `hash_lines_in_code_html_and_front_matter_are_not_headings` (AC-1), `unclosed_fence_runs_to_end` (AC-7),
  `degenerate_inputs_round_trip` (AC-7), `front_matter_agrees_with_records` (AC-7: same end as
  `liquers_records::search::front_matter_end` on a set of inputs; a dev-dependency).
- Unit, `liquers-markdown/src/section.rs`: `sections_nest_by_level` (AC-1, AC-2), `section_numbers_and_addresses` (AC-12),
  `section_text_children_and_heading_flags` (AC-2), `find_by_heading_matches_own_or_path` (AC-9),
  `title_and_first_paragraph` (AC-9), `parses_a_large_document_quickly` (AC-7).
- Unit, `liquers-markdown/src/address.rs`: `address_parse_steps` (AC-11: numbers, text, `..`, `_`, `__`,
  reserved `_`, empty steps), `address_display_round_trips` (AC-11), `address_text_prefers_exact_then_prefix` (AC-11),
  `address_placeholder_matches_any` (AC-11), `address_ambiguous_lists_candidates` (AC-3),
  `address_missing_names_deeper_match` (AC-3), `address_number_out_of_range` (AC-3).
- Unit, `liquers-markdown/src/inline.rs`: `inline_links_images_and_autolinks` (AC-5),
  `reference_links_resolve_definitions` (AC-5), `links_inside_code_spans_are_ignored` (AC-5),
  `query_targets_are_queries` (AC-8: `~~`, `_`, `*` kept, `kind = query`), `link_resolve_against_key` (AC-5).
- Integration, `liquers-markdown/tests/specs_round_trip.rs`: `every_spec_document_round_trips` (AC-7: concatenated
  block texts equal the file, blocks contiguous, lines consistent).
- Integration, `liquers-lib/tests/markdown_commands.rs` (gated on `markdown`): `outline_lists_headings` (AC-1, AC-12),
  `section_by_number_text_and_placeholder` (AC-2, AC-11), `section_flags` (AC-2), `section_errors_name_candidates` (AC-3),
  `sections_level_filter` (AC-4), `links_resolve_against_the_input_key` (AC-5), `table_under_heading` (AC-6),
  `table_missing_is_an_error` (AC-6).
- Integration, `liquers-lib/tests/register_all_commands.rs`: `markdown_feature_off_has_no_md_commands` (AC-10, gated on
  `not(feature = "markdown")`) and the registry export test with the new group (AC-10).
- Regression (AC-4, AC-9): the existing tests in `liquers-lib/tests/search_commands.rs` (`md_split`), `mem_commands.rs`
  (`read` section, `edit` row, `check` sections and dead links, `search unit=section`), `store_config` tests
  (`markdown_tiers`), `mem/tiers.rs` and `liquers-agent`'s tests, unchanged. A new
  `md_split_counts_setext_and_ignores_html_headings` in `search_commands.rs` records the two
  intended changes (AC-4, AC-9), and `dead_links_checks_reference_links` in `mem_commands.rs` the third (AC-5).
- Commands: `cargo test -p liquers-markdown`; `cargo test -p liquers-lib --lib --tests`;
  `cargo test -p liquers-lib --no-default-features --features records --lib --tests`;
  `cargo test -p liquers-agent`; the build-matrix rows for `markdown`, one at a time.

## Learning Log

- All queries above were checked with `liquers-validate --command md/<name>`; `ns-md/section-_.8`
  parses with `_.8` as one parameter. `*` is refused by the query parser, which is why `_` is the
  placeholder.
- `children` and `heading` are positional after the address, so `section-_.8-f` is the short
  form of `children=false`.
- `md_split`'s section rows span to the next heading of the same or a higher level (the section
  with its subsections), while `ns-mem/search unit=section` cuts at the next heading of any level;
  `Section` carries both ends (`span` and the own-text end) so both keep their meaning.
