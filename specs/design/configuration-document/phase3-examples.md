# Phase 3: Examples & Use-cases — configuration-document

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | The minimal file and its defaults | an empty document, every default named | AC-6 |
| 2 | Example | An environment without a server | a file with no `server:`, queries from Python | AC-1, AC-3 |
| 3 | Example | A specialised deployment | several mounts, recipes, assets, CMR, `python`, `server` | AC-1, AC-3 |
| 4 | Example | Keywords over the file | `pickle=True`, `job_capacity=` over the file | AC-4 |
| 5 | Example | A Rust host | the same file read from Rust | AC-7 |
| 6 | Test | `liquers-core` `environment_config::tests::*` | splitting, unknown sections, keys in step, defaults | AC-2, AC-3, AC-6 |
| 7 | Test | `liquers-py/tests/test_CONFIG_document.py` | the Python host end to end | AC-1–AC-6 |
| 8 | Test | `liquers-axum/tests/server_config_document.rs` | `ServerConfig` from a document; a Rust host | AC-6, AC-7 |

## Example 1: the minimal file and its defaults

An empty file, `{}`, or no `config=` at all configures this (AC-6); each line is the default spelt
out, and writing it out changes nothing (`test_CONFIG_written_defaults_equal_empty`):

```yaml
store:
  stores: []              # none: liquers-py mounts one memory store at the root
recipes: default          # liquers-py (records on): [recipes_yaml, manifest]
assets:
  job_capacity: 4         # queued manager's own default
  dependency_audit: explicit
  verify_versions: on_read
  external_change: user_input
  recipe_cache_strategy: all
  query_cache_strategy: all
  cut_predecessors: true
# default_namespaces absent: the command registry's chain stands (["", "root"])
# command_registry absent: the registry is what code registers
python:
  conversion: permissive
  pickle: false
  async_loops: 1
server:                   # used only by env.serve(); nothing listens until then
  host: 127.0.0.1
  port: 8080
  prefix: /liquer
  apis: [query, store, assets, recipes]
  read_only: false        # writes enabled: "batteries included" (security-sensitive)
  web: true               # GET {prefix}/web/… serves store key web/…; / redirects there
```

## Example 2: an environment without a server

`server:` is optional and nothing listens unless `serve` is called, so a script that only evaluates
uses the same file:

```yaml
# analysis.yaml
store:
  stores:
    - {type: filesystem, prefix: data, config: {path: ./data}}   # relative to the working directory
command_registry: commands.yaml
```

```python
from pathlib import Path
import liquers as lq

env = lq.Environment(config=Path("analysis.yaml"))

@env.command
def greet(name: str = "world") -> str:
    return f"Hello, {name}!"

print(env.evaluate("greet-Ann"))                       # Hello, Ann!
print(env.command_metadata("greet").label)              # Greet someone (from commands.yaml)
```

`commands.yaml`, a CMR in the `specs/command_registry.yaml` format (validated with
`liquers-validate --registry-file commands.yaml -- greet-Ann`):

```yaml
commands:
  - name: greet
    label: Greet someone
    doc: Returns a greeting for a name.
    arguments:
      - {name: name, label: Name, default: !Value world, argument_type: string, gui_info: !TextField 20}
default_namespaces: ['', root]
global_enums: {}
```

## Example 3: a specialised deployment

A report site on a LAN: data in a folder, a read-only web app, a remote S3 archive, scratch memory,
tuned caching, a CMR, pickled Python objects and a narrowed server.

```yaml
store:
  stores:
    - {type: filesystem, prefix: data, config: {path: /srv/reports/data}}
    - {type: filesystem, prefix: web, read_only: true, config: {path: /srv/reports/site}}
    - type: s3
      prefix: archive
      read_only: true
      config: {bucket: reports-archive, region: eu-west-1,
               access_key_id: "${AWS_ACCESS_KEY_ID}", secret_access_key: "${AWS_SECRET_ACCESS_KEY}"}
    - {type: memory, prefix: tmp}
recipes: [recipes_yaml, manifest]
assets: {job_capacity: 8, query_cache_strategy: none}
default_namespaces: [pl]
command_registry: commands.yaml
python: {pickle: true, async_loops: 2}
server:
  host: 0.0.0.0           # reachable from the LAN: security-sensitive
  port: 8000
  prefix: /api
  apis: [query, assets]   # no store or recipes API
  read_only: true         # assets mutations refused
```

