# Phase 2: Solution & Architecture — agent-command-proposal

## Overview

A tool set may declare `proposals`; `ToolRegistry::build` then adds `<prefix>propose` and
`<prefix>proposals`. A propose call builds an alias of an allowed target, registers it in the
staging realm with `EnvRef::update_commands`, and writes it as a one-command
`CommandMetadataRegistry` JSON file. The list tool reads the live registry; at start
`McpServer::restore_proposals` registers the files again. Registration needs the concrete
`CommandRegistry<E>`, so `McpServer::with_proposals` (which carries that bound) installs it behind a
private trait object. Written for the recommended answers to questions 1-5.

Rejected: a registering *command* (a command cannot reach `EnvRef`); tools in `liquers-agent`
only (the engine would need a hook anyway); a `CommandMetadata` provenance field (question 4); one
shared proposals file (read-modify-write races, not reviewable one by one).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `POST-INIT-COMMAND-REGISTRATION` | draft | P3 | provides `update_commands`, which this design calls | Yes | Yes, for Steps 3-6 | implement `post-init-command-registration` first; question 1 |
| `AGENT-WORKBENCH-SURFACE` | draft | P2 | adds other tool kinds to the same enum | No | No | rebase whichever lands second |
| `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` | draft | P2 | the allow-list stays the safety boundary, as for the query tool | No | No | none |

The blocker is a design dependency (P3), not a defect: the dependency, not priority, sets the order.

## Interfaces

`liquers-mcp/src/decl.rs`:

```rust
pub struct ToolSetDecl { /* existing fields */ #[serde(default)] pub proposals: Option<ProposalsDecl> }

/// Lets agents stage alias commands. Requires the set's `query_tool`; targets must match its allow-list.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposalsDecl {
    /// Store key of the folder holding one `<name>.json` per proposal, relative to the store root.
    pub folder: String,
    #[serde(default = "default_realm")]       pub realm: String,        // "staging"
    #[serde(default = "default_propose")]     pub propose_tool: String, // "propose"
    #[serde(default = "default_list")]        pub list_tool: String,    // "proposals"
    #[serde(default = "default_proposer")]    pub proposer: String,     // "agent"
}
```

`liquers-mcp/src/server.rs`:

```rust
pub enum ToolEntry { Basic(BasicTool), Query(QueryTool), Propose(ProposalTool), Proposals(ProposalTool) }
#[derive(Clone, Debug)]
pub struct ProposalTool { pub decl: ProposalsDecl, pub set: usize, pub input_schema: Arc<Map<String, Value>> }

impl<E: Environment> McpServer<E> {
    /// Installs the registration backend. Without it, a propose call fails with `not_supported`.
    pub fn with_proposals(self) -> Self where E: Environment<CommandExecutor = CommandRegistry<E>>;
    /// Registers every proposal file of every set again. Returns one warning per skipped file.
    pub async fn restore_proposals(&self) -> Result<Vec<String>, Error>;
}

// private
struct Proposal { key: CommandKey, target: CommandKey, heads: Vec<CommandParameterValue>,
                  arguments: Vec<ArgumentInfo>, label: String, doc: String,
                  hint: serde_json::Value, replace: bool }
trait ProposalBackend: Send + Sync {
    fn stage<'a>(&'a self, proposals: Vec<Proposal>) -> BoxedFuture<'a, Result<(), Error>>;
    fn withdraw<'a>(&'a self, key: CommandKey) -> BoxedFuture<'a, Result<(), Error>>;
}
type BoxedFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
```

`McpServer` gains private `proposals: Option<Arc<dyn ProposalBackend>>` and `session: String`
(`BackendTime::now(TimePrecision::Seconds)`, `liquers_core::metadata`, taken in `new`). The backend `RegistryBackend<E> { envref: EnvRef<E> }` calls
`envref.update_commands(|cr| …)`: per proposal, refuse an existing key unless `replace`, call
`cr.register_alias(key, target, heads, arguments)`, set `label`, `doc` and `hints["proposal"]`, and
refuse when `CommandMetadata::check()` has errors. `withdraw` calls `cr.unregister(key)`. Updates
are async and serialized by `update_commands`; no lock is held across an `.await`.

Propose arguments: `name` and `doc` (strings, required), `target` (`{realm?, namespace?, name}`,
required), `head` (literal JSON values, default empty), `label`, `arguments` (optional
`{name?, label?, description?, default?}` per remaining target argument, by position; otherwise
copied from the target) and `replace` (default `false`). Checked before staging: `name` parses
(`parse_query`) as one action without parameters; the target matches the set's `query_tool.allow`
(`allowed`), is found with `definition: Registered`, and its realm is not in
`NON_EXECUTABLE_REALMS`. Each head is wrapped as `CommandParameterValue::Value(v)`, never
deserialized in the tagged form, so a link head cannot be written. After staging,
`<folder>/<name>.json` is written with `AsyncStore::set`; if that fails, the alias is withdrawn.
The result is JSON `{key, target, file, example}` (`example`: `-<realm>/<name>`).

