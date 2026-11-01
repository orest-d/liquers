# Phase 3: Examples & Use-cases — liquers-py-store

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A dict as a store | The Phase 1 problem example, staged | AC-1 |
| 2 | Example | Records on a minimal store | Derived data-only access keeps a `metadata` section effective | AC-17 |
| 3 | Test | `test_STORE_python_protocol.py` | Protocol, errors, keys, threads | AC-1-AC-6 |
| 4 | Test | `test_STORE_conformance.py` | `STORE01`-`STORE08`, `STORE10` per store | AC-7, AC-8 |
| 5 | Test | `test_STORE_configured.py` | `STORE09`, `STORE11`-`STORE17`, AC-17 | AC-9-AC-14, AC-17 |
| 6 | Test | `test_STORE_registration.py` | Declaration, late registration, release | AC-15, AC-16 |
| 7 | Test | Rust `store::adapter::tests` | The method table and derived capabilities | AC-2, AC-12, AC-17 |

## Example 1: A dict as a store

Text assets kept in a Python dict, read by queries beside a memory mount: register (stage i),
the first `evaluate` builds the router (ii), the query reads `Notes.get` through `PyStore` (iii).

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

The same class gains `set`, `remove`, `listdir` and `is_dir`, and the mount a `metadata` section.
The adapter derives the five data-only methods, so `StoreRouterBuilder` wraps it in the Rust
pipeline (Decision 5): records live in the dict as sidecar objects, and `get` is not called for
metadata.

```python
class Files(Notes):
    def set(self, key, data, metadata): self.files[str(key)] = data
    def remove(self, key): self.files.pop(str(key), None)
    def is_dir(self, key): return any(k.startswith(f"{key}/") for k in self.files)
    def listdir(self, key):
        return sorted({k[len(str(key)) + 1:].split("/")[0] for k in self.files if k.startswith(f"{key}/")})

files = Files({})
env.register_store_factory("files", lambda prefix: files)
# config: {"type": "files", "prefix": "n", "metadata": {"storage": "per_entry"}}
record = lq.MetadataRecord(); record.media_type = "text/markdown"
env.store.set(lq.parse_key("n/a.md"), b"# A", record)
assert env.store.listdir(lq.parse_key("n")) == ["a.md"]          # sidecars hidden
assert "n/a.md.__metadata__" in files.files                      # the record is a data object
assert env.store.get_metadata(lq.parse_key("n/a.md")).file_size == 3   # on_read: file_facts
```

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| Object lacks `get`; has `listdir` without `is_dir`; `async def is_supported` | `ConfigurationError` at build naming mount and method | `test_STORE_protocol_resolved_once` |
| `get` returns `None` / raises `lq.KeyNotFoundError` / raises `ValueError` | `KeyNotFoundError` / same / `KeyReadError` with the traceback | `test_STORE_none_raise_and_error_are_distinct` |
| `get` returns `str` | `KeyReadError` naming the expected type | same |
| Relative key, direct or routed | `KeyNotAbsoluteError`, Python not called (a counter stays 0) | `test_STORE05_*`, `test_STORE05b_*` |
| `get` blocked on an event (`def`, `async def`) | a `-R/scratch/x.txt` query completes first | `test_STORE_rust_mount_does_not_wait_on_python` |
| Factory calls `env.evaluate` | `ConfigurationError`, no hang (`timeout`) | `test_STORE_type_declaration_and_failures` |
| `data_access = False`, or no `listdir`, under `metadata:` | build fails naming `'d'` and data-only access | `test_STORE15_*` |
| Write under `read_only: true` | `NotSupportedError`, Python's `set` never called | `test_STORE14_*` |
| `lq.Store` held after `shutdown()` | `shut_down_error()` | `test_STORE_late_registration_and_release` |

## Test Plan

Python (`liquers-py/tests/`), each test citing its scenario:

- `test_STORE_python_protocol.py`
  - `test_STORE_problem_example` (AC-1): Example 1, including the absent key.
  - `test_STORE_protocol_resolved_once` (AC-2): the three refused objects; a method assigned to
    the object after the build is not used.
  - `test_STORE_none_raise_and_error_are_distinct` (AC-3): the table rows, error types asserted.
  - `test_STORE_absent_optional_method` (AC-4): `listdir` absent → `KeyNotSupportedError` naming
    it; `contains`/`get_metadata` derived; `is_supported` absent → supported.
  - `test_STORE05_unsupported_key`, `test_STORE05b_the_trap_that_makes_this_test_lie` (AC-5):
    Appendix A bodies on `lq.Store.from_object(Files({}))`, plus a routed call; Python's call
    counter stays 0.
  - `test_STORE_rust_mount_does_not_wait_on_python[def|async]` (AC-6): while `get` waits on an
    event, `-R/scratch/x.txt` evaluates.
