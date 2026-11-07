---
id: CONFIGURATION-DOCUMENT
kind: design
title: "One configuration document for every aspect of a host: environment, commands, server, language settings"
form: compact
workflow: liquers-project
status: in_review
phase: architecture
readiness: needs-decision
autofix: not-eligible
area: [core/context, axum, py]
issues: [HOSTS-READ-CONFIGURATION-DOCUMENTS-DIFFERENTLY]
affects_docs: [specs/reference/ENVIRONMENT_CONFIG.md, specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md, specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md, specs/README.md]
created: 2026-11-07
---
# One configuration document for every aspect of a host

Split on 2026-11-07 out of [`liquers-py-web-server`](../liquers-py-web-server/DESIGN.md) Phase 2,
whose size it doubled, after Orest asked for "a single configuration file (yaml or json) that would
configure all aspects - server, store, environment, recipe providers, command metadata registry (a
separate file)", assembled "from specific config structs" (that design's Decision 4). Compact form
(`M`). Required by `liquers-py-web-server` and `axum-server-assembly`; lands first, with or before
`axum-server-assembly`. Phases 1 and 2 written together, since the owner set the direction, and
reviewed once by an independent reviewer before the split.

## Phase 1: High-Level Design

### Purpose

Today each host reads its own document: `EnvironmentConfig` (store, recipes, assets,
`default_namespaces`) silently ignores any other key (`an_unknown_key_is_currently_ignored`,
`liquers-core/src/environment_config.rs`), `liquers-agent`'s `AgentConfig` cuts `memory` out by
hand, and liquers-web reads only a `StoreRouterConfig`. A server and language settings have no
place at all. One document, readable everywhere, with per-host sections that each host takes and an
error for a section nobody took, lets one file set up a whole application.

### Problem Example

```yaml
store:
  stores:
    - {type: filesystem, prefix: web, read_only: true, config: {path: ./site}}
command_registry: commands.yaml
python:
  pickle: true
server:
  port: 8080
sever:                 # typo
  port: 9000
```

**Today** `lq.Environment(config=…)` keeps the store, ignores the rest, and the typo passes.
**After** the Python host takes `server` and `python`, loads `commands.yaml` as its command metadata registry, and refuses the
document: `ConfigurationError: unknown section 'sever' (this host knows: store, recipes, assets,
default_namespaces, command_registry, server, python)`.

### Scope and Acceptance Criteria

- **AC-1** One file configures a Python host
  WHEN `lq.Environment(config=Path("liquers.yaml"))` reads the problem example without the typo
  THEN the store, `pickle` and the server settings come from it, and `env.serve()` listens on 8080
- **AC-2** An unknown section is refused
  WHEN a document has a section the host does not take
  THEN construction fails with `ConfigurationError` naming it and the known sections
- **AC-3** The command metadata registry file
  WHEN `command_registry` names a file relative to the document
  THEN it is loaded as the command metadata registry, as `import_metadata` loads metadata today
- **AC-4** Keyword arguments override the file
  WHEN the file sets `python.pickle: false` and the call passes `pickle=True`, or `serve(port=0)`
  THEN the keyword wins
- **AC-5** A file name passed as text is explained
  WHEN `config="liquers.yaml"` is a `str` that parses to a YAML scalar
  THEN the error says to pass a `pathlib.Path` to read a file
- **AC-6** An empty document is the defaults
  WHEN the document is empty, or `config` is omitted
  THEN the environment and server equal the documented defaults
- **AC-7** A Rust host uses the same document
  WHEN a Rust program loads the file, takes `server` and finishes
  THEN it builds the same environment and server (`axum-server-assembly` Phase 2 snippet)

Non-goals: adopting the document in liquers-web and `liquers-agent` (follow-up, under the source
issue); store paths relative to the file (they stay relative to the working directory, as today:
`STORE-PATHS-ARE-NOT-RELATIVE-TO-THE-CONFIGURATION-FILE`, filed with this design); environment
variable selection of a file.

### Documentation Intent

- Reference: `ENVIRONMENT_CONFIG.md` gains §The configuration document (sections, `command_registry`,
  hosts and their sections, unknown sections, defaults).
- Guide: `ENVIRONMENT_CONSTRUCTION_GUIDE.md` "Configure from one file"; `PYTHON_GUIDE.md`
  "Configure with a file".
- API docs: `liquers-core/src/environment_config.rs` (components, example) and the new
  `liquers-py/src/config.rs` (all five parts).

### Open Questions

1. **Decided: one field names the CMR file.** Orest, 2026-11-07: "I only ask for a field in config
   saying which file to load as a CMR." `command_registry` names a file in the
   `CommandMetadataRegistry` format (`specs/command_registry.yaml`), relative to the document; the
   host loads it as it loads command metadata today (liquers-py: `import_metadata`). Absent, the
   default registry (what code registers) is used. What a CMR file can express is the existing
   format's business, not this design's.
2. **One rule for keyword arguments (recommended: keyword wins).** A keyword given explicitly
   overrides the file, the file overrides the built-in default. Today `job_capacity=` that differs
   from `assets.job_capacity` is an error (`liquers-py-async`); under this rule it would override.
   Recommended: one rule everywhere.

