# Phase 2: Solution & Architecture — configuration-document

## Overview

A `ConfigDocument` in `liquers-core` splits a document's top-level keys into the environment's
keys (to `EnvironmentConfig`, unchanged), `command_registry` and waiting host sections; a host takes
its sections with `take_section` and calls `finish`, which refuses what is left. Each host
assembles a typed struct: liquers-py's `PyConfig`; a Rust server its own, around
`ServerConfig::from_document`. Rejected: one portable assembled struct in liquers-lib (the server
lives in liquers-axum, which liquers-lib does not depend on, and liquers-web has no server: what is
portable is the document, not the set of sections); `deny_unknown_fields` on `EnvironmentConfig`
(breaks every document that carries a host section, such as the agent's `memory`).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `HOSTS-READ-CONFIGURATION-DOCUMENTS-DIFFERENTLY` | draft | P3 | the source issue | — | no | this design, first step |
| `STORE-PATHS-ARE-NOT-RELATIVE-TO-THE-CONFIGURATION-FILE` | draft | P3 | store paths resolve against the working directory | no | no | documented |

## Interfaces

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

Commands, ExtValue variants: none.

## Integration Points

- Parsing: YAML (and JSON) becomes a `serde_json::Value` map first, so YAML tags and non-string
  keys are refused; an empty document (`null`) is the empty map; the environment keys go to `EnvironmentConfig` through the existing
  `from_json`. A test keeps `ENVIRONMENT_KEYS` in step with `EnvironmentConfig` by round-tripping
  a fully set instance and comparing the serialized YAML (`EnvironmentConfig` has no `PartialEq`).
- liquers-py `Environment(config=…)`: `os.PathLike` (via `__fspath__`) → `ConfigDocument::load`;
  `str` → `from_yaml` (a scalar result raises AC-5's hint); `dict` → `json.dumps` → `from_json`.
  Keywords `conversion`, `pickle`, `async_loops`, `job_capacity` become `None` by default in the
  signature and the stubs, resolved keyword → file → built-in default (Decision 2); `EnvCell` keeps
  its `conversion` and `pickle` fields, filled from the resolved values, and holds the `PyConfig`.
  The `command_registry` file is read and passed to `import_metadata` when the environment is
  built; a read or parse error raises at construction.
- liquers-py does not enable core's `toml` feature, so a `.toml` path raises naming it.

## Error Handling

| Failure | Constructor |
|---|---|
| syntax error; a YAML tag or a non-string key at section level | `Error::parse_error` (as `from_yaml` today) |
| unknown section; a section's bad value or unknown key | `Error::configuration_error` |
| `.toml` without the feature, unknown extension | `Error::configuration_error` |
| unreadable file (document or CMR) | `Error::from_error(ErrorType::General, io_error)` naming the path |

## Relevant Commands

None; no namespace is involved.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/ENVIRONMENT_CONFIG.md` | reference | §The configuration document; History row |
| `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` | guide | "Configure from one file" |
| `specs/reference/PYTHON_INTEGRATION.md`, `specs/guides/PYTHON_GUIDE.md` | reference, guide | `config=` forms, `python:` section, override rule |
| `liquers-core/src/environment_config.rs` | API doc | components and example gain `ConfigDocument` |
| `liquers-py/src/config.rs` | API doc | all five parts |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `environment_config.rs`, `liquers-py/src/{config,environment}.rs`, stubs, `records/resolver.rs` (a test call site of `EnvCell::new`), `ServerConfig` |
| Existing tests likely to change | the conflict case of `liquers-py/tests/test_ASYNC_loops.py` (line 52, Decision 2); core's `an_unknown_key_is_currently_ignored` stays |
| Compatibility | through `lq.Environment`, an unknown top-level key used to be ignored and becomes an error |
| Recovery | revert |
| Certainty | high |
