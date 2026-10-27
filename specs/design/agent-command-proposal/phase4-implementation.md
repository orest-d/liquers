# Phase 4: Implementation Plan — agent-command-proposal

## Overview

Prerequisite: `post-init-command-registration` implemented (its Phase 4), which supplies
`EnvRef::update_commands`. Steps 1-2 do not need it and may land first. Then come the registration
backend, the two tools, start-up restore, the `liquers-agent` profile setting, and the
documentation. All the code is in `liquers-mcp` and `liquers-agent`.

## Progress

- [ ] Step 1: declaration types and build-time checks
- [ ] Step 2: proposal parsing and validation helpers
- [ ] Step 3: the registration backend and `with_proposals`
- [ ] Step 4: the propose and list tools
- [ ] Step 5: start-up restore and the `liquers-agent` profile setting
- [ ] Step 6: integration tests
- [ ] Step 7: documentation

## Implementation Steps

### Step 1: declaration types and build-time checks
- Files / symbols: `liquers-mcp/src/decl.rs` `ProposalsDecl`, `ToolSetDecl::proposals`, the
  `default_*` functions; `liquers-mcp/src/server.rs` `ToolEntry::{Propose, Proposals}`,
  `ProposalTool`, `ToolEntry::set`, `ToolRegistry::build`, `ToolRegistry::mcp_tools`;
  `liquers-mcp/src/schema.rs` (the two fixed input schemas); `liquers-mcp/src/lib.rs` re-exports.
- Change: `build` adds `<prefix><propose_tool>` and `<prefix><list_tool>` through `insert_unique`.
  It refuses with `Error::configuration_error` a `proposals` without `query_tool`, and a realm that
  is empty, in `NON_EXECUTABLE_REALMS`, or contains `-` or `/`. `McpServer::recipe` / `plan` refuse
  the new entries ("does not evaluate a query"). The new match arms are explicit.
- Depends on: none.
- Proof: `cargo test -p liquers-mcp proposals_declaration_is_checked`; the existing
  `cargo test -p liquers-agent --test mcp_tools` still passes (tool table unchanged).
- Rollback: revert; the field is `#[serde(default)]`, so no configuration depends on it yet.

### Step 2: proposal parsing and validation helpers
- Files / symbols: `liquers-mcp/src/server.rs` private `struct Proposal`, `fn parse_proposal(
  tool: &ProposalTool, set: &ToolSetDecl, cmr: &CommandMetadataRegistry, arguments: &Map<String,
  Value>, proposer: &str, session: &str) -> Result<Proposal, Error>`, and `fn proposal_file(decl:
  &ProposalsDecl, name: &str) -> Result<Key, Error>`.
- Change: the argument and target checks of Phase 2 (`parse_query` on the name, `allowed` against
  `set.query_tool`, `find_command`, `CommandDefinition::Registered`, `NON_EXECUTABLE_REALMS`). Heads
  are wrapped as `CommandParameterValue::Value`. The remaining arguments are copied from the target
  and the overrides applied. The `proposal` hint is built with `BackendTime::now`.
- Depends on: Step 1.
- Proof: unit tests in `server.rs` `mod tests` for each refusal of Phase 3's table that needs no
  environment (`parse_proposal_refuses_*`). They are implementation tests and cite no scenario.
- Rollback: revert; nothing calls the helpers yet.

### Step 3: the registration backend and `with_proposals`
- Files / symbols: `server.rs` private `trait ProposalBackend`, `struct RegistryBackend<E>`,
  `McpServer` fields `proposals` and `session`, `McpServer::new` (the session),
  `McpServer::with_proposals`.
- Change: `stage` runs one `envref.update_commands` per call. Inside the closure, per proposal:
  check for an existing key unless `replace`, call `CommandRegistry::register_alias`, set `label` /
  `doc` / `hints["proposal"]`, and return `Err` when `CommandMetadata::check()` has errors.
  `roll_back` runs `cr.unregister(key)` for a new key and re-registers the returned previous alias otherwise. Re-open `update_commands`' signature as merged before
  writing this step, and adapt the closure type to it. Check that its future is `Send`: that
  design boxes a non-`Send` `dyn FnOnce` for `update_command_registry`. If it is not, run the
  synchronous registry update in `tokio::task::spawn_blocking` and await only the refresh; dropping
  `+ Send` from `BoxedFuture` works only if rmcp's `call_tool` future need not be `Send`.
