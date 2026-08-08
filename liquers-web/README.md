# liquers-web

Browser/JavaScript integration of Liquers, compiled to WebAssembly.

A page constructs an environment, evaluates queries as `Promise`s, and registers commands written
in JavaScript. Those commands compose with the built-in Rust ones in a single query — a JavaScript
command's result is converted structurally, so `myCommand/to_text` works.

Design documents: [`specs/liquers-web/`](../specs/liquers-web/). The integration follows
[`specs/LANGUAGE-INTEGRATION_GUIDE.md`](../specs/LANGUAGE-INTEGRATION_GUIDE.md).

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

Three harnesses, because three different things are being tested.

```bash
# 1. Conformance suites, under Node. The bulk of the tests; no browser needed.
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles

# 2. Declarations and artifact structure.
./examples-web/quickstart/build.sh
./scripts/check-stubs.sh

# 3. The delivery form, in a real browser.
cd tests/e2e && npm install && npx playwright test
```

`--features debug-handles` exposes a live count of retained JavaScript function handles, which is
how `RUNTIME05` asserts that unregistering a command releases its closure deterministically rather
than depending on GC timing. Without the feature that one test is compiled out.

Per [`CLAUDE.md`](../CLAUDE.md), run the browser loop **after `cargo clean`** and separately from
the native one — the disk allowance does not fit both.

## Known limitations

| Limitation | Tracked as |
|---|---|
| `Asset.cancel()` is inert: the immediate asset manager evaluates during `getAsset`, so an asset is already terminal when a caller receives it. The issue records what fixing it requires — the obstacle is not the absence of a task spawner | `WEB-CANCELLATION-INERT` |
| `encodeParam` refuses values containing a lone colon, most punctuation, or any non-ASCII character — the query grammar has no entity for them. Refusing is deliberate; the core encoder emits unparseable text instead | `PARAMETER-ESCAPING-INCOMPLETE` |
| Registering a command after the first evaluation rebuilds the environment, discarding its asset cache. Register before evaluating to avoid it | `POST-INIT-COMMAND-REGISTRATION` |
| A thrown exception's class and stack reach the caller inside `message`, not as `jsClass`/`jsStack` — `liquers_core::Error` has no field to carry them through the asset lifecycle | `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` |
| `registerCommand` is refused on an explicit `Environment` instance — the handle holds a shared environment with no mutable path. Register on the singleton | `POST-INIT-COMMAND-REGISTRATION` |

All are in [`specs/ISSUES.md`](../specs/ISSUES.md), each with the condition that reverses it.

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
