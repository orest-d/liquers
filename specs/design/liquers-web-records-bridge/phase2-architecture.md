# Phase 2: Solution & Architecture — liquers-web-records-bridge

## Overview

Map `ExtValue::RecordSource` to a new exported `RecordSource` handle holding the `Arc<dyn
RecordSource>`; traversal methods evaluate chunks through `liquers_records::EnvResolver` over the
web environment's `EnvRef` (`with_global()`), resolved at each call. `stream()` returns an async
iterable built by a small JavaScript companion (`src/records_stream.js`) over a Rust
`RecordStream` handle whose `nextBatch()` returns a `Promise<RecordBatch | null>`. Both handle kinds
register their `Arc` in a thread-local registry under a `u32` id exposed as a marker property, so
`from_js_custom` can give Rust back the same value. The web environment registers the `ns-rec`
command set.

Rejected:
- An environment argument on `stream`/`materialize` — there is one environment, the web
  environment; a multi-environment (client/server) system is not defined yet (maintainer, 2026-10-17).
- Consume the handle with wasm-bindgen's owned `TryFromJsValue` — invalidates the page's handle on a
  read-only pass-through.
- Materialize a source on crossing — defeats the record-streams design for any large source.
- Convert JavaScript-built tables in `from_js_custom` — duplicates `ns-rec/from_json` and guesses a
  schema at the boundary.
- `Symbol.asyncIterator` defined from Rust via `Reflect` on the class prototype at `init()` — works,
  but a companion module is the pattern `records_column.js` already set and is testable alone.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `WEB-RECORDS-SINGLE-CELL-VIEW-CROSSES-AS-BATCH` | draft | P2 | Same `to_js_custom` arm | Yes | No | Land `web-records-single-cell-scalar` first; rebase onto it |
| `WEB-STORE-CONFORMANCE-TEST-GAPS` | draft | P3 | Shares the README `## Conformance` section | No | No | Create or extend the section, whichever lands first |
| `WEB-CANCELLATION-INERT` | accepted | P3 | A `RecordStream` cannot cancel an in-flight chunk evaluation | No | No | Document; `return()` on the iterator only frees the stream |
| `POST-INIT-COMMAND-REGISTRATION` | accepted | P3 | `ns-rec` registered in `new_environment`, so rebuilds keep it | No | No | None |

## Interfaces

New, all in `liquers-web/src/records.rs`, `#[cfg(feature = "records")]`, synchronous unless noted:

```rust
#[wasm_bindgen(js_name = RecordSource)]
pub struct LiquersRecordSource { inner: Arc<dyn RecordSource>, id: u32 }

#[wasm_bindgen(js_class = RecordSource)]
impl LiquersRecordSource {
    pub fn chunks(&self) -> Result<JsValue, JsValue>;            // [{kind, query}]
    #[wasm_bindgen(getter)] pub fn bounded(&self) -> bool;       // ChunkList::Known
    #[wasm_bindgen(js_name = schemaJson)] pub fn schema_json(&self) -> Option<String>;
    #[wasm_bindgen(js_name = manifestJson)] pub fn manifest_json(&self) -> Option<String>;
    pub fn stream(&self) -> js_sys::Promise;                     // async
    pub fn materialize(&self, max_rows: usize) -> js_sys::Promise; // async
    #[wasm_bindgen(getter, js_name = __liquersRecordHandle)] pub fn handle_id(&self) -> u32;
}

#[wasm_bindgen(js_name = RecordStream)]
pub struct LiquersRecordStream { inner: Rc<RefCell<Option<BoxRecordStream>>> }
impl LiquersRecordStream {
    #[wasm_bindgen(js_name = nextBatch)] pub fn next_batch(&self) -> js_sys::Promise; // async
}
```

`LiquersRecordBatch` gains `id: u32`, the same `__liquersRecordHandle` getter, and
`chunkIdJson()` / `sourcesJson()` (origin readable from JavaScript for `RECORDS01`); its `Drop`
becomes unconditional (registry removal), the live count stays behind `debug-handles`.