The list tool (no arguments) returns, as JSON, every command of the live registry in the staging
realm: `key`, `target`, `heads`, `doc`, `proposal`, `file`. `restore_proposals` lists `<folder>`
(`listdir_keys`; absent means none), parses each `*.json` as a `CommandMetadataRegistry` and stages
its commands of the configured realm with `replace: true`, one `update_commands` per file. A file
that fails adds a warning naming it, and the rest still load (AC-4).

`liquers-agent/src/lib.rs`: `MemorySettings` gains `#[serde(default)] pub proposals:
Option<ProposalsDecl>`, which is attached to the built-in `memory_` set (a profile with its own
`memory.tools` declares `proposals` in its set instead). `build_server_with` calls
`McpServer::new(..).with_proposals()`, then `restore_proposals().await`, writes each warning with
`eprintln!` and appends a count to the instructions.

## Integration Points

- `liquers-mcp/src/decl.rs`: `ProposalsDecl`, the `proposals` field and the defaults.
- `liquers-mcp/src/server.rs`: the `ToolEntry` arms in `set`, `mcp_tools`, `recipe` / `plan`
  (refused: "does not evaluate a query") and `call` (dispatched before `recipe`);
  `ToolRegistry::build` adds the two tools and checks the declaration (the set has a `query_tool`;
  the realm is non-empty, executable and free of `-` and `/`); new `propose`, `list_proposals`,
  `restore_proposals`, `with_proposals` and the private backend.
- `liquers-mcp/src/schema.rs`: the two fixed input schemas.
- `liquers-mcp/src/lib.rs`: re-export `ProposalsDecl`, `ProposalTool`.
- `liquers-agent/src/lib.rs`: as above. `liquers-agent/tools/memory.yaml` is unchanged (off by
  default, AC-5).

## Error Handling

- Bad argument types: `Error::parse_error`. A missing name, target or doc, a reserved or invalid
  name (`IssueReport::short_summary`), or an existing key without `replace`: `Error::general_error`.
- Target outside the allow-list, a non-executable realm, or no backend installed:
  `Error::not_supported`, naming the target and the allow-list as `check_allowed` does.
- Target missing, an alias chain or a shape mismatch: `alias_target_not_registered`,
  `alias_chain_not_supported`, `invalid_alias`.
- Store failures: propagated after the withdraw. Restore: per-file warnings; an unreadable folder
  is an error.

## Relevant Commands

None added. Staged commands are aliases in realm `staging`, namespace `""`; targets are whatever
the query tool allows (`memory_`: the `mem` read commands, `rec`, `search`).
`specs/command_registry.yaml` is unchanged, since staged commands exist only at run time.

## Documentation Architecture

- `specs/reference/AGENT_MEMORY_SERVICE.md` (reference, mcp) §9: the `proposals` tool kind, its
  tools, the file format, `memory.proposals`.
- `specs/reference/COMMAND_ALIASES.md` §1: run-time aliases in a staging realm; the `proposal` hint.
- `specs/reference/REALMS.md`: a staging realm (executable, run-time only, not exported).
- `specs/guides/COMMAND_REGISTRATION_GUIDE.md`: "Promoting a proposed alias" (review with
  `liquers-validate --registry-file`, write `register_alias` in the governed crate, regenerate the
  registry, delete the file).
- Proposed `affects_docs`: the four above. `specs/README.md`: link this design.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-mcp/src/{decl,server,schema,lib}.rs`, `liquers-agent/src/lib.rs`, new tests in both crates |
| Crates and workflows affected | `liquers-mcp`, `liquers-agent`; two more tools, only when enabled |
| Existing tests likely to change | none (the built-in tool table is unchanged) |
| New validation | Phase 3 tests; `cargo test -p liquers-mcp`, `cargo test -p liquers-agent` |
| Compatibility / data | additive config key; `ToolEntry` gains variants, which breaks an outside exhaustive match (none in the workspace) |
| Concurrency | updates are serialized by `update_commands`; a query planned before an update keeps its snapshot (that design's AC-5) |
| Security | a staged alias is never more than its allowed target (Consolidated Findings); files are written only under `folder`; the name is parsed as one action, so it cannot form a path |
| Performance / recovery | one registry clone per proposal; delete the files and restart, or revert |
| Certainty | high on HEAD evidence; question 1 decides whether it can be built at all |
