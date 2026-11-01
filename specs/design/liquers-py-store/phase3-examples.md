# Phase 3: Examples & Use-cases — liquers-py-store

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A dict as a store | The Phase 1 problem example, staged | AC-1 |
| 2 | Example | Records on a minimal store | Derived data-only access keeps a `metadata` section effective | AC-17 |
| 3 | Test | `test_STORE_python_protocol.py` | Protocol, errors, keys, threads | AC-1-AC-6 |
| 4 | Test | `test_STORE_conformance.py` | `STORE01`-`STORE08`, `STORE10` per store | AC-5, AC-7, AC-8 |
| 5 | Test | `test_STORE_configured.py` | `STORE09`, `STORE11`-`STORE17` | AC-9-AC-14, AC-17 |
| 6 | Test | `test_STORE_registration.py` | Declaration, late registration, release | AC-15, AC-16 |
| 7 | Test | Rust `store::adapter::tests` | The method table, derived capabilities | AC-2, AC-12, AC-17 |

## Example 1: A dict as a store

Text assets kept in a Python dict, read beside a memory mount: register (stage i), the first
`evaluate` builds the router (ii), the query reads `Notes.get` through `PyStore` (iii).

```python
class Notes:
    def __init__(self, files): self.files = files
    def get(self, key): return self.files.get(str(key))      # None: absent

def notes(prefix, greeting: str = "Hi"):
    return Notes({f"{prefix}/hello.txt": greeting.encode()})

env = lq.Environment(config={"store": {"stores": [
    {"type": "notes", "prefix": "notes", "config": {"greeting": "Hello"}},
    {"type": "memory", "prefix": "scratch"}]}})
env.register_store_factory("notes", notes, doc="Notes kept in a dict")
assert env.evaluate("-R/notes/hello.txt") == "Hello"
with pytest.raises(lq.AssetNotFoundError):
    env.evaluate("-R/notes/absent.txt")
assert env.store_types()[-1]["store_type"] == "notes"            # chained after the built-ins
```

`get` returns no metadata, so it comes from the key (`text/plain`). `Notes` has no `set`, so a
write to `notes/…` raises `KeyNotSupportedError` naming `set`.

## Example 2: Records on a minimal store

`Files` adds `set`, `remove`, `listdir` and `is_dir`; its mount has a `metadata` section. The
adapter derives the data-only methods from them, so the Rust pipeline wraps the mount (Decision
5) and the record is a data object in the dict, read back through `get`.

```python
class Files(Notes):
    def set(self, key, data, metadata): self.files[str(key)] = data
    def remove(self, key): self.files.pop(str(key), None)
    def is_dir(self, key): return any(k.startswith(f"{key}/") for k in self.files)
    def listdir(self, key):
        return sorted({k[len(str(key)) + 1:].split("/")[0] for k in self.files if k.startswith(f"{key}/")})

files = Files({})
env = lq.Environment(config={"store": {"stores": [
    {"type": "files", "prefix": "n", "metadata": {"storage": "per_entry"}}]}})
env.register_store_factory("files", lambda prefix: files)
record = lq.MetadataRecord(); record.media_type = "text/markdown"
env.store.set(lq.parse_key("n/a.md"), b"# A", record)
assert env.store.listdir(lq.parse_key("n")) == ["a.md"]          # sidecars hidden
assert "n/a.md.__metadata__" in files.files                      # the record is a data object
assert env.store.get_metadata(lq.parse_key("n/a.md")).file_size() == 3   # on_read: file_facts
```

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| No `get`; `listdir` without `is_dir`; `async def is_supported` | `ConfigurationError` at build naming mount and method | `test_STORE_protocol_resolved_once` |
| `get` returns `None` / raises `lq.KeyNotFoundError` / `ValueError` / returns `str`; `set` raises | `KeyNotFoundError` / same / `KeyReadError` with traceback / `KeyReadError` naming the type; `KeyWriteError` | `test_STORE_none_raise_and_error_are_distinct` |
| Relative key, direct or routed | `KeyNotAbsoluteError`; Python's counter stays 0 | `test_STORE_relative_key_never_reaches_python` |
| Factory calls `env.evaluate` | `ConfigurationError`, within a thread-join timeout | `test_STORE_type_declaration_and_failures` |
| `data_access = False`, or no `listdir`, under a non-empty `metadata:` | build fails naming `'d'` and data-only access | `test_STORE15_*` |
| Write under `read_only: true` | `NotSupportedError`; Python's `set` not called | `test_STORE14_*` |
| `lq.Store` held after `shutdown()` | `shut_down_error()` | `test_STORE_late_registration_and_release` |

## Test Plan

Fixture `DictStore`: the whole protocol on a dict (metadata kept, `set_metadata`, `removedir`,
`makedir`), no data-only methods. `DataStore`: Appendix A's, with the five data methods and a
`get` counter.

