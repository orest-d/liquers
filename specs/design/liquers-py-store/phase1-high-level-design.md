# Phase 1: High-Level Design — liquers-py-store

## Purpose

Let a Python program back Liquers with storage that only Python reaches (an internal API, an ORM, a
dictionary in a notebook), declared as a store *type* with arguments and composed with the Rust
stores from one configuration document: the `STORE` feature, directions 1, 2 and 3, of
`LANGUAGE-INTEGRATION_GUIDE.md` (`VISION.md`, Python data science). Milestone M4 of the liquers-py
plan, with [`liquers-py-recipes`](../liquers-py-recipes/DESIGN.md).

## Design Readiness

- **Readiness:** `ready`
- **Automatic fixing:** `not-eligible`: an `L` feature with a new public Python API.
- **Leading issue:** None.
- **Explanation:** Phase 1 approved by Orest on 2026-11-01 (`proceed all`), pre-approving Phases
  2-4, the implementation and Phase 5. Decisions 1-5 are his; questions 4-12 are explicit
  assumptions taken as recommended. The Phase 4 consistency pass (2026-11-01) left them standing.
- **Decision log (pre-approval):** (a) Phase 2 review: derived data-only access needs `get` and
  `listdir` (the pipeline lists objects and recovers staged writes), so a store without `listdir`
  takes no `metadata` section and is refused at build (AC-12) rather than failing at run time; and
  a derived `get_stored_metadata` reads only a provided `get_metadata`. A narrowing of Decision 5's
  "any Python store", in its spirit; **decided** by Orest on 2026-11-01: narrowing accepted.
  (b) Phase 4 review: the router calls `is_supported` synchronously from its async methods, so a
  Python `is_supported` takes the GIL on a runtime worker. It is asked only for absolute keys under
  the mount's prefix, so a Rust mount's keys never reach it unless prefixes overlap; it goes
  through the exit gate and answers `false` while the interpreter exits. Documented; within
  Decision 5's goal, no decision needed.
