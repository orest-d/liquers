# Phase 1: High-Level Design — liquers-py-store

## Purpose

Let a Python program back Liquers with storage that only Python reaches (an internal API, an ORM, a
dictionary in a notebook), and compose it with the Rust stores from one configuration document:
the `STORE` feature, directions 1, 2 and 3, of `LANGUAGE-INTEGRATION_GUIDE.md` (`VISION.md`, Python
data science). It is milestone M4 of the liquers-py plan, together with
[`liquers-py-recipes`](../liquers-py-recipes/DESIGN.md).

## Design Readiness

- **Readiness:** `needs-decision`
- **Automatic fixing:** `not-eligible`: an `L` feature with a new public Python API.
- **Leading issue:** Open design question 1 (the minimum method set).
- **Explanation:** questions 1-3 have viable alternatives and wait for Orest; questions 4-13 have
  one defensible answer each and are shown for review. Provisional until the Phase 4 consistency pass.
- **Open questions:**
  1. **Open design - minimum method set** (`WEB-JS-STORE-WITH-ONLY-GET-CANNOT-BE-QUERIED`).
     Recommended: only `get` is mandatory, because a `-R/` query also calls `contains`,
     `get_metadata` and `is_dir`, and these are *derived* when absent: `get_metadata` from `get`;
     `contains` from it, an absent key answering `False`, never an error; `is_dir` `False` when
     `listdir` is absent too (a flat store). `listdir` without `is_dir` is refused at registration.
     Every other absent method raises `KeyNotSupported` naming it. A derived `contains` costs a
     second `get` per read, so the guide recommends writing it. liquers-web takes the same rule in
     its own PR. Alternative: all four mandatory for a queried store.
  2. **Open design - registering after the store is built.** Recommended: `register_store_object`
     raises `ConfigurationError` once the environment's store exists (after the first `evaluate`,
     `store_set` or `env.store`), since a rebuilt router would start with empty in-memory mounts;
     `Environment(stores={...})` registers at construction. Alternative: rebuild the store and
     document the loss. A late *command* keeps rebuilding and dropping the asset cache
     (`LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION`), documented as an open problem.
  3. **Open design - Python access to stores.** Recommended: `env.store` is an `lq.Store` over the
     built store, with blocking methods and `_async` twins (the `evaluate` / `evaluate_async`
     convention), including `is_read_only()` and `sync()` (its report's warnings readable);
     `lq.Store.from_object(obj)` and `lq.Store.from_config(entry)` give one store held directly
     (`STORE05` must not go through a router). After `shutdown()` an `lq.Store` refuses every call.
  4. **Proposed resolution - naming.** Entry type `python`, object in `config.object`, as
     liquers-web's `js`. `register_store_object` is an `Environment` method, where a future
     store-*type* registration would sit too (`language-store-type-registration` question 1);
     Python store types stay out of scope (`LANGUAGE-STORE-TYPE-NOT-DEFINABLE`).
  5. **Proposed resolution - routing.** The prefix and relative-key checks run in Rust; a Python
     `is_supported` is asked only for keys under the mount's prefix, so a key routed to a Rust mount
     never takes the GIL. Absent means supported; a raised exception means not supported. It must
     be a `def`: an `async def is_supported` is refused at registration.
  6. **Proposed resolution - sync and async methods.** Each other method may be `def` or
     `async def`, detected once at registration: `def` runs on the blocking pool with the GIL taken
     there, `async def` on liquers-py's command loops (`liquers-py-async` Decision 2).
  7. **Proposed resolution - what crosses.** Keys as `lq.Key`, whole, not stripped of the prefix
     (as `JsStore` and the Rust backends see them); data as `bytes`, copied both ways
     (`SHAREABLE-METADATA-AND-BYTES-ARE-DEEP-COPIED`); metadata as `lq.Metadata`, accepted back
     also as a `dict`. `get` returns `bytes`, `(bytes, metadata)` or `None`; returned metadata wins
     over what the key implies, and missing fields are filled from the key as for any store
     without metadata (so `hello.txt` reads as text, `STORE10`).
  8. **Proposed resolution - data-only access.** `get_data`, `set_data`, `remove_data`,
     `listdir_data` and `data_facts` all or none (none: no `metadata` section); `rename_data` and
     `get_stored_metadata` optional; an `atomic_write = True` attribute declares atomic writes.
     `data_facts` returns `None` or `{size, modified, etag, precision}`, `modified` an aware
     `datetime` or RFC 3339 text, brought to canonical form by `BackendTime::parse`.
  9. **Proposed resolution - absent versus failure.** `None` means absent (`KeyNotFound`); a raised
     `lq.LiquersError` keeps its type; any other exception is `KeyReadError` on a read and
     `KeyWriteError` on a write, with the Python traceback. Against today's error types
     (`ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED` may refine it).
  10. **Proposed resolution - consistency and re-entry.** The adapter adds no locking: concurrent
      calls reach the object concurrently, and atomicity is the object's (`STORE06` asserts no torn
      value, last write wins). A method may evaluate queries through its `Environment` under the
      `RUNTIME` rules; reading its own store from inside its own method can deadlock the object and
      is documented, not prevented. A store "without writes" (`is_read_only()`) lacks every write method.
  11. **Proposed resolution - relative keys.** Refused with `KeyNotAbsolute` before Python is called
      (`Key::as_absolute`); `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` is deferred past M4.
  12. **Proposed resolution - STORE18 is `NA`**, linking `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`;
      `STORE12`'s override half is `NA`: `python` is no other factory's type name.
  13. **Proposed resolution - shutdown.** `shutdown()` drops every registered object and the store;
      no `close()` is called (`ENVIRONMENT-ORDERLY-SHUTDOWN`).

