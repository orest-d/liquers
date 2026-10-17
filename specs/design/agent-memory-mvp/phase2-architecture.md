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
  and offers `mcp` (stdio), `check`, `search`, `read` and `query`.

Storage is configuration only: `read_only` mounts and `metadata` sections from `store-adapters`,
plus one new read handler for fallback tiers.

One decision shapes the rest (decided at the Phase 2 gate, 2026-10-17). **The commands are written
once, in Liquers style, and served through two interfaces.**

- **The commands take what they act on as state.** A document comes from `-R/<key>`, and a folder
  or the memory itself from `-R-key/<path>`, which evaluates to the key as a `Value::Key`. So
  `-R/specs/issues/X.md/-/ns-mem/read-l1` is an ordinary query, and the `ns-mem`
  commands sit beside `ns-search` and `ns-rec` in queries an agent writes itself.
- **Basic tools** are standard MCP tools, one per command except `similar` (which `new` runs
  itself, and which `memory_query` reaches), for any agent. Their schemas are
  generated: the command's arguments as in any tool, plus one **key argument** that stands for
  the state. The engine fills it into the tool query's resource segment structurally, never by
  editing query text (§`liquers-mcp`).
- **Power use** is one read-only tool, `memory_query`, that evaluates any Liquers query whose
  commands are on an allow-list, plus a query guide and a command reference served as MCP
  resources. The server's instructions point to them.

Overrides still reach only the last action of a query (`RECIPE-OVERRIDE-ONLY-LAST-ACTION`), and
core has no placeholder for a resource key. Both are designed around: every tool query has one
action, and the key argument is filled by the engine. A core mechanism that would make a tool a
`Recipe` with holes (option C in `/mnt/project-files/agent-memory-mvp/decision1-liquers-style-tools.md`)
is excluded for now; it is filed as `RECIPE-CANNOT-PARAMETERIZE-RESOURCE-KEY`.

Composite behaviour lives inside the commands, not in the tool layer: the duplicate check in
`new`, the edit operations, and hiding superseded memories.

Rejected:

| Alternative | Why not |
|---|---|
| Keys as command arguments, every command a source command (Phase 2 draft) | The commands would not compose, so a query tool would add nothing, and a document read inside a command body records no dependency |
| Splicing the key argument into the query text | Query injection: a key such as `x/-/ns-mem/write-…` would change the command. The engine substitutes a parsed, validated `Key` into the parsed `Query` instead |
| Key placeholders and multi-action overrides in core (option C) | A core design of its own with open encoding questions; excluded for now and filed |
| Tools hidden at first and revealed later (MCP `list_changed`) | Clients support it unevenly. All tools are listed from the start, and the instructions point to the power layer |
| Composite tools assembled in `liquers-mcp` (`memory_new` = `similar` then `new`) | Puts memory semantics in the generic engine, against `workbench-direction.md` §2. The schema could no longer be generated from one command |
| Write body as the evaluation payload (`interfaces-and-mcp.md` §6) | Argument overrides already carry long, unescaped text by name. A payload type is specific to the environment, and `payload: required` would make the CLI path special |
| `ns-mem` commands returning one JSON object each (Phase 2 draft) | A generic object serializes only to JSON and composes with nothing. Tables are record views (Markdown, CSV, JSON, HTML through the serialization system, and `ns-rec` downstream); counts and versions travel as log notes |
| L0/L1 always equal to `title`/`description` | Orest, 2026-10-17: L0/L1 serve agents finding a resource, title and description serve humans in UI and reports. They may overlap or not, so the mapping is configuration to experiment with |
| Collections in the `liquers-agent` configuration file | Commands see only the store, not the binary's configuration. P6 also asks for the schema to be data in the memory |
| Re-serialising front-matter with `serde_yaml` on edit (as `front_matter_sync` does) | Loses comments and key order in hand-maintained files, so every edit would make a noisy diff |
| A new storage adapter for tiers | `store-adapters` shipped the pipeline. A read handler is all that is missing |

## Changes from Phase 1 and the companion analyses

These need confirmation at this gate. Each has a reason.

