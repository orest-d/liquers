# Phase 2: Solution & Architecture — Agent Memory Service (MVP)

## Overview

Three layers, each usable without the next:

- **`ns-mem`** (`liquers-lib/src/mem/`, feature `mem`) holds the memory's bookkeeping as ten
  commands. They read the memory's schema from `<root>/_schema/memory.yaml`, read documents through
  the store and the search crate, and write through `AssetManager::set_binary`.
- **`liquers-mcp`** (a new crate over `liquers-core` and `rmcp` 3.5.1) serves any set of commands
  as MCP tools. A tool is a declared query whose last action is one command, and its input schema
  is generated from that command's `CommandMetadata`.
- **`liquers-agent`** (a new binary crate) reads one configuration file, builds the environment,
  and offers `mcp` (stdio), `check`, `search` and `read`.

Storage is configuration only: `read_only` mounts and `metadata` sections from `store-adapters`,
plus one new read handler for fallback tiers.

One decision shapes the rest. **Every memory tool calls exactly one `ns-mem` command, and every
tool argument is an argument of that command.** Overrides reach only the last action of a query
(`RECIPE-OVERRIDE-ONLY-LAST-ACTION`), never its resource segment. So the commands take their keys
as arguments, not as state. Composite behaviour lives inside the commands, not in the tool layer:
the duplicate check in `new`, the edit operations, and hiding superseded memories.

Rejected:

| Alternative | Why not |
|---|---|
| Composite tools assembled in `liquers-mcp` (`memory_new` = `similar` then `new`) | Puts memory semantics in the generic engine, against `workbench-direction.md` §2. The schema could no longer be generated from one command |
| Write body as the evaluation payload (`interfaces-and-mcp.md` §6) | Argument overrides already carry long, unescaped text by name. A payload type is specific to the environment, and `payload: required` would make the CLI path special |
| `ns-mem` commands returning record views | MCP results need `shown`/`total`, versions, and either/or shapes (`new`). Records stay available through `ns-search`, which the commands call |
| Collections in the `liquers-agent` configuration file | Commands see only the store, not the binary's configuration. P6 also asks for the schema to be data in the memory |
| Re-serialising front-matter with `serde_yaml` on edit (as `front_matter_sync` does) | Loses comments and key order in hand-maintained files, so every edit would make a noisy diff |
| A new storage adapter for tiers | `store-adapters` shipped the pipeline. A read handler is all that is missing |

## Changes from Phase 1 and the companion analyses

These need confirmation at this gate. Each has a reason.

1. **`memory_edit`'s arguments are flat**: `op` (`replace` | `set_field` | `append_row`), `target`
   and `text`. `interfaces-and-mcp.md` §4.1 had a one-of of objects. A tool's schema is generated
   from one command, and `register_command!` has no union type. AC-14's "as tabulated there" now
   refers to the table in §Relevant Commands below. `memory_overview` gains `log_lines` and
   `memory_new` gains `reason`.
2. **`set_field`, `append_row` and `section` are not separate commands.** The first two are `edit`
   operations and `section` is an argument of `read`. Phase 1 Core Interactions listed all three as
   commands.
3. **Fallback tiers are configured per mount**, as an ordered list of section names
   (`markdown_tiers`). AC-1 said "the collection's declared fallback". One mount's list covers its
   collections: issues have `Problem`, design phases have `Purpose`, and everything else falls back
   to the first paragraph. Collections needing different rules get their own mount
   (`STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY`).
4. **One small store handler is new code** (`markdown_tiers`), although Phase 1 said "no new store
   code". It is a compiled step registered beside `front_matter`, not an adapter.
5. **Folder tiers through the generic assets API are NEXT.** `tiers-and-storage.md` §3 recommended
   them. In the MVP, `list` and `overview` read `<folder>/README.md`, because a store handler runs
   only for entries that have data.
6. **`liquers-agent` does not depend on `liquers-axum` in the MVP**, since `serve` is NEXT (Phase 1
   decision 5). The dependency arrives with `serve`.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `STORE-WRITE-HAS-NO-PRECONDITION` | draft | P2 · M | AC-6 is check-then-write: race-free across processes only once fixed | No | No | Design around (P8, one writer); monitor |
