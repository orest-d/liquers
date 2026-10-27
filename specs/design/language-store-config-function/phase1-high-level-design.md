# Phase 1: High-Level Design — Language-defined store_config handlers

## Purpose

A Python module or a web page can name the built-in `store_config` handlers in a store's
`metadata` section, but it cannot add a handler of its own, because a handler's builder is a plain
`fn` pointer that cannot capture a language callable. This design lets a Rust closure, and through
it a Python or JavaScript function, be a handler. It serves VISION's "embedded in Python" and
browser uses: an application's metadata rule (a domain field, a tag) is written where the
application lives, and runs on every store the configuration wraps.

## Problem Example

Python, at HEAD:

```python
env = lq.Environment(config={"store": {"stores": [{"type": "memory", "prefix": "n",
    "metadata": {"on_read": "media_type-fill/page_tag"}}]}})

@env.store_config_handler          # does not exist
def page_tag(record):
    record.info("seen by page_tag")
    return record

env.store_set("n/a.txt", "abc")
asset = env.get_asset("-R/n/a.txt")
```

Today there is no way to register `page_tag`: `liquers-core/src/store/realm.rs` `StepBuilder` is
`fn(Stage, &ResolvedParameterValues) -> Result<CompiledStep, Error>` and
`StoreConfigFunctions::register_step` takes one, so neither `liquers-py` nor `liquers-web` can
put a callable behind it. The build refuses the section, since `page_tag` is not a registered
handler. `liquers-web/README.md` records `STORE18` as `NA` for this reason. Should be: `asset.metadata.log()`
(the record read for `n/a.txt`) has the handler's entry; a raising `page_tag` becomes a warning on that record. The
spec and the query validate: `liquers-validate --command store_config//page_tag -- '-store_config/media_type-fill/page_tag' '-R/n/a.txt'` → both `Ok`.

## Scope and Acceptance Criteria

- **AC-1** A Python handler runs in its stage
  WHEN Python registers `page_tag` before the store is built and `on_read` names it
  THEN the record read for a key carries the handler's change
- **AC-2** A read failure is a warning
  WHEN the handler raises while reading
  THEN the read succeeds and the record has a warning naming the handler and the exception
- **AC-3** A validate failure aborts the write
  WHEN `on_validate` names a handler that raises
  THEN the write fails with the converted Liquers error (Python class and traceback kept) and nothing is written
- **AC-4** Arguments
  WHEN the spec is `tag_with-draft` and the function is `tag_with(record, tag)`
  THEN the function receives `"draft"`, resolved once when the store is built
- **AC-5** Late registration is never ignored
  WHEN a handler is registered after the environment's store was built
  THEN Python refuses it with a `ConfigurationError` naming the handler, and JavaScript rebuilds the store with it
- **AC-6** JavaScript (`STORE18`)
  WHEN a page registers a handler and `configureStore` names it in `on_read` and `on_validate`
  THEN AC-1 to AC-3 hold for a JavaScript function
- **AC-7** Synchronous only
  WHEN the function is `async def` (Python) or returns a Promise (JavaScript)
  THEN Python refuses the registration; a JavaScript Promise result is an error under the stage rules
- **AC-8** Rust builders unchanged
  WHEN the built-in handlers and the existing `fn` builders register
  THEN they compile without change and every `store_config` test passes
- **AC-9** A capturing Rust closure is a builder
  WHEN Rust registers a closure that captures state as a handler builder
  THEN a spec naming it compiles and runs it

Non-goals: language-defined storage models and write strategies (`ComponentBuilder` stays a
`fn`); data bytes for language handlers (Q3); `compileStoreConfig`; a Python language store
(`language-store-type-registration`); registering into a shared environment without a rebuild
(`post-init-command-registration`).

## Core Interactions

- Store config (`liquers-core/src/store/realm.rs`): builder type and registration.
- Commands (`liquers-core/src/commands.rs` `CommandRegistry::store_config`): carries the new builders.
- Bindings: `liquers-py` (declarations, rebuild, decorator) and `liquers-web` (retention, rebuild, `.d.ts`).

## Crate Placement

The builder change is `liquers-core`, where the realm lives. Each language half lives in its
binding crate. `liquers-store` and `liquers-lib` only register `fn` builders and need no change.

## Documentation Intent