- `test_STORE_conformance.py`, parametrised by `each_store` (memory, filesystem, OpenDAL `fs` on
  `tmp_path`, Python `Files` through `env.store`, Python `Files` through `lq.Store.from_object`):
  - `test_STORE01`-`test_STORE07` (AC-7): Appendix A bodies, blocking `lq.Store` calls.
  - `test_STORE08_integration_provided_store_satisfies_the_contract` (AC-7): the parameter list
    covers every constructible local type in `store_types()`.
  - `test_STORE10_metadata_inference_follows_documented_precedence` (AC-8): a `get` returning
    bytes only for `d/input.csv` gives `text/csv`; one returning `media_type="application/json"`
    for `d/blob` keeps it.
- `test_STORE_configured.py`:
  - `test_STORE09_read_only_store_refuses_writes`, `test_STORE11_configured_router_routes_by_prefix`
    (AC-9): documents with a Python mount; no fall-through.
  - `test_STORE12_integration_store_types_are_configurable` (AC-10): memory, filesystem, `fs`,
    `notes` build from one document; the override half `NA` (no shared type name: Python names
    are refused when taken).
  - `test_STORE13_unavailable_store_type_names_the_reason` (AC-10): `redis` (not in
    `services-default`) names `services-redis`.
  - `test_STORE14_read_only_mount_refuses_writes` (AC-11).
  - `test_STORE15_metadata_without_data_access_fails_at_build` (AC-12): both refused objects.
  - `test_STORE16_records_round_trip_as_data_objects[store-storage]` (AC-13): memory,
    filesystem, `fs` and a Python store providing the five data methods, × `per_entry`,
    `grouped`; the Python `get` counter stays 0.
  - `test_STORE17_created_round_trips_from_backend_time` (AC-14): `data_facts` reports
    `2026-10-15T12:00:00.123Z`; `created` and `etag` read back equal.
  - `test_STORE_minimal_store_keeps_configuration_effective` (AC-17): Example 2 with
    `on_write: "defaults/media_type-fill/file_facts"` and `on_read: "media_type-fill/file_facts"`;
    through `env.store` and `env.evaluate("-R/n/a.md")`.
  - `test_STORE18_language_store_config_handler`: `NA` — `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`.
- `test_STORE_registration.py`:
  - `test_STORE_type_declaration_and_failures` (AC-16): `store_types()` lists `greeting` as an
    optional string; missing required, unknown key, factory raising (traceback kept), factory
    returning a coroutine or evaluating its environment, taken names (`memory`, `fs`,
    `opendal_fs`, a second `notes`): each `ConfigurationError`, within a timeout.
  - `test_STORE_declare_from_signature` (AC-16): `_declare` rules — `bool` before `int`, `**kwargs`
    partial, positional-only refused, `arguments=` for an uninspectable callable.
  - `test_STORE_late_registration_and_release` (AC-15): a document naming `notes` fails, then
    registering `notes` lets it build with an earlier `env.store.set` to `scratch` still there;
    `live_handle_count()` returns to its baseline after `shutdown()`; a held `lq.Store` raises.
- Rust (`liquers-py/src/store/adapter.rs`, `Python::initialize`):
  `method_table_records_presence_and_kind` (AC-2), `data_access_is_derived_only_with_get_and_listdir`
  (AC-12, AC-17), `get_stored_metadata_needs_a_provided_get_metadata` (AC-17).
- Commands: `bash scripts/test-liquers-py.sh -k STORE`; CI runs 3.10, 3.14 and 3.14t.

Queries validated (`--command greet`): `-R/notes/hello.txt`, `-R/notes/absent.txt`,
`-R/scratch/x.txt`, `-R/n/a.md`, `-R/d/in.txt/-/greet`, each a `GetAsset` of the intended key.

## Learning Log

- Appendix A's async bodies become blocking `lq.Store` calls; `run_async` covers `_async` paths.
- `STORE13` needs a type real but gated off in *this* build; `redis` qualifies because
  `services-default` leaves it out, which the test should assert from `store_types()` first so it
  fails loudly if the default changes.
- `STORE12`'s override half is `NA` by construction: registration refuses every taken name.
- A writable Python store under `metadata:` receives sidecar names (`n/a.md.__metadata__`); the guide says so.
