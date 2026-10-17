# Phase 3: Examples & Use-cases — Agent Memory Service (MVP)

Runnable tests throughout. The one exception is AC-20 (S3): it needs an S3-compatible server, so
its test is `#[ignore]` in the default loop and runs where `LIQUERS_TEST_S3_ENDPOINT` points at one
(MinIO, say). Every query below was checked with `liquers-validate` (`--command mem/<name>` for the
`ns-mem` commands, which do not exist yet).

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | An agent session over the specs | Basic tools: orient, search, read a section, one combined edit | AC-1, AC-2, AC-3, AC-7, AC-15, AC-16 |
| 2 | Example | A power user | `memory_query` composing `ns-mem` with `ns-rec`, a CSV format, the allow-list | AC-14 |
| 3 | Tests | `liquers-lib` unit tests (`src/mem/`, `store_config.rs`) | Editor, tiers bridge, layout, write path, `markdown_tiers` | AC-1, AC-4 to AC-7, AC-16, AC-17 |
| 4 | Tests | `liquers-lib/tests/mem_commands.rs` | Every command through query evaluation | AC-1 to AC-8, AC-11 to AC-13, AC-15 to AC-17, AC-19 |
| 5 | Tests | `liquers-mcp` unit and `tests/mcp_roundtrip.rs` | Schemas, key filling, allow-list, MCP over a duplex transport | AC-14 |
| 6 | Tests | `liquers-agent/tests/` | The specs profile, the `docs_index.py` oracle, S3 | AC-9, AC-18, AC-20 |

## Example 1: An agent session over the specs

An agent is asked to accept `NO-MARKDOWN-NAVIGATION-COMMANDS` and record it in the store reference.
It runs `liquers-agent mcp` with the "liquers-specs" profile (Phase 2). Each tool call becomes one
query: the key argument is filled into the resource segment and the other arguments are
overrides of the last action.

| Call | Query evaluated | Result (abridged) |
|---|---|---|
| `memory_overview {}` | `-R-key/./-/ns-mem/overview/overview.md` | Markdown: the README's L0/L1, `issues 300 · designs 61 · …`, the last ten log lines |
| `memory_search {path: "specs/issues", text: "markdown sections", filter: "-status:closed*", limit: 5}` | `-R-key/specs/issues/-/ns-mem/search/search.md` | A pipe table `key \| l0 \| l1 \| excerpt \| score …`, then the note `shown 5 of 12; raise limit to see more` |
| `memory_read {key: "specs/issues/NO-MARKDOWN-NAVIGATION-COMMANDS.md", section: "Problem"}` | `-R/specs/issues/NO-MARKDOWN-NAVIGATION-COMMANDS.md/-/ns-mem/read/read.md` | The `## Problem` section only, then the notes `key …` and `version 3f9c…` (32 hex digits) |
| `memory_edit {key: "specs/issues/NO-MARKDOWN-NAVIGATION-COMMANDS.md", field: "status", value: "accepted", expected_version: "3f9c…", reason: "triaged"}` | `-R-key/specs/issues/NO-MARKDOWN-NAVIGATION-COMMANDS.md/-/ns-mem/edit/edit.json` | `{"key": …, "version_before": "3f9c…", "version_after": "81d0…"}` |
| `memory_edit {key: "specs/reference/STORE_SEMANTICS.md", old: "no section navigation", new: "section navigation: NO-MARKDOWN-NAVIGATION-COMMANDS", section: "History", row: "\| 2026-10-17 \| Linked the section-navigation issue. \| NO-MARKDOWN-NAVIGATION-COMMANDS \|"}` | `-R-key/specs/reference/STORE_SEMANTICS.md/-/ns-mem/edit/edit.json` | One write (the `old` text stands for a sentence in that document): the text replaced, the row appended, `reviewed:` set to today, `updated:` stamped |

After the session the issue's diff is one line plus `updated:`; the reference's diff is the
replacement, the row and two stamps. `memory/_log/2026-10.md` has gained two lines of the form
`- 2026-10-17T18:20:04Z edit specs/issues/NO-MARKDOWN-NAVIGATION-COMMANDS.md 3f9c1a2b→81d0e4f5 — triaged`.
Had the issue's status been `closed`, the transition table would have refused the first edit,
naming `closed → accepted`, and nothing would have been written.

## Example 2: A power user

The same agent reads `liquers://memory/query-guide` and asks for every draft issue, sorted by L0,
as CSV:

```
memory_query {query: "-R-key/specs/issues/-/ns-mem/list-1-false-1000/ns-rec/eq-status-draft/ns-rec/sort-l0/drafts.csv"}
```

`check_allowed` finds `-mem-list`, `-rec-eq` and `-rec-sort`, all allowed. The value is a
record view and the filename makes the engine write it as CSV. The note `shown 300 of 300` travels
along. The same query ending in `drafts.md` gives a pipe table. `memory_query` with
`-R-key/specs/issues/X.md/-/ns-mem/edit-…` is refused before evaluation, naming `-mem-edit` as
outside the allow-list.

## Corner Cases: Edge and Error Cases

