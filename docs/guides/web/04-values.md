---
title: Values, state and metadata
status: ready
sources: [reference/VALUE_TYPE_SYSTEM.md, guides/JAVASCRIPT_GUIDE.md, reference/PIPELINES.md]
reviewed: 2026-11-10
---
# 4. Values, state and metadata

PURPOSE: The reader knows what a command receives and returns in JavaScript in the browser, and what metadata travels with a value.

Inside the module, a value is a Rust value. Your code sees JavaScript values. This chapter is
about the border between the two: what crosses it, in what form, and what travels with a value.

## 4.1 Values in JavaScript

A value crosses in both directions: what a JavaScript command returns becomes a Liquers value,
and the result of `evaluate`, or the input of a command, becomes a JavaScript value. The
conversion goes by kind:

| JavaScript | In Liquers | Back in JavaScript |
|---|---|---|
| `null`, `undefined` | none | `null` |
| `boolean`, `string` | the same | the same |
| `number`, a whole number up to 2^53 | integer | `number` |
| other `number`, NaN and infinities included | float | `number` |
| `BigInt` | 64-bit integer; a larger one is a `conversion_error` | `number` |
| `Uint8Array`, `ArrayBuffer` | bytes | `Uint8Array` |
| `Array`, plain object | array, object | `Array`, plain object |
| `liquers.opaque(x)` | an opaque JavaScript object | `x` itself |
| anything else: `Map`, `Set`, `Date`, a class instance, a function | refused with `conversion_error` | |

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "record", run: () => ({ name: "Ada", scores: [1, 2.5, null] }) });
liquers.registerCommand({ name: "bytes", run: () => new Uint8Array([1, 2, 3]) });
liquers.registerCommand({ name: "big", run: () => 2n ** 40n });
liquers.registerCommand({ name: "kind", state: "value", run: (value) => typeof value });

assert.deepStrictEqual(await liquers.evaluate("record"), { name: "Ada", scores: [1, 2.5, null] });
assert.deepStrictEqual(await liquers.evaluate("bytes"), new Uint8Array([1, 2, 3]));
assert.strictEqual(await liquers.evaluate("big"), 1099511627776);
assert.strictEqual(await liquers.evaluate("big/kind"), "number");     // a BigInt comes back as a number
```

A few rules are worth knowing before they surprise you:

- **A value is copied when it crosses.** The object a command returns and the object `evaluate`
  gives you are different objects with the same content. Changing one does not change what
  Liquers keeps.
- **The keys of an object come back in alphabetical order**, not in the order you wrote them.
- **An integer outside ±(2^53 − 1)** is refused rather than rounded when it goes back to
  JavaScript.
- **Inside an array or object**, NaN and the infinities become the strings `"NaN"`, `"inf"` and
  `"-inf"`, and a `Uint8Array` is refused, because the nested form is JSON and JSON has no bytes.
  Return bytes on their own.
- **Anything that is not plain data is refused**, so that nothing loses its meaning silently. A
  `Date` does not become a string by accident; convert it yourself, or pass it as an opaque
  object (section 4.2).

Because a command's result is converted, a command written in Rust can read what a JavaScript
command produced, and the other way round. Converting a large object costs a few milliseconds
(about 5 ms for 1 000 properties), which is invisible at the sizes a page usually handles.

Some values of the library do not convert at all, because copying them would be wasteful. A
table and a user-interface element reach JavaScript as *handles* (section 4.5).

## 4.2 Opaque objects

To pass a JavaScript object from one of your commands to another without converting it, wrap it
with `liquers.opaque(x)`. The next JavaScript command receives the same object:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
const today = new Date(2026, 9, 26);
liquers.registerCommand({ name: "today", run: () => liquers.opaque(today) });
liquers.registerCommand({ name: "year", state: "value", run: (date) => date.getFullYear() });
liquers.registerCommand({ name: "same", state: "value", run: (date) => date === today });

assert.strictEqual(await liquers.evaluate("today/year"), 2026);
assert.strictEqual(await liquers.evaluate("today/same"), true);
await assert.rejects(liquers.evaluate("today/to_text"), (e) => e.errorType === "conversion_error");
```

An opaque object is the browser's counterpart of a native value type: a chart object, a parsed
document, a `Blob`, a DOM node. A command written in Rust cannot look inside one, and it cannot
be written to a store. It is kept by reference, so if you change the object later, the cached
result changes with it.

## 4.3 The state and its metadata

