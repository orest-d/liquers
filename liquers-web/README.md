# liquers-web

Browser/JavaScript integration of Liquers, compiled to WebAssembly.

A page constructs an environment, evaluates queries as `Promise`s, and registers commands written
in JavaScript. Those commands compose with the built-in Rust ones in a single query — a JavaScript
command's result is converted structurally, so `myCommand/to_text` works.

Design documents: [`specs/design/liquers-web/`](../specs/design/liquers-web/). The integration follows
[`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`](../specs/guides/LANGUAGE-INTEGRATION_GUIDE.md).

## Quick start

```html
<script type="module">
  import init, * as liquers from "./liquers_web.js";

  await init();            // load the wasm module
  await liquers.init();    // create the environment — a Promise, never a blocking initializer

  liquers.registerCommand({
    name: "hello",
    run: () => "Hello, world!",          // no state → a source command
  });

  liquers.registerCommand({
    name: "shout",
    state: "text",
    run: (text) => text.toUpperCase(),   // arguments inferred: none beyond the state
  });

  liquers.registerCommand({
    name: "repeat",
    state: "text",
    arguments: [{ name: "count", type: "int", default: 2 }],   // explicit: the reliable path
    run: (text, count) => text.repeat(count),
  });

  await liquers.evaluate("hello/shout");        // "HELLO, WORLD!"
  await liquers.evaluate("hello/repeat-3");     // "Hello, world!" ×3
  await liquers.evaluate("hello/to_text");      // a built-in Rust command reading a JS result
</script>
```

A runnable version is in [`examples-web/quickstart/`](examples-web/quickstart/).

## Building

`liquers-web` is **wasm32-only**. `JsValue` is `!Send`/`!Sync` on every target, and on native the
`MaybeSend`/`MaybeSync` markers resolve to `Send`/`Sync`, so the bridge types cannot exist there.
The crate body is `wasm32`-gated — a native build produces an intentionally empty crate — and the
workspace's `default-members` excludes it, so the native test loop never builds it.

```bash
cargo check -p liquers-web --target wasm32-unknown-unknown
```

### The quick-start page

```bash
./examples-web/quickstart/build.sh          # or --release
python3 -m http.server 8090 --directory examples-web/quickstart/dist
```

`build.sh` does what `trunk build` does — build the cdylib for wasm32, run `wasm-bindgen` over it,
copy the page next to the output — without requiring trunk to be installed. If you have trunk,
`cd examples-web/quickstart && trunk serve` works too and serves on the same port.

## Testing

Four harnesses, because four different things are being tested.

```bash
# 1. Conformance suites, under Node. The bulk of the tests; no browser needed.
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles

# 2. Suites that genuinely need a browser API (`localStorage`). WebDriver required.
CHROMEDRIVER=$(which chromedriver) cargo test -p liquers-web \
  --target wasm32-unknown-unknown --features browser-tests

# 3. Declarations and artifact structure.
./examples-web/quickstart/build.sh
./scripts/check-stubs.sh

# 4. The delivery form, in a real browser.
cd tests/e2e && npm install && npx playwright test
```

`--features debug-handles` exposes a live count of retained JavaScript function handles, which is
how `RUNTIME05` asserts that unregistering a command releases its closure deterministically rather
than depending on GC timing. Without the feature that one test is compiled out.

`--features browser-tests` compiles the test files carrying
`wasm_bindgen_test_configure!(run_in_browser)`. They are gated rather than always present because
a single such file makes the *whole* Node loop demand a WebDriver, which would cost every other
suite its fast, dependency-free harness. Only tests that cannot work otherwise belong behind it —
`LocalStorageStore`, because `web_sys::window()` returns `None` under Node. Its unit tests are in
`tests/store_local_STORE.rs` and its conformance suite (`C9`) in
`tests/store_conformance_browser_CONF.rs`; the Node-side conformance suites (`C8`, `C10`) stay in
`tests/store_conformance_CONF.rs`.