| Symptom | Expected behaviour | Test |
|---|---|---|
| A key argument `x/-/ns-mem/write-a`, `../outside`, `/abs`, `-R` | Refused by `fill_key`, nothing evaluated | `fill_key_rejects_query_shaped_keys` |
| A write-capable command hidden in a link argument of an allowed one | Refused: links are planned and checked | `check_allowed_walks_link_queries` |
| `read`'s version differs from `commit`'s on the same bytes | Every `expected_version` would fail; the test runs both over every file in `specs/` | `read_version_matches_commit_version` |
| `edit` with `old` found twice, together with a valid `field`/`value` | Refused with the count; nothing written, no log line | `failed_replace_writes_nothing` |
| `edit` with `old` but no `new`, or with no pair | Refused naming the missing argument, before reading | `half_given_pair_is_refused` |
| The file edited by hand between two reads (asset loaded) | `read` and `key/info` show the new front-matter | `front_matter_projection_follows_external_edit` |
| A log append fails after the data write | The write stands; the error names the missing log line | `log_failure_keeps_the_write` |
| A command panics inside a tool call | A tool error, the server keeps serving | `panicking_command_is_a_tool_error` |

## Test Plan

Run with `cargo test -p liquers-lib --lib --tests` (the `mem` feature is default),
`cargo test -p liquers-mcp`, and `cargo test -p liquers-agent` (not a default member; needs
`python3` for the oracle).

- Unit: `liquers-lib/src/mem/front_matter.rs` — `set_field_changes_one_line` (AC-16): bytes
  outside the changed line are identical; `set_list_field_writes_flow_list` (AC-16);
  `toml_front_matter_is_not_supported` (AC-16).
- Unit: `liquers-lib/src/mem/tiers.rs` — `default_tiers_read_title_and_description` (AC-1);
  `first_nonempty_source_wins` (AC-17); `section_source_takes_first_paragraph` (AC-17).
- Unit: `liquers-lib/src/mem/layout.rs` — `collection_is_longest_folder_prefix` (AC-5);
  `field_default_fills_status_draft` (AC-4).
- Unit: `liquers-lib/src/mem/write.rs` — `edit_applies_every_pair_once` (AC-16, AC-7): one version
  check, one write, one log line; `half_given_pair_is_refused` (AC-16); `failed_replace_writes_nothing`
  (AC-16, AC-6); `log_failure_keeps_the_write` (AC-7).
- Unit: `liquers-lib/src/store_config.rs` — `markdown_tiers_fills_only_empty_fields` (AC-17).
- Integration: `liquers-lib/tests/mem_commands.rs` (`#![cfg(feature = "mem")]`; an environment
  with an `AsyncMemoryStore`, a fixture `_schema/memory.yaml` and ten documents; temporary
  directories where a filesystem is the point):
  - `list_entries_carry_l0_l1` (AC-1)
  - `search_ranks_and_filters_before_limit` (AC-2): `shown`/`total` notes count only unhidden matches
  - `read_section_returns_section_and_version_note` (AC-3)
  - `read_version_matches_commit_version` (AC-3, AC-6)
  - `new_returns_candidates_then_writes_with_force` (AC-4)
  - `write_refuses_value_outside_vocabulary` (AC-5)
  - `stale_expected_version_is_refused` (AC-6)
  - `write_stamps_and_logs` (AC-7)
  - `supersedes_marks_old_and_hides_it` (AC-8)
  - `front_matter_projection_follows_external_edit` (AC-11)
  - `list_and_search_do_not_evaluate_recipes` (AC-12)
  - `read_only_mount_refuses_write_and_leaves_no_sidecar` (AC-13)
  - `overview_and_describe_orient` (AC-15)
  - `edit_combined_pairs_and_forbidden_transition` (AC-16)
  - `fallback_tiers_without_front_matter` (AC-17)
  - `openviking_layout_lists_creates_and_searches` (AC-19)
  - `list_composes_with_rec_and_formats` (AC-1): Example 2's query as CSV and as Markdown
- Unit: `liquers-mcp/src/schema.rs`, `decl.rs` — `schema_from_metadata_hides_root_and_adds_key`
  (AC-14); `fill_key_rejects_query_shaped_keys` (AC-14); `check_allowed_walks_link_queries` (AC-14);
  `format_argument_changes_the_filename` (AC-14); `build_refuses_multi_action_tool_queries` (AC-14).
- Integration: `liquers-mcp/tests/mcp_roundtrip.rs` — over `tokio::io::duplex` with an `rmcp`
  client: `lists_tools_with_schemas_and_calls_them` (AC-14); `tool_error_is_a_result_not_a_protocol_error`
  (AC-14); `query_tool_refuses_a_write_command` (AC-14); `panicking_command_is_a_tool_error` (AC-14).
- Integration: `liquers-agent/tests/specs_profile.rs` — over a temporary copy of `specs/`,
  `.claude/skills` and `memory/`: `specs_profile_lists_reads_and_edits_in_place` (AC-18): no file
  outside the edited one changes; `check_matches_docs_index_oracle` (AC-9): runs
  `python3 scripts/docs_index.py --check` in the copy (script copied beside it, since it resolves
  the repository from its own path) and compares the set of `(file, §rule)` pairs from its
  `error:` lines with `liquers-agent check` — once on the copy, once after seeding one violation per
  rule.
- Integration: `liquers-agent/tests/s3_profile.rs` — `s3_mount_round_trip` (AC-20), `#[ignore]`:
  write, read, list and search through the tools, then restart the server and read again.

## Documentation and Learning Log

- `-R-key/<path>` evaluates to a `Value::Key` without touching the store, which is why folder
  commands take it: `-R/<folder>` fails today (`DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE`).
- `liquers-validate --command mem/<name>` validates queries for commands not yet registered.
- `docs_index.py` resolves the repository from its own location (`REPO`), so an oracle run on a
  fixture copies the script into the fixture.
- Generic `Value::Object` serializes only to JSON; record views write `md`, `csv`, `json`, `html`
  and more. This is what makes Markdown tables the agent-facing default (Phase 2 decision 6).