Liquers never passes a bare value around. A value travels as a *state*: the value together with
its *metadata*, a description of what it is and how it was made. `getAsset(query)` gives you the
asset, and `asset.get()` its state:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "hello", run: () => "world" });

const asset = await liquers.getAsset("hello/hello.txt");
const state = await asset.get();

assert.strictEqual(state.toJS(), "world");                 // the value, as evaluate gives it
assert.strictEqual(state.value().identifier, "Text");      // its type
assert.strictEqual(state.status, "ready");

const metadata = state.metadata();                         // a plain object
assert.strictEqual(metadata.query, "hello/hello.txt");
assert.strictEqual(metadata.type_identifier, "Text");
assert.strictEqual(metadata.filename, "hello.txt");
assert.strictEqual(metadata.data_format, "txt");
assert(metadata.dependencies.length > 0);                  // what the result was made from
assert(Array.isArray(state.log()));                        // what was logged while it was made
```

The fields you will use most:

| Field of `metadata()` | Meaning |
|---|---|
| `query` | the query that made the value |
| `type_identifier` | the type: `Text`, `I64`, `F64`, `Bool`, `None`, `Bytes`, `Array`, `Object`, … |
| `filename`, `data_format` | the file name at the end of the query, and the format it names |
| `status` | `"Ready"`, `"Error"`, …; note the capital letter, where `asset.status()` and `state.status` say `"ready"` |
| `dependencies` | the keys, queries and commands the value was made from, each with a version |
| `log` | the log entries, also returned by `state.log()` |
| `is_volatile`, `expires` | whether the value may be reused, and until when (chapter 7) |

`asset.getAssetInfo()` resolves with a shorter summary that adds the `media_type`, here
`text/plain`.

A command can receive the state instead of the value. Declare `state: "state"` and the first
parameter is `{ value, metadata, status, log }`:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "hello", run: () => "world" });
liquers.registerCommand({
  name: "describe",
  state: "state",
  run: (state) => `${state.value} is ${state.metadata.type_identifier}, made by ${state.metadata.query}`,
});

assert.strictEqual(await liquers.evaluate("hello/describe"), "world is Text, made by hello");
```

## 4.4 Values as files

A value has a byte form whenever it is stored. Which form it is follows from the extension of
the key, and so does the value you get when a query reads the key:

| Key ends with | A query that reads it gives |
|---|---|
| `.txt`, `.md`, `.html`, `.toml` | a string |
| `.json`, `.yaml` | the parsed data: an object or an array |
| `.csv`, `.png`, anything else, or no extension | the bytes, a `Uint8Array` |

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "data" }] });
const bytes = (s) => new TextEncoder().encode(s);
await env.store().set("data/note.txt", bytes("milk"), {});
await env.store().set("data/config.json", bytes('{"limit": 3}'), {});
await env.store().set("data/sales.csv", bytes("region,amount\nEU,10\n"), {});

assert.strictEqual(await liquers.evaluate("-R/data/note.txt"), "milk");
assert.deepStrictEqual(await liquers.evaluate("-R/data/config.json"), { limit: 3 });
assert((await liquers.evaluate("-R/data/sales.csv")) instanceof Uint8Array);
assert.strictEqual(await liquers.evaluate("-R/data/sales.csv/-/ns-rec/to_record/nrows"), 1);
```

A CSV file is bytes until a command reads it as a table, as `to_record` does in the last line.

In the other direction, a file name at the end of a query (`hello/hello.txt`) records the name
and the format in the metadata, as section 4.3 showed. `evaluate` still resolves with the value
itself, not with the bytes of the file: the byte form is made when a result is stored, which in
a page happens for results that have a recipe (chapter 7). To put bytes of your own making into
the store, call `env.store().set(key, bytes, {})`.

## 4.5 Handles

Three kinds of values reach JavaScript as a *handle*: a small object that refers to data inside
the module instead of copying it.

| Value | Handle | Chapter |
|---|---|---|
| a table | `RecordBatch` | [9](09-data.md) |
| a table in chunks | `RecordSource` | [9](09-data.md) |
| a user-interface element | `UIElement` | [11](11-ui.md) |

The objects the module gives you for its own work are handles too: `Asset`, `State`, `Value`,
`Query`, `Key`, `Store` and the editors.

JavaScript's garbage collector does not see the memory inside the module. A handle that is
dropped is cleaned up eventually, but when is up to the browser. So every handle has a `free()`
method, and for large data, a table above all, you should call it when you are done. After
`free()` the handle must not be used. A handle passed back to Liquers, as the result of a
command for example, is the same value again, not a copy.
