---
title: Liquers Project Overview
kind: reference
audience: internal
area: [core/query, core/plan, core/assets, core/store, core/value]
reviewed: 2026-08-17
---
# Liquers Project Overview

## Executive Summary

**Liquers** is a query-driven data transformation framework with a custom domain-specific language (DSL). It enables users to express data pipelines as URL-compatible query strings that describe:
1. **Resources** to load (files, stored data)
2. **Transformations** to apply (commands/actions)
3. **Output format** (file extension determines serialization)

Example: `-R/data/test.csv/-/filter-a1/add_columns/result.json`

The Rust implementation is a complete reimplementation and redesign of an [older Python version](https://orest-d.github.io/liquer/site/index.html), intended to supersede it while maintaining Python compatibility via bindings.

---

## Design Philosophy

### Query Language Requirements
1. **URL-compatible**: Queries must be valid URL path segments
2. **Path-like appearance**: Familiar to users, resembles file paths
3. **Sparse syntax**: Most pipelines should be one-liners
4. **Composable**: Commands chain naturally via path separators

### Core Principles
- **Async-first**: Default execution model for web (WASM), servers, and ecosystem compatibility
- **Trait-based extensibility**: Custom values, stores, and command executors via traits
- **Three-layer value encapsulation**: Progressive abstraction from data to managed resources
- **Realm-based command separation**: Different environments (desktop GUI, headless server, browser) can coexist

---

## Architecture Overview

### Crate Structure

```
liquers-core (foundation - all core abstractions)
    │
    ├── liquers-macro (proc-macro: #[register_command])
    │   └─ Code generation for command registration
    │
    ├── liquers-store (storage backends)
    │   ├─ OpenDAL integration (S3, FTP, SFTP, WebDAV, etc.)
    │   ├─ Config-driven store routing
    │   └─ Implements AsyncStore trait
    │
    ├── liquers-lib (rich value types + UI)
    │   ├─ Extended value types (DataFrames, Images, UI commands)
    │   ├─ Polars integration for tabular data
    │   ├─ egui-based interactive UI
    │   └─ Implements Environment trait
    │
    ├── liquers-axum (HTTP REST API server)
    │   ├─ Query evaluation endpoints
    │   ├─ Store API (CRUD operations)
    │   └─ Implements Environment trait
    │
    ├── liquers-web (browser/JavaScript bindings via wasm-bindgen)
    │   ├─ Value bridge: structural conversion, plus opaque retention by opt-in
    │   ├─ Commands written in JavaScript, composed with the Rust ones in one query
    │   ├─ Query evaluation as Promises; no blocking entry point exists
    │   └─ wasm32-only; excluded from the workspace default-members
    │
    └── liquers-py (Python bindings via PyO3)
        └─ FFI wrappers for Python interoperability
```

### liquers-core Module Structure

| Module | Lines | Purpose |
|--------|-------|---------|
| `query.rs` | ~2600 | Query DSL: Query, Key, ActionRequest, segments |
| `metadata.rs` | ~1500 | Metadata, Status, logging, progress tracking |
| `store.rs` | ~1200 | Storage abstraction: Store, AsyncStore, routers |
| `assets.rs` | ~1400 | Asset lifecycle management, async execution |
| `interpreter.rs` | ~400 | Plan execution engine |
| `commands.rs` | ~300 | Command execution framework |
| `command_metadata.rs` | ~600 | Command registry, argument specs |
| `context.rs` | ~300 | Environment, Session, Context hierarchy |
| `value.rs` | ~400 | ValueInterface trait, built-in Value enum |
| `state.rs` | ~150 | State = Value + Metadata |
| `plan.rs` | ~200 | Execution plan representation |
| `recipes.rs` | ~200 | Recipe definitions (queries + metadata + overrides) |
| `cache.rs` | ~150 | Query result caching |
| `parse.rs` | ~400 | nom-based query parser |
| `error.rs` | ~300 | Error types and handling |
| `dependencies.rs` | ~150 | Version tracking for cache invalidation |

---

## Core Concepts

### 1. Query Language

**Syntax**: Queries consist of segments, each prefixed to indicate type:

```
-R/resource/path/-/action-param1-param2/action2/output.ext
```

**Segment Prefixes**:
- `-R` - **Resource segment**: resolves a managed asset by key; use the `stored` selector
  (`-R-stored/path`) for a direct `GetResource` store read
- `-` - **Transform segment**: sequence of commands/actions

**Future prefixes** (under consideration):
- `-S` - **Selection segment**: select part of data (row, column, range, JSON element)

**Components**:
- **Resource segment**: `-R/data/input.csv` evaluates or reuses the managed keyed asset;
  `-R-stored/data/input.csv` reads the stored resource directly
- **Transform segment**: `-/filter-column-value` - applies command with args
- **Output filename**: `result.json` - determines filename and serialization format
- **Segment separator**: `/-/` separates resource from transform

**Legacy shorthand**: If a query has exactly two parts and the second is a transform, the first is treated as a resource (may be phased out due to confusion).

**Segment Headers**: Queries can specify realm in the segment header. Realm applies to the whole segment. Namespace can change within a segment using `ns` instruction.

**Parameter arity**: every parameter written in a query must be consumed. An action supplying more
parameters than its command declares is an **error** at plan build time, carrying the position of
the first surplus parameter so an editor or validator can point at it:

```
ns-pl/select_columns-a-b
Too many parameters for command 'select_columns': accepts 1, but parameter #2 'b' was supplied
```

The count a command accepts is not simply its number of arguments. *Injected* arguments are supplied
by the execution context and consume no query parameter, and an alias's head parameters fill leading
argument slots before the action is consulted; neither is available to the writer. An argument
declared `multiple` consumes every remaining parameter, so a command with one is never over-supplied
— that is the sanctioned way to accept a variable-length list. (Declaring one through
`register_command!` is not yet possible; see
[`COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE`](../issues/COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE.md).)

The special instructions resolve no command metadata, so each carries its own rule: `v` and `q` take
no parameters and reject any, while **`ns` is variadic by design** — every parameter names a
namespace, so `ns-one-two` is correct and must keep working.

A resource header takes exactly one instruction, and surplus header parameters are an error on the
same terms. Its *name*, by contrast, is only warned about and then ignored — the name is reserved
for a future realm interpretation, and rejecting it now would refuse queries a later version
accepts. The two are treated differently on purpose: an input that will acquire meaning is warned
about, an input nothing can ever consume is rejected.

**Special encoding** (for URL compatibility):
- `~~` → `~`
- `~_` → `-`
- `~I` or `~/` → `/`
- `~.` → space
- `~H` → `https://`, `~h` → `http://`

`~X~...~E` delimits a **link action parameter** — a nested query supplied as an
argument value. It is not an entity: it does not decode to a character inside a string
parameter but selects a different kind of parameter. See `liquers_core::parse` for the
syntax, the nesting bounds, and why the resource/transform shorthand is rejected
inside a link.

### 2. Three-Layer Value Encapsulation

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 3: Asset                                              │
│   - Handle to resource (may not exist yet)                  │
│   - Async lifecycle (submitted → processing → ready/error)  │
│   - Channels for progress/status updates                    │
│   - Manages serialized binary form                          │
│   - Recipe that produced it (can re-execute)                │
├─────────────────────────────────────────────────────────────┤
│ Layer 2: State<V>                                           │
│   - Value + Metadata (immutable, Arc-wrapped)               │
│   - Thread-safe, shareable                                  │
│   - Input/output of command execution                       │
├─────────────────────────────────────────────────────────────┤
│ Layer 1: Value (V: ValueInterface)                          │
│   - Raw data (scalars, collections, bytes, etc.)            │
│   - Serialization/deserialization capabilities              │
│   - Type identification                                     │
└─────────────────────────────────────────────────────────────┘
```

### 3. Command System

**Identification**: `CommandKey(realm, namespace, name)`

- **Realm**: Environment capability separation

  - Example of capabilities:
    - Desktop GUI (can draw on display)
    - Headless server (no UI)
    - Browser frontend (limited APIs)

  - Allows routing: web client sends "backend" realm to server, executes "frontend" in browser
  - Realm interpretation is the responsibility of a plan interpreter. (Currently there is no multi-realm interpreter implemented.) 

- **Namespace**: Logical grouping of related commands
  - Multiple namespaces can be active
  - Searched in order during command resolution

- **Name**: Specific command identifier. (Typically a function name.)

**Registration**: Via `#[register_command]` proc-macro (liquers-macro) or manually via the registration API, see `liquers-core/src/commands.rs`, `CommandRegistry::register_command` method.

**First Commands**: Commands that generate data without requiring input (e.g., database queries, datetime). Currently, commands that ignore their input effectively act as first commands. Better support in command metadata may be added.

**Volatile Commands**: Commands that may produce different output each time (e.g., `datetime`, random generators). A query becomes volatile if it contains a volatile command or volatile resource. Volatile queries are re-executed on each request rather than cached.

### 4. Recipes

Recipes generalize queries by adding:
1. **Extra metadata**: title, description
2. **Hierarchical storage**: recipes may reside in key structure (via AsyncRecipeProvider), enabling on-demand resource creation
3. **Parameter overrides**: convenience for long arguments (e.g., SQL scripts)
4. **Complex ad-hoc queries**: JSON API representation

```rust
pub struct Recipe {
    pub query: String,              // Base query
    pub title: String,
    pub description: String,
    pub arguments: HashMap<String, Value>,  // Overrides
    pub links: HashMap<String, String>,     // Link overrides
    pub cwd: Option<String>,
    pub volatile: bool,
}
```

`cwd` is a logical Liquers key, not an operating-system directory. Recipes loaded by
`DefaultRecipeProvider` inherit the directory containing `recipes.yaml`; YAML authors do not set
the field. Programmatic callers may set it directly. Conversion to a plan preserves relative
operands and prepends one `SetCwd` plus a non-executable initialization diagnostic. The interpreter,
not `PlanBuilder`, is responsible for resolving subsequent keys, queries, links, and nested plans.

### 5. Storage (Store)

**Key-based abstraction** - Keys are path-like but not filesystem paths:
- `folder/subfolder/file.txt` - hierarchical structure
- Relative navigation (`.`, `..`) is a **plan-level** feature: it is resolved against a current
  working directory while the plan is built (`Key::to_absolute`)
- **A key given to a store must be absolute** - no element may be `.` or `..`. A store never
  resolves them, so a relative key reaching one is a malformed address and is refused with
  `ErrorType::KeyNotAbsolute` (400 over HTTP). Refusal rather than normalization, because a key is
  an address: quietly equating `a/../b` with `b` would make two addresses alias one asset. This is
  well-formedness, not authorization. Check with `Key::as_absolute`; see the `liquers_core::store`
  module documentation, and `specs/reference/api/API_DOCS_GAP_ANALYSIS.md` §7 for what DOC-07 owes

**Operations**:
- `get(key)` / `get_bytes(key)` / `get_metadata(key)`
- `set(key, data, metadata)` / `set_metadata(key, metadata)`
- `remove(key)` / `listdir(key)` / `is_dir(key)`

**Routing**: `AsyncStoreRouter` directs requests by key prefix to appropriate backends
`AsyncStoreRouter` implements the `AsyncStore` interface, so it can be used as a store.

### 6. Execution Flow

```
User Query (String)
       │
       ↓ parse_query()
Query AST
       │
       ↓ PlanBuilder::build()
Execution Plan (Vec<Step>)
       │
       ↓ apply_plan() [async loop]
do_step() for each Step
  ├── GetResource → AsyncStore
  ├── Action → CommandExecutor
  └── Evaluate → recursive
       │
       ↓
State + Metadata
       │
       ↓
Optional serialization to a store
```

Execution is managed and monitored via assets (`AssetRef`).
Assets are handles that represent the whole process and get progress updates.

During plan finalization, dependency analysis simulates ordered CWD changes without changing the
live execution Context. During execution, each relative operand is normalized against the current
logical CWD immediately before use. Thus recipe CWD and later `-R-cwd` instructions compose in
order, while an absolute outer query's source resource remains rooted independently of that live
CWD. Relative child links still use the live CWD. With no CWD, the first relative operand installs
logical root `/` and records one warning for the shared evaluation Context.

### 7. Context Hierarchy

**Environment** - Global shared state providing access to services:
```
Environment (global, shared across all queries)
  ├── get_command_executor()           // Execute commands
  ├── get_command_metadata_registry()  // Command documentation
  ├── get_async_store()                // Storage access
  ├── get_asset_manager()              // Asset lifecycle
  └── get_recipe_provider()            // Recipe loading
```
- Typically one Environment per application (chosen at compile time via generic parameters)
- Multiple environments possible for isolated subsystems or different realms with very different capabilities
- **Asset manager is pluggable** via the `Environment::AssetManager` associated type: the threaded
  `DefaultAssetManager` (JobQueue + background tasks) natively, or the spawn-free
  `ImmediateAssetManager` (inline evaluation, no `tokio::spawn`/timers) for single-threaded targets.
  `liquers-core` compiles to `wasm32-unknown-unknown` and runs in the browser via
  `ImmediateAssetManager` + target-gated conditional-`Send` (`MaybeSend`/`MaybeSync` markers +
  `#[async_trait(?Send)]` on wasm); see `specs/design/async-wasm-refactor/`.

**Context** - Per-evaluation execution context, shared by command-facing clones in a pipeline:
```
Context (per-evaluation, shared by command-facing clones)
  ├── envref        // Reference to Environment
  ├── assetref      // Reference to current Asset (for progress/logging)
  ├── cwd_key       // Current working directory (Key)
  ├── service_tx    // Channel to communicate with Asset
  └── payload       // Arbitrary user data (see below)
```

The mutex-backed `cwd_key` is live evaluation state. Interpreter `SetCwd` steps update it, and
`Context::evaluate`, `Context::get_dependency_state`, and `Context::apply` resolve relative queries
against it. Nested linked evaluations inherit the current CWD as a scoped starting point; their
subsequent CWD changes do not escape into the caller. Dependencies, cycle checks, cache lookup, and
keyed-recipe ownership use resolved identities rather than raw relative spellings.

**Service Channel** (`service_tx`) - Commands communicate with their Asset via messages:
- Progress updates (primary and secondary)
- Log messages
- Status changes
- Error reporting

**Payload** - Arbitrary data structure passed through Context during query evaluation:
- Type is specified by the Environment (generic parameter)
- Associated with a single query evaluation
- **Mutable**: Commands can modify payload (interior mutability)
- **Inherited**: Sub-queries receive the parent's payload when the command declares `payload: required`
- Use case: UI window handle, request context, accumulated state
- **Limitation**: Not available to background/async evaluation, nor to keyed assets (a key is global, a payload is per-evaluation). Requiring a payload implies `volatile`

**Session** (planned/minimal):
```
Session (user session - currently minimal)
  └── get_user()    // Current user info
```
- Intended for tracking user sessions (e.g., from web service)
- Should enable authorization: read, write, execute, delete rights
- **Design challenge**: Assets are shared across users, so asset creation can't depend on who executed it. Authorization must be handled at access points, not during asset creation.

---

## Key Design Decisions

### Queries and Recipes define stateless executions
- Queries and recipes should provide a complete description of how to create an asset.
- Queries and recipes are stateless - if the commands are stateless and the data stored in the store is constant. This should integrate well with a REST API.
- In practice commands may interact with non-static systems, e.g. databases and data in the store may be modified by the user. Such issues will partly be mitigated by dependency checking.  

### Keys identify named resources
- Keys form a natural hierarchical structure
- Keys allow for a unified access to both data available from store and assets created on demand.

### Async-First Strategy
- **Primary**: Async execution for WASM, servers, Rust ecosystem
- **Sync**: Wrapper over async, mainly for Python user convenience
- **Store**: Async-only in medium term (sync store to be removed)

### Error Handling
- `liquers_core::error::Error` with `ErrorType` enum
- Position tracking for precise error location in queries
- Query/Key context preserved in errors

### Metadata Tracking
- Complete audit trail of asset creation
- Status lifecycle: None → Recipe/Source → Submitted → Dependencies → Processing → Ready/Error
- Structured logging with timestamps
- Progress tracking (primary + secondary)

### Volatility
- **Volatile commands**: May produce different output each time (e.g., datetime, random)
- **Volatile queries**: Contain volatile commands or volatile resources
- **Volatile recipes**: Depend on volatile queries or contain volatile links
- **Volatile resources**: Defined by volatile recipes
- **Behavior**: Volatile assets are re-executed on each request, not cached

---

## Future Plans

### Priority Areas
1. **DataFrames (Polars)** - Highest priority data type for liquers-lib
2. **More storage backends** - Database integrations, cloud services
3. **Better Python integration** - Tighter data science ecosystem integration
4. **Web UI/Dashboard** - Interactive interface for queries
5. **Extended library** - Images, matrices/tensors, ML models

### UI Roadmap
1. **Phase 1**: Desktop egui application (current focus)
2. **Phase 2**: WASM egui (port desktop to browser)
3. **Phase 3**: HTML GUI (likely Dioxus-based)
4. **Also planned**: Terminal UI (TUI)

### Technical Debt / Gaps

**Implementation Gaps**:
- Dependency checking: Designed in `dependencies.rs` but not implemented
- Multi-realm interpreter: Design exists but no implementation yet
- Asset garbage collection: Not designed; strategy should be configurable (reference counting likely)
- Cache module: Legacy from Python, may be phased out (Assets provide natural caching)
- First command metadata: Commands that generate data need better metadata support

**Code Quality**:
- Query encoding: only string action parameters are escaped (`encode_token`).
  Resource names, action names, header names and values, and filenames are
  emitted raw, so a programmatically constructed value in one of *those*
  positions can still break the encode/parse round-trip. Not reachable from
  parsed input. String action parameters are no longer part of this caveat:
  `encode_token` escapes every character the grammar cannot carry, including
  `~`, so `parse(encode(p)) == p` holds for any parameter value.
- Query parsing is exponential in link nesting depth, currently contained by a
  depth bound rather than fixed; see `QUERY-LINK-EXPONENTIAL-BACKTRACKING` in
  `specs/issues/` (indexed by `specs/index.csv`)
- Documentation needs to be written (current Python docs are obsolete/incomplete)
- Some sync code may be considered obsolete
- Testing gaps: Both unit tests and integration tests need improvement

---

## References

### Documentation
- [Python LiQuer docs](https://orest-d.github.io/liquer/site/index.html) (obsolete but relevant)
- [Query language spec](https://raw.githubusercontent.com/orest-d/liquer/refs/heads/master/docs/query.md)
- [Store Config FSD](./STORE_CONFIG_FSD.md)

### Key Source Files
- `liquers-core/src/query.rs` - Query DSL implementation
- `liquers-core/src/store.rs` - Storage abstraction
- `liquers-core/src/assets.rs` - Asset lifecycle
- `liquers-core/src/interpreter.rs` - Execution engine
- `liquers-core/src/command_metadata.rs` - Command registry

---

## Glossary

| Term | Definition |
|------|------------|
| **Query** | URL-compatible string describing a data pipeline |
| **Key** | Path-like identifier for stored resources |
| **Segment** | Part of a query, prefixed with `-R` (resource) or `-` (transform) |
| **Action** | Single command with parameters in a transformation |
| **State** | Value + Metadata (immutable, shareable) |
| **Asset** | Managed resource with lifecycle (may not exist yet) |
| **Recipe** | Query + metadata + parameter overrides |
| **Realm** | Environment capability context (GUI, server, browser) |
| **Namespace** | Logical grouping of commands |
| **Store** | Key-value storage backend |
| **Plan** | Compiled sequence of execution steps |
| **Volatile** | Command/query/recipe that may produce different results each time |
| **First Command** | Command that generates data without requiring input |
| **Segment Header** | Query metadata specifying realm (applies to whole segment) |
| **Environment** | Global shared state providing access to services (store, assets, recipes) |
| **Context** | Per-evaluation execution context shared by command-facing clones in a pipeline |
| **Payload** | Mutable user data passed through Context; inherited by sub-queries that declare `payload: required`; type defined by Environment |

---

*Last updated: 2026-08-11*

## History

| Date | Change | Source |
|---|---|---|
| 2026-08-17 | Corrected §5 Storage: it claimed "safe encoding prevents arbitrary file access", which was not true — a key containing `..` escaped the file store root. States the absolute-key precondition, its error and where relative navigation actually belongs. | `design/store-key-guard/` |
| 2026-08-14 | Recorded that string action parameters now escape every character, so a parameter round-trips for any value; the raw-emission caveat is narrowed to resource names, action names, headers and filenames. | PARAMETER-ESCAPING-INCOMPLETE |
| 2026-08-11 | Reviewed recipe planning and execution; documented provider/programmatic CWD provenance, interpreter-owned ordered resolution, scoped nested evaluation, and resolved identities. | phase-5 |
| 2026-08-08 | Last substantive edit, carried into `reference/` unchanged. Not reviewed against the implementation since. | migration |
| 2026-08-12 | Documented parameter arity: surplus action parameters are a positioned error, `multiple` consumes the remainder, the `v`/`q` instructions take none while `ns` is variadic, and the resource header errors on surplus parameters while still only warning about its reserved name. | design/excess-action-parameters-error |