Registry (private): `thread_local! { static HANDLES: RefCell<HashMap<u32, RecordHandle>> }` with
`enum RecordHandle { View(Arc<dyn RecordView>), Source(Arc<dyn RecordSource>) }`. Single-threaded
wasm, so `thread_local` + `RefCell` is sufficient; never borrowed across an `await`.

`stream()` is async because `RecordSource::stream` returns a future; it resolves to the iterable
from `records_stream.js` `asyncIterable(stream)`. `chunks()` stays sync (no I/O, per the trait).

Commands: none new. `liquers-web/src/builtins.rs` `register_builtin_commands` adds
`liquers_lib::register_records_commands!(cr)` (namespace `rec`).

## Integration Points

- `liquers-web/src/default_value.rs`: `to_js_custom` `RecordSource` arm → `LiquersRecordSource`;
  `from_js_custom` reads `__liquersRecordHandle` (only on objects) and returns
  `ExtValue::RecordView`/`RecordSource` from the registry.
- `liquers-web/src/records.rs`: the types above; `src/records_stream.js` companion.
- `liquers-web/src/environment.rs`: no change — `with_global()` gives the `EnvRef`.
- `liquers-web/src/builtins.rs`: `ns-rec` registration.
- `liquers-web/src/typescript.rs`: declarations for `RecordSource`, the iterable, `chunks()` entries.
- `liquers-web/scripts/check-stubs.sh` / quickstart: the new exports appear in the `.d.ts` check.

## Error Handling

- Freed handle (the marker getter throws, or the id is absent): `Error::conversion_error("a freed
  RecordBatch handle" / "a freed RecordSource handle", "a live record handle")`.
- Second `nextBatch()` while one is pending: `Error::general_error("RecordStream.nextBatch called
  while a previous call is pending")`.
- `materialize` past `max_rows`, schema mismatch, chunk failures: the `liquers-records` errors,
  unchanged, via `liquers_error_to_js`.
- `init()` not run: the existing `with_global()` error.
- Until Step 3 lands, Step 1's refusal: `Error::conversion_error("RecordSource", "a JavaScript value
  (a record source has no JavaScript representation yet; materialize it with ns-rec/materialize)")`.

## Relevant Commands

The existing `rec` namespace (`ns-rec/from_json`, `to_record`, `to_record_source`, `materialize`,
`nrows`, …), unchanged. No new command; `specs/command_registry.yaml` does not change.

## Documentation Architecture

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (guide, internal, `[web, records]`): §RECORDS "What
  `liquers-web` does" — the source handle, traversal in the web environment, handles crossing back; History row,
  `reviewed:` bump.
- `liquers-web/README.md`: a records usage section and `RECORDS` rows in `## Conformance`.
- `affects_docs`: the two above.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `default_value.rs`, `records.rs`, `records_stream.js`, `builtins.rs`, `typescript.rs`, `tests/records_RECORDS.rs`, README, guide |
| Crates and workflows affected | `liquers-web` only; the Node test loop, the stubs check, the quickstart build |
| Existing tests likely to change | `records05`/`records06` (handle constructor gains an id); stubs snapshot |
| New validation | `records01`, `02`, `04`, `07`, `08`, AC-6/AC-7 tests under Node |
| Compatibility / data / concurrency / performance / security | Additive JS API. The `.wasm` grows with `ns-rec`; Step 2 records the before/after size of the quickstart build (no size budget exists today). Registry holds `Arc`s only while handles live; a leaked JS handle leaks as today. Single-threaded, no locks across awaits. |
| Recovery | Each step reverts alone; Step 2 (commands) can be feature-gated off if the size budget fails |
| Certainty and open questions | High for the mechanism; the JS→Rust direction (Phase 1 open question 1) is the open part |