- `test_STORE_python_protocol.py`
  - `test_STORE_problem_example` (AC-1): Example 1.
  - `test_STORE_protocol_resolved_once` (AC-2): the refused objects; a method assigned after the
    build is not used.
  - `test_STORE_none_raise_and_error_are_distinct` (AC-3): the table row, error types asserted.
  - `test_STORE_absent_optional_method` (AC-4): no `listdir` → `KeyNotSupportedError` naming it
    and `is_dir` false; `contains` and `get_metadata` derived; no `is_supported` → supported.
  - `test_STORE_relative_key_never_reaches_python` (AC-5): direct and routed, counter 0.
  - `test_STORE_rust_mount_does_not_wait_on_python[def|async]` (AC-6): `scratch/x.txt` set
    first; while `get` waits on an event, `-R/scratch/x.txt` evaluates to its content.
- `test_STORE_conformance.py`, `each_store` = memory, filesystem, OpenDAL `fs` (`root:
  tmp_path`), `DictStore` mounted (`env.store`), `DictStore` through `lq.Store.from_object`:
  - `test_STORE01`-`test_STORE05b`, `test_STORE07` (AC-5, AC-7): Appendix A bodies as blocking
    calls; `STORE07` skips the `from_object` parameter (not mounted in an environment).
  - `test_STORE06_concurrent_update_policy` (AC-7): ten `set_async` under `run_async`.
  - `test_STORE08_integration_provided_store_satisfies_the_contract` (AC-7): the parameter types
    are a subset of `store_types()`.
  - `test_STORE10_metadata_inference_follows_documented_precedence` (AC-8): for `d/input.csv`,
    no metadata gives `text/csv`, written or returned `application/json` wins.
- `test_STORE_configured.py`:
  - `test_STORE09_read_only_store_refuses_writes`, `test_STORE11_configured_router_routes_by_prefix`
    (AC-9): Python mounts, `KeyNotSupported` from a store without `set`, no fall-through.
  - `test_STORE12_integration_store_types_are_configurable` (AC-10): memory, filesystem, `fs`,
    `notes` from one document; override half `NA` (taken names are refused).
  - `test_STORE13_unavailable_store_type_names_the_reason` (AC-10): `redis` names `services-redis`.
  - `test_STORE14_read_only_mount_refuses_writes` (AC-11): `is_read_only()` true.
  - `test_STORE15_metadata_without_data_access_fails_at_build` (AC-12).
  - `test_STORE16_records_round_trip_as_data_objects[store-storage]` (AC-13): memory, filesystem,
    `fs`, `DataStore` × `per_entry`, `grouped`; `media_type` filled by `on_read`; `gets == 0`.
  - `test_STORE17_created_round_trips_from_backend_time[store]` (AC-14): the same stores;
    `DataStore` reports `2026-10-15T12:00:00.123Z` and `created`, `etag` read back equal.
  - `test_STORE_minimal_store_keeps_configuration_effective` (AC-17): Example 2 with explicit
    `on_write` and `on_read` chains, through `env.store` and `-R/n/a.md`.
  - `test_STORE18_language_store_config_handler`: `NA`, `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`.
- `test_STORE_registration.py`:
  - `test_STORE_type_declaration_and_failures` (AC-16): `store_types()` lists `greeting` optional;
    missing, unknown, raising (traceback), coroutine-returning, environment-using factories, and
    taken names (`memory`, `fs`, `opendal_fs`, a second `notes`), each `ConfigurationError`.
  - `test_STORE_declare_from_signature` (AC-16): `_declare`'s rules and `arguments=` with
    `lq.StoreArgumentInfo`s.
  - `test_STORE_late_registration_and_release` (AC-15): (a) a document naming `notes` fails, the
    factory is registered, the next `evaluate` builds; (b) after a use and a `scratch` write, a
    late command and a late factory rebuild with the write kept; `live_handle_count()` returns to
    baseline after `shutdown()`, and a held `lq.Store` raises.
- Rust: `pycall::tests::live_handle_counts_create_and_drop` (AC-15); `store::adapter::tests`
  `method_table_records_presence_and_kind` (AC-2),
  `data_access_is_derived_only_with_get_and_listdir` (AC-12, AC-17),
  `get_stored_metadata_needs_a_provided_get_metadata` (AC-17).
- Commands: `bash scripts/test-liquers-py.sh -k STORE` (pytest), then
  `cargo test -p liquers-py --no-default-features --features debug-handles --lib`; CI 3.10, 3.14, 3.14t.

Queries validated (`--command greet`): `-R/notes/hello.txt`, `-R/notes/absent.txt`,
`-R/scratch/x.txt`, `-R/n/a.md`, `-R/d/in.txt/-/greet`, each a `GetAsset` of the intended key.

## Learning Log

- Appendix A's async bodies become blocking `lq.Store` calls; `run_async` covers `_async` paths.
- `lq.Metadata.file_size` is a method, `MetadataRecord.file_size` a property.
- A failed build keeps no store; a successful one is kept (Decision 2), so a late factory only
  matters after a failed build: the configuration cannot name it otherwise.
- `metadata: {}` is no section; `STORE15` needs a field set.
- A writable Python store under `metadata:` receives sidecar names (`n/a.md.__metadata__`).