## Example 4: keywords over the file

```python
env = lq.Environment(config=Path("deploy.yaml"), pickle=False, job_capacity=2)
server = env.serve(port=0)    # an ephemeral port instead of 8000
```

The keyword wins over `python.pickle`, `assets.job_capacity` and `server.port` (Decision 2).

## Example 5: a Rust host

The program of `axum-server-assembly` Phase 2 §Integration Points reads the file of Example 3
(with its own sections instead of `python`) and serves it; `finish` refuses `python` there, so a
Rust host that shares a file with Python takes the `python` section and ignores it explicitly
(`doc.take_section::<serde_json::Value>("python")?`).

## Edge and Error Cases

| Symptom | Expected behaviour | Test |
|---|---|---|
| `sever:` typo | `ConfigurationError` naming `sever` and the known sections | `test_CONFIG_unknown_section_is_refused` |
| unknown key inside `server:` or `python:` | `ConfigurationError` naming the key | `test_CONFIG_unknown_key_in_a_section` |
| `config="liquers.yaml"` (a `str`) | the error says to pass a `pathlib.Path` | `test_CONFIG_str_file_name_is_explained` |
| `.toml` path in liquers-py | `ConfigurationError` naming core's `toml` feature | `test_CONFIG_toml_needs_the_feature` |
| `command_registry` file missing or unparsable | raises at `lq.Environment`, naming the path | `test_CONFIG_missing_command_registry_file` |
| YAML tag (`!Value`) at section level | `ParseError`: sections are plain data | `document_refuses_tags_in_sections` |
| `async_loops: 0` | `ConfigurationError`, as the keyword | `test_CONFIG_async_loops_zero` |

## Test Plan

- Unit: `liquers-core/src/environment_config.rs`
  - `document_splits_environment_and_sections` (AC-3): environment keys reach `EnvironmentConfig`,
    `command_registry` resolves against `base`, others wait.
  - `finish_refuses_an_untaken_section` (AC-2): the message names it and the known keys.
  - `environment_keys_match_the_struct` (AC-6): a fully set `EnvironmentConfig` round-trips through
    `ConfigDocument` unchanged.
  - `empty_document_is_the_default` (AC-6); `document_refuses_tags_in_sections`.
- Integration: `liquers-axum/tests/server_config_document.rs`
  - `server_config_defaults_from_an_empty_document` (AC-6);
  - `rust_host_builds_from_one_document` (AC-7): a core `EnvironmentBuilder` from `finish()`,
    `bind` on port 0, `GET {prefix}/q/…` answers.
- Python: `liquers-py/tests/test_CONFIG_document.py`
  - `test_CONFIG_one_file_configures_the_host` (AC-1, with `port: 0`, so CI does not bind 8080);
  - `test_CONFIG_unknown_section_is_refused` (AC-2); `test_CONFIG_unknown_key_in_a_section` (AC-2);
  - `test_CONFIG_command_registry_file` (AC-3): Example 2's label;
  - `test_CONFIG_keywords_override_the_file` (AC-4): `pickle`, `job_capacity`, `serve(port=0)`;
  - `test_CONFIG_str_file_name_is_explained` (AC-5);
  - `test_CONFIG_written_defaults_equal_empty` (AC-6): Example 1 written out equals `{}`;
  - the remaining edge-case tests above.
- Commands: `cargo test -p liquers-core --lib environment_config`,
  `cargo test -p liquers-axum --test server_config_document`, `bash scripts/test-liquers-py.sh`.

## Learning Log

- A full CMR file needs `default_namespaces` and `global_enums` to parse as a
  `CommandMetadataRegistry` (`liquers-validate --registry-file`); liquers-py's `import_metadata`
  reads only `commands` and ignores the other keys.
- Example 2's CMR was checked with `liquers-validate --registry-file`: `greet-Ann` plans, `report`
  is `ActionNotRegistered`.
