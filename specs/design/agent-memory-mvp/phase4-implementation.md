# Phase 4: Implementation Plan — Agent Memory Service (MVP)

## Overview

Bottom up: the `ns-mem` data model and write path in `liquers-lib` first (Steps 1–5), then the
commands (6–8), then the generic MCP engine (9, which depends only on `liquers-core` and can run in
parallel with 2–8), then the binary, the profiles and the end-to-end tests (10–12), and last the
build matrix (13). Each step lands as its own commit on this branch with its proof passing.

No prerequisite issue must be fixed first (Phase 2 preflight). `liquers-template` (PR #139) is not
needed: `memory_new` copies a template as it is. Disk: the session allowance is 30 GB, so the build
matrix runs one row at a time, deleting `target/` when space is low (`BUILD-MATRIX-EXHAUSTS-SESSION-DISK`).
Signatures re-opened at HEAD 2026-10-17: `register_lib_store_config` (`liquers-lib/src/store_config.rs`),
`register_all_commands!` (`liquers-lib/src/commands.rs`), `Group` (`liquers-lib/src/bin/export_command_registry.rs`),
`AssetManager::set_binary` and `listdir_asset_info` (`liquers-core/src/assets.rs`), `Context::info`
(`liquers-core/src/context.rs`), `default_environment_builder` (`liquers-lib/src/environment.rs`),
`EnvironmentBuilder::with_config` (`liquers-core/src/environment_config.rs`), `liquers-lib/src/search/`
(`catalog.rs`, `md_split.rs`, `extract.rs`).

## Progress

- [ ] Step 1: `mem` feature, module skeleton and registry group
- [ ] Step 2: schema types (`layout.rs`)
- [ ] Step 3: line-preserving front-matter editor (`front_matter.rs`)
- [ ] Step 4: L0/L1 bridge (`tiers.rs`) and the `markdown_tiers` handler
- [ ] Step 5: write path (`write.rs`): commit, log, edit pairs
- [ ] Step 6: read commands: overview, describe, list, search, read, similar
- [ ] Step 7: `check` and its rules
- [ ] Step 8: write commands: new, write, edit; registry regenerated
- [ ] Step 9: `liquers-mcp` crate
- [ ] Step 10: `liquers-agent` binary and the built-in tool set
- [ ] Step 11: the "liquers-specs" profile, `memory/` folder and repository tests
- [ ] Step 12: the "openviking" profile and the S3 test
- [ ] Step 13: build matrix and final checks

## Implementation Steps

### Step 1: `mem` feature, module skeleton and registry group
- Files / symbols: `liquers-lib/Cargo.toml` (feature `mem = ["records", "dep:indexmap"]` in `default`,
  optional `indexmap` 2 with `serde`); `liquers-lib/src/mem/mod.rs` (`register_mem_commands!` and a
  no-op stand-in under `not(feature = "mem")`); `liquers-lib/src/lib.rs`; `register_all_commands!`;
  `Group::Mem` in the exporter; `liquers-lib/tests/registry_export.rs` gate and `full_registry()`.
- Depends on: none.
- Proof: `cargo check -p liquers-lib` and `cargo check -p liquers-lib --no-default-features`.
- Rollback: revert the commit; nothing else references `mem` yet.

### Step 2: schema types
- Files / symbols: `liquers-lib/src/mem/layout.rs` — `MemoryLayout` (with `version`),
  `CollectionSchema`, `FieldRule`, `IdRule`, `IdSource`, `IdCase`, `FieldEquals`, `FieldType`,
  `TierSources`, `TierSource` (text form `kind:arg`), `MemoryLayout::load(context, root)` reading
  `<root>/_schema/memory.yaml` through the store, `collection_for(key)` (longest folder prefix).
- Depends on: Step 1.
- Proof: `cargo test -p liquers-lib --lib mem::layout` — `field_default_fills_status_draft`; the
  version check behind `newer_schema_version_is_refused` lands here as a unit test.
- Rollback: revert; no caller yet.

### Step 3: front-matter editor
- Files / symbols: `liquers-lib/src/mem/front_matter.rs` — `FrontMatterDoc::{parse, get, set, body,
  render}`, `compose`, `parse_version`.
- Depends on: Step 1. Parallel with Step 2.
- Proof: `cargo test -p liquers-lib --lib mem::front_matter` — `set_field_changes_one_line`,
  `set_list_field_writes_flow_list`; `malformed_expected_version_is_a_parse_error`.
- Rollback: revert; no caller yet.

### Step 4: L0/L1 bridge and `markdown_tiers`
- Files / symbols: `liquers-lib/src/mem/tiers.rs` — `resolve(sources, record, text) -> (l0, l1)` and
  the shared `first_heading`, `first_paragraph`, `section_paragraph` (over `search/md_split.rs`
  `sections`); `liquers-lib/src/store_config.rs` — `markdown_tiers(sections: Vec<String> multiple)`
  at stages `Default` and `Read`, registered in `register_lib_store_config`.
- Depends on: Steps 2, 3.
- Proof: `cargo test -p liquers-lib --lib mem::tiers store_config` — `default_tiers_read_title_and_description`,
  `first_nonempty_source_wins`, `section_source_takes_first_paragraph`,
  `markdown_tiers_fills_only_empty_fields`; `liquers-validate` on the profile's `on_read` spec.
- Rollback: revert; the handler is only reachable from configuration.

### Step 5: write path
- Files / symbols: `liquers-lib/src/mem/write.rs` — `WriteRequest`, `WriteOp` (`Edit(Vec<EditOp>)`),
  `EditOp::{Replace, SetField, AppendRow}`, `WriteOutcome`, `commit`, `append_log`,
  `apply_edits` (fixed order, all-or-nothing), the `history` stamp.
- Depends on: Steps 2, 3.
- Proof: `cargo test -p liquers-lib --lib mem::write` — `edit_applies_every_pair_once`,
  `half_given_pair_is_refused`, `failed_replace_writes_nothing`, `log_failure_keeps_the_write`.
- Rollback: revert; no command uses it yet.

### Step 6: read commands
- Files / symbols: `liquers-lib/src/mem/commands.rs` (`overview`, `describe`, `list`, `search`,
  `read`, `similar`, registered per Phase 2 with their `filename:`); `liquers-lib/src/mem/read.rs`
  (record views built with `liquers_records`, notes through `context.info`); key-state conversion
  via `TryFrom<Value> for Key`; document state for `read`.
- Depends on: Steps 2, 4.
- Proof: `cargo test -p liquers-lib --test mem_commands` — `list_entries_carry_l0_l1`,
  `search_ranks_and_filters_before_limit`, `read_section_returns_section_and_version_note`,
  `list_and_search_do_not_evaluate_recipes`, `overview_and_describe_orient`,
  `fallback_tiers_without_front_matter`, `front_matter_projection_follows_external_edit`,
  `read_only_mount_refuses_write_and_leaves_no_sidecar` (read half), `mem_results_are_never_stored`,
  `newer_schema_version_is_refused`, `recipe_results_drop_back_to_recipe`.
- Rollback: revert; remove the registrations.

### Step 7: `check`
- Files / symbols: `liquers-lib/src/mem/check.rs` — schema rules and the named built-in rules
  (`scenarios`, `readiness`, `compact_form`, `dead_links`, `reviewed`/`history`), the `unsupported`
  list; `check` command.
- Depends on: Step 6.
- Proof: unit tests per rule in `check.rs`; the oracle comparison runs in Step 11.
- Rollback: revert; `check` is independent of the other commands.

### Step 8: write commands and registry
- Files / symbols: `new`, `write` (with `supersedes`), `edit` in `commands.rs`;
  `specs/command_registry.yaml` regenerated with
  `cargo run -p liquers-lib --features cli --bin export-command-registry -- --format yaml -o specs/command_registry.yaml`
  and a CHANGELOG line.
- Depends on: Steps 5, 6.
- Proof: `cargo test -p liquers-lib --test mem_commands` — `new_returns_candidates_then_writes_with_force`,
  `write_refuses_value_outside_vocabulary`, `create_without_title_or_description_is_refused`,
  `stale_expected_version_is_refused`, `write_stamps_and_logs`, `supersedes_marks_old_and_hides_it`,
  `edit_combined_pairs_and_forbidden_transition`, `read_only_mount_refuses_write_and_leaves_no_sidecar`,
  `openviking_layout_lists_creates_and_searches`, `many_writes_do_not_grow_retained_assets`;
  `cargo test -p liquers-lib --test registry_export`.
- Rollback: revert and regenerate the registry.

### Step 9: `liquers-mcp` (parallel with Steps 2–8)
- Files / symbols: new crate `liquers-mcp/` (workspace member and default member; `liquers-core`,
  `rmcp` 3.5.1 `server` + `transport-io`, dev: `client` + `transport-async-rw`); `src/decl.rs`
  (`ToolSetDecl`, `ToolDecl`, `KeyArgumentDecl`, `QueryToolDecl`, `ResourceDecl`); `src/schema.rs`
  (`argument_schema`); `src/server.rs` (`ToolRegistry::build`, `fill_key`, `check_allowed`,
  `McpServer::{new, plan, call}`, `ServerHandler`, notes as a second content block); `src/lib.rs`
  (`serve_stdio`).
- Depends on: none in this design.
- Proof: `cargo test -p liquers-mcp` — the unit tests and `tests/mcp_roundtrip.rs` of Phase 3.
- Rollback: remove the crate from the workspace.

### Step 10: `liquers-agent` and the tool set
- Files / symbols: new crate `liquers-agent/` (member, not a default member); `src/config.rs`
  (`AgentConfig`, `MemorySettings`); `src/main.rs` (environment build order of Phase 2,
  subcommands `mcp`, `check`, `search`, `read`, `query`; notes to stderr); `tools/memory.yaml`;
  `tools/query-guide.md`.
- Depends on: Steps 8, 9.
- Proof: `cargo test -p liquers-agent --test mcp_tools` — `memory_tools_match_the_tool_table`,
  `query_tool_refuses_a_write_command`, `allow_list_namespaces_contain_no_writing_command`,
  `query_guide_queries_validate`, `list_composes_with_rec_and_formats`.
- Rollback: remove the crate.

### Step 11: "liquers-specs" profile
- Files / symbols: `memory/README.md`, `memory/_schema/{memory,areas,issue-status}.yaml`,
  `memory/liquers-specs.yaml`; `liquers-agent/tests/specs_profile.rs`.
- Depends on: Steps 7, 10.
- Proof: `cargo test -p liquers-agent --test specs_profile` — `specs_profile_runs_the_scenarios`,
  `read_version_matches_commit_version`, `check_matches_docs_index_oracle`,
  `storage_none_listing_cost`; `python3 scripts/docs_index.py --check` still clean.
- Rollback: delete `memory/` and the test; no `specs/` file is changed by this step.

### Step 12: "openviking" profile and S3
- Files / symbols: `liquers-agent/tests/s3_profile.rs`, dev-dependency `s3s-fs` (with `s3s`),
  `liquers-store` feature `services-s3` for `liquers-agent`.
- Depends on: Step 10.
- Proof: `cargo test -p liquers-agent --test s3_profile` — `s3_mount_round_trip`. If `s3s-fs` cannot
  serve OpenDAL's S3 client, mark the test `#[ignore]` behind `LIQUERS_TEST_S3_ENDPOINT` and record
  the deviation from AC-20 here.
- Rollback: drop the dev-dependency and the test.

### Step 13: build matrix and final checks
- Files / symbols: `scripts/check-build-matrix.sh` rows with and without `mem` (native and wasm32).
- Depends on: Steps 1–12.
- Proof: the new rows, then the rest one row at a time; `cargo test -p liquers-lib --lib --tests`;
  `cargo test -p liquers-mcp`; `cargo test -p liquers-agent`; `python3 scripts/docs_index.py --check`.
- Rollback: revert the script rows.

## Agent Assignment

One implementing session works the steps in order. Step 9 may go to a second, parallel session
or worker on its own branch, since it touches only the new `liquers-mcp` crate; it merges back
before Step 10. Reviews after Steps 8 and 13 are independent reviewers.

## Testing Plan

Each step's proof runs before its commit. After Step 8 and after Step 13, the full
`cargo test -p liquers-lib --lib --tests`. The known intermittent
`volatile_dependency_has_one_value_per_evaluation` is rerun once if it fails, and not counted
against this change if it passes (`EVALUATION-SESSION-VOLATILE-TEST-INTERMITTENT`).

## Rollback Plan

The change is additive: two new crates, one new default feature and one new handler. Whole
rollback: revert the branch; or, after a partial landing, remove `mem` from `default` and the two
crates from the workspace. Memory files are plain Markdown and stay usable without the service.

## Documentation Updates

Written in Phase 5 from what was built (Phase 2 §Documentation Architecture):
`reference/AGENT_MEMORY_SERVICE.md` (new), `guides/AGENT_MEMORY_MIGRATION_GUIDE.md` (new, from
`migration-guide-draft.md`), `.claude/skills/liquers-memory/SKILL.md` (new), updates to
`STORE_CONFIG_GUIDE.md`, `COMMAND_REGISTRATION_GUIDE.md`, `PROJECT_OVERVIEW.md`,
`DOCS_STRUCTURE_GUIDE.md`, `CLAUDE.md` and `specs/README.md`, each reference or guide change with a
History row and a `reviewed:` bump. Regenerated: `specs/command_registry.yaml` (Step 8),
`specs/index.csv` and `specs/index.md` with `scripts/docs_index.py`.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
