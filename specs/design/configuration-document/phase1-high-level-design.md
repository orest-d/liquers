# Phase 1: High-Level Design — configuration-document

## Purpose

Today each host reads its own document: `EnvironmentConfig` (store, recipes, assets,
`default_namespaces`) silently ignores any other key (`an_unknown_key_is_currently_ignored`,
`liquers-core/src/environment_config.rs`), `liquers-agent`'s `AgentConfig` cuts `memory` out by
hand, and liquers-web reads only a `StoreRouterConfig`. A server and language settings have no
place at all. One document, readable everywhere, with per-host sections that each host takes and an
error for a section nobody took, lets one file set up a whole application (Orest, 2026-11-07:
"a single configuration file (yaml or json) that would configure all aspects").

## Design Readiness

- **Readiness:** `ready` for Phase 3: Phases 1 and 2 approved 2026-11-09 ("proceed").
- **Automatic fixing:** `not-eligible`: new public API in core and liquers-py.
- **Leading issue:** `HOSTS-READ-CONFIGURATION-DOCUMENTS-DIFFERENTLY` (P3 M).
- **Decisions:**
  1. Orest, 2026-11-07: "I only ask for a field in config saying which file to load as a CMR."
     `command_registry` names a file in the `CommandMetadataRegistry` format
     (`specs/command_registry.yaml`), relative to the document; the host loads it as it loads
     command metadata today (liquers-py: `import_metadata`). Absent, the default registry (what
     code registers) is used. What a CMR file can express is the existing format's business.
  2. Accepted with the Phase 2 `proceed`, 2026-11-09 (the recommendation of former question 2):
     an explicit keyword argument overrides the file, and the file overrides the built-in
     default. `job_capacity=` that differs from `assets.job_capacity` overrides it instead of
     raising (a change to `liquers-py-async`'s rule).
  3. Orest, 2026-11-07: Phase 3 shows use without a server, the server configured in code, a
     minimal file with every default described, and specialised configurations.

## Problem Example

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
**After** the Python host takes `server` and `python`, loads `commands.yaml` as its command
metadata registry, and refuses the document: `ConfigurationError: unknown section 'sever' (this
host knows: store, recipes, assets, default_namespaces, command_registry, server, python)`.

## Scope and Acceptance Criteria

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
`STORE-PATHS-ARE-NOT-RELATIVE-TO-THE-CONFIGURATION-FILE`); selecting a file by environment
variable.

## Core Interactions

Configuration: the document and its sections. Commands: the CMR file. Bindings: `lq.Environment`.
Web/API: the `server` section.

## Crate Placement

`liquers-core` (`ConfigDocument`, portable, `load` native-only), `liquers-axum` (`ServerConfig`
reads its section), `liquers-py` (`PyConfig`).

## Documentation Intent

- Reference: `ENVIRONMENT_CONFIG.md` gains §The configuration document (sections,
  `command_registry`, hosts and their sections, unknown sections, defaults).
- Guide: `ENVIRONMENT_CONSTRUCTION_GUIDE.md` "Configure from one file"; `PYTHON_GUIDE.md`
  "Configure with a file".
- API docs: `liquers-core/src/environment_config.rs` (components, example) and the new
  `liquers-py/src/config.rs` (all five parts).

## Design Dependencies

- `axum-server-assembly`, `liquers-py-web-server`: **required-by**.
- `liquers-py-async` (complete): **changes** its `job_capacity` conflict rule (Decision 2).