| `SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES` | draft | P2 · M | A cached `ns-mem` read would go stale | No | No | Every `ns-mem` command is `volatile`; the schema is read through the store |
| `ASSET-MANAGER-CANNOT-BE-SYNCHRONIZED-WITH-THE-STORE` | draft | — | AC-11 with the asset loaded | No | No | Per-mount `content_hash-on_change` and `version_verification` (`store-adapters` step 9); Phase 3 tests the loaded case |
| `RECIPE-OVERRIDE-ONLY-LAST-ACTION` | draft | P3 · M | Tool arguments reach only the last action | No | No | Designed around: one command per tool |
| `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` | draft | P2 · M | `front_matter` projects only `title`/`description` | No | No | `ns-mem` reads the other fields from the front-matter itself |
| `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` | draft | P2 · M | Needed only for a raw query tool | No | No | No raw query tool; `root` is never a tool argument |
| `NO-MARKDOWN-NAVIGATION-COMMANDS` | draft | P3 · L | AC-3 needs a section by heading | No | No | Absorb the minimum: `read`'s `section`, using `search/md_split.rs` `sections` |
| `DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE` | draft | P3 · M | Folder queries | No | No | Commands take folder paths as arguments and list through the asset manager |
| `STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY` | draft | P3 · M | Tier handlers apply to a whole mount | No | No | `markdown_tiers` acts on `text/markdown` only (change 3) |
| `REPLACING-WRITE-KEEPS-DROPPED-DEPENDENCY-EDGES` | draft | P3 · S | A memory rewritten by `set_binary` may expire a dependent needlessly | No | No | Needless recomputation only; monitor |
| `SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY`, `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` | draft | — | A long-lived stdio server grows with writes | No | No | Monitor in Phase 3 (a test with many writes); writes keep no `AssetRef` |
| `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` | draft | — | A panic in a linked library ends the MCP server | No | No | The new code has none; each tool call runs in a spawned task, so a panic becomes a tool error |
| `EVALUATION-SESSION-VOLATILE-TEST-INTERMITTENT` | draft | — | Every `ns-mem` command is volatile | No | No | Tests call the commands through `apply`, not through a session |
| `COMMAND-METADATA-ENHANCEMENTS` | accepted | P2 · L | MCP schemas need enum values | No | No | A local `EnumArgument` is enough; global enums are resolved in `build` |
| `COMMAND-REGISTRY-HEADER-NAMES-GROUPS-NOT-FEATURES` | draft | — | A new registry group, `mem` | No | No | Follow the existing group convention |
| `RESOURCE-NAME-ASCII-ONLY` | draft | — | Ids derived from titles | No | No | `IdCase::ScreamingKebab` transliterates to ASCII and drops the rest |
| `AGENT-WORKBENCH-SURFACE` | draft | P2 · L | Direction check | No | No | `liquers-mcp` stays generic: `plan` validates without running, and resources are declared per tool set |
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 · L | Identity | No | No | Out of scope (Phase 1 non-goals) |

There is no blocker. Namespaces involved, for confirmation at the gate:

- **`mem`** — new.
- **`store_config`** — one new handler.
- `search` — called as a library, not through queries.

## Interfaces — Data Structures, Function Signatures, Trait Implementations

### `liquers-lib` — `src/mem/` (feature `mem = ["records", "dep:indexmap"]`, in `default`)