- **Open questions:**
  1. **Decided (Decision 3) - minimum method set** (`WEB-JS-STORE-WITH-ONLY-GET-CANNOT-BE-QUERIED`).
     Recommended: only `get` is mandatory, because a `-R/` query also calls `contains`,
     `get_metadata` and `is_dir`, and these are *derived* when absent: `get_metadata` from `get`;
     `contains` from it, an absent key answering `False`, never an error; `is_dir` `False` when
     `listdir` is absent too (a flat store). `listdir` without `is_dir` is refused. Every other
     absent method raises `KeyNotSupported` naming it. A derived `contains` costs a second `get`
     per read, so the guide recommends writing it. liquers-web takes the same rule in its own PR.
     Alternative: all four mandatory for a queried store.
  2. **Decided (Decision 4) - Python access to stores.** Recommended: `env.store` is an `lq.Store` over the
     environment's current store (resolved on each call, so a rebuild is followed), with blocking
     methods and `_async` twins, including `is_read_only()` and `sync()` (its warnings readable);
     `lq.Store.from_object(obj)` adapts one object directly (`STORE05` must not go through a
     router). After `shutdown()` an `lq.Store` refuses every call.
  3. **Decided (Decision 5) - `store_config` and Python stores.** Original question: Python
     `store_config` handlers in M4? Recommended: not in M4: `STORE18` stays `NA` linking `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`,
     and `language-store-config-function` adds them to the same stage later. Alternative: take that
     design into M4 (`L`, a core `StepBuilder` change).
  4. **Proposed resolution - the factory declaration.** `env.register_store_factory(type, factory,
     *, label="", doc="", arguments=None)`: `arguments` is derived from the factory's keyword
     parameters (rules in Phase 2) or given as `lq.StoreArgumentInfo`s. liquers-py checks required
     arguments, fills defaults and refuses unknown `config:` keys (unless the factory takes
     `**kwargs`), since core leaves that to each factory; `prefix` is passed when declared.
  5. **Proposed resolution - unique names.** A name any built-in factory resolves (`memory`,
     `filesystem`, any OpenDAL name) or another Python type holds is refused at registration, and
     the Python factory is chained *after* the built-ins, so a built-in always wins; `STORE12`'s
     override half is `NA` for that reason.
  6. **Proposed resolution - the factory call.** A callable (a function or a class), not
     `async def`: `StoreFactory::create` is synchronous. It runs at build with the GIL; an exception
     is a `ConfigurationError` naming the mount, and its result is adapted at once. It **may not
     use its environment**: the build holds the environment's lock, so such a call raises
     `ConfigurationError` rather than deadlocking.
  7. **Proposed resolution - routing.** The prefix and relative-key checks run in Rust; a Python
     `is_supported` (a `def`) is asked only for keys under the mount's prefix, so a key routed to a
     Rust mount never takes the GIL. Absent means supported; an exception means not supported.
  8. **Proposed resolution - sync and async methods.** Every other method may be `def` (blocking
     pool, GIL taken there) or `async def` (liquers-py's command loops, `liquers-py-async` Decision 2).
  9. **Proposed resolution - what crosses.** Keys as `lq.Key`, whole (as the Rust backends see
     them); data as `bytes`, copied both ways; metadata as `lq.Metadata` or a `dict`. `get` returns
     `bytes`, `(bytes, metadata)` or `None`; returned metadata wins over the key, missing fields are
     filled from it (`STORE10`). `None` is `KeyNotFound`; a raised `lq.LiquersError` keeps its
     type; any other exception is `KeyReadError` or `KeyWriteError` with the traceback.
  10. **Proposed resolution - data-only access.** `get_data`, `set_data`, `remove_data`,
      `listdir_data`, `data_facts` all or none; `rename_data`, `get_stored_metadata` optional;
      `atomic_write = True` declares atomic writes. When they are absent the adapter derives them
      (Decision 5) when `get` and `listdir` exist: `get_data` from `get`, `set_data` from `set` with
      empty metadata, `remove_data` from `remove`, `listdir_data` from `listdir`, `data_facts` as the
      data's size (a read) with no time or ETag, `get_stored_metadata` from a provided
      `get_metadata` only. A provided method always wins; `data_access = False` opts out. `data_facts` returns `None` or `{size,
      modified, etag, precision}`, `modified` an aware `datetime` or RFC 3339 text
      (`BackendTime::parse`).
  11. **Proposed resolution - consistency and re-entry.** No adapter locking; atomicity is the
      object's (`STORE06`). A store method may evaluate queries under the `RUNTIME` rules; reading
      its own store from inside itself can deadlock and is documented. `is_read_only()` is true
      when every write method is absent.
  12. **Proposed resolution - keys and shutdown.** A relative key is refused with `KeyNotAbsolute`
      before Python is called (`STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` deferred); `shutdown()` drops
      every factory and store object, after releasing the lock, and calls no `close()`.

## Decisions

Orest, 2026-10-31:

1. **A configuration names a factory.** "The type completely specifies the factory and it has an
   arbitrary (free, unique) name"; the factory's parameters are the entry's arguments (`config:`
   for a store). No fixed `python` type and no object registry: Python registers store *types*,
   absorbing the Python half of `LANGUAGE-STORE-TYPE-NOT-DEFINABLE`.
2. **Staged use; rebuilding any time is acceptable.** Recommended flow: "(i) register factories
   (store and recipe provider), register store_config commands and eventually in the future other
   commands necessary for configuration of the environment, e.g. access (ii) build environment from
   configuration (iii) use environment" (`store_config` handlers: question 3). A late registration
   marks the environment for a rebuild that keeps the built store: a store factory registered after
   a failed build lets the next use build the store.

Orest, 2026-11-01:

3. **Question 1 as recommended**: only `get` is mandatory and the other three are derived when
   absent, "but the methods also can be provided": a provided method is always used.