**The chromedriver's major version must match the installed browser.** A mismatch is refused
outright (`This version of ChromeDriver only supports Chrome version N`), and the error names both
versions. Where the two cannot be matched, `NO_HEADLESS=1` makes the runner serve the tests at
`http://127.0.0.1:8000` for a browser you drive yourself — Playwright over CDP works, and needs no
WebDriver at all.

Per [`CLAUDE.md`](../CLAUDE.md), run the browser loop **after `cargo clean`** and separately from
the native one — the disk allowance does not fit both.

## Stores

A page gets four store backends and the same declarative configuration `liquers-store` uses
natively, so one document can describe both. See `specs/reference/STORE_CONFIG_FSD.md` for the
format and `specs/design/liquers-web-store/` for the design.

```js
const env = liquers.Environment.global();
await env.configureStore({
  stores: [
    { type: 'http',         prefix: 'data',
      config: { url_prefix: './reference/', keys: ['input.csv'] } },
    { type: 'localstorage', prefix: 'local', config: { namespace: 'myapp' } },
  ],
});

const store = env.store();
await store.set('local/out.txt', new TextEncoder().encode('hello'));
const bytes = await store.get('data/input.csv');
const info = await store.getAssetInfo('data/input.csv');  // info.media_type === 'text/csv'
```

`getMetadata` returns the raw stored record, whose `media_type` holds only a declared override and
is usually `null`. `getAssetInfo` returns what the system derives from it — the effective
`media_type`, `data_format`, `filename` and `is_dir` — and is what a page should read to decide how
to show a key.

| `type` | Backend | Writes |
|---|---|---|
| `localstorage` | `localStorage`, full contract including directories | yes |
| `http` / `https` | `fetch`; `url_prefix` + the key minus its routing prefix | no |
| `js` | an object the page implements, registered with `registerStoreObject` | as implemented |

A page-implemented store needs only `get`. Return `null` or `undefined` from `get` or
`getMetadata` for an absent key — throwing signals a *failure* instead, and the two surface as
`key_not_found` and `key_read_error`. A key that exists but has no metadata to report should
return `{}` from `getMetadata`: `null` there means absent (before 2026-09-29 it produced an empty
record). Omitted optional methods report `key_not_supported` rather than quietly answering "empty",
so a half-written store says so.

A directory has no data, so `get` reports it absent; implement `isDir` and `listdir` and the store
answers directory metadata listing its children. A thrown error is never treated as a directory.

To report a specific failure rather than `key_read_error`, throw a `LiquersError` built by the
page: `new liquers.LiquersError(errorType, message, query?)`. `errorType` is a name as `errorType`
reports it (`'key_not_found'`, `'key_read_error'`, …); an unknown name throws `TypeError` rather than
falling back to `general`. A key travels as a key query in `query` (`'-R/data/a.txt'`, not the bare
`'data/a.txt'`), and a query that does not parse throws `TypeError`. The error keeps its type when
Liquers receives it. It is a wasm-bindgen class, not a subclass of `Error`, so it carries no
JavaScript stack, and `key`, `jsClass` and `jsStack` stay `null`.

```js
throw new liquers.LiquersError('key_read_error', 'permission denied', '-R/private/notes.txt');
```

```js
env.registerStoreObject('mine', {
  get(key) { return files.has(key) ? { data: files.get(key) } : null; },  // null: absent
  isDir(key) { return dirs.has(key); },
  listdir(key) { return childrenOf(key); },
});
```

### Read-only stores and metadata sections

Any entry may carry `read_only: true`, which refuses every write with `not_supported` before it
reaches the backend (HTTP 501 through the native server), and a
`metadata` section, which decides how the store's metadata is stored and shaped
(`specs/reference/STORE_CONFIG_FSD.md`, "Metadata section"). `store.isReadOnly()` reports whether
every write is refused.