- Depends on: Step 2 and `post-init-command-registration` merged.
- Proof: `cargo check -p liquers-mcp`; exercised by Step 6.
- Rollback: revert Steps 3-5 together.

### Step 4: the propose and list tools
- Files / symbols: `server.rs` `McpServer::call` (dispatch for the two entries before `recipe`),
  `async fn propose`, `async fn list_proposals`.
- Change: `propose` does `parse_proposal`, then `stage`, then `AsyncStore::set` of the file (a JSON
  `CommandMetadataRegistry::new()` holding the staged metadata, read back from the live registry),
  and on a write error `roll_back` with the previous metadata `stage` returned (restore a replaced proposal, unregister a new one). It returns `ToolOutput { format: "json", .. }`. `list_proposals`
  filters `get_command_metadata_registry().commands` by realm.
- Depends on: Step 3.
- Proof: Step 6 tests 3, 5, 8, 9 and 10.
- Rollback: as Step 3.

### Step 5: start-up restore and the `liquers-agent` profile setting
- Files / symbols: `server.rs` `McpServer::restore_proposals`; `liquers-agent/src/lib.rs`
  `MemorySettings::proposals`, `build_server_with` (sets the built-in set's `proposals` from it;
  `memory_tool_set` is unchanged).
- Change: Phase 2 §restore. `build_server_with` calls `.with_proposals()` and then
  `restore_proposals().await` right after `McpServer::new`, before its early return for
  `!builtin || !overview` (`liquers-agent/src/lib.rs`), so custom tool sets and the CLI path
  restore too. It writes each warning with `eprintln!`, and appends "N staged command
  proposals" to the instructions when N > 0. Without a `proposals` section, nothing is read.
- Depends on: Step 4.
- Proof: Step 6 tests 6 and 7.
- Rollback: as Step 3.

### Step 6: integration tests
- Files / symbols: new `liquers-agent/tests/proposals.rs` (its own `fixture` helper with the
  `proposals` line, as `mcp_tools.rs` builds one); the test `proposals_declaration_is_checked` from
  Step 1.
- Change: the tests of Phase 3, one per scenario.
- Depends on: Step 5.
- Proof: `cargo test -p liquers-agent --test proposals` and `cargo test -p liquers-mcp`.
- Rollback: not needed; tests only.

### Step 7: documentation
- Files / symbols: `specs/reference/AGENT_MEMORY_SERVICE.md` §9 and §10;
  `specs/reference/COMMAND_ALIASES.md` §1; `specs/reference/REALMS.md`;
  `specs/guides/COMMAND_REGISTRATION_GUIDE.md` ("Promoting a proposed alias"); `specs/README.md`.
- Change: as Phase 2 §Documentation Architecture, each with a `## History` row and a `reviewed:`
  bump. The source issue is closed with a resolution note, and the Python follow-up is filed as an
  issue (`AGENT-CANNOT-PROPOSE-A-PYTHON-COMMAND`, `requires` this design).
- Depends on: Step 6.
- Proof: `python3 scripts/docs_index.py --check`; every query in the new text passes
  `liquers-validate` (with the overlay where it uses `-staging/`).
- Rollback: revert the text with the code.

## Testing Plan

After each code step: `cargo test -p liquers-mcp`. After Steps 5-6:
`cargo test -p liquers-agent --test proposals --test mcp_tools`. Final check:
`cargo test -p liquers-mcp` and `cargo test -p liquers-agent` (the agent crate needs Rust 1.96 for
`s3s`, per `CLAUDE.md`). No `cfg(feature)` or optional dependency changes, so the build matrix is
not needed. `specs/command_registry.yaml` does not change, so `registry_export` is unaffected.

## Rollback Plan

Steps 1-2 are inert without Steps 3-5 and may stay. Reverting Steps 3-5 removes the capability.
Proposal files left in a store are then never read. With only Steps 1-2 landed, `liquers-agent`
has no `memory.proposals` setting (Step 5), so no shipped profile can enable the tools. A custom
`memory.tools` set that declares `proposals` gets the two tools listed, but a call reaches
`McpServer::recipe` (dispatch arrives in Step 4) and fails with "does not evaluate a query".

## Documentation Updates

`AGENT_MEMORY_SERVICE.md`, `COMMAND_ALIASES.md`, `REALMS.md` and `COMMAND_REGISTRATION_GUIDE.md`
(Step 7), with History rows and `reviewed:` bumps; `specs/README.md` links this design;
`specs/index.csv` regenerated with `docs_index.py`. No generated registry change.

## Phase 5 Entry Criteria

None — four-phase design.
