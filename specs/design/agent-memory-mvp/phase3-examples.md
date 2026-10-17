# Phase 3: Examples & Use-cases — Agent Memory Service (MVP)

Runnable tests throughout. AC-20 needs an S3-compatible endpoint; Phase 1 allows an in-process
one, so the test starts `s3s-fs` (a filesystem-backed S3 server, a dev-dependency of
`liquers-agent`) on a local port. If Phase 4 finds it unusable, the test falls back to
`#[ignore]` with `LIQUERS_TEST_S3_ENDPOINT`, and that deviation from AC-20 is recorded there. Every query below was checked with `liquers-validate` (`--command mem/<name>` for the
`ns-mem` commands, which do not exist yet).

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | An agent session over the specs | Basic tools: orient, search, read a section, one combined edit | AC-1, AC-2, AC-3, AC-7, AC-15, AC-16 |
| 2 | Example | A power user | `memory_query` composing `ns-mem` with `ns-rec`, a CSV format, the allow-list | AC-14 |
| 3 | Tests | `liquers-lib` unit tests (`src/mem/`, `store_config.rs`) | Editor, tiers bridge, layout, write path, `markdown_tiers` | AC-1, AC-4, AC-6, AC-7, AC-16, AC-17 |
| 4 | Tests | `liquers-lib/tests/mem_commands.rs` | Every command through query evaluation | AC-1 to AC-8, AC-11 to AC-13, AC-15 to AC-17, AC-19 |
| 5 | Tests | `liquers-mcp` unit and `tests/mcp_roundtrip.rs` | Key filling, allow-list, errors and notes over a duplex transport | AC-14 |
| 6 | Tests | `liquers-agent/tests/` | The real tool set over stdio, the specs profile, the `docs_index.py` oracle, S3 | AC-3, AC-6, AC-9, AC-14, AC-18, AC-20 |

## Example 1: An agent session over the specs

An agent starts work on `NO-MARKDOWN-NAVIGATION-COMMANDS` and records it in the store reference.
It runs `liquers-agent mcp` with the "liquers-specs" profile (Phase 2). Each tool call becomes one
query: the key argument is filled into the resource segment and the other arguments are
overrides of the last action.

| Call | Query evaluated | Result (abridged) |
|---|---|---|
| `memory_overview {}` | `-R-key/./-/ns-mem/overview/overview.md` | Markdown: the README's L0/L1, each collection with its count (illustrative: `issues 334 · designs 183 · …`), the last ten log lines |
| `memory_search {path: "specs/issues", text: "markdown sections", filter: "-status:closed*", limit: 5}` | `-R-key/specs/issues/-/ns-mem/search/search.md` | A pipe table `key \| l0 \| l1 \| excerpt \| score …`, and, in a second content block, the note `shown 5 of 12; raise limit to see more` |
| `memory_read {key: "specs/issues/NO-MARKDOWN-NAVIGATION-COMMANDS.md", section: "Problem"}` | `-R/specs/issues/NO-MARKDOWN-NAVIGATION-COMMANDS.md/-/ns-mem/read/read.md` | The `## Problem` section only; the notes block carries `key …` and `version 3f9c…` (32 hex digits) |
| `memory_edit {key: "specs/issues/NO-MARKDOWN-NAVIGATION-COMMANDS.md", field: "status", value: "in_progress", expected_version: "3f9c…", reason: "starting work"}` | `-R-key/specs/issues/NO-MARKDOWN-NAVIGATION-COMMANDS.md/-/ns-mem/edit/edit.json` | `{"key": …, "version_before": "3f9c…", "version_after": "81d0…"}` |
| `memory_edit {key: "specs/reference/STORE_SEMANTICS.md", old: "no section navigation", new: "section navigation: NO-MARKDOWN-NAVIGATION-COMMANDS", section: "History", row: "\| 2026-10-17 \| Linked the section-navigation issue. \| NO-MARKDOWN-NAVIGATION-COMMANDS \|"}` | `-R-key/specs/reference/STORE_SEMANTICS.md/-/ns-mem/edit/edit.json` | One write (the `old` text stands for a sentence in that document): the text replaced, the row appended, `reviewed:` set to today, `updated:` stamped |

After the session the issue's diff is the `status:` line plus `updated:`; the reference's diff is
the replacement, the row and two stamps. No file in `specs/` has `updated:` today, so the first
edit of each adds that line (AC-7); `docs_index.py --check` has no rule against extra fields. `memory/_log/2026-10.md` has gained two lines of the form
`- 2026-10-17T18:20:04Z edit specs/issues/NO-MARKDOWN-NAVIGATION-COMMANDS.md 3f9c1a2b→81d0e4f5 — starting work`.
Had the issue's status been `closed`, the transition table would have refused the first edit,
naming `closed → in_progress`, and nothing would have been written. (Moving an issue to
`accepted` is a human's call, `DOCS_STRUCTURE_GUIDE.md` §4.8; the profile's transition table
reflects that.)