1. **`memory_edit`'s arguments are flat, and every operation given is applied** (decided
   2026-10-17, Orest): `old`/`new` replace text, `field`/`value` set a front-matter field, and
   `section`/`row` append a table row. Any combination may be given in one call; it is one edit,
   with one version check, one write and one log line. `interfaces-and-mcp.md` §4.1 had a one-of
   of objects. A tool's schema is generated from one command, and `register_command!` has no union
   type. AC-14's "as tabulated there" now
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
7. **A query tool and two resources are added**: `memory_query` (read-only, allow-listed
   commands), `liquers://memory/query-guide` and `liquers://commands/{namespace}`. Phase 1 decision
   11 listed nine tools; the MVP now has ten. AC-14 counts the basic tools, unchanged.
8. **L0/L1 are separate from `title`/`description`**, joined by a configurable bridge (`tiers` in
   `memory.yaml`, per memory and per collection). The default maps them to `title` and
   `description`, so the specs behave as in Phase 1. Results and `read`'s tier speak of `l0`/`l1`.
9. **Results go through the serialization system.** Tables are record views and documents are
   text, each with a default format chosen for agents (Markdown); the query's filename picks
   another (`list.csv`, `list.json`).
10. **Migration is kept small by design**, and a migration guide is added (Orest, 2026-10-17;
   draft in [`migration-guide-draft.md`](./migration-guide-draft.md)). `memory.yaml` gains a
   `version`. Phase 1's documentation intent did not list the guide.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `STORE-WRITE-HAS-NO-PRECONDITION` | draft | P2 · M | AC-6 is check-then-write: race-free across processes only once fixed | No | No | Design around (P8, one writer); monitor |
