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
- `-R` - **Resource segment**: loads data from store by key
- `-` - **Transform segment**: sequence of commands/actions

**Future prefixes** (under consideration):
- `-S` - **Selection segment**: select part of data (row, column, range, JSON element)

**Components**:
- **Resource segment**: `-R/data/input.csv` - loads from store
- **Transform segment**: `-/filter-column-value` - applies command with args
- **Output filename**: `result.json` - determines filename and serialization format
- **Segment separator**: `/-/` separates resource from transform

**Legacy shorthand**: If a query has exactly two parts and the second is a transform, the first is treated as a resource (may be phased out due to confusion).

**Segment Headers**: Queries can specify realm in the segment header. Realm applies to the whole segment. Namespace can change within a segment using `ns` instruction.

**Special encoding** (for URL compatibility):
- `~~` → `~`
- `~_` → `-`
- `~I` or `~/` → `/`
- `~.` → space
- `~X~...~E` → nested query (embedded link)
- `~H` → `https://`, `~h` → `http://`

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

### 5. Storage (Store)

**Key-based abstraction** - Keys are path-like but not filesystem paths:
- `folder/subfolder/file.txt` - hierarchical structure
- Safe encoding prevents arbitrary file access
- Supports relative navigation (`.`, `..`)

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

**Context** - Per-action execution context, created for each command in a pipeline:
```
Context (per-action, created for each command execution)
  ├── envref        // Reference to Environment
  ├── assetref      // Reference to current Asset (for progress/logging)
  ├── cwd_key       // Current working directory (Key)
  ├── service_tx    // Channel to communicate with Asset
  └── payload       // Arbitrary user data (see below)
```

**Service Channel** (`service_tx`) - Commands communicate with their Asset via messages:
- Progress updates (primary and secondary)
- Log messages
- Status changes
- Error reporting

**Payload** - Arbitrary data structure passed through Context during query evaluation:
- Type is specified by the Environment (generic parameter)
- Associated with a single query evaluation
- **Mutable**: Commands can modify payload (interior mutability)
- **Inherited**: Sub-queries (e.g., link parameters) receive the same payload as parent
- Use case: UI window handle, request context, accumulated state
- **Limitation**: Only available for immediate query evaluation; background/async evaluation uses a default payload

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
- Query encoding needs more careful design for embedded links
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
| **Context** | Per-action execution context, created for each command in a pipeline |
| **Payload** | Mutable user data passed through Context; inherited by sub-queries; type defined by Environment |

---

*Last updated: 2026-01-18*
