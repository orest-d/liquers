# Phase 1: High-Level Design — Language-defined store_config handlers

## Purpose

Python and JavaScript can name the built-in `store_config` handlers but not add one, because a
handler's builder is a `fn` pointer that cannot capture a callable. This design lets a closure, and
through it a Python or JavaScript function, be a handler, so an application's metadata rule lives
in its own language (VISION: embedded in Python, browser).

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

Today nothing can register `page_tag`: `realm.rs` `StepBuilder` is a `fn` pointer, so the build
refuses the section (`page_tag` is not a registered handler), and `liquers-web/README.md` records
`STORE18` as `NA`. Should be: `asset.metadata.log()` has the handler's entry; a raising handler
becomes a warning. Spec and query validate (`--command store_config//page_tag`): both `Ok`.

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

Store config (`realm.rs` builder type and registration; `CommandRegistry::store_config`), bindings
(`liquers-py` declarations and decorator, `liquers-web` retention, rebuild and `.d.ts`).

## Crate Placement

The builder change is in `liquers-core`; each language half in its binding crate. `liquers-store`
and `liquers-lib` register `fn` builders only and do not change.

## Documentation Intent

- Reference: `specs/reference/STORE_CONFIG_FSD.md`, `specs/reference/PYTHON_INTEGRATION.md`.
- Guide: `specs/guides/STORE_CONFIG_GUIDE.md`, `specs/guides/PYTHON_GUIDE.md`,
  `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §STORE (`STORE18` wording).
- Other: both binding READMEs. To update: these; `specs/README.md` row.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — size `L`; changes the `pub` type `StepBuilder` and
  `StoreConfigFunctions::register_step`; adds a Python decorator and a JavaScript function (rules 1, 4).
- **Leading issue:** Q1, the binding API.
- **Explanation:** the core change is determined by the code (only the builder table changes);
  how each language spells and times registration is user-visible and open.
- **Open questions:**
  1. **Open design - binding API.** (a) **Recommended**: dedicated `@env.store_config_handler(name=,
     label=, doc=)` (the guide's `STORE18` blueprint) and `liquers.registerStoreConfigHandler(declaration)`
     (realm forced to `store_config`, `run` the handler); the command entry points refuse realm
     `store_config` with a pointer to them. (b) Reuse the command API with realm `store_config`:
     one entry point, but a different callable contract (record in/out, synchronous). Today
     JavaScript's `registerCommand` accepts that realm and registers an executor that never runs;
     `py-command-decorator-declaration-keywords` adds `realm=` to the Python decorators, creating the
     same hazard there. Under (a) both refuse it (that design recommends refusing too); under (b)
     both route it to handler registration.
  2. **Open design - late registration (AC-5).** Recommended: each integration's lifecycle. Python
     keeps its built store across rebuilds (`Declarations::store`), so a late handler is refused;
     web rebuilds the store (`apply_store`), so it takes effect (refused once
     `language-late-registration-without-rebuild` removes that rebuild). `STORE18` reworded to
     "takes effect in a rebuilt store or is refused; never ignored".
  3. **Open design - bytes.** Recommended not now: record and arguments only; `needs_bytes` can be
     added later. A listing runs the read chain per entry.
  4. **Proposed resolution - return.** The record to keep; `None` / `undefined` keeps the input;
     anything else is an error under the stage rules.
  5. **Proposed resolution - threads.** The Python step takes the GIL through `liquers-py`'s
     existing `attach_off_worker` (`block_in_place` on a multi-thread runtime); per-entry cost documented.

## Design Dependencies

| Design / issue | Relation | Effect |
|---|---|---|
| `py-command-decorator-declaration-keywords` | overlaps (T2) | Its `realm=` decides what `realm="store_config"` on a decorator means (Q1); not merged: separate sources, merge would be `XL` (E2). Whichever lands later adds the refusal or routing. |
| `language-store-type-registration` | overlaps | Weak: same gap for store types; a Python store it adds is wrapped like any store. |
| `post-init-command-registration` | overlaps | Clones `CommandRegistry` incl. `store_config`; `Arc` builders keep it cheap. |
| `language-late-registration-without-rebuild` | overlaps | Weak: changes the lifecycle AC-5 relies on (Q2). |
| `store-config-key-scoped-handlers` | overlaps | Edits the same `compile_plan` handler arm; second to land rebases. |
| `multi-language-traceback` | overlaps | A handler's exception carries its segments through the unchanged bridges; no order. |
| `py-metadata-fields-as-properties` | overlaps | Makes `metadata.log()` a property; later one updates Example 1 and test 6. |

## Consolidated Findings

- Only the builder table changes: compiled closures already capture; `register_step` becomes
  generic, so `handlers.rs`, `test_support.rs` and `liquers-lib/src/store_config.rs` compile unchanged.
- Python needs no Python store: `EnvironmentBuilder::build` compiles sections against
  `command_registry.store_config`, so handlers run over `memory` and `filesystem` stores.
- Read-stage errors become `Metadata handler failed: {error}` warnings without the traceback;
  validate-stage errors keep it.
- `STORE18`'s Python blueprint uses `record.title` / `record.tags`, which `liquers-py` lacks: not
  filed separately, because this design's Phase 4 step 7 corrects the blueprint.
