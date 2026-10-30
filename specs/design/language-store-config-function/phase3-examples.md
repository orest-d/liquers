# Phase 3: Examples & Use-cases — Language-defined store_config handlers

Under Phase 1's recommended answers. Specs and queries validated with
`target/debug/liquers-validate --command store_config//page_tag --command store_config//tag_with -- '-store_config/media_type-fill/page_tag' '-store_config/tag_with-draft' '-R/n/a.txt'`
(all `Ok`, encoded unchanged). A store spec is written without its realm header in a `metadata`
section; `fill_realm` adds it, which is why the validated form carries `-store_config/`.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A Python tag handler | Registration, read stage, warning on failure | AC-1, AC-2 |
| 2 | Example | A JavaScript validator | `registerStoreConfigHandler`, abort on validate | AC-3, AC-6 |
| 3 | Test | `liquers_core::store::realm::tests::closure_builder_compiles_and_runs` | Capturing closure as builder | AC-9 |
| 4 | Test | `liquers_core::store::realm::tests::literal_parameters_in_declaration_order` | Arguments for a language | AC-4 |
| 5 | Test | existing `store::realm`, `store::handlers`, `store::pipeline`, `liquers-lib` `store_config` tests | `fn` builders unchanged | AC-8 |
| 6 | Test | `liquers-py/tests/test_STORE_config_handlers.py` `test_STORE18_handler_runs_on_read` | Problem example | AC-1 |
| 7 | Test | `test_STORE18_read_failure_is_a_warning` | Warning, read succeeds | AC-2 |
| 8 | Test | `test_STORE18_validate_failure_aborts_the_write` | `store_set` raises the Python class; key absent | AC-3 |
| 9 | Test | `test_STORE18_handler_receives_its_arguments` | `tag_with-draft` | AC-4 |
| 10 | Test | `test_STORE18_late_registration_is_refused` | `ConfigurationError` after first use | AC-5 |
| 11 | Test | `test_STORE18_async_handler_is_refused` | `async def` refused | AC-7 |
| 12 | Test | `liquers-web/tests/store_js_STORE.rs` `store18_language_store_config_handler` | Read, warning, validate abort | AC-6 |
| 13 | Test | `liquers-web/tests/store_js_STORE.rs` `store18_late_handler_rebuilds_the_store` | Rebuild after `configureStore` | AC-5 |
| 14 | Test | `liquers-web/tests/store_js_STORE.rs` `store18_promise_result_is_an_error` | Promise → error under stage rule | AC-7 |

## Example 1: A Python tag handler

```python
env = lq.Environment(config={"store": {"stores": [{"type": "memory", "prefix": "n",
    "metadata": {"on_read": "media_type-fill/page_tag"}}]}})

@env.store_config_handler
def page_tag(record):
    record.info("seen by page_tag")
    return record

env.store_set("n/a.txt", "abc")
asset = env.get_asset("-R/n/a.txt")
assert asset.get() == "abc"
assert any("seen by page_tag" in str(e) for e in asset.metadata.log())     # AC-1
# (`asset.metadata.log` if py-metadata-fields-as-properties lands first)
```

`page_tag` is registered before first use, so the first build registers it into
`command_registry.store_config` and compiles `on_read` against it. If `page_tag` raises
`ValueError("no tags")` instead, `asset.get()` still returns `"abc"` and the record carries
`Metadata handler failed: …ValueError: no tags…` as a warning (AC-2). Arguments come after the record: `def tag_with(record, tag)`
named as `tag_with-draft` receives `"draft"` (AC-4).

## Example 2: A JavaScript validator

```javascript
liquers.registerStoreConfigHandler({
  name: "require_title",
  run: (record) => { if (!record.title) throw new TypeError("title required"); },
});
await env.configureStore({ stores: [{ type: "js", prefix: "n", config: { object: "notes" },
  metadata: { storage: "per_entry", on_validate: "require_title" } }] });
await env.store().set("n/b.txt", bytes, {});   // rejects: ExecutionError "TypeError: title required"
```

Returning `undefined` keeps the record (Phase 1 Q4). Registering a second handler after
`configureStore` rebuilds the store with it (AC-5); a function returning a Promise makes every
call an error, so on `on_validate` the write fails and on `on_read` a warning appears (AC-7).

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| Handler named in a spec but never registered | Build refused, as today ("not a registered store_config handler") | 5 (existing) |
| Handler registered twice under one name | The later replaces the earlier (as `register_step` does) | 6 (second registration) |
| Handler returns a non-record | Error under the stage rule | 7 (a second handler returning `42`), 14 |
| `registerCommand` with `realm: "store_config"` | Refused, naming `registerStoreConfigHandler` | 12 (setup) |
| Python handler on a worker thread | Runs under `block_in_place`; other evaluations proceed | 6 (runs inside an evaluation) |
| wasm build of core | Alias without `Send + Sync` | `cargo check -p liquers-core --target wasm32-unknown-unknown` |

## Test Plan

- Unit: `liquers-core/src/store/realm.rs` `tests` — tests 3-4. Test 3 registers a closure that
  captures an `Arc<AtomicUsize>` and a tag string; compiling `-store_config/tagged` for
  `Stage::Read` and running the chain on a fresh `MetadataRecord` increments the counter and
  applies the tag. Test 4 checks a two-argument and a `multiple` declaration.
- Regression: `cargo test -p liquers-core --lib store::` and `cargo test -p liquers-lib --lib --tests` (test 5).
- Python: new `liquers-py/tests/test_STORE_config_handlers.py`, tests 6-11, each with its own
  `lq.Environment` (a memory mount with a `metadata` section). Test 10 calls `env.evaluate("-R/n/a.txt")`
  once, then expects the decorator to raise `lq.ConfigurationError`. Run: `bash scripts/test-liquers-py.sh`.
- Web: tests 12-14 in `liquers-web/tests/store_js_STORE.rs` beside `STORE14`-`STORE17`, reusing
  its `DataStore`-style object with data-only methods. Run:
  `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`.

## Learning Log

- `fn` items satisfy a generic `F: Fn(..) + MaybeSend + MaybeSync + 'static`, so changing
  `register_step` to a generic is source-compatible for every caller in the workspace.
- `liquers-py` already compiles store sections against `command_registry.store_config`
  (`EnvironmentBuilder::build`), so a Python store type is not a prerequisite.
- `compile` turns a read-stage error into a warning with `{error}` (Display), so a handler's
  traceback is not in the record; validate-stage errors keep it.