4. **Question 2 as recommended** (`env.store`, `lq.Store.from_object`).
5. **`store_config` stays effective (options A+B).** Python need not implement `store_config`
   handlers yet (`STORE18` `NA`), but it must be able to call them, or a Python store must be
   wrapped by default so the configuration takes effect. So the chains run in Rust around the
   store (A: a mount's `metadata` section wraps it in the pipeline, and everything Python reads or
   writes through `env.store` passes through it), and data-only access is derived when the store
   does not provide it (B, question 10), so any Python store takes a `metadata` section. A chain
   callable from Python (`env.store_config(spec)`) is left for later.

## Problem Example

```python
import liquers as lq

class Notes:                                    # a store with one method
    def __init__(self, files):
        self.files = files
    def get(self, key):                         # bytes, (bytes, metadata), or None if absent
        return self.files.get(str(key))

def notes(prefix, greeting: str = "Hi"):        # the arguments are the factory's parameters
    return Notes({f"{prefix}/hello.txt": greeting.encode()})

env = lq.Environment(config={"store": {"stores": [
    {"type": "notes", "prefix": "notes", "config": {"greeting": "Hello"}},
    {"type": "memory", "prefix": "scratch"}]}})
env.register_store_factory("notes", notes, doc="Notes kept in a dict")    # (i) register
assert env.evaluate("-R/notes/hello.txt") == "Hello"                     # (ii) build, (iii) use
```

**Today** there is no store factory registration and no store protocol: the build fails with an
unknown store type `notes`. **After** the factory builds the mount and the query reads `Notes.get`.

## Scope and Acceptance Criteria

- **AC-1** A Python store answers a query
  WHEN the problem example runs
  THEN `-R/notes/hello.txt` evaluates to `"Hello"`, and `-R/notes/absent.txt` raises `AssetNotFoundError`
- **AC-2** The protocol is resolved once
  WHEN a factory returns an object without `get`, with `listdir` but no `is_dir`, or with an `async def is_supported`
  THEN the build raises `ConfigurationError` naming the mount and the method; a method added later changes nothing
- **AC-3** Absent and failure stay distinct
  WHEN a Python method returns `None`, raises `lq.LiquersError`, or raises another exception
  THEN the caller sees `KeyNotFound`, that error's type, or a read/write error with the traceback
- **AC-4** Absent optional methods are loud or derived
  WHEN a store lacks an optional method
  THEN the call raises `KeyNotSupported` naming it, except question 1's derived methods and `is_supported`
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
  WHEN a document names the memory, file, an OpenDAL and a Python type, and one gated off in this build
  THEN the first four build; the last is refused naming the missing feature (`STORE12`, `STORE13`)
- **AC-11** `read_only` on a Python mount
  WHEN a Python mount carries `read_only: true`
  THEN every write is refused before Python is called, and `is_read_only()` is true (`STORE14`)
- **AC-12** A `metadata` section needs data-only access
  WHEN a Python mount whose object sets `data_access = False`, or has no `listdir`, carries a `metadata` section
  THEN the build fails naming the mount and the missing capability (`STORE15`)
- **AC-13** Records round-trip
  WHEN every store providing its data-only methods (the Rust ones and a Python store with them) is
  configured with `per_entry`, then `grouped`
  THEN records round-trip, the default `on_read` chain applies, and a Python `get` is never called
  for metadata (`STORE16`)
- **AC-14** Backend time compares equal
  WHEN a record's `created` comes from `data_facts` and the entry is re-read
  THEN it compares equal with a fresh reading, for each such store (`STORE17`)
- **AC-17** Configuration stays effective on a minimal store
  WHEN a store with only `get`, `set`, `remove` and `listdir`/`is_dir` is mounted with a `metadata` section
  THEN it builds through derived data-only access, records round-trip under `per_entry`, and the
  configured `on_read` and `on_write` chains run on every access through `env.store` and queries