## Problem Example

```python
import liquers as lq

class Notes:                                    # a store with one method
    def __init__(self):
        self.files = {"notes/hello.txt": b"Hello"}
    def get(self, key):                         # bytes, (bytes, metadata), or None if absent
        return self.files.get(str(key))

env = lq.Environment(config={"store": {"stores": [
    {"type": "python", "prefix": "notes", "config": {"object": "notes"}},
    {"type": "memory", "prefix": "scratch"}]}})
env.register_store_object("notes", Notes())
assert env.evaluate("-R/notes/hello.txt") == "Hello"
```

**Today** there is no `register_store_object`, no store protocol and no `python` type: the build
fails with an unknown store type. **After** the query reads through `Notes.get`.

## Scope and Acceptance Criteria

- **AC-1** A Python store answers a query
  WHEN the problem example runs
  THEN `-R/notes/hello.txt` evaluates to `"Hello"`, and `-R/notes/absent.txt` raises `AssetNotFoundError`
- **AC-2** The protocol is resolved once
  WHEN an object without `get`, with `listdir` but no `is_dir`, or with an `async def is_supported` is registered
  THEN registration raises `ConfigurationError` naming the method; a method added later changes nothing
- **AC-3** Absent and failure stay distinct
  WHEN a Python method returns `None`, raises `lq.LiquersError`, or raises another exception
  THEN the caller sees `KeyNotFound`, that error's type, or a read/write error with the traceback
- **AC-4** Absent optional methods are loud or derived
  WHEN a store lacks an optional method
  THEN the call raises `KeyNotSupported` naming it, except the derived methods of question 1 and
  `is_supported` (absent means supported)
- **AC-5** Relative keys are refused
  WHEN a key with a `.` or `..` segment reaches a Python store, directly or routed
  THEN it fails with `KeyNotAbsolute` and Python is not called (`STORE05`, `STORE05b`)
- **AC-6** Rust mounts do not wait on Python
  WHEN a Python store's `get` (a `def`, then an `async def`) is blocked on an event
  THEN a `-R/scratch/…` query on the memory mount completes before the event is set
- **AC-7** Every store passes the suite
  WHEN `STORE01`-`STORE07` run against the memory, file, one OpenDAL and the Python store
  THEN they pass, each through an `lq.Store` held directly (`STORE08`)
- **AC-8** Metadata precedence
  WHEN a store returns no metadata, or metadata disagreeing with the key's extension
  THEN the first is filled from the key and the second wins, for each store (`STORE10`)