## Example 2: A power user

The same agent reads `liquers://memory/query-guide` and asks for every draft issue, sorted by L0,
as CSV:

```
memory_query {query: "-R-key/specs/issues/-/ns-mem/list-1-false-1000/ns-rec/eq-status-draft/ns-rec/sort-l0/drafts.csv"}
```

`check_allowed` finds `-mem-list`, `-rec-eq` and `-rec-sort`, all allowed. The value is a
record view and the filename makes the engine write it as CSV; the result parses as CSV because
notes travel in their own content block. `list`'s note counts what `list` returned
(`shown 334 of 334`), before `eq` filters it; the query guide says so. The same query ending in `drafts.md` gives a pipe table. `memory_query` with
`-R-key/specs/issues/X.md/-/ns-mem/edit-…` is refused before evaluation, naming `-mem-edit` as
outside the allow-list.

## Corner Cases: Edge and Error Cases

| Symptom | Expected behaviour | Test |
|---|---|---|
| A key argument `x/-/ns-mem/write-a`, `../outside`, `/abs`, `-R` | Refused by `fill_key`, nothing evaluated | `fill_key_rejects_query_shaped_keys` |
| A write-capable command hidden in a link argument of an allowed one | Refused: links are planned and checked | `check_allowed_walks_link_queries` |
| `read`'s version differs from `commit`'s on the same bytes | Every `expected_version` would fail; the test compares them over every `.md` file of a copy of the repository | `read_version_matches_commit_version` |
| `edit` with `old` found twice, together with a valid `field`/`value` | Refused with the count; nothing written, no log line | `failed_replace_writes_nothing` |
| `edit` with `old` but no `new`, or with no pair | Refused naming the missing argument, before reading | `half_given_pair_is_refused` |
| The file edited by hand between two reads (asset loaded) | `read` and `key/info` show the new front-matter | `front_matter_projection_follows_external_edit` |
| A log append fails after the data write | The write stands; the error names the missing log line | `log_failure_keeps_the_write` |
| A command panics inside a tool call | A tool error, the server keeps serving | `panicking_command_is_a_tool_error` |
| A hidden argument (`root`) passed to a tool | Refused, naming it | `hidden_argument_is_refused` |
| `expected_version` not 32 hex digits | `Error::parse_error` | `malformed_expected_version_is_a_parse_error` |
| A CSV result with notes | The CSV parses; notes are a separate block | `list_composes_with_rec_and_formats`, `notes_are_a_separate_content_block` |
| Many writes in one long-lived server | Retained assets stay bounded | `many_writes_do_not_grow_retained_assets` |

## Test Plan

Run with `cargo test -p liquers-lib --lib --tests` (`mem` is a default feature),
`cargo test -p liquers-mcp`, and `cargo test -p liquers-agent` (not a default member; needs
`python3` for the oracle). `liquers-mcp` depends only on `liquers-core`, so its tests use a
small stand-in command set; the real tool set is tested from `liquers-agent`. The round trips
need `rmcp` with `client` and `transport-async-rw` as dev-dependency features.

**`liquers-lib` unit tests**

- `src/mem/front_matter.rs` — `set_field_changes_one_line` (AC-16): bytes outside the changed
  line are identical; `set_list_field_writes_flow_list` (AC-16).
- `src/mem/tiers.rs` — `default_tiers_read_title_and_description` (AC-1);
  `first_nonempty_source_wins` (AC-17); `section_source_takes_first_paragraph` (AC-17).
- `src/mem/layout.rs` — `field_default_fills_status_draft` (AC-4).
- `src/mem/write.rs` — `edit_applies_every_pair_once` (AC-16, AC-7): one version check, one
  write, one log line; `half_given_pair_is_refused` (AC-16); `failed_replace_writes_nothing`
  (AC-16); `log_failure_keeps_the_write` (AC-7); `malformed_expected_version_is_a_parse_error`
  (AC-6).
- `src/store_config.rs` — `markdown_tiers_fills_only_empty_fields` (AC-17).

**`liquers-lib/tests/mem_commands.rs`** (`#![cfg(feature = "mem")]`). An environment with an
`AsyncMemoryStore`, a fixture `_schema/memory.yaml` and ten documents; temporary filesystem
directories where files are the point. Each test evaluates queries as the tools would.

- `list_entries_carry_l0_l1` (AC-1)
- `search_ranks_and_filters_before_limit` (AC-2): descending score, no `closed*`, the
  `shown/total` note counting only unhidden matches
- `read_section_returns_section_and_version_note` (AC-3)
- `new_returns_candidates_then_writes_with_force` (AC-4)
- `write_refuses_value_outside_vocabulary` (AC-5); `create_without_title_or_description_is_refused`
  (AC-5)