```js
await env.configureStore({
  stores: [
    { type: 'js', prefix: 'notes', config: { object: 'notes' },
      metadata: { storage: 'grouped', on_read: 'media_type-fill/file_facts' } },
  ],
});
```

The section runs in Rust, around the store, and reaches a page-implemented store only through its
**data-only methods**: `getData`, `setData`, `removeData`, `listdirData` and `dataFacts`, all five
or none, plus optional `renameData` and an `atomicWrite: true` property. `dataFacts` returns
`{ size, modified?, precision?, etag? }` (or `null` when absent), `modified` as an RFC 3339 string.
`getStoredMetadata` is needed only by the default `native` storage model. A `metadata` section on
an object without the five methods is refused when the store is configured, naming the entry.

The built-in stores offer data-only access too, so they take a `metadata` section with no page
code:

- **`localstorage`** — all of it. A `per_entry` or `grouped` record is one more entry in the
  namespace, and the default `native` model keeps the store's own metadata entry. Writes and
  renames are atomic (one synchronous `setItem`; a rename has no interleaving point), and
  `dataFacts` reports the size and an ETag (a hash of the stored entry) but no modification time,
  because the store keeps none.
- **`http`** — the read half. Reads, listing (the configured `keys`, raw) and facts (one `HEAD`:
  `Content-Length`, `ETag`, `Last-Modified` at seconds precision, when the server sends them)
  work, and the record `native` sees is the one derived from the response headers. A record file
  the host serves — `a.txt.__metadata__` for `per_entry`, `__metadata__.json` for `grouped` —
  must be listed in `keys` like any other. Every data write is refused, so pair the section with
  `read_only: true`.

```js
await env.configureStore({
  stores: [
    { type: 'localstorage', prefix: 'local', config: { namespace: 'myapp' },
      metadata: { storage: 'per_entry' } },
    { type: 'http', prefix: 'data', read_only: true, metadata: { storage: 'grouped' },
      config: { url_prefix: 'https://example.org/data/', keys: ['input.csv', '__metadata__.json'] } },
  ],
});
```

## Record streams

A command that returns a record view reaches JavaScript as a `RecordBatch` handle (or as its scalar,
when the view is a single cell); one that returns a record source reaches it as a `RecordSource`.
Both are handles to the Rust object, not copies, and both cross back unchanged: pass a handle as a
command argument or return it from a JavaScript command and Rust receives the same view, with its
chunk identity and origin intact. The `ns-rec` commands are registered in the web environment.

```js
// `people` is any command whose result converts to records.
const source = await env.evaluate('people/ns-rec/to_record_source');
for (const chunk of source.chunks()) {
  console.log(chunk.kind, chunk.query);        // data only; nothing is evaluated yet
  // kind "query" or "key": `query` evaluates to the chunk; kind "memory": a label only
}
for await (const batch of await source.stream()) {
  console.log(batch.numRows);                  // one chunk at a time
  batch.free();
}
const all = await source.materialize(10000);   // one batch, refused beyond maxRows rows
source.free();
```

`free()` releases a handle; a freed handle passed back to Rust is refused with a `ConversionError`
naming its class. Leaving a `for await` loop early (`break`, `return`, a throw) frees the stream.
Only one `nextBatch()` may be pending on a stream at a time. A source resolves its chunks through the
environment that produced it: the singleton, or the `new Environment()` instance it was evaluated in.

## Conformance

Where liquers-web stands against [`LANGUAGE-INTEGRATION_GUIDE.md`](../specs/guides/LANGUAGE-INTEGRATION_GUIDE.md),
for the Browser JavaScript profile. Each row is updated by the change that moves it.

| Feature | Status | Tests | Gaps and `NA` decisions |
|---|---|---|---|
| `VALUE` / RECORDS | complete | 01, 02, 04–08, 10, `tests/records_RECORDS.rs` | `03`, `09` and `11` are `NA`, see below |

