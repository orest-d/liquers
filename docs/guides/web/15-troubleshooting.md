---
title: Troubleshooting
status: ready
sources: [guides/JAVASCRIPT_GUIDE.md]
reviewed: 2026-11-10
---
# 15. Troubleshooting

PURPOSE: The reader can read a Liquers error, find its cause, and knows the known limitations.

Sections 15.1 and 15.2 describe errors as they are in every Liquers, in the words shared by all
three guides. They write the types as Rust does, `ActionNotRegistered`. In JavaScript the same
types are written in lower case with underscores, `action_not_registered`. One sentence of
section 15.2 does not hold in the browser yet: a failed evaluation is not kept as an asset
there, so there is no log of it to read afterwards. The error object is what you have.

{{> ../_shared/errors.md}}

## 15.3 Errors in JavaScript

Every Liquers error reaches JavaScript as a `LiquersError`: a rejected promise from the
asynchronous functions, a thrown exception from the synchronous ones. There is one class. The
kind of error is its `errorType`, and the groups it belongs to are in `categories`:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
try {
  await liquers.evaluate("-R/data/missing.txt");
  assert.fail("expected a not-found error");
} catch (e) {
  assert(e instanceof liquers.LiquersError);
  assert.strictEqual(e.errorType, "asset_not_found");    // nothing stored, and no recipe makes it
  assert(e.categories.includes("not_found"));
  assert.strictEqual(e.key, "data/missing.txt");
  assert(e.message.includes("data/missing.txt"));
}
```

| Member | Meaning |
|---|---|
| `errorType` | the kind of error, one of the types of section 15.1 in lower case |
| `categories` | the groups the kind belongs to, such as `"not_found"`, `"store"`, `"keyed_asset"` |
| `message` | the message; for an exception thrown by a command, its class, message and stack |
| `query`, `key` | what the error is about, when known |
| `position` | `{ offset, line, column }` in the query or template text, when known |
| `report` | for a refused `registerCommands`, the full list of problems |

JavaScript has no typed `catch`, so test `errorType` or `categories`. A category is the better
test when several types mean the same to you: `not_found` covers a missing key and a missing
asset.

`LiquersError` is not a subclass of `Error`. `e instanceof Error` is `false`, it has no `stack`
of its own, and a logging tool that expects an `Error` may print it badly; log `e.errorType` and
`e.message`. To raise one yourself, from a command or a store:
`new liquers.LiquersError(errorType, message, query)`. An unknown type throws a `TypeError`.

The errors you will meet early:

| Situation | `errorType` |
|---|---|
| Using Liquers before `liquers.init()` or after `shutdown()` | `not_available` |
| A malformed query | `parse_error` |
| A command name that is not registered, or a missing `ns-…` | `action_not_registered` |
| Too many arguments for a command | `too_many_parameters` |
| A required argument left out | `argument_missing` |
| Text where a number is declared; a value that has no JavaScript form | `conversion_error` |
| An exception inside a JavaScript command | `execution_error` |
| A key nothing is stored under and no recipe makes | `asset_not_found` |
| A write to a read-only mount; a store type the browser does not have | `not_supported` |
| A store asked for something it cannot do | `key_not_supported` |
| A refused command declaration | `parameter_error`, with a `report` |

## 15.4 Checking before running

Three checks run no command (section 14.6 shows them in tests):

- `liquers.Query.parse(text)` throws a `parse_error` with the position when the text is not a
  query;
- a `QueryEditor` refuses an action that names no registered command, and an argument the
  command does not have (section 3.9);
- `liquers.validateTemplate(template)` lists every tag whose query cannot work.

To see what a query *means*, which is how you catch a query that is valid but not what you
intended, convert it with `liquers.queryToPipeline(text)` and look at its elements
(section 3.8). `-R/data/report.txt/to_text` and `-R/data/report.txt/-/to_text` both parse; the
first has one element, a key that ends in `to_text`, and the second has a key and an action.

## 15.5 Frequent mistakes

| What you see | The usual cause |
|---|---|
| `not_available` on the first call | `liquers.init()` was not awaited; `init()` alone only loads the WebAssembly (section 2.2) |
| `action_not_registered` for a command you registered | the registration runs after the evaluation, or the command is in a namespace and the query lacks `ns-…` (section 5.6) |
| `action_not_registered` for `ns-rec` commands | the module was built without `--features records` (section 2.5) |
| `action_not_registered` for `ns-pl`, `ns-img` or `ns-search` commands | those libraries are not in the browser module (sections 9.5 and 13.2) |
| `asset_not_found` for every `-R/` query | no store is configured, or no mount has the key's prefix (section 6.2) |
| `asset_not_found` with a key that ends in a command's name | the `/-/` between the key and the actions is missing (section 3.4) |
| `asset_not_found` for a file that is on the server | an `http` store knows only the keys listed in `keys` (section 6.3) |
| a store that was filled is empty, results are computed again | a command was registered after the first evaluation, which rebuilt the environment (section 8.3) |
| a result does not change after its input changed | results are not refreshed in the browser yet (section 7.7) |
| a command that reads a clock, a form or a server returns an old value | it is not declared `volatile` (section 5.6) |
| `conversion_error` naming a `Map`, a `Date` or a class | only plain data crosses into Liquers; convert it or wrap it with `liquers.opaque` (section 4.2) |
| a `Uint8Array` where a table was expected | a stored CSV file is bytes until `ns-rec/to_record` reads it (section 4.4) |
| a query argument arrives cut at a `-` or a space | the value was pasted into the query unescaped; use `liquers.encodeParam` (section 3.9) |
| "declared synchronous but returned a Promise" | the command is `async: false` and returned a promise; declare `async: true` (section 5.7) |
| "cannot infer arguments" at registration | `run` has a default value, a rest parameter or destructuring; declare `arguments` (section 5.3) |
| a recipe is ignored | the store cannot list its folder: an `http` store without `recipes.yaml` in `keys`, a `js` store without `listdir` (section 7.8) |
| the page freezes while a query runs | a synchronous command is doing long work on the page's only thread (section 5.7) |
| numbers of a table column arrive as `BigInt` | integer columns are 64-bit; use `Number(x)` or `to_json` (section 9.3) |

## 15.6 Known limitations

What the browser module does not do, or not yet. Each is described where it matters; this is the
list in one place.

**By the nature of a browser:**

- One environment per page, shared by everything the page does (section 8.1).
- One thread. Evaluation is inline, queries do not run in parallel, and a long synchronous
  command blocks the page (sections 5.7 and 7.5).
- No file system and no cloud storage: the stores are memory, `localStorage`, files over HTTP
  and objects of the page (section 6.3).
- A page cannot serve queries to other programs (chapter 12).
- The module is a download of several megabytes (section 2.4).

**Left out of the build:**

- No Polars and no image commands (section 9.5).
- No search library and no agent memory (chapter 13).
- The record library only in a build with `--features records` (section 2.5).

**Not done yet:**

- Cancelling does nothing (section 7.9).
- A result is not refreshed when something it was made from changes, and a recipe's stored
  result stays until it is removed (sections 7.7 and 7.8).
- A failed evaluation is not kept, so its log cannot be read afterwards.
- A file name at the end of a query does not give `evaluate` the bytes of that format; it
  resolves with the value (section 9.6).
- Registering a command after the first evaluation rebuilds the environment (section 8.3).
- Command lookup is by name only: `describeCommand` sees the root namespace, and
  `unregisterCommand` removes the commands of that name in every namespace.
- An isolated `new Environment()` cannot have commands or a store of its own (section 8.4).
- A declaration cannot write a link default as query text (section 5.5).
- The class and the stack of an exception thrown by a command are part of the error's `message`,
  not fields of their own.
- A slow query freezes a mounted user interface, and elements defined in JavaScript show in the
  browser only (section 11.6).
- No store for IndexedDB, and none that reads and writes a Liquers server (section 6.3).
- No npm package and no versioned address on a CDN (section 2.3).
- The interfaces can still change between releases: Liquers is at version 0.1.

## 15.7 When the module does not load

Problems before the first query are problems of hosting, and the browser's console and network
panel show them.

| What you see | The cause |
|---|---|
| "Cannot use import statement outside a module" | the script tag lacks `type="module"` |
| the import fails with a CORS error on a page opened from disk | browsers do not load modules from a `file:` address; serve the page with a web server |
| the import fails with a CORS error from another site | the server of the files does not send `Access-Control-Allow-Origin` (section 2.4) |
| "Failed to load module script: expected a JavaScript module script" | the server sends `liquers_web.js` with the wrong media type, or answered with an error page |
| a 404 for `liquers_web_bg.wasm` or for a file under `snippets/` | the files were not copied together; they must sit next to `liquers_web.js` under their own names (section 2.1) |
| a warning that `WebAssembly.instantiateStreaming` failed | the server does not send `application/wasm`; the module loads, but more slowly |
| "Refused to compile or instantiate WebAssembly module" | the site's Content Security Policy lacks `'wasm-unsafe-eval'` |
| errors after a new deployment that a hard reload cures | the browser combined a cached `.js` with a new `.wasm`; put each build in a folder of its own (section 2.3) |
| a long pause before anything happens | the first download; compress the `.wasm`, cache it, and load it after the page has appeared (sections 2.2 and 2.4) |