The schema is a document in the memory. It is read through the store on every call, not through
`get_dependency_state`, which would serve a stale keyed asset. It is small, so it is deserialised
each time:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryLayout {                                     // <root>/_schema/memory.yaml
    #[serde(default = "default_readme")] pub readme: String,  // "README.md"
    #[serde(default = "default_log")] pub log: String,        // "_log"
    #[serde(default)] pub instructions: String,               // appended to the server instructions
    pub collections: Vec<CollectionSchema>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollectionSchema {
    pub name: String,
    pub folder: String,                                       // relative to the root
    #[serde(default = "default_filename")] pub filename: String,  // "{id}.md"
    #[serde(default)] pub file: Option<String>,               // a single-document collection (profile.md)
    #[serde(default)] pub entry: Option<String>,              // a folder is one memory (DESIGN.md, SKILL.md)
    #[serde(default = "yes")] pub writable: bool,
    #[serde(default)] pub id: IdRule,
    #[serde(default)] pub template: Option<String>,           // key of a Markdown template
    #[serde(default)] pub fields: IndexMap<String, FieldRule>,  // order = front-matter order
    #[serde(default)] pub sections: Vec<String>,              // required headings
    #[serde(default)] pub history: Option<HistoryRule>,       // { section: History, stamp: reviewed }
    #[serde(default)] pub checks: Vec<String>,                // named built-in rules
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FieldRule {
    #[serde(default)] pub required: bool,
    #[serde(default)] pub default: Option<serde_json::Value>,   // filled on create (status: draft)
    #[serde(default, rename = "enum")] pub values: Vec<String>,
    #[serde(default)] pub enum_from: Option<String>,          // key of a YAML list
    #[serde(default)] pub list: bool,
    #[serde(default)] pub equals: Option<FieldEquals>,        // FilenameStem
    #[serde(default)] pub link: Option<String>,               // target collection
    #[serde(default)] pub reciprocal: Option<String>,         // field on the target
    #[serde(default)] pub transitions: Option<String>,        // key of a map: from -> [to]
    #[serde(default, rename = "type")] pub field_type: FieldType,  // Text | Date | Key
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IdRule { pub from: IdSource, pub case: IdCase, pub max_len: Option<usize> }
```

The small enums are closed: `IdSource` is `Title | Date | DateTitle` and `IdCase` is
`ScreamingKebab | Kebab | AsIs`, and `FieldEquals` and `FieldType` are likewise small. Every
`match` over them is explicit. `indexmap` 2 (with `serde`) is a new optional dependency.

`mem/front_matter.rs` is a **line-preserving editor**: it keeps every byte outside the lines it
changes, so a `set_field` diff is one line.

```rust
pub struct FrontMatterDoc { /* front-matter lines, body offset */ }
impl FrontMatterDoc {
    pub fn parse(text: &str) -> Result<Self, Error>;        // YAML `---`; TOML -> not_supported
    pub fn get(&self, field: &str) -> Option<serde_json::Value>;
    pub fn set(&mut self, field: &str, value: &serde_json::Value) -> Result<(), Error>; // scalar or flow list
    pub fn body(&self) -> &str;
    pub fn render(&self) -> String;
}
pub fn compose(fields: &IndexMap<String, serde_json::Value>, body: &str) -> String; // new documents
pub fn parse_version(text: &str) -> Result<Version, Error>;  // 32 hex digits -> Version::new
```

`mem/write.rs` holds the one write path that every write command uses:

```rust
pub(crate) struct WriteRequest { pub key: Key, pub bytes: Vec<u8>, pub expected: Option<Version>,
                                 pub op: WriteOp, pub reason: String }
pub(crate) enum WriteOp { Create, Write, Edit(EditOp) }       // EditOp: Replace | SetField | AppendRow
pub(crate) struct WriteOutcome { pub key: Key, pub version_before: Option<Version>,
                                 pub version_after: Version }
pub(crate) async fn commit<E: Environment>(context: &Context<E>, root: &Key, layout: &MemoryLayout,
                                           req: WriteRequest) -> Result<WriteOutcome, Error>;
pub(crate) async fn append_log<E: Environment>(context: &Context<E>, root: &Key, layout: &MemoryLayout,
                                               line: &str) -> Result<(), Error>;
```

`commit` runs these steps in order:

1. **Read** the current bytes with `store.get` through `context.get_envref().get_async_store()`.
   `KeyNotFound` means the memory is new; any other error propagates.
2. **Check the precondition:** compare `Version::from_content(current)` with `expected`.
3. **Validate** the new bytes against the key's collection, which is the one whose `folder` is the
   longest prefix of the key. A key outside every collection, or in a collection with
   `writable: false`, is refused.
4. **Stamp** `updated`, and `created` on a create, in `YYYY-MM-DD` form.
5. **Write** with `AssetManager::set_binary(key, bytes, record)`. The record has
   `type_identifier "text"`, a `type_name`, `data_format "md"` and `media_type "text/markdown"`,
   plus the `title` and `description` from the front-matter, so the live asset is right even
   before a re-read.
6. **Log** with `append_log`, which goes through the same `set_binary` without schema validation.

**Log line format** (W7, AC-7). Each write appends one Markdown list item to
`<root>/<log>/YYYY-MM.md`. Versions are shown by their first 8 hex digits, and a create shows `∅`
before the arrow:

```
- 2026-10-17T15:20:58Z write specs/issues/X.md 1a2b3c4d→5e6f7a8b — reason text
```

**Store handler** (`liquers-lib/src/store_config.rs`, beside `front_matter`, registered by
`register_lib_store_config`): `markdown_tiers(sections: Vec<String> multiple)`, at stages `Default`
and `Read`.

- It acts only on a `text/markdown` entry whose record has an empty `title` or `description`.
- It fills `title` from the first heading.
- It fills `description` from the first paragraph of the first listed section that is present,
  otherwise from the first paragraph after the front-matter.
- It never overwrites and never writes.
- It needs the bytes under the same condition as `front_matter` (`changed_since_stored`).
- It is a synchronous compiled step.

### `liquers-mcp` (new crate; `liquers-core`, `rmcp` 3.5.1 with `server` + `transport-io`, `serde_json`, `tokio`)

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolSetDecl {
    pub prefix: String,                                       // "memory_"
    #[serde(default)] pub cwd: Option<String>,                // the tool set's root; Recipe::cwd
    #[serde(default)] pub instructions: String,
    pub tools: Vec<ToolDecl>,
    #[serde(default)] pub resources: Vec<ResourceDecl>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDecl {
    pub name: String,                                         // without the prefix
    pub query: String,                                        // encoded; last action = the command
    #[serde(default)] pub description: Option<String>,        // default: CommandMetadata::doc
    #[serde(default)] pub fixed: BTreeMap<String, serde_json::Value>,  // overrides not offered
    #[serde(default)] pub read_only: bool,                    // MCP readOnlyHint
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceDecl { pub uri_template: String, pub tool: String, pub argument: String }
// e.g. { uri_template: "liquers://memory/{+key}", tool: read, argument: key }

pub struct ToolRegistry { /* full name -> Tool { decl, set: usize, query: Query, command: CommandKey,
                              input_schema: Arc<serde_json::Map<String, serde_json::Value>>,
                              hidden: BTreeSet<String> } */ }
impl ToolRegistry {
    pub fn build(sets: Vec<ToolSetDecl>, cmr: &CommandMetadataRegistry) -> Result<Self, Error>;
}
pub fn argument_schema(command: &CommandMetadata, hidden: &BTreeSet<String>)
    -> serde_json::Map<String, serde_json::Value>;

pub struct McpServer<E: Environment> { envref: EnvRef<E>, tools: Arc<ToolRegistry> }
impl<E: Environment> McpServer<E> {
    pub fn new(envref: EnvRef<E>, tools: ToolRegistry) -> Self;
    pub fn plan(&self, tool: &str, arguments: &serde_json::Map<String, serde_json::Value>)
        -> Result<Plan, Error>;                               // validate without running
    pub async fn call(&self, tool: &str, arguments: serde_json::Map<String, serde_json::Value>)
        -> Result<serde_json::Value, Error>;
}
impl<E: Environment> rmcp::ServerHandler for McpServer<E> { /* get_info, list_tools, call_tool,
    list_resource_templates, read_resource */ }
pub async fn serve_stdio<E: Environment>(server: McpServer<E>) -> Result<(), Error>;
```

- **`build`** parses each query and plans it once with `Recipe::to_plan`. From the last action it
  takes the `CommandKey`, and the `CommandMetadata` after `resolve_global_enums`. It refuses these
  with `Error::configuration_error`: a duplicate full name, a `fixed` name the command does not
  have, an unregistered command, and a resource that names an unknown tool.
- **Hidden arguments** are `injected` ones, `fixed` names, and **every argument whose default link
  has a relative operand** (`Query::has_relative_operand`; this is how `root` is hidden). So an
  agent cannot redirect a tool outside its root, and `call` refuses a hidden name.
- **`argument_schema`** maps argument types to JSON schema:
  - `String`, `Integer`, `Float` and `Boolean` map to their JSON types, and the `…Option` variants
    to a type-or-`null`.
  - `Enum` maps to `enum` over its aliases, or to `anyOf` when `others_allowed`.
  - `Any` maps to `{}`, with no `default`.
  - `None` is skipped, and a `multiple` argument becomes an array.
  - `ArgumentInfo::description` and `label` become the description and title.
  - A `Value` default becomes `default`. A link default makes the argument optional.
- **`plan` and `call`** share one builder. It makes a `Recipe` from the tool's query, with
  `arguments` set to the call's arguments plus `fixed`, and `cwd` set to the tool set's `cwd`.
  - `call` then runs `get_asset_manager().apply(recipe, State::new(), None)`, `AssetRef::get` and
    `ValueInterface::try_into_json_value`. This evaluation is unmanaged, so nothing is cached
    between calls.
  - It runs in a spawned task, so a panic becomes a tool error.
- **`call_tool`** sends an object value as structured content plus its JSON text, and any other
  value as text only. An `Error` becomes `CallToolResult::error` with its Display, so the agent
  reads it. Only an unknown tool name is a protocol error.
- **`get_info`** joins the tool sets' `instructions`. The memory tool set adds the README's L1 by
  calling its `overview` tool once at start.
- **Resources** come from `ResourceDecl`. `read_resource` matches the URI template and calls the
  named tool. The engine knows nothing about memory.

Bounds: `E: Environment` with `E::Value: ValueInterface`. `ServerHandler` requires
`Send + Sync + 'static`, which `EnvRef<E>` and `Arc<ToolRegistry>` satisfy on native targets. The
server holds no mutable state.

### `liquers-agent` (new binary crate; `liquers-lib` with `mem` + `cli`, `liquers-store`, `liquers-mcp`)

```rust
#[derive(Deserialize)]
struct AgentConfig {
    #[serde(flatten)] environment: EnvironmentConfig,   // store, recipes, assets: unchanged
    #[serde(default)] memory: MemorySettings,
}
#[derive(Deserialize, Default)]
struct MemorySettings { #[serde(default)] root: String,
                        #[serde(default)] tools: Option<Vec<ToolSetDecl>> }  // default: built-in `memory`
```

**Environment.** The binary builds it in this order:

1. Start from `liquers_lib::environment::default_environment_builder::<Value, SimpleUIPayload>()`.
2. Run `register_all_commands!(&mut builder.command_registry)`.
3. Call `.with_config(cfg, Box::new(liquers_store::store_factory::default_store_factory()))`, which
   brings S3 and the other OpenDAL services.
4. Call `build()`.

Commands are registered before `build()`, because building compiles the mounts' `metadata`
sections against the registry. The built-in tool set is `tools/memory.yaml`, compiled in with
`include_str!`, and its `cwd` is set from `memory.root`.

**Subcommands.** `mcp`, `check [PATH]`, `search TEXT [--in PATH] [--limit N]` and
`read KEY [--tier T] [--section S]`. Each one calls `McpServer::call` with the matching tool, so the
CLI and MCP share one code path.

- Output is JSON on stdout, or YAML with `--format yaml`. Diagnostics go to stderr.
- `check` exits 1 when it finds an error.
- In the MVP, memories are created only through MCP.

### Sync vs Async

| Item | Choice | Why |
|---|---|---|
| `ns-mem` commands, `commit`, `append_log`, `McpServer::call`, `serve_stdio` | async | They read and write the store, and evaluate through the asset manager |
| `FrontMatterDoc`, `compose`, `parse_version`, `argument_schema`, `ToolRegistry::build`, `McpServer::plan`, schema validation | sync | Pure computation over text and metadata already in memory |
| `markdown_tiers` | sync | A compiled `store_config` step is a synchronous closure |

The only trait implemented is `rmcp::ServerHandler` for `McpServer<E>`. No Liquers trait changes.

## Relevant Commands

All ten commands are registered in namespace `mem` by `register_mem_commands!` in
`liquers-lib/src/mem/mod.rs`, with a no-op stand-in when `mem` is off. They are named here by
`CommandKey` Display (realm empty, so `-mem-<name>`).

Every command:

- is `async` and `volatile: true` (see the preflight);
- takes no state;
- returns a JSON-shaped `Value` (`ValueInterface::try_from_json_value`);
- ends with `root: Key = query "-R-key/."` (`COMMAND_REGISTRATION_GUIDE.md` §cwd).

`root` comes last, against the guide's ordering pitfall, for two reasons. Promotion affects only
the encoded identity of a cached asset, and these commands are volatile. With the key arguments
first, the CLI form also reads naturally (`ns-mem/read-specs~/issues~/X.md`). `liquers-template`
made the same choice (its question 14). The MCP server sets the root through `Recipe::cwd` and
never exposes `root`.

```rust
register_command!(cr, async fn overview(log_lines: i64 = 10, root: Key = query "-R-key/.", context) -> result
    namespace: "mem" volatile: true doc: "The memory's README tiers, its collections with counts, and the latest log lines")?;
register_command!(cr, async fn describe(collection: String = "", root: Key = query "-R-key/.", context) -> result
    namespace: "mem" volatile: true doc: "Collections, fields, vocabularies, templates; which are writable")?;
register_command!(cr, async fn list(path: String = "", depth: i64 = 1, include_superseded: bool = false,
    limit: i64 = 100, root: Key = query "-R-key/.", context) -> result namespace: "mem" volatile: true)?;
register_command!(cr, async fn search(text: String = "", path: String = "",
    unit: String = "document" (enum: ["document", "section"]), filter: String = "", limit: i64 = 10,
    include_superseded: bool = false, root: Key = query "-R-key/.", context) -> result namespace: "mem" volatile: true)?;
register_command!(cr, async fn read(key: String, tier: String = "full" (enum: ["title", "description", "full"]),
    section: String = "", lines: String = "", root: Key = query "-R-key/.", context) -> result namespace: "mem" volatile: true)?;
register_command!(cr, async fn similar(collection: String, title: String, description: String = "",
    limit: i64 = 5, root: Key = query "-R-key/.", context) -> result namespace: "mem" volatile: true)?;
register_command!(cr, async fn new(collection: String, title: String, description: String, fields: Value = "",
    body: String = "", force: bool = false, reason: String = "", root: Key = query "-R-key/.", context) -> result
    namespace: "mem" volatile: true)?;
register_command!(cr, async fn write(key: String, body: String = "", title: String = "", description: String = "",
    fields: Value = "", expected_version: String = "", supersedes: String = "", reason: String = "",
    root: Key = query "-R-key/.", context) -> result namespace: "mem" volatile: true)?;
register_command!(cr, async fn edit(key: String, op: String (enum: ["replace", "set_field", "append_row"]),
    target: String, text: String, expected_version: String = "", reason: String = "",
    root: Key = query "-R-key/.", context) -> result namespace: "mem" volatile: true)?;
register_command!(cr, async fn check(path: String = "", root: Key = query "-R-key/.", context) -> result
    namespace: "mem" volatile: true)?;
```

`fields: Value` accepts both an empty text (the query-text default) and a JSON object (an MCP
override).

| Tool (`memory_`) | Command | Returns | AC |
|---|---|---|---|
| `overview` | `-mem-overview` | `{readme: {title, description}, collections: [{name, title, description, count}], log: [lines]}` | AC-15 |
| `describe` | `-mem-describe` | the `CollectionSchema`s, with `enum_from` resolved and `writable` | AC-15, AC-13 |
| `list` | `-mem-list` | `{entries: [{key, title, description, kind, status, asset_status, is_dir}], shown, total}` | AC-1, AC-8, AC-12, AC-17 |
| `search` | `-mem-search` | `{hits: [{key, title, description, excerpt, score, matched, heading?, line?}], shown, total}` | AC-2, AC-8, AC-12 |
| `read` | `-mem-read` | `{key, version, front_matter, content}` | AC-3 |
| `new` | `-mem-new` | `{written: false, candidates}` or `{written: true, key, version}` | AC-4, AC-5 |
| `write` | `-mem-write` | `{key, version_before, version_after}` | AC-5 to AC-8 |
| `edit` | `-mem-edit` | as `write` | AC-6, AC-7, AC-16 |
| `check` | `-mem-check` | `{violations: [{rule, key, severity, message}], counts, unsupported}` | AC-9 |

How each acceptance scenario is met:

- **AC-1, AC-17 (tiers).** `title` and `description` come from `AssetInfo`, which the mount's
  chain fills with `front_matter` and then `markdown_tiers`. A folder's tiers come from its
  `README.md`. `status` and `kind` are front-matter fields, and `asset_status` is the Liquers
  `Status`.
- **AC-2 (search).** The catalog is built with `search/catalog.rs` and `field_extract`. The text,
  `filter` and the superseded test are joined into **one predicate before** `search_view` ranks and
  applies `limit`. So `total` (`SearchReport.matched`) never counts hidden memories, and `shown` is
  `SearchReport.shown`. `unit: section` searches `md_split` sections.
- **AC-3 (section read).** `read` with `section` uses `md_split.rs` `sections`, and returns the
  document's `version`.
- **AC-4, AC-5 (create).** `new` first runs `similar`: BM25 over `title` and `description` in the
  collection. Its candidates are the hits sharing at least half of the title's significant words,
  and they are returned unless `force` is set. Otherwise `new`:
  1. derives the id;
  2. takes the body from `body`, or when that is empty copies the template's body as it is
     (rendering it belongs to `agent-memory-derived`);
  3. applies the field `default`s, so `status: draft` is a schema default, not code;
  4. writes the document with `commit`.

  Validation refuses a value outside `enum` or `enum_from`, and a missing `title` or
  `description`.
- **AC-6, AC-7.** The precondition is `commit` step 2. The stamps and the log line are steps 4 and 6.
- **AC-8 (supersession).** `write` with `supersedes` makes two commits: the new memory first, then
  `set_field superseded_by` on the old one, and logs both. `list` and `search` hide a memory with a
  non-empty `superseded_by` unless `include_superseded` is set.
- **AC-11 (projection).** This is configuration only.
  - `on_read` includes `front_matter`; `on_write` includes `front_matter` and `front_matter_sync`
    (write-back on `set_metadata`).
  - `storage: none` keeps the projection fresh, because the record is re-derived on every read. It
    also writes no sidecar files into the checkout.
  - `content_hash-on_change`, together with the asset manager's per-mount `version_verification`,
    covers a loaded asset.
- **AC-12.** Listing goes through `get_asset_manager().listdir_asset_info`, so a recipe-only key
  appears with `asset_status: Recipe` and is not evaluated (P5, as in `ns-search`).
- **AC-13.** A `read_only: true` mount, plus `writable: false` on its collection, which `describe`
  reports.
- **AC-14.** The nine tools above, served by `liquers-agent mcp`.
- **AC-16 (edits).**
  - `replace` requires exactly one match.
  - `set_field` checks `enum` and `transitions`.
  - `append_row` appends a Markdown row to the named section. When that section is the collection's
    `history.section`, it also stamps `history.stamp` (`reviewed:`) with today's date.
- **AC-18 to AC-20.** These are profiles, below.

**Check rules (AC-9).** The schema expresses these rules:

- required fields, `enum`, `enum_from`, and `id = filename stem`;
- a duplicate id;
- `link` exists, and `reciprocal`;
- required `sections`;
- `superseded_by` exists;
- a title over 120 characters (warning).

The remaining per-document rules of `docs_index.py --check` are named built-in rules: `scenarios`
(§5.2.1), `readiness` (§5.1.1), `compact_form` (§5), `dead_links` (§8.4), and `reviewed` with
`history` (§9.2, §9.5). The rules about stale generated files (`index.csv`, `index.md`) belong to
`agent-memory-derived`, and `check` lists them under `unsupported`.

**Profile "liquers-specs"** (AC-18). Mounts are matched by prefix, most specific first. The
catch-all `memory/` mount holds `README.md`, `_schema/`, `_log/` and `notes/`:

```yaml
store:
  stores:
    - { type: filesystem, prefix: specs/archive, read_only: true, config: { path: ./specs/archive } }
    - type: filesystem
      prefix: specs
      config: { path: ./specs }
      metadata:
        storage: none
        on_read: "media_type/file_facts/content_hash-on_change/front_matter/markdown_tiers-Problem-Purpose"
        on_write: "defaults/media_type-fill/file_facts/front_matter/front_matter_sync"
    - { type: filesystem, prefix: skills, read_only: true, config: { path: ./.claude/skills } }
    - type: filesystem
      prefix: ""
      config: { path: ./memory }
      metadata: { storage: none, on_read: "media_type/file_facts/front_matter/markdown_tiers" }
memory: { root: "" }
```

`memory/_schema/memory.yaml` declares these collections: `issues`, `designs` (`entry: DESIGN.md`),
`reference`, `guides`, `archive` (`writable: false`), `skills` (`entry: SKILL.md`,
`writable: false`) and `notes`.

**Profile "openviking"** (AC-19). It is the same code with different data. The `user` mount is S3,
which also covers AC-20; tests use an in-process or local S3-compatible endpoint.

```yaml
store:
  stores:
    - { type: filesystem, prefix: resources, read_only: true, config: { path: ./docs } }
    - { type: s3, prefix: user, config: { bucket: "${MEM_BUCKET}", root: "u/${USER_ID}",
        endpoint: "${S3_ENDPOINT}" }, metadata: { storage: none } }
    - { type: filesystem, prefix: "", config: { path: ./memory } }
```

```yaml
# _schema/memory.yaml
readme: .overview.md
collections:
  - { name: profile,     folder: user/memories, file: profile.md }
  - { name: preferences, folder: user/memories/preferences, id: { from: title, case: kebab } }
  - { name: entities,    folder: user/memories/entities,    id: { from: title, case: kebab } }
  - { name: events,      folder: user/memories/events,      id: { from: date_title, case: kebab },
      fields: { date: { type: date, required: true } } }
```

## Integration Points

| Crate | Change |
|---|---|
| `liquers-lib` | `Cargo.toml`: feature `mem = ["records", "dep:indexmap"]` in `default`, and an optional `indexmap`. New `src/mem/{mod,layout,front_matter,write,read,check,commands}.rs`. `src/store_config.rs`: `markdown_tiers`. `src/commands.rs`: `register_mem_commands!` inside `register_all_commands!`, with a no-op stand-in. `src/bin/export_command_registry.rs`: `Group::Mem`. Reuses `search/mod.rs` (`info_of`), `search/catalog.rs`, `search/md_split.rs` (`sections`) and `liquers_records::search` (`parse_structure_with_report`, `search_view`, `SearchReport`) |
| `liquers-mcp` (new) | `Cargo.toml`; `src/{lib,decl,schema,server}.rs`; a workspace member and default member, native only |
| `liquers-agent` (new) | `Cargo.toml`, `src/main.rs`, `src/config.rs`, `tools/memory.yaml`; a workspace member, not a default member |
| Repository | `memory/README.md`, `memory/_schema/{memory,areas,issue-status}.yaml`, `memory/liquers-specs.yaml` (the profile) |
| Tests and scripts | `liquers-lib/tests/registry_export.rs`: `mem` in its gate and in `full_registry()`. `scripts/check-build-matrix.sh`: rows with and without `mem`, native and wasm32 (`mem` adds no `tokio` or `rmcp`). `specs/command_registry.yaml` regenerated, with a CHANGELOG line |

## Error Handling

| Case | Constructor |
|---|---|
| Schema file missing or malformed; a tool set naming an unknown command, argument or tool; a duplicate tool name | `Error::configuration_error` |
| A write that violates the schema: an unknown field value, a missing `title`/`description`, a forbidden transition, a key outside every collection, a collection with `writable: false` | `Error::general_error`, naming the field, value and allowed values, as `store_config`'s validate step does |
| `expected_version` differs from the current content hash | `Error::dependency_version_mismatch(&DependencyKey::from(&key), "expected …, found …")` |
| `expected_version` is not 32 hex digits | `Error::parse_error` |
| `edit replace` text not found, or found more than once | `Error::general_error`, with the count |
| `read` or `edit` of a missing key | `Error::key_not_found` |
| TOML front-matter on an edit | `Error::not_supported` |
| A write into a `read_only` mount | The store's refusal, propagated unchanged |
| A hidden argument passed to a tool | `Error::general_error` naming it |
| The data write succeeded but the log append failed | The data write stands. The error names the key and says the log line is missing, and `check` warns about a memory newer than its last log line |

In MCP every `Error` becomes a tool error (`CallToolResult::error`) carrying its Display. In the
CLI, errors go to stderr and the exit code is 1.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `reference/AGENT_MEMORY_SERVICE.md` | reference, new; area `[lib/commands, mcp]`; for agents and developers | Layout, the `memory.yaml` format, tiers and fallbacks, commands and tools, the log format, the CLI, the two profiles |
| `.claude/skills/liquers-memory/SKILL.md` | skill, new | When to use memory: orientation first, search before writing, L0/L1 with every write |
| `guides/STORE_CONFIG_GUIDE.md` | update | `markdown_tiers` in the handler catalogue; `storage: none` for checkouts edited by hand. History row, `reviewed:` bump |
| `guides/COMMAND_REGISTRATION_GUIDE.md` | update | New section "Commands served as MCP tools": one command per tool, keys as arguments, relative-default arguments hidden. History row, `reviewed:` bump |
| `reference/PROJECT_OVERVIEW.md` | update | The crate list gains `liquers-mcp` and `liquers-agent`. History row, `reviewed:` bump |
| `DOCS_STRUCTURE_GUIDE.md` | update | §3 gains area `mcp` (`liquers-mcp` and `liquers-agent`); §4.8 names the `memory_new` route beside `docs_index.py new`. History row |
| `CLAUDE.md` | update | Project structure and dependency flow gain the two crates |
| `README.md` (specs) | update | The capability map links the reference document |

Proposed `affects_docs`: `reference/AGENT_MEMORY_SERVICE.md`, `guides/STORE_CONFIG_GUIDE.md`,
`guides/COMMAND_REGISTRATION_GUIDE.md`, `reference/PROJECT_OVERVIEW.md`, `DOCS_STRUCTURE_GUIDE.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | About 15 new files in two new crates and `liquers-lib/src/mem/`. Existing files: `store_config.rs`, `commands.rs`, the registry exporter and its test, the workspace and `liquers-lib` `Cargo.toml`, the registry YAML, the build-matrix script |
| Crates and workflows affected | `liquers-lib` gains a default feature that adds only `indexmap`; two new crates. `liquers-core` is untouched |
| Existing tests likely to change | `registry_export` (new group, regeneration) |
| New validation | Unit tests per module; an MCP round trip over an in-process duplex transport; an oracle comparison with `docs_index.py --check` (AC-9); the build matrix with and without `mem`, native and wasm32 |
| Compatibility | Additive. The `Version` text form is fixed at 32 hex digits, because agents keep it between calls |
| Data | Writes edit hand-maintained files. The line-preserving editor and the write log keep this reviewable. A partial supersede (new memory written, old one not marked) is visible to `check` |
| Concurrency | `set_binary` holds `key_mutation_lock`, but the version check runs before it, so two in-process writers can race. Accepted under P8; closed by `STORE-WRITE-HAS-NO-PRECONDITION` |
| Performance | Volatile reads re-list and re-read a collection per call (`specs/issues`: about 300 files, 1.15 MB). Acceptable at an agent's call rate; revisited with the search-dependency fix |
| Security | No raw query tool, and `root` cannot be overridden. Writes are limited to writable collections and mounts. Stdout carries MCP frames, so a stray `println!` in a linked library breaks the server (`CLAUDE.md` forbids it) |
| Dependencies | `rmcp` 3.5.1 (MSRV 1.88 ≤ 1.95), features `server` and `transport-io` only. The version is pinned, because protocol types change between minor versions |
| Recovery | Revert the crates, and remove `mem` from `default`. Memory files are plain Markdown |
| Certainty and open questions | High for the commands and the engine. **Medium** for byte-for-byte `check` parity, because `docs_index.py` also has rules over the whole corpus, and for the cost of `storage: none` plus `front_matter` on large listings, which Phase 3 measures |

## Open Questions

1. **Keys as arguments, not state.** Every `ns-mem` command is a source command that takes `key`
   or `path` arguments, so one MCP tool is one command. Recommended: yes.
2. **The write body as an argument** (an override), not the evaluation payload. Recommended: yes.
3. **All `ns-mem` commands volatile** until `SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES` is
   fixed. Recommended: yes.
4. **Schema location:** one `<root>/_schema/memory.yaml`, with vocabularies in sibling files.
   Recommended: yes. Per-collection files can come later without a format change.
5. **Fallback tiers as a `store_config` handler** (`markdown_tiers`), configured per mount (change
   3). Recommended: yes. The handler also makes `key/info` show the fallback.
6. **Results are JSON-shaped values**, not record views. Recommended: yes, with `ns-search` as the
   composable layer.
7. **`mem` as a default feature of `liquers-lib`**, so that the exported registry and
   `liquers-validate` know `ns-mem`. It adds only `indexmap`. Recommended: yes.
8. **`storage: none` on writable checkout mounts.** No `.__metadata__` sidecars are written into
   the repository, and the record is re-derived on every read. Recommended: yes.