**`RECORDS03` is `NA`**: there is no Arrow route out of the browser build; it becomes required if an
Arrow export is added. **`RECORDS09` is `NA`**: JavaScript has its own async model, which the
`RecordSource` iterator uses. **`RECORDS11` is `NA`**: JavaScript cannot implement `RecordView` or
`RecordSource`; it becomes required with a JavaScript-implemented source.

## Known limitations

**`-R/` queries do not evaluate in the browser yet.** The stores work and are reachable through
`env.store()`, but `env.evaluate("-R/…")` hangs: `ImmediateAssetManager::get` runs an asset inline
and `evaluate_recipe` calls `get` on its *own* key, so keyed evaluation recurses until the wasm
stack is exhausted, killing the instance. This is `liquers-core`, not this crate —
`specs/issues/CORE-IMMEDIATE-MANAGER-KEYED-RECURSION.md` (P1). The blocked end-to-end tests are
kept as `fixme` in `tests/e2e/store.spec.ts` so that removing the marker proves the fix.

| Limitation | Tracked as |
|---|---|
| `Asset.cancel()` is inert: the immediate asset manager evaluates during `getAsset`, so an asset is already terminal when a caller receives it. The issue records what fixing it requires — the obstacle is not the absence of a task spawner | `WEB-CANCELLATION-INERT` |
| `encodeParam` refuses values containing a lone colon, most punctuation, or any non-ASCII character — the query grammar has no entity for them. Refusing is deliberate; the core encoder emits unparseable text instead | `PARAMETER-ESCAPING-INCOMPLETE` |
| Registering a command after the first evaluation rebuilds the environment, discarding its asset cache. Register before evaluating to avoid it | `POST-INIT-COMMAND-REGISTRATION` |
| A thrown exception's class and stack reach the caller inside `message`, not as `jsClass`/`jsStack` — `liquers_core::Error` has no field to carry them through the asset lifecycle | `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` |
| `registerCommand` is refused on an explicit `Environment` instance — the handle holds a shared environment with no mutable path. Register on the singleton | `POST-INIT-COMMAND-REGISTRATION` |

All are in [`specs/issues/`](../specs/issues/), each with the condition that reverses it.

## Extending

Two tiers, both supported:

- **Tier 1 — retain a JavaScript value opaquely.** `liquers.opaque(x)` carries `x` by identity
  rather than converting it. Explicit by design, so opacity is never accidental.
- **Tier 2 — your own value type.** Everything in `bridge`, `eval`, `asset` and `command` is
  generic over the value type; only [`src/default_value.rs`](src/default_value.rs) names a concrete
  one. Implement **`JsExtensionBridge` for your own extension type** — not `JsValueBridge` for the
  combined value — and a blanket impl carries it up to the whole value type.

  The distinction is the orphan rule, not taste: `impl JsValueBridge for CombinedValue<SimpleValue,
  MyExt>` is `error[E0117]` from a downstream crate, because both the trait and `CombinedValue` are
  foreign there. `MyExt` is yours, so a foreign trait on it is always allowed. This crate takes the
  same route — `default_value.rs` implements `JsExtensionBridge for ExtValue` — so the documented
  path is the one that gets exercised.

  Worked example: [`tests/second_value_type.rs`](tests/second_value_type.rs).

## Boundary cost

Median round trip (JavaScript → `Value` → JavaScript), `--release`, under Node:

| Input | Structural | Opaque |
|---|---|---|
| object, 10 properties | 0.078 ms | 0.006 ms |
| object, 1 000 properties | 5.23 ms | 0.005 ms |
| object, 10 000 properties | 58.5 ms | 0.008 ms |
| `Uint8Array`, 1 MB | 0.868 ms | 0.006 ms |

Opaque retention is flat; structural conversion is linear. **Reach for `opaque()` because you want
the same object back, not because you want speed** — at realistic sizes the conversion is
invisible, and it only costs a frame at ten thousand properties.

Reproduce with:

```bash
cargo test -p liquers-web --target wasm32-unknown-unknown --release \
    --test boundary_benchmark -- --nocapture
```