- **AC-15** Late registration and release
  WHEN a factory or a command is registered after the first use, or `shutdown()` is called
  THEN the next use rebuilds with the store kept (a factory after a failed build lets it build);
  shutdown releases every factory and store object (a `debug-handles` count) and a held
  `lq.Store` refuses further calls
- **AC-16** A Python store type is declared
  WHEN a factory is registered, then a mount omits a required argument, adds an unknown one, the
  name is taken, the factory raises, or it calls its environment
  THEN `store_types()` lists the type with its arguments; each failure raises `ConfigurationError`
  naming the mount and the cause (the taken name at registration), and none hangs

Non-goals: Python `store_config` handlers (question 3); recipe providers; new Rust backends;
calling `close()`.

## Core Interactions

Store: an `AsyncStore` adapter and one `StoreFactory` holding the Python types, chained after
`default_store_factory()`; `read_only` and `metadata` applied by `StoreRouterBuilder` unchanged.
Bindings: `register_store_factory`, `lq.StoreArgumentInfo`, `env.store`, `lq.Store`.

## Crate Placement

`liquers-py` only, a new `src/store/`; `StoreTypeInfo` / `StoreArgumentInfo` already describe a type.

## Documentation Intent

- Reference: a §Stores in `PYTHON_INTEGRATION.md` (factories, protocol, derived methods, errors,
  threading, staged use); `STORE_CONFIG_FSD.md` (integration-registered types in `store_types()`).
- Guide: a "Stores" chapter in `PYTHON_GUIDE.md`. API: docstrings on every new class and method.
- Also `liquers-py/README.md` (STORE row); `LANGUAGE-INTEGRATION_GUIDE.md` §STORE ("What the
  language cannot contribute: a store type" now holds only for JavaScript) and §6 Python;
  `LANGUAGE-STORE-TYPE-NOT-DEFINABLE` narrowed to web; `PY-NO-LANGUAGE-DEFINED-STORE`'s sketch
  marked superseded; `specs/README.md`.
- Open problems: the copies; STORE18; Python types invisible to `liquers-validate` and the
  registry export, so a document naming one is not portable; the derived `contains` cost;
  self-reading deadlock; a late command's rebuild drops the asset cache.

## Design Dependencies

- `PY-NO-LANGUAGE-DEFINED-STORE` — **implemented**; `LANGUAGE-STORE-TYPE-NOT-DEFINABLE` — Python half
  **absorbed** (Decision 1).
- `liquers-py-recipes` — **sibling**; same factory model and method dispatch.
- `language-store-type-registration` — **overlaps**: liquers-web's counterpart.
- `language-store-config-function` — **overlaps** (question 3).
- `WEB-JS-STORE-WITH-ONLY-GET-CANNOT-BE-QUERIED` — **overlaps** (question 1); fixed in its own PR.
- `language-late-registration-without-rebuild` — **overlaps** (Decision 2).

## Scope Changes

- **2026-10-31** — Orest: a configuration names a Python *factory* (Decision 1) in place of a
  `python` type with a registered object; late registration rebuilds (Decision 2). Added AC-16;
  AC-2, AC-10, AC-15 reworded. Questions 1-3 decided 2026-11-01 (Decisions 3-5); AC-17 added and AC-12 reworded for Decision 5. Questions renumbered from the first draft: former 2 (late
  registration) and 4 (naming) retired into the decisions; former 3 is 2; former 12 split into 3
  (STORE18) and 5 (STORE12); new 4 and 6 (factories); former 5 is 7, 6 is 8, 7 and 9 merged into 9,
  8 is 10, 10 is 11, 11 and 13 merged into 12. Size: still `L`. Phases: 1.
- **2026-11-01** — Phase 2 review: AC-12 and AC-13 narrowed to the derivation's requirements
  (decision log (a)). Phases: 1, 2.

## References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §Service adapters, §STORE, Appendix A STORE
- `liquers-web/src/store/js_store.rs`; `specs/guides/STORE_IMPLEMENTATION_GUIDE.md`, `STORE_FACTORY_GUIDE.md`