- `stale_expected_version_is_refused` (AC-6)
- `write_stamps_and_logs` (AC-7)
- `supersedes_marks_old_and_hides_it` (AC-8)
- `front_matter_projection_follows_external_edit` (AC-11): twice, with the asset loaded and not
  loaded
- `list_and_search_do_not_evaluate_recipes` (AC-12)
- `read_only_mount_refuses_write_and_leaves_no_sidecar` (AC-13)
- `overview_and_describe_orient` (AC-15)
- `edit_combined_pairs_and_forbidden_transition` (AC-16)
- `fallback_tiers_without_front_matter` (AC-17): a document and a folder `README.md`
- `openviking_layout_lists_creates_and_searches` (AC-19)
- `many_writes_do_not_grow_retained_assets` (AC-7): 500 edits; the asset manager's live count
  stays bounded (Phase 2 preflight)

**`liquers-mcp`**

- Unit (`src/schema.rs`, `src/decl.rs`, `src/server.rs`): `fill_key_rejects_query_shaped_keys`
  (AC-14); `fill_key_encodes_relative_to_the_base` (AC-14): `.` + `specs/issues` encodes as
  `-R-key/specs/issues`; `check_allowed_walks_link_queries` (AC-14);
  `format_argument_changes_the_filename` (AC-14); `build_refuses_multi_action_tool_queries`
  (AC-14); `hidden_argument_is_refused` (AC-14).
- `tests/mcp_roundtrip.rs`, over `tokio::io::duplex` with an `rmcp` client:
  `tool_error_is_a_result_not_a_protocol_error` (AC-14); `notes_are_a_separate_content_block`
  (AC-14); `panicking_command_is_a_tool_error` (AC-14).

**`liquers-agent/tests/`**

- `mcp_tools.rs` — `memory_tools_match_the_tool_table` (AC-14): spawns `liquers-agent mcp` over
  stdio, lists the nine basic tools and `memory_query`, compares names and argument schemas with
  Phase 2's table, and calls `memory_search`; `query_tool_refuses_a_write_command` (AC-14);
  `allow_list_namespaces_contain_no_writing_command` (AC-14): every `-rec-*` and `-search-*`
  command in the registry is checked against a list of commands known not to write;
  `query_guide_queries_validate` (AC-14): each query in `tools/query-guide.md` plans against the
  registry; `list_composes_with_rec_and_formats` (AC-14): Example 2's query parses as CSV and
  renders as Markdown.
- `specs_profile.rs`, over a temporary copy of the repository (`specs/`, `.claude/skills`,
  `memory/`, `scripts/docs_index.py`, and the crate sources that `specs/` links to):
  - `specs_profile_runs_the_scenarios` (AC-18): runs AC-1, AC-2, AC-3, AC-6, AC-7, AC-11, AC-13,
    AC-15, AC-16 and AC-17 against the profile; then checks that no file other than the edited
    ones changed. AC-4, AC-5, AC-8, AC-9 and AC-12 are covered against the same profile by the
    tests below and by `check_matches_docs_index_oracle`.
  - `read_version_matches_commit_version` (AC-3, AC-6): over every `.md` file in the copy,
    `read`'s version equals `Version::from_content` of the stored bytes. A non-UTF-8 file is
    refused by `read` with `Error::general_error`, and the test counts such files as skipped.
  - `check_matches_docs_index_oracle` (AC-9): runs `python3 scripts/docs_index.py --check` in
    the copy and maps each `error:` line to a rule name by message pattern (`no front-matter`,
    `duplicate id`, `does not match filename`, `design … does not exist`, `superseded_by …`,
    `§`-tagged rules, …), dropping the rules `check` declares unsupported (`index.csv is stale`,
    `index.md is stale`). The set of `(file, rule)` pairs must equal `liquers-agent check`'s,
    once on the copy and once after seeding one violation per rule. It also asserts AC-9's second
    clause: `check` lists the unsupported rules.
  - `storage_none_listing_cost` (AC-1): times `memory_list` over `specs/issues` (334 files) with
    `storage: none`; fails above 2 s in debug, and records the figure for Phase 5.
- `s3_profile.rs` — `s3_mount_round_trip` (AC-20): write, read, list and search through the tools
  against `s3s-fs`, restart the server, read again.

## Documentation and Learning Log

- `-R-key/<path>` evaluates to a `Value::Key` without touching the store, which is why folder
  commands take it: `-R/<folder>` fails today (`DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE`).
- `liquers-validate --command mem/<name>` validates queries for commands not yet registered.
- `docs_index.py` resolves the repository from its own location (`REPO`), so an oracle run on a
  fixture copies the script into the fixture.
- Generic `Value::Object` serializes only to JSON; record views write `md`, `csv`, `json`, `html`
  and more. This is what makes Markdown tables the agent-facing default (Phase 2 decision 6).