- **AC-9** Routing from a document
  WHEN a configuration has overlapping prefixes, a `read_only` mount over a writable one, and an unmatched key
  THEN the first match wins, writes do not fall through, and the key is absent (`STORE09`, `STORE11`)
- **AC-10** Store types from a document
  WHEN a document names the memory, file and an OpenDAL type, and one gated off in this build
  THEN the first three build; the last is refused naming the missing feature (`STORE12`, `STORE13`)
- **AC-11** `read_only` on a Python mount
  WHEN a Python mount carries `read_only: true`
  THEN every write is refused before Python is called, and `is_read_only()` is true (`STORE14`)
- **AC-12** A `metadata` section needs data-only access
  WHEN a Python mount without the data-only methods carries a `metadata` section
  THEN the build fails naming the mount and the missing methods (`STORE15`)
- **AC-13** Records round-trip
  WHEN every store that takes a `metadata` section (the Python one with data-only methods included)
  is configured with `per_entry` and then `grouped` storage
  THEN records round-trip, the default `on_read` chain applies, and a Python `get` is never called
  for metadata (`STORE16`)
- **AC-14** Backend time compares equal
  WHEN a record's `created` comes from `data_facts` and the entry is re-read
  THEN it compares equal with a fresh reading, for each such store (`STORE17`)
- **AC-15** Registration and release
  WHEN a document names an unregistered object, a command is registered after the first query,
  or `shutdown()` is called
  THEN the build fails naming the object; the rebuild keeps the store; shutdown releases every
  registered object (a `debug-handles` count) and a held `lq.Store` refuses further calls

Non-goals: Python store types; Python `store_config` handlers (STORE18); recipe providers; new Rust
backends; calling `close()`; reconfiguring a built store.

## Core Interactions

- Store: an `AsyncStore` adapter, a `StoreFactory` for `python` chained before
  `default_store_factory()`; `read_only` and `metadata` applied by `StoreRouterBuilder` unchanged.
- Bindings: `register_store_object`, `Environment(stores=…)`, `env.store`, `lq.Store`.
- Errors: Python exceptions to store errors (ERROR bridge).

## Crate Placement

`liquers-py` only: a new `src/store/` (adapter, factory, `lq.Store`). No core change is planned;
the adapter follows `AsyncStore` as it is.

## Documentation Intent

- Reference: a §Stores in `specs/reference/PYTHON_INTEGRATION.md` (protocol table, derived methods,
  error map, threading, configuration); `python` in `specs/reference/STORE_CONFIG_FSD.md`'s type tables.
- Guide: a "Stores" chapter in `specs/guides/PYTHON_GUIDE.md` (writing a store, mounting it, the
  Rust stores, `env.store`).
- API: docstrings on every new class and method (`test_DOCS_docstrings.py`).
- Also `liquers-py/README.md` (STORE row), `LANGUAGE-INTEGRATION_GUIDE.md` §6 Python, `specs/README.md`.
- Open problems documented: the copies, no store types, STORE18, late registration and the
  rebuild's cache loss, the derived `contains` cost, self-reading deadlock.

## Design Dependencies

- `PY-NO-LANGUAGE-DEFINED-STORE` — **implemented** by this design.
- `liquers-py-recipes` — **sibling** (M4); same registration and method-dispatch rules.
- `language-store-type-registration` — **required by**: its Python half needs this adapter.
- `language-store-config-function` — **overlaps**; would make STORE18 apply.
- `WEB-JS-STORE-WITH-ONLY-GET-CANNOT-BE-QUERIED` — **overlaps** (question 1); fixed in its own PR.
- `WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON` — avoided: registration acts on its own
  `Environment`; liquers-py has no singleton store.
- `language-late-registration-without-rebuild` — **overlaps** (question 2): would remove the
  command rebuild that AC-15 keeps the store across.

## References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §Service adapters, §STORE, Appendix A STORE
- `liquers-web/src/store/js_store.rs` (the JavaScript counterpart); `specs/guides/STORE_IMPLEMENTATION_GUIDE.md`
- `/mnt/project-files/plans/liquers-py-implementation-plan.md` Pre-M4 B, M4