## Phase 2: Architecture

### Solution

A `ConfigDocument` in `liquers-core` splits a document's top-level keys into the environment's
keys (to `EnvironmentConfig`, unchanged), `command_registry` and waiting host sections; a host takes its
sections with `take_section` and calls `finish`, which refuses what is left. Each host assembles a
typed struct: liquers-py's `PyConfig`; a Rust server's own struct around `ServerConfig::from_document`.
Rejected: one portable assembled struct in liquers-lib (the server lives in liquers-axum, which
liquers-lib does not depend on, and liquers-web has no server: what is portable is the document,
not the set of sections); `deny_unknown_fields` on `EnvironmentConfig` (breaks every document that
carries a host section, such as the agent's `memory`).

### Changes

```rust
// liquers-core/src/environment_config.rs — additive, sync (startup)
#[derive(Debug, Clone, Default)]
pub struct ConfigDocument {
    pub environment: EnvironmentConfig,
    pub command_registry: Option<String>,          // the CMR file
    pub base: Option<PathBuf>,                     // the file's folder: `command_registry` only
    sections: BTreeMap<String, serde_json::Value>, // host sections not taken yet
}
impl ConfigDocument {
    pub const ENVIRONMENT_KEYS: &'static [&'static str] =
        &["store", "recipes", "assets", "default_namespaces"];
    pub fn from_yaml(text: &str) -> Result<Self, Error>;
    pub fn from_json(text: &str) -> Result<Self, Error>;
    #[cfg(feature = "toml")] pub fn from_toml(text: &str) -> Result<Self, Error>;
    /// By extension: `.yaml`/`.yml`, `.json`, `.toml` (an error naming the feature without it).
    #[cfg(not(target_arch = "wasm32"))] pub fn load(path: &Path) -> Result<Self, Error>;
    pub fn take_section<T: DeserializeOwned>(&mut self, name: &str) -> Result<Option<T>, Error>;
    /// `Err(configuration_error("unknown section …"))` for a section nobody took.
    pub fn finish(self) -> Result<EnvironmentConfig, Error>;
    /// `command_registry` resolved against `base`.
    pub fn command_registry_path(&self) -> Option<PathBuf>;
}
// liquers-axum/src/server/config.rs
impl ServerConfig { pub fn from_document(doc: &mut ConfigDocument) -> Result<Self, Error>; }

// liquers-py/src/config.rs — new
pub struct PyConfig { pub environment: EnvironmentConfig, pub command_registry: Option<PathBuf>,
                      pub server: ServerConfig, pub python: PythonSettings }
#[derive(Debug, Clone, Default, Deserialize)] #[serde(deny_unknown_fields)]
pub struct PythonSettings { pub conversion: Option<String>, pub pickle: Option<bool>,
                            pub async_loops: Option<NonZeroUsize> }
impl PyConfig {
    pub fn from_document(mut doc: ConfigDocument) -> Result<Self, Error> {
        let server = ServerConfig::from_document(&mut doc)?;
        let python = doc.take_section("python")?.unwrap_or_default();
        let command_registry = doc.command_registry_path();
        let environment = doc.finish()?;
        Ok(PyConfig { environment, command_registry, server, python })
    }
}
```

- Parsing: YAML (and JSON) becomes a `serde_json::Value` map first, so YAML tags and non-string
  keys are refused; the environment keys go to `EnvironmentConfig` through the existing
  `from_json`. A test keeps `ENVIRONMENT_KEYS` in step with `EnvironmentConfig` by round-tripping
  a fully set instance. Syntax errors: `Error::parse_error` (as `from_yaml` today); unknown section
  or a section's bad value: `Error::configuration_error`.
- liquers-py `Environment(config=…)`: `os.PathLike` (via `__fspath__`) → `ConfigDocument::load`;
  `str` → `from_yaml` (a scalar result raises AC-5's hint); `dict` → `json.dumps` → `from_json`.
  Keywords `conversion`, `pickle`, `async_loops` become `None` by default in the signature and the
  stubs, resolved keyword → `python:` → built-in default (question 2); `EnvCell` keeps its
  `conversion` and `pickle` fields, filled from the resolved values, and holds the `PyConfig`.
  The `command_registry` file is read and passed to `import_metadata` when the environment is
  built; a read or parse error raises at construction.
- liquers-py does not enable core's `toml` feature, so a `.toml` path raises naming it.
- Documents: as Documentation Intent; `ENVIRONMENT_CONFIG.md` History row.

### Risks

Files: `environment_config.rs`, `liquers-py/src/{config,environment}.rs`, stubs,
`records/resolver.rs` (a test call site of `EnvCell::new`), `ServerConfig`. Compatibility: through
`lq.Environment`, an unknown top-level key used to be ignored and becomes an error; core's
`EnvironmentConfig::from_yaml` keeps ignoring it, so `an_unknown_key_is_currently_ignored` stays.
Recovery: revert. Certainty: high.
