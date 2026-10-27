# Phase 3: Examples & Use-cases — agent-command-proposal

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | An agent stages `drafts` and uses it | the whole loop | AC-1, AC-2, AC-3 |
| 2 | Example | A person reviews and promotes it | the human step | AC-9 |
| 3 | Test | `liquers-agent/tests/proposals.rs::propose_then_query_matches_the_direct_query` | same rows, no restart | AC-1 |
| 4 | Test | `…::proposal_carries_provenance` | the `proposal` hint | AC-2 |
| 5 | Test | `…::list_tool_shows_only_staged_commands` | listing | AC-3 |
| 6 | Test | `…::proposals_survive_a_restart_and_bad_files_are_skipped` | restore | AC-4 |
| 7 | Test | `…::no_proposals_section_means_no_tools` | off by default | AC-5 |
| 8 | Test | `…::proposal_cannot_escalate` | allow-list, alias, realm | AC-6 |
| 9 | Test | `…::proposal_breaking_alias_rules_is_refused` | shape, reserved and invalid names | AC-7 |
| 10 | Test | `…::reproposal_requires_replace` | replace | AC-8 |
| 11 | Test | `…::proposal_file_is_a_validation_overlay` | `ValidationRegistryBuilder` | AC-9 |
| 12 | Test | `liquers_mcp::server::tests::proposals_declaration_is_checked` | build-time checks | AC-10 |

## Example 1: An agent stages `drafts` and uses it

Profile (`liquers-agent.yaml`) excerpt:

```yaml
memory:
  root: mem
  proposals: { folder: _proposals }      # realm defaults to staging, proposer to agent
```

The agent calls the tools (the arguments are MCP JSON):

```
memory_propose   {"name": "drafts", "target": {"namespace": "rec", "name": "eq"},
                  "head": ["status", "draft"], "doc": "Rows whose status is draft"}
  → {"key": "staging--drafts", "target": "-rec-eq", "file": "_proposals/drafts.json",
     "example": "-staging/drafts"}
memory_query     {"query": "-R-key/./-/ns-mem/list-0-false-1000/-staging/drafts/drafts.csv"}
  → key,l0,l1,…   one row: CACHE-SERVES-STALE
memory_proposals {}
  → [{"key": "staging--drafts", "target": "-rec-eq", "heads": ["status", "draft"],
      "doc": "Rows whose status is draft",
      "proposal": {"proposed_by": "agent", "session": "2026-10-27T10:00:00Z",
                   "proposed_on": "2026-10-27T10:03:12Z"},
      "file": "_proposals/drafts.json"}]
```

The query plans to `GetAsset…`, `mem/list`, then one `Action` for `-rec-eq` with
`origin: Alias(staging--drafts)` and an init-step note "Command 'staging--drafts' is an alias of
'-rec-eq'". The query tool's allow-list check sees `-rec-eq`, which `-rec-*` allows.

## Example 2: A person reviews and promotes it

```bash
liquers-validate --registry-file specs/command_registry.yaml \
  --registry-file <store>/_proposals/drafts.json -- \
  '-R-key/./-/ns-mem/list-0-false-1000/-staging/drafts/drafts.csv'     # Ok
```

Promotion is the guide's procedure. Write `cr.register_alias(CommandKey::new("", "rec", "drafts"),
…)` in the governed crate, regenerate `specs/command_registry.yaml`, delete the file. Queries then
use `ns-rec/drafts`, and results built through `staging--drafts` are recomputed once, because the
key changed. Nothing in this design automates any of it.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| Target `mem/write` (not in the allow-list) | `not_supported` naming the target; no file, not registered | 8 |
| Target `rec/records_schema` (an alias) | `alias_chain_not_supported`; nothing written | 8 |
| Target `store_config/defaults`, in a custom set whose allow-list has `store_config-*` (the `memory_` allow-list would refuse it first) | refused with the non-executable message | 8 |
| Three heads for `rec/eq` | `invalid_alias` (ParameterError) | 9 |
| Name `q`, `a/b` or `drafts-1` | refused; the name is not one parameterless action, or is reserved | 9 |
| Store write fails (read-only mount) | error, and `-staging/drafts` is not registered afterwards | 6 |
| Proposal file whose target no longer exists | skipped at start with a warning naming the file | 6 |
| Unparsable proposal file | skipped with a warning; the other files load | 6 |
| A file of another realm in the folder | its commands ignored (not staged) | 6 |
| A head that is a JSON object `{"Query": "-R/x"}` | staged as a literal value, never as a link | 9 |

## Test Plan

- Integration (`liquers-agent/tests/proposals.rs`), on a copy of `tests/fixtures/memory`, as in
  `tests/mcp_tools.rs` (the `fixture` helper copied with a `proposals` line added):
  - `propose_then_query_matches_the_direct_query` (AC-1): Example 1. The CSV of the staged query
    equals that of `…/ns-rec/eq-status-draft/drafts.csv`.
  - `proposal_carries_provenance` (AC-2): the metadata of `staging--drafts` in
    `server.envref().get_command_metadata_registry()` has `hints["proposal"]` with the three keys;
    `proposed_by` is the profile's `proposer`.
  - `list_tool_shows_only_staged_commands` (AC-3): the JSON array has exactly the staged keys.
  - `proposals_survive_a_restart_and_bad_files_are_skipped` (AC-4): propose, rebuild the server
    from the same config with two extra files (`broken.json`, and `gone.json` targeting
    `rec/missing`); `drafts` still works; the warnings name both files. A read-only mount case
    checks the withdraw.
  - `no_proposals_section_means_no_tools` (AC-5): the plain fixture, with a valid
    `_proposals/drafts.json` present. No `memory_propose` tool; the staged query fails with "not
    registered".
  - `proposal_cannot_escalate` (AC-6), `proposal_breaking_alias_rules_is_refused` (AC-7): the
    table rows. After each refusal, the folder is empty and the key is absent. The non-executable
    case uses a second fixture whose `memory.tools` set allows `-rec-*` and `store_config-*` and
    declares `proposals`, and asserts the "cannot be executed" message, not the allow-list one.
  - `reproposal_requires_replace` (AC-8): a second `drafts` without `replace` is refused; with
    `replace` and heads `status`, `closed`, the staged query returns `WRITES-RACE`.
  - `proposal_file_is_a_validation_overlay` (AC-9): `ValidationRegistryBuilder::new()`, then
    `merge_str` of `specs/command_registry.yaml` and of the written file, then `build()`;
    `validate_query(staged query, 0, ValidationLevel::Plan, &cmr)` is `Ok`.
- Unit (`liquers-mcp/src/server.rs` `mod tests`):
  - `proposals_declaration_is_checked` (AC-10): `ToolRegistry::build` refuses a set with
    `proposals` but no `query_tool`, and the realms `""`, `store_config` and `a-b`.
- Commands to run: `cargo test -p liquers-mcp`, `cargo test -p liquers-agent --test proposals`.

## Learning Log

- An alias in a non-default realm may target a default-realm command. The planner emits the
  target's step and records the alias in `origin`, so allow-lists keyed on targets keep working.
  Verified with `target/debug/liquers-validate` and an overlay file (the overlay needs
  `default_namespaces` and `global_enums` beside `commands`, since it is a whole
  `CommandMetadataRegistry`).
- Validated queries (all `Ok`): the staged and the direct `drafts` queries above, with the overlay
  for the staged one.