| `SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES` | draft | P2 · M | A cached result of a command that reads files in its body would go stale | No | No | Every `ns-mem` command that reads the store in its body is `volatile`. `read` is not: its document arrives as state through `-R/<key>`, a recorded dependency |
| `ASSET-MANAGER-CANNOT-BE-SYNCHRONIZED-WITH-THE-STORE` | draft | — | AC-11 with the asset loaded; `read`'s state is the keyed asset | No | No | Per-mount `content_hash-on_change` and `version_verification` (`store-adapters` step 9); Phase 3 tests the loaded case, including a hand edit between two `read`s |
| `RECIPE-OVERRIDE-ONLY-LAST-ACTION` | draft | P3 · M | Tool arguments reach only the last action | No | No | Designed around: every tool query has one action; the key argument is filled by the engine |
| `RECIPE-CANNOT-PARAMETERIZE-RESOURCE-KEY` | draft (filed with this phase) | P3 · L | No core placeholder for the key of a resource segment | No | No | `liquers-mcp` substitutes the key structurally; core option C excluded for now |
| `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` | draft | P2 · M | `front_matter` projects only `title`/`description` | No | No | `ns-mem` reads the other fields from the front-matter itself |
| `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` | draft | P2 · M | `memory_query` must not write, but a command's body could | No | No | Enforced on the plan, not the context: every command in the plan, link queries included, must be on the tool set's allow-list, which names only commands reviewed not to write |
| `NO-MARKDOWN-NAVIGATION-COMMANDS` | draft | P3 · L | AC-3 needs a section by heading | No | No | Absorb the minimum: `read`'s `section`, using `search/md_split.rs` `sections` |
| `DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE` | draft | P3 · M | `-R/<folder>` fails | No | No | Folder commands take `-R-key/<folder>`, a `Value::Key`, and list through the asset manager |
| `STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY` | draft | P3 · M | Tier handlers apply to a whole mount | No | No | `markdown_tiers` acts on `text/markdown` only (change 3) |
| `REPLACING-WRITE-KEEPS-DROPPED-DEPENDENCY-EDGES` | draft | P3 · S | A memory rewritten by `set_binary` may expire a dependent needlessly | No | No | Needless recomputation only; monitor |
| `SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY`, `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` | draft | — | A long-lived stdio server grows with writes | No | No | Monitor in Phase 3 (a test with many writes); writes keep no `AssetRef` |
| `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` | draft | — | A panic in a linked library ends the MCP server | No | No | The new code has none; each tool call runs in a spawned task, so a panic becomes a tool error |
| `EVALUATION-SESSION-VOLATILE-TEST-INTERMITTENT` | draft | — | Most `ns-mem` commands are volatile | No | No | Tests call the commands through `apply`, not through a session |
| `COMMAND-METADATA-ENHANCEMENTS` | accepted | P2 · L | MCP schemas need enum values | No | No | A local `EnumArgument` is enough; global enums are resolved in `build` |
| `COMMAND-REGISTRY-HEADER-NAMES-GROUPS-NOT-FEATURES` | draft | — | A new registry group, `mem` | No | No | Follow the existing group convention |
| `RESOURCE-NAME-ASCII-ONLY` | draft | — | Ids derived from titles | No | No | `IdCase::ScreamingKebab` transliterates to ASCII and drops the rest |
| `AGENT-WORKBENCH-SURFACE` | draft | P2 · L | Direction check | No | No | `liquers-mcp` stays generic: `plan` validates without running, and resources are declared per tool set |
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 · L | Identity; the general replacement for the query tool's allow-list | No | No | Out of scope (Phase 1 non-goals); the allow-list is the MVP stand-in |

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
    #[serde(default = "one")] pub version: u32,               // schema format; newer than supported -> refused
    #[serde(default = "default_readme")] pub readme: String,  // "README.md"
    #[serde(default = "default_log")] pub log: String,        // "_log"
    #[serde(default)] pub instructions: String,               // appended to the server instructions
    #[serde(default)] pub tiers: TierSources,                 // the L0/L1 bridge, memory-wide
    pub collections: Vec<CollectionSchema>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TierSources {                                      // first non-empty source wins
    #[serde(default = "default_l0")] pub l0: Vec<TierSource>, // ["meta:title"]
    #[serde(default = "default_l1")] pub l1: Vec<TierSource>, // ["meta:description"]
}
pub enum TierSource {                                         // (de)serialized from "kind:arg" text
    Field(String),      // "field:l0"         a front-matter field
    Meta(String),       // "meta:title"       a field of the store's metadata record
    Section(String),    // "section:Problem"  first paragraph of that section
    Heading,            // "heading"          the first heading
    Paragraph,          // "paragraph"        the first paragraph after the front-matter
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
    #[serde(default)] pub tiers: Option<TierSources>,         // overrides the memory-wide bridge
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
pub(crate) enum WriteOp { Create, Write, Edit(Vec<EditOp>) }  // EditOp: Replace | SetField | AppendRow
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

`mem/tiers.rs` resolves a document's L0 and L1 from the collection's `TierSources` (else the
memory's). `Meta` sources read the record the mount's pipeline produced, so the default costs no
extra read; `Field`, `Section`, `Heading` and `Paragraph` parse the document with
`FrontMatterDoc` and `md_split.rs` `sections`. The bridge is the place to experiment: changing
which parts an agent sees as L0/L1 is an edit of `memory.yaml`, with no code or store change. The
`markdown_tiers` handler below shares its section and paragraph extraction.

**Store handler** (`liquers-lib/src/store_config.rs`, beside `front_matter`, registered by
`register_lib_store_config`): `markdown_tiers(sections: Vec<String> multiple)`, at stages `Default`
and `Read`.

- It is the bridge for humans: it fills the record's `title` and `description`, which UI, `key/info`
  and reports show. Agents see L0/L1 through `tiers`, which by default read these fields.
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
    #[serde(default)] pub query_tool: Option<QueryToolDecl>,  // the power layer
    #[serde(default)] pub resources: Vec<ResourceDecl>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDecl {
    pub name: String,                                         // without the prefix
    pub query: String,                                        // encoded; one action = the command
    #[serde(default)] pub key: Option<KeyArgumentDecl>,       // stands for the state
    #[serde(default)] pub description: Option<String>,        // default: CommandMetadata::doc
    #[serde(default)] pub fixed: BTreeMap<String, serde_json::Value>,  // overrides not offered
    #[serde(default)] pub read_only: bool,                    // MCP readOnlyHint
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyArgumentDecl {
    pub name: String,                                         // "key" or "path"
    #[serde(default)] pub description: Option<String>,        // default: "Key relative to the root"
    #[serde(default)] pub required: bool,                     // else empty = the query's own key
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryToolDecl {
    pub name: String,                                         // "query"
    pub allow: Vec<String>,                                   // CommandKey Display or "-ns-*" patterns
    #[serde(default)] pub description: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResourceDecl {
    Tool { uri_template: String, tool: String, argument: String },
    // e.g. { uri_template: "liquers://memory/{+key}", tool: read, argument: key }
    Text { uri: String, name: String, description: String, text: String },
    // the query guide, compiled into the binary that declares it
    Commands { uri_template: String, namespaces: Vec<String> },
    // "liquers://commands/{namespace}": signatures and docs from the CommandMetadataRegistry
}

pub struct ToolRegistry { /* full name -> Tool { decl, set: usize, query: Query, command: CommandKey,
                              input_schema: Arc<serde_json::Map<String, serde_json::Value>>,
                              hidden: BTreeSet<String> }, plus each set's QueryTool { allow } */ }
impl ToolRegistry {
    pub fn build(sets: Vec<ToolSetDecl>, cmr: &CommandMetadataRegistry) -> Result<Self, Error>;
}
pub fn argument_schema(command: &CommandMetadata, key: Option<&KeyArgumentDecl>,
                       hidden: &BTreeSet<String>) -> serde_json::Map<String, serde_json::Value>;
pub fn fill_key(query: &Query, argument: &str) -> Result<Query, Error>;   // structural, validated
pub fn check_allowed(plan: &Plan, allow: &[String]) -> Result<(), Error>;  // walks link queries too

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
  have, an unregistered command, a resource that names an unknown tool, a tool query with more
  than one action, a `key` on a query that does not begin with a resource segment, and a `key`
  name that clashes with a command argument.
- **The key argument** (`fill_key`). The tool query's resource segment holds the base key, usually
  `.` (`-R/./-/ns-mem/read`, `-R-key/./-/ns-mem/list`). The engine parses the argument with
  `parse_key` and refuses an absolute key, an empty or `..` segment, and a segment beginning with
  `-`; then it joins the key onto the base **in the parsed `Query`**, never in the text. Since the
  recipe's `cwd` is the tool set's root, every key resolves under the root. An empty, optional key
  leaves the base. In the schema the key argument comes first, as a string, with the declared
  description; the command's `state_argument` supplies nothing else.
- **The query tool** (`check_allowed`). `memory_query(query)` parses the text, plans it with the
  tool set's `cwd`, and refuses the plan unless every action, including those of link queries
  (`ParameterValue::DefaultLink`, `ParameterLink`, `OverrideLink`, `EnumLink`, planned
  recursively), names a command matched by `allow`. Resource steps are allowed: the store is the
  agent's own memory. The allow-list is the safety boundary, because a context cannot yet be
  restricted (`COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT`). It is evaluated as `call` does.
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
- **`plan` and `call`** share one builder. It takes the key argument out of the call's arguments
  and fills it with `fill_key`, then makes a `Recipe` from the resulting query, with `arguments`
  set to the remaining arguments plus `fixed`, and `cwd` set to the tool set's `cwd`.
  - `call` then runs `get_asset_manager().apply(recipe, State::new(), None)` and `AssetRef::get`,
    and serializes the value with `DefaultValueSerializer::as_bytes` in the state's
    `get_data_format()`, which the query's filename set. The evaluation is unmanaged, so the tool
    result itself is not cached; its inputs are, through their own assets.
  - The command's log notes are sent as a **second text content block**, never inside the
    serialized value, so a CSV or JSON result stays parseable. The CLI writes them to stderr.
  - It runs in a spawned task, so a panic becomes a tool error.
- **`call_tool`** sends the serialized text. When the format is `json`, it also sends the parsed
  object as structured content. An `Error` becomes `CallToolResult::error` with its Display, so the agent
  reads it. Only an unknown tool name is a protocol error.
- **`get_info`** joins the tool sets' `instructions`. The memory tool set adds the README's L1 by
  calling its `overview` tool once at start, and one sentence pointing to the power layer: "For
  anything the tools above cannot express, read `liquers://memory/query-guide` and use
  `memory_query`."
- **Resources** come from `ResourceDecl`. A `Tool` resource matches the URI template and calls the
  named tool; a `Text` resource returns its text; a `Commands` resource renders the named
  namespace's `CommandMetadata` (signature, arguments, doc) as Markdown, so the reference never
  drifts from the registry. The engine knows nothing about memory.

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

**Subcommands.** `mcp`, `check [PATH]`, `search TEXT [--in PATH] [--limit N]`,
`read KEY [--tier T] [--section S]` and `query QUERY`. Each one calls `McpServer::call` with the
matching tool, so the CLI and MCP share one code path and one allow-list.

**Built-in tool set** (`tools/memory.yaml`, abridged):

```yaml
prefix: memory_
tools:
  - { name: overview, query: "-R-key/./-/ns-mem/overview/overview.md", read_only: true }
  - { name: list,   query: "-R-key/./-/ns-mem/list/list.md",   key: { name: path }, read_only: true }
  - { name: search, query: "-R-key/./-/ns-mem/search/search.md", key: { name: path }, read_only: true }
  - { name: read,   query: "-R/./-/ns-mem/read/read.md",       key: { name: key, required: true }, read_only: true }
  - { name: edit,   query: "-R-key/./-/ns-mem/edit/edit.json",   key: { name: key, required: true } }
  # describe, new, write, check likewise
query_tool:
  name: query
  allow: ["-mem-overview", "-mem-describe", "-mem-list", "-mem-search", "-mem-read",
          "-mem-similar", "-mem-check", "-rec-*", "-search-*"]
resources:
  - { kind: tool, uri_template: "liquers://memory/{+key}", tool: read, argument: key }
  - { kind: text, uri: "liquers://memory/query-guide", name: query-guide,
      description: "Liquers queries over this memory", text: "<include_str!(query-guide.md)>" }
  - { kind: commands, uri_template: "liquers://commands/{namespace}", namespaces: [mem, rec, search] }
```

`tools/query-guide.md` is short: the query shape (`-R/<key>` for a document, `-R-key/<folder>`
for a folder, `/-/` before the first command), escaping, five worked queries, and the link to
`liquers://commands/{namespace}`. Its queries are checked by `liquers-validate` in a test.
Whether `-rec-*` and `-search-*` contain no writing command is checked by that test too, against
the registry.

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

- is `async`;
- takes state, in one of two kinds:
  - **document state** (`read` only): the document's value from `-R/<key>`. Its key is the
    state's `MetadataRecord.key`; the value supplies the text, and the metadata supplies `title`
    and `description` (filled by `front_matter` and `markdown_tiers`). A state without a key is
    accepted, and `read` then reports no `key` and no `version`;
  - **key state** (all others): a `Value::Key` from `-R-key/<path>`, converted with
    `TryFrom<Value> for Key`, as link arguments are (added for `liquers-lib`'s `Value` by this
    design; core has it only for its own). Any other value is refused with
    `Error::general_error`. A key state avoids evaluating a folder
    (`DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE`) and, for writes, a document that is
    about to be replaced;
- is `volatile: true`, except `read`, whose input is a recorded dependency (see the preflight);
- returns a value the serialization system writes in several formats, with a `filename:` whose
  extension is the default chosen for agents (table below): a **record view** for tables, **text**
  (Markdown) for documents and orientation, and a small JSON object for write results;
- reports counts, truncation and versions as log notes (`context.info`), e.g.
  `shown 10 of 47; raise limit to see more` or `version 1a2b…`, so the table itself stays a table;
- ends with `root: Key = query "-R-key/."` (`COMMAND_REGISTRATION_GUIDE.md` §cwd), the memory's
  root, where `_schema/memory.yaml` is found. A key state is relative to the same working
  directory, so `-R-key/./…` and `root` agree unless a query overrides `root`.

`root` comes last, against the guide's ordering pitfall: promotion affects only the encoded
identity of a cached asset, the volatile commands are never cached, and `read` does not use
`root`. `liquers-template` made the same choice (its question 14). The MCP server sets the root
through `Recipe::cwd` and never exposes `root`.

```rust
register_command!(cr, async fn overview(state, log_lines: i64 = 10, root: Key = query "-R-key/.", context) -> result
    namespace: "mem" filename: "overview.md" volatile: true doc: "The memory's README tiers, its collections with counts, and the latest log lines")?;
register_command!(cr, async fn describe(state, collection: String = "", root: Key = query "-R-key/.", context) -> result
    namespace: "mem" filename: "describe.md" volatile: true doc: "Collections, fields, vocabularies, templates; which are writable")?;
register_command!(cr, async fn list(state, depth: i64 = 1, include_superseded: bool = false,
    limit: i64 = 100, root: Key = query "-R-key/.", context) -> result namespace: "mem" filename: "list.md" volatile: true)?;
register_command!(cr, async fn search(state, text: String = "",
    unit: String = "document" (enum: ["document", "section"]), filter: String = "", limit: i64 = 10,
    include_superseded: bool = false, root: Key = query "-R-key/.", context) -> result namespace: "mem" filename: "search.md" volatile: true)?;
register_command!(cr, async fn read(state, tier: String = "full" (enum: ["l0", "l1", "full"]),
    section: String = "", lines: String = "", context) -> result namespace: "mem" filename: "read.md")?;
register_command!(cr, async fn similar(state, collection: String, title: String, description: String = "",
    limit: i64 = 5, root: Key = query "-R-key/.", context) -> result namespace: "mem" filename: "similar.md" volatile: true)?;
register_command!(cr, async fn new(state, collection: String, title: String, description: String, fields: Value = "",
    body: String = "", force: bool = false, reason: String = "", root: Key = query "-R-key/.", context) -> result
    namespace: "mem" filename: "new.json" volatile: true)?;
register_command!(cr, async fn write(state, body: String = "", title: String = "", description: String = "",
    fields: Value = "", expected_version: String = "", supersedes: String = "", reason: String = "",
    root: Key = query "-R-key/.", context) -> result namespace: "mem" filename: "write.json" volatile: true)?;
register_command!(cr, async fn edit(state, old: String = "", new: String = "", field: String = "",
    value: String = "", section: String = "", row: String = "", expected_version: String = "", reason: String = "",
    root: Key = query "-R-key/.", context) -> result namespace: "mem" filename: "edit.json" volatile: true)?;
register_command!(cr, async fn check(state, root: Key = query "-R-key/.", context) -> result
    namespace: "mem" filename: "check.md" volatile: true)?;
```

The state is what the command acts on: the memory's root for `overview`, `describe`, `similar`
and `new` (whose `collection` argument picks the folder); a folder for `list`, `search` and
`check`; the document's key for `write` and `edit`. `read`'s `version` is
`Version::from_content` of the state's text, the function `commit` uses on the stored bytes;
Phase 3 tests that the two agree for every document in `specs/`, since a disagreement would make
every `expected_version` fail.

Equivalent queries, as an agent writes them in `memory_query` or the CLI:

```
-R-key/specs/issues/-/ns-mem/list-2
-R/specs/issues/NO-MARKDOWN-NAVIGATION-COMMANDS.md/-/ns-mem/read-l1
-R-key/specs/-/ns-mem/search-tiers
```

`fields: Value` accepts both an empty text (the query-text default) and a JSON object (an MCP
override).

| Tool (`memory_`) | Key argument → state | Command | Returns (default file) | AC |
|---|---|---|---|---|
| `overview` | — (root) | `-mem-overview` | text: README L0/L1, collections with L0/L1 and counts, latest log lines (`overview.md`) | AC-15 |
| `describe` | — (root) | `-mem-describe` | text: collections, fields, vocabularies, writability (`describe.md`) | AC-15, AC-13 |
| `list` | `path` (optional) → `-R-key/<path>` | `-mem-list` | record view `key, l0, l1, kind, status, asset_status, is_dir` (`list.md`); note `shown/total` | AC-1, AC-8, AC-12, AC-17 |
| `search` | `path` (optional) → `-R-key/<path>` | `-mem-search` | record view `key, l0, l1, excerpt, score, matched, heading, line` (`search.md`); note `shown/total` | AC-2, AC-8, AC-12 |
| `read` | `key` → `-R/<key>` | `-mem-read` | text: the tier, section or lines asked for (`read.md`); notes `key`, `version` | AC-3 |
| `new` | — (root) | `-mem-new` | JSON `{written: false, candidates}` or `{written: true, key, version}` (`new.json`) | AC-4, AC-5 |
| `write` | `key` → `-R-key/<key>` | `-mem-write` | JSON `{key, version_before, version_after}` (`write.json`) | AC-5 to AC-8 |
| `edit` | `key` → `-R-key/<key>` | `-mem-edit` | as `write` (`edit.json`) | AC-6, AC-7, AC-16 |
| `check` | `path` (optional) → `-R-key/<path>` | `-mem-check` | record view `rule, key, severity, message` (`check.md`); notes counts, `unsupported` | AC-9 |
| `query` | — (any query) | allow-listed commands | the query's value, in the format its filename (or the value's default) selects | power layer |

Markdown is the default because agents read it with the least overhead: a record view's `md`
format is a pipe table, and documents are Markdown already. A power user asks for another format
with the query's filename (`…/ns-mem/list/list.csv`, `…/list.json`), or pipes a table on
(`…/ns-mem/list/ns-rec/sort-l0/sorted.md`). Each tool's query ends with its default filename, and
every basic tool also takes an optional `format` argument that the engine applies by changing that
filename's extension in the parsed query.

How each acceptance scenario is met:

- **AC-1, AC-17 (tiers).** L0 and L1 come from the `tiers` bridge. By default it reads `title` and
  `description` from `AssetInfo`, which the mount's chain fills with `front_matter` and then
  `markdown_tiers`. A folder's tiers come from its
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
- **AC-8 (supersession).** `write` with `supersedes` makes two commits: the new memory first, with
  `supersedes: <old key>` in its front-matter, then `superseded_by` set on the old one, and logs
  both. `check`'s `supersede_reciprocal` rule reports a partial supersede. `list` and `search` hide a memory with a
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
- **AC-14.** The nine basic tools above, served by `liquers-agent mcp`, plus `memory_query`.
- **AC-16 (edits).** Each pair of arguments is one operation, and `edit` applies every pair that
  is given, in a fixed order (replace, then set field, then append row), to the text in memory,
  then validates and commits once. A half-given pair (`old` without `new`, `field` without
  `value`, `section` without `row`), or no pair at all, is refused before anything is read.
  `new` may be empty, which deletes `old`.
  - replace (`old`, `new`) requires exactly one match of `old`.
  - set field (`field`, `value`) checks `enum` and `transitions`. `value` is parsed as a YAML
    scalar or flow list (`[a, b]`), so list fields take one argument.
  - append row (`section`, `row`) appends a Markdown row to the named section. When that section is the collection's
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
| `memory.yaml` `version` newer than supported | `Error::configuration_error`, naming both versions |
| Schema file missing or malformed; a tool set naming an unknown command, argument or tool; a duplicate tool name | `Error::configuration_error` |
| A write that violates the schema: an unknown field value, a missing `title`/`description`, a forbidden transition, a key outside every collection, a collection with `writable: false` | `Error::general_error`, naming the field, value and allowed values, as `store_config`'s validate step does |
| `expected_version` differs from the current content hash | `Error::dependency_version_mismatch(&DependencyKey::from(&key), "expected …, found …")` |
| `expected_version` is not 32 hex digits | `Error::parse_error` |
| `edit` with `old` not found, or found more than once | `Error::general_error`, with the count; nothing is written, whatever other pairs were given |
| `edit` with a half-given pair, or with no pair | `Error::general_error`, naming the missing argument |
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
| `guides/AGENT_MEMORY_MIGRATION_GUIDE.md` | guide, new; area `[lib/commands, docs]`; for maintainers of a memory | What is durable and what is derived, the rules that keep it so, and the steps per kind of change; from `migration-guide-draft.md` |
| `.claude/skills/liquers-memory/SKILL.md` | skill, new | When to use memory: orientation first, search before writing, L0/L1 with every write |
| `guides/STORE_CONFIG_GUIDE.md` | update | `markdown_tiers` in the handler catalogue; `storage: none` for checkouts edited by hand. History row, `reviewed:` bump |
| `guides/COMMAND_REGISTRATION_GUIDE.md` | update | New section "Commands served as MCP tools": one command per tool, the state as a key argument filled structurally, relative-default arguments hidden, the query tool's allow-list. History row, `reviewed:` bump |
| `reference/PROJECT_OVERVIEW.md` | update | The crate list gains `liquers-mcp` and `liquers-agent`. History row, `reviewed:` bump |
| `DOCS_STRUCTURE_GUIDE.md` | update | §3 gains area `mcp` (`liquers-mcp` and `liquers-agent`); §4.8 names the `memory_new` route beside `docs_index.py new`. History row |
| `CLAUDE.md` | update | Project structure and dependency flow gain the two crates |
| `README.md` (specs) | update | The capability map links the reference document |

Proposed `affects_docs`: `reference/AGENT_MEMORY_SERVICE.md`, `guides/AGENT_MEMORY_MIGRATION_GUIDE.md`, `guides/STORE_CONFIG_GUIDE.md`,
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
| Security | Tool arguments cannot change a query's shape: the key is substituted structurally and validated, and `root` is hidden. `memory_query` is limited by its allow-list, checked on the whole plan including link queries; it can read any mount of the agent's store and cannot write. Writes are limited to writable collections and mounts. Stdout carries MCP frames, so a stray `println!` in a linked library breaks the server (`CLAUDE.md` forbids it) |
| Dependencies | `rmcp` 3.5.1 (MSRV 1.88 ≤ 1.95), features `server` and `transport-io` only. The version is pinned, because protocol types change between minor versions |
| Recovery | Revert the crates, and remove `mem` from `default`. Memory files are plain Markdown |
| Certainty and open questions | High for the commands and the engine. **Medium** for byte-for-byte `check` parity, because `docs_index.py` also has rules over the whole corpus, and for the cost of `storage: none` plus `front_matter` on large listings, which Phase 3 measures |

## Open Questions

1. **Two interfaces over Liquers-style commands.** DECIDED 2026-10-17 (Orest): commands take
   their document or key as state; basic tools are generated from command metadata with a key
   argument filled structurally; `memory_query` plus a query guide and command reference form the
   power layer. Core option C excluded for now (`RECIPE-CANNOT-PARAMETERIZE-RESOURCE-KEY`).
2. **The write body as an argument** (an override), not the evaluation payload. DECIDED (A).
3. **Every `ns-mem` command volatile except `read`** until
   `SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES` is fixed; `read`'s input is a recorded
   dependency. DECIDED (A).
4. **Schema location:** one `<root>/_schema/memory.yaml`, with vocabularies in sibling files.
   DECIDED (A). Per-collection files can come later without a format change.
5. **Fallback tiers as a `store_config` handler** (`markdown_tiers`), configured per mount (change
   3). DECIDED (A), with the L0/L1 bridge (change 8): front-matter plus configuration, kept
   flexible because the right tiers are not known yet.
6. **Results through the serialization system.** DECIDED 2026-10-17 (Orest): `ns-mem` returns
   what agents digest best (record views and Markdown text, JSON for write results), and power
   users pick other formats through the query's filename (change 9).
7. **`mem` as a default feature of `liquers-lib`**, so that the exported registry and
   `liquers-validate` know `ns-mem`. It adds only `indexmap`. DECIDED (A).
8. **`storage: none` on writable checkout mounts.** No `.__metadata__` sidecars are written into
   the repository, and the record is re-derived on every read. DECIDED (A) for the specs; front-matter
   is where their metadata is maintained.
9. **The query tool's allow-list**: the read-only `ns-mem` commands plus all of `ns-rec` and
   `ns-search` (none of which writes, checked against the registry in a test). DECIDED for the MVP
   (Orest, 2026-10-17). Polars (`ns-pl`) and other namespaces can be added per profile. The general
   answer is access control (`CORE-SESSION-AND-KEY-ACL`): once it exists, the allow-list should
   give way to permissions checked by core rather than by `liquers-mcp`.