- Reference: extend `specs/reference/STORE_CONFIG_FSD.md` (handlers defined outside Rust) and `specs/reference/PYTHON_INTEGRATION.md`.
- Guide: extend `specs/guides/STORE_CONFIG_GUIDE.md` (a closure builder), `specs/guides/PYTHON_GUIDE.md`, `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §STORE (`STORE18` wording, the answer to the handler question).
- Other documents: `liquers-web/README.md` (`STORE18` required), `liquers-py/README.md` (a `STORE` row note).
- Documents to update: the above; `specs/README.md` gains this design's row.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — size `L`; changes the `pub` type `StepBuilder` and the
  signature of `StoreConfigFunctions::register_step` in `liquers-core`, and adds a Python decorator
  and a JavaScript function (rules 1 and 4).
- **Leading issue:** **Open design - binding API** (Q1).
- **Explanation:** the core change is small and fully determined by the code (only the builder
  table changes; compiled closures already capture). What remains open is how each language
  spells and times the registration, which users see.
- **Open questions:**
  1. **Open design - binding API.** (a) *Dedicated entry points* (**recommended**):
     `@env.store_config_handler(name=…, label=…, doc=…)` in Python (the shape the guide's `STORE18`
     blueprint already uses) and `liquers.registerStoreConfigHandler(declaration)` in JavaScript,
     the declaration in the Command Declaration Format with realm forced to `store_config` and
     `run` the handler. (b) *Reuse the command API* with `realm: "store_config"`: one entry point,
     but the callable's contract (record in, record out, synchronous) differs from a command's, and
     the Python decorators take no `realm=`. Under (b) a page passing `realm: "store_config"` to
     `registerCommand` today gets an executor that never runs, so (b) also has to change what that
     call means; under (a) that call is refused with a pointer to the new function.
  2. **Open design - late registration (AC-5).** Recommended: follow each integration's existing
     lifecycle. `liquers-py` keeps its built store across rebuilds (`Declarations::store`), so a
     handler registered after the first build cannot reach it and is refused. `liquers-web`
     rebuilds the store whenever it rebuilds the environment (`apply_store`), so a late handler
     takes effect (refused instead once `language-late-registration-without-rebuild` removes that
     rebuild). `STORE18`'s "is refused if registered after the store is built" is reworded to
     "takes effect in a rebuilt store or is refused; never ignored".
  3. **Open design - bytes.** Recommended: not now. A language handler gets the record and its
     arguments only; a `needs_bytes` declaration can be added later without changing this one. A
     listing runs the read chain per entry, and bytes would cross the bridge each time.
  4. **Proposed resolution - return contract.** The function returns the record to keep; `None` /
     `undefined` keeps its input; anything else is an error under the stage rules.
  5. **Proposed resolution - Python threads.** A chain runs synchronously inside a store call on a
     tokio worker. The Python handler takes the GIL inside `tokio::task::block_in_place` when the
     current runtime is multi-threaded (`liquers-py`'s is), so other tasks move off that worker;
     per-entry GIL cost is documented.

## Design Dependencies

| Design / issue | Relation | Effect |
|---|---|---|
| `language-store-type-registration` (`LANGUAGE-STORE-TYPE-NOT-DEFINABLE`) | overlaps | Weak: the same asymmetry for store *types*; independent change sites. A Python language store it adds would be wrapped by these handlers like any store. |
| `post-init-command-registration` | overlaps | Its `update_commands` clones `CommandRegistry`, including `store_config`; the `Arc` builders keep that clone cheap. A handler added through it reaches only stores built afterwards, which AC-5 states. |
| `language-late-registration-without-rebuild` | overlaps | Weak: it changes the registration lifecycle this design relies on, at different change sites; no T1-T4 test holds. If it lands, web no longer rebuilds on registration, so a late handler cannot reach a built store: AC-5 then becomes "refused" in JavaScript too (Q2). |
| `store-adapters` (complete) | — | Source of the realm and of §Language Integration item 4, which this design builds. |

## Consolidated Findings

- **Only the table changes.** `CompiledFn` and `NeedsBytesFn` are already boxed closures (with
  `Send + Sync` off wasm), so a language closure fits them; `StepBuilder` becomes an `Arc`'d closure
  with the same cfg pair, and `register_step` becomes generic, so every existing `fn` caller
  (`handlers.rs`, `test_support.rs`, `liquers-lib/src/store_config.rs`) compiles unchanged.
- **Python is not blocked on a Python store.** `liquers-py` already builds configured stores
  through `EnvironmentBuilder`, which compiles sections against `command_registry.store_config`,
  so a Python handler runs over the built-in stores (`memory`, `filesystem`) today.
- **Errors keep their traceback on validate**; on read `compile` writes `Metadata handler failed:
  {error}` as a warning, so the traceback is not in the record (Display omits it). Accepted.
- **Guide blueprint drift (not filed here):** `STORE18`'s Python blueprint uses `record.title` and
  `record.tags`, which `liquers-py`'s `MetadataRecord` does not expose. Phase 3 uses existing
  accessors; the guide update in step 7 corrects the blueprint.
