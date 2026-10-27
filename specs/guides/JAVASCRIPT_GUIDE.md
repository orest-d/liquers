---
title: Using Liquers from JavaScript in the browser
kind: guide
audience: user
area: [web]
reviewed: 2026-10-26
---
# Using Liquers from JavaScript in the browser

How to use Liquers from a web page with `liquers-web`: build the WebAssembly module, load it,
register commands written in JavaScript, configure stores and evaluate queries. How the module is
built on the Rust crates, and the exact rules it follows, are in
[`reference/JAVASCRIPT_INTEGRATION.md`](../reference/JAVASCRIPT_INTEGRATION.md).

Every example below runs as written against the module built from this repository:
`liquers-web/scripts/test-guide-examples.sh` runs them all under Node. They check their results with
`assert`, which is Node's `node:assert`; on a page, read an `assert` as a statement of what the line
gives. The few examples that need something Node does not have are marked, with the reason.

## 1. Building and loading

There is no npm package yet. Build the module from the repository root; this needs the
`wasm32-unknown-unknown` Rust target and a `wasm-bindgen` CLI of the same version as the crate:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version <the wasm-bindgen version in Cargo.lock>
./liquers-web/examples-web/quickstart/build.sh --release     # add --features records for §10
python3 -m http.server 8090 --directory liquers-web/examples-web/quickstart/dist
```

`build.sh` compiles the crate for wasm32 and runs `wasm-bindgen --target web`. Its `dist/` holds
`liquers_web.js` (an ES module), `liquers_web_bg.wasm` and `liquers_web.d.ts`, and the quick-start
page that uses them; open `http://127.0.0.1:8090/` to see it. Copy the three generated files next to
your own page, and the generated `snippets/` folder too when the build has one (a `records` build
does: the module imports JavaScript helpers from it), keeping its relative paths. No bundler is involved, and the page needs no server beyond static files.

A page imports the module and makes two calls, in this order:

```js
import init, * as liquers from "./liquers_web.js";

await init();            // loads the wasm module
await liquers.init();    // creates the environment
assert(liquers.isInitialized());
```

The default export `init` loads the WebAssembly; `liquers.init()` creates the Liquers environment.
Both return `Promise`s. Calling `liquers.init()` again is harmless: it keeps the environment you have.

## 2. What is in the module

| Purpose | Names |
|---|---|
| Lifecycle | `init` (default export), `liquers.init`, `isInitialized`, `shutdown`, `version` |
| Running queries | `evaluate`, `getAsset`, `Environment`, `Asset`, `State`, `Value` |
| Writing commands | `registerCommand`, `registerCommands`, `validateCommands`, `unregisterCommand`, `describeCommand`, `opaque` |
| Errors | `LiquersError` |
| The query language | `Query`, `Key`, `encodeParam` |
| Stores | `Environment.configureStore`, `Environment.registerStoreObject`, `Store` |
| Record streams (`records` builds) | `RecordBatch`, `RecordSource`, `RecordStream` |

`liquers_web.d.ts` declares all of them, with the shapes of the plain objects they take and return
(`LiquersCommandSpec`, `LiquersStoreConfig`, `LiquersStoreObject`, `LiquersIssueReport`, …), so an
editor with TypeScript support checks your calls. Liquers' own progress messages go to the console.

## 3. A first query

```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
const listing = await liquers.evaluate("commands_doc");   // Markdown: every command you can use
assert(listing.startsWith("# Commands"));
```

`liquers.evaluate` runs a query and resolves with its value as a JavaScript value. The environment
comes with a small set of built-in commands: `to_text`, `to_metadata`, `commands_doc` and the `ns-dep`
introspection commands, plus the `ns-rec` record commands in a `records` build. The large command
libraries of native Liquers — Polars, images, templates — are not in the browser build.

A query is a path of actions: `hello/repeat-3` applies `hello`, then `repeat` with the argument `3`.
`-R/data/note.txt` reads a key from the store, and `-R/data/note.txt/-/to_text` reads it and applies
`to_text` (the `/-/` starts the actions; without it, `to_text` would be part of the key). The query
language is described in [`reference/PROJECT_OVERVIEW.md`](../reference/PROJECT_OVERVIEW.md).

Each result Liquers computes is an *asset*: the value together with its status and its metadata
(type, log, and how it was made).

## 4. The environment

A page has **one** Liquers environment, the one `liquers.init()` creates, and the module-level
functions — `evaluate`, `registerCommand`, … — act on it. `liquers.Environment.global()` returns a
handle to the same environment, which is where its store is configured (§9):

```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "hello", run: () => "hello" });

const env = liquers.Environment.global();
assert.strictEqual(await env.evaluate("hello"), "hello");   // the same environment
assert(env.commandNames().includes("hello"));
```

**Lifecycle.** `liquers.shutdown()` releases the environment and every command registered on it;
afterwards `evaluate` rejects with `not_available` until `liquers.init()` runs again. A page rarely
needs it; tests do (§11).

**Register first, then evaluate.** Commands registered before the first evaluation go straight into
the environment. One registered later still works, but rebuilds the environment: the commands and
the store configuration are kept, the results computed so far are not.

**Isolated environments.** `new liquers.Environment()` makes a separate environment with only the
built-in commands and no store. It is meant for tests that must not see the page's commands. It
cannot have commands registered on it, and its `configureStore` configures the page's environment,
not itself (`WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON`).

## 5. Registering a command

A command is a JavaScript function registered with a declaration: an object with the command's
`name` and its implementation `run`. That is all a declaration needs:

```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "hello", run: () => "hello" });
liquers.registerCommand({
  name: "repeat",
  state: "text",
  arguments: [{ name: "count", type: "int", default: 2 }],
  run: (text, count) => text.repeat(count),
});

assert.strictEqual(await liquers.evaluate("hello/repeat-3"), "hellohellohello");
assert.strictEqual(await liquers.evaluate("hello/repeat"), "hellohello");   // the default
assert.strictEqual(await liquers.evaluate("hello/repeat/to_text"), "hellohello");   // a Rust command after it
```

`hello` declares only what it must, so it has the defaults: no input (it starts a query), no
arguments, the root namespace, its name as its label. `repeat` adds the two things that differ: it
takes the text before it, and one integer argument. The declaration is Liquers' shared command
declaration format, written as an object literal; every field, its default and how it becomes the
command's metadata is in [`reference/COMMAND_DECLARATION.md`](../reference/COMMAND_DECLARATION.md).
What is specific to JavaScript is below.

The names `ns`, `q` and `v` are query instructions, not command names: `ns` is refused, and a
command named `q` or `v` registers but never runs (`COMMAND-NAMES-Q-AND-V-ARE-NOT-RESERVED`). The
namespace `web` is reserved for `liquers-web`'s own commands.

### The input

`state` says whether the command takes the value before it in the query, and in what form:

| `state` | `run` receives first |
|---|---|
| omitted, or `"none"` | nothing: the command starts a query |
| `"value"` | the input value, converted to JavaScript (§7) |
| `"text"` | the input as a string |
| `"state"` | `{ value, metadata, status, log }`: the value with its metadata |

The arguments follow it, in the order they are declared.

### Arguments

An argument is given in the query after the command name, separated by `-`: `repeat-3`. Declared
with a `type`, the text of the query is converted for you — `int`, `float`, `bool`, `string` or
`any`. With a `default`, the argument may be left out; a default is a string, number, boolean or
`null`.

Leave `arguments` out and the names are read from `run`'s parameters. They then have type `any`,
which arrives as the query's text, and no default:

```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "join", run: (left, right) => `${left}+${right}` });

assert.strictEqual(await liquers.evaluate("join-a-b"), "a+b");
assert.strictEqual(liquers.describeCommand("join").argumentsInferred, true);
```

Reading the parameters works only for plain names: a function with a default value, a rest
parameter or destructuring is refused with an error saying so, and so is a bound function. A
minifier renames parameters, which changes only how the arguments are labelled. Declare `arguments`
whenever a type or a default matters.

An argument containing `-`, `/` or a space is written with an escape: `~_` for `-`, `~/` for `/`,
`~.` for a space, `~~` for `~`. `liquers.encodeParam(text)` does it for you, and throws for a
character the query language cannot carry, such as a non-ASCII letter:

```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "echo", arguments: [{ name: "text", type: "string" }], run: (t) => t });

const text = "a-b c/d";
assert.strictEqual(liquers.encodeParam(text), "a~_b~.c~/d");
assert.strictEqual(await liquers.evaluate(`echo-${liquers.encodeParam(text)}`), text);
```

### Asynchronous commands

A command may return a `Promise`; Liquers waits for it. `async: true` states it, and `async: false`
makes a returned `Promise` an error instead of a value:

```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({
  name: "later",
  async: true,
  run: async () => {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return "done";
  },
});

assert.strictEqual(await liquers.evaluate("later"), "done");
```

An async command can evaluate another query with `await liquers.evaluate(…)`. A JavaScript command
has no execution context, though: it cannot write to its result's log, make another query a
dependency of its result, or check for cancellation (`JS-COMMAND-CANNOT-ACCESS-CONTEXT`).

### Metadata

Further fields describe the command. The common ones:

| Field | Meaning |
|---|---|
| `namespace` | a group of commands; a query selects it with `ns-<namespace>` before the command |
| `label`, `doc` | a display name and a description |
| `volatile` | `true` for a result that must never be reused |

```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "hello", run: () => "hello" });
liquers.registerCommand({ name: "shout", namespace: "demo", state: "text", run: (t) => t.toUpperCase() });

assert.strictEqual(await liquers.evaluate("hello/ns-demo/shout"), "HELLO");
```

Any other field of the declaration format, such as `filename` or `cached`, is accepted too.

`describeCommand(name)` returns what was registered for a command in the root namespace, or `null`.
It returns Liquers' own metadata record, so an argument's type is `argument_type` and a default reads
`{ Value: 2 }`, which is not the shape the `.d.ts` declares
(`WEB-DESCRIBE-COMMAND-SHAPE-DIFFERS-FROM-DECLARATION`).

### Registering many at once

`registerCommands(input, impls)` registers a batch, all or nothing. `input` is an array of
declarations, or a JSON or YAML document in the format of
[`command_registry.yaml`](../command_registry.yaml); `impls` gives each command's function by name,
either the function itself or `{ run, state, async }`:

```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
const yaml = `
commands:
  - name: greet
    arguments:
      - { name: who, argument_type: string, default: world }
`;
const report = liquers.registerCommands(yaml, { greet: (who) => `Hello, ${who}!` });

assert.strictEqual(report.hasErrors, false);
assert.strictEqual(await liquers.evaluate("greet"), "Hello, world!");
assert.strictEqual(await liquers.evaluate("greet-Ada"), "Hello, Ada!");
```

If anything in the batch is wrong, nothing is registered and a `LiquersError` of type
`parameter_error` is thrown; its `report` lists every problem, not only the first.
`validateCommands(input, impls)` returns the same report without registering anything, and works
before `liquers.init()`:

```js
import init, * as liquers from "./liquers_web.js";

await init();
const report = liquers.validateCommands([
  { name: "ok", run: () => 1 },
  { name: "missing_run" },
  { name: "odd", state: "sideways", run: (x) => x },
]);

assert.strictEqual(report.errorCount, 2);
assert.deepStrictEqual(report.issues.map((issue) => issue.command.name), ["missing_run", "odd"]);
```

Each issue is `{ severity, message, command }`; `summary` is the message a refusal throws.
`registerCommand(declaration)` is a batch of one, and throws the same way. Registering a batch after
the first evaluation rebuilds the environment once, not once per command.

### Replacing and removing commands

Registering a name that already exists replaces the command, with a `console.warn` naming what was
replaced — a built-in command can be overridden this way. `unregisterCommand(name)` removes your
command and returns whether there was one:

```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "hello", run: () => "hello" });
assert.strictEqual(await liquers.evaluate("hello/to_text"), "hello");

liquers.registerCommand({ name: "to_text", state: "text", run: (t) => `<${t}>` });   // warns
assert.strictEqual(await liquers.evaluate("hello/to_text"), "<hello>");

assert.strictEqual(liquers.unregisterCommand("to_text"), true);
assert.strictEqual(await liquers.evaluate("hello/to_text"), "hello");   // the built-in is back
```

Two cautions. `unregisterCommand` takes a name only, and removes your commands of that name in every
namespace. And before the first evaluation, unregistering the name of a built-in command removes the
built-in, while after it the built-in stays (`WEB-COMMAND-LOOKUP-IGNORES-NAMESPACE`,
`WEB-UNREGISTER-DEPENDS-ON-FIRST-EVALUATION`). Removing a command releases its function.

### Errors from a command

An exception a command throws fails the query. The caller receives a `LiquersError` of type
`execution_error`, whose message starts with the exception's class and message and continues with
its stack. A command that throws a `LiquersError` keeps its type:

```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "fails", run: () => { throw new TypeError("bad input"); } });
liquers.registerCommand({
  name: "lookup",
  arguments: [{ name: "id", type: "string" }],
  run: (id) => { throw new liquers.LiquersError("key_not_found", `no record ${id}`); },
});

await assert.rejects(liquers.evaluate("fails"), (e) =>
  e.errorType === "execution_error" && e.message.startsWith("TypeError: bad input"));
await assert.rejects(liquers.evaluate("lookup-a"), (e) =>
  e.errorType === "key_not_found" && e.message === "no record a");
```

## 6. Running a query

| Call | Resolves with |
|---|---|
| `liquers.evaluate(query)` | the value |
| `liquers.getAsset(query)` | an `Asset`: the result with its status and metadata |
| `env.evaluate(query)`, `env.getAsset(query)` | the same, on an `Environment` handle |
| `env.evaluateQuery(liquers.Query.parse(text))` | the value of an already-parsed query |

A failed query rejects with a `LiquersError` (§8). There is no synchronous evaluation: a page must
never block its event loop.

### Metadata, status and the log

```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "hello", run: () => "hello" });

const asset = await liquers.getAsset("hello");
assert.strictEqual(await asset.status(), "ready");
const state = await asset.get();
assert.strictEqual(state.toJS(), "hello");
assert.strictEqual(state.value().identifier, "Text");
const metadata = state.metadata();
assert.strictEqual(metadata.type_identifier, "Text");
assert.strictEqual(metadata.query, "hello");
assert(Array.isArray(state.log()));
```

`Asset` methods all return `Promise`s: `status()`, `get()` (a `State`, rejecting if the asset
failed), `getAssetInfo()` and `cancel()`. `State.toJS()` gives the value as `evaluate` would;
`state.value()` gives it as a `Value`, with its type `identifier` and `typeName`. `metadata()` is a
plain object; note that its `status` is capitalized (`"Ready"`), while `asset.status()` and
`state.status` are lowercase (`WEB-METADATA-STATUS-CASE-DIFFERS`). The log holds Liquers' own
entries, such as detected dependencies; a JavaScript command cannot add to it yet.

**Cancelling.** `asset.cancel()` exists, but does nothing yet: in the browser a query is evaluated
before `getAsset` resolves, so there is nothing left to cancel (`WEB-CANCELLATION-INERT`).

**Results are reused.** A query evaluated twice is computed once while the environment keeps the
result. A command that must run every time is declared `volatile: true`. Registering a command
after the first evaluation discards the kept results (§4).

### Named results: recipes

A *recipe* gives a query's result a key in the store, so `-R/data/triple.txt` computes it when
nothing is stored there. Recipes live in a `recipes.yaml` in the folder they belong to, and the key
is the folder plus the query's last segment, its file name. Recipes need a store that can list a
folder (§9); this example uses one the page implements:

```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "hello", run: () => "hello" });
liquers.registerCommand({
  name: "repeat", state: "text",
  arguments: [{ name: "count", type: "int", default: 2 }],
  run: (text, count) => text.repeat(count),
});

const files = new Map([
  ["data/recipes.yaml", new TextEncoder().encode("recipes:\n  - query: hello/repeat-3/triple.txt\n")],
]);
const env = liquers.Environment.global();
env.registerStoreObject("files", {
  get: (key) => (files.has(key) ? { data: files.get(key) } : null),
  contains: (key) => files.has(key),
  isDir: (key) => [...files.keys()].some((k) => k.startsWith(key + "/")),
  listdir: (key) => [...files.keys()].filter((k) => k.startsWith(key + "/")).map((k) => k.slice(key.length + 1)),
});
await env.configureStore({ stores: [{ type: "js", prefix: "data", config: { object: "files" } }] });

assert.strictEqual(await liquers.evaluate("-R/data/triple.txt"), "hellohellohello");
```

How recipes are resolved is in [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md).

### Checking a query without running it

`liquers.Query.parse(text)` parses a query and throws a `parse_error` with the position when it is
malformed; `.encode()` gives its canonical text and `.filename` its file name, if it has one:

```js
import init, * as liquers from "./liquers_web.js";

await init();
const query = liquers.Query.parse("hello/repeat-3/triple.txt");
assert.strictEqual(query.filename, "triple.txt");
assert.throws(() => liquers.Query.parse("hello/repeat-~"), (e) => e.errorType === "parse_error");
```

## 7. Values

Values cross between JavaScript and Liquers by kind:

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
| anything else: `Map`, `Set`, `Date`, a class instance, a function | refused with `conversion_error` | — |

An integer outside ±(2^53 − 1) is refused rather than rounded when it goes back to JavaScript. Inside
an array or object three things differ from the top level: NaN and the infinities become the strings
`"NaN"`, `"inf"` and `"-inf"`; and a `Uint8Array` or any other non-plain object is refused, because
JSON has no byte type. Return bytes on their own, or convert them to an array of numbers yourself. A
nested `null` comes back as `null`.

```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "record", run: () => ({ name: "Ada", scores: [1, 2.5, null] }) });
liquers.registerCommand({ name: "bytes", run: () => new Uint8Array([1, 2, 3]) });

assert.deepStrictEqual(await liquers.evaluate("record"), { name: "Ada", scores: [1, 2.5, null] });
assert.deepStrictEqual(await liquers.evaluate("bytes"), new Uint8Array([1, 2, 3]));
```

Because a command's result is converted, a Rust command can read what a JavaScript command
produced, as `hello/to_text` did in §5. A large object costs a few milliseconds to convert (about 5 ms for 1 000
properties), which is invisible at the sizes a page usually handles.

**Opaque objects.** An object Liquers cannot convert is refused, so that nothing becomes opaque by
accident. To pass one from one JavaScript command to another, wrap it with `liquers.opaque(x)`: the
next command receives the same object.

```js
import init, * as liquers from "./liquers_web.js";

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

A Rust command cannot look inside an opaque object, and one cannot be written to a store.

## 8. Errors

Every Liquers error reaches JavaScript as a `LiquersError`. There is one class; the kind of error is
its `errorType`, a string such as `"key_not_found"`, and the groups it belongs to are in `categories`
(`"store"`, `"keyed_asset"`, `"not_found"`). JavaScript has no typed `catch`, so test those:

```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
try {
  await liquers.evaluate("-R/data/missing.txt");
  assert.fail("expected a not-found error");
} catch (e) {
  assert(e instanceof liquers.LiquersError);
  assert.strictEqual(e.errorType, "asset_not_found");   // nothing stored, and no recipe makes it
  assert(e.categories.includes("not_found"));
  assert.strictEqual(e.key, "data/missing.txt");
}
```

| Member | Meaning |
|---|---|
| `errorType` | the kind of error |
| `categories` | the groups the kind belongs to |
| `message` | the message; for a thrown exception, its class, message and stack |
| `query`, `key` | where it happened, when known |
| `position` | `{ offset, line, column }` for a parse error, else `null` |
| `report` | for a refused `registerCommands`, the full report |
| `jsClass`, `jsStack` | `null` on errors from evaluation: the class and stack are in `message` (`LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`) |

To raise one, from a command or a store: `new liquers.LiquersError(errorType, message, query?)`. An
unknown type throws a `TypeError`. It is not a subclass of `Error`, so it has no stack of its own.

Errors you will meet early:

| Situation | `errorType` |
|---|---|
| A command name that is not registered | `action_not_registered` |
| Too many arguments for a command | `too_many_parameters` |
| A key nothing is stored under and no recipe makes | `asset_not_found` |
| A malformed query | `parse_error` |
| An exception inside a JavaScript command | `execution_error` |
| A value that cannot be converted | `conversion_error` |
| Using Liquers before `liquers.init()` or after `shutdown()` | `not_available` |

The complete mapping is in [`reference/JAVASCRIPT_INTEGRATION.md`](../reference/JAVASCRIPT_INTEGRATION.md) §6.

## 9. Stores

A new environment has no store. `env.configureStore(config)` gives it one or more, each answering
for the keys under its `prefix`; the format is the one native Liquers uses
([`reference/STORE_CONFIG_FSD.md`](../reference/STORE_CONFIG_FSD.md)), as an object or a YAML string,
so one document can describe a page and a server. Three types work in the browser:

| `type` | Where the data is | Writes |
|---|---|---|
| `localstorage` | the browser's `localStorage`, under `config.namespace` | yes |
| `http`, `https` | fetched from `config.url_prefix` + the key; `config.keys` lists what exists | no |
| `js` | an object your page implements, registered with `env.registerStoreObject(name, object)` | as implemented |

<!-- example: not run — localStorage and a served data folder exist only on a page -->
```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
const env = liquers.Environment.global();
await env.configureStore({
  stores: [
    { type: "http", prefix: "data", config: { url_prefix: "./data/", keys: ["input.csv"] } },
    { type: "localstorage", prefix: "local", config: { namespace: "myapp" } },
  ],
});

const store = env.store();
await store.set("local/out.txt", new TextEncoder().encode("hello"), {});
assert.strictEqual(await liquers.evaluate("-R/local/out.txt"), "hello");
const csv = await store.get("data/input.csv");                  // a Uint8Array
const info = await store.getAssetInfo("data/input.csv");        // info.media_type === "text/csv"
```

`env.store()` is the configured store, with `get`, `set(key, bytes, metadata)`, `remove`,
`listdir`, `getAssetInfo` (what Liquers makes of a key: media type, file name, whether it is a
folder) and the rest listed in the `.d.ts`. Configuring again replaces the configuration, and
discards results computed against the old one.

**A store your page implements.** Return `{ data }` from `get` for a key and `null` for an absent
one; throw for a failure, or throw a `LiquersError` to say which. The `.d.ts` marks only `get` as
required, but a store that queries read needs `contains` too, and `isDir` so that a missing key is
reported as missing (`WEB-JS-STORE-WITH-ONLY-GET-CANNOT-BE-QUERIED`):

```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
const files = new Map([["notes/a.txt", new TextEncoder().encode("first")]]);
const env = liquers.Environment.global();
env.registerStoreObject("notes", {
  get: (key) => (files.has(key) ? { data: files.get(key) } : null),
  contains: (key) => files.has(key),
  isDir: (key) => [...files.keys()].some((k) => k.startsWith(key + "/")),
  getMetadata: (key) => (files.has(key) ? {} : null),
  set: (key, data) => { files.set(key, data); },
  setMetadata: () => {},
});
await env.configureStore({ stores: [{ type: "js", prefix: "notes", config: { object: "notes" } }] });

assert.strictEqual(await liquers.evaluate("-R/notes/a.txt"), "first");
await env.store().set("notes/b.txt", new TextEncoder().encode("second"), {});
assert(files.has("notes/b.txt"));
await assert.rejects(liquers.evaluate("-R/notes/missing.txt"), (e) => e.categories.includes("not_found"));
```

Any method may return a `Promise`. A method you leave out is reported as unsupported
(`key_not_supported`) when Liquers needs it, rather than answering "empty": here, `remove` or
`listdir`. Recipes need `listdir`, as in §6; the full list of methods is `LiquersStoreObject` in the
`.d.ts`.

**Read-only stores and metadata sections.** Any store entry may say `read_only: true`, which refuses
every write. An entry may also carry a `metadata` section deciding how the store's metadata is kept;
for a page-implemented store that needs five more methods. Both are described in
`liquers-web/README.md` ("Read-only stores and metadata sections") and in
[`reference/STORE_CONFIG_FSD.md`](../reference/STORE_CONFIG_FSD.md).

## 10. Record streams

A build with `--features records` adds the `ns-rec` commands, and record values reach JavaScript
as handles: a table as a `RecordBatch`, a stream of tables as a `RecordSource`. A handle refers to
the data inside the module rather than copying it; call `free()` when you are done with one.

<!-- example: not run — the default build has no record streams; build with --features records -->
```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
const env = liquers.Environment.global();
// `people` is any command whose result converts to records.
const source = await env.evaluate("people/ns-rec/to_record_source");
for await (const batch of await source.stream()) {
  console.log(batch.numRows, batch.columnCopy(0));   // one batch at a time
  batch.free();
}
const all = await source.materialize(10000);           // one batch, refused beyond 10 000 rows
source.free();
```

Column access without copying, chunk listings and the other methods are described in
`liquers-web/README.md` ("Record streams") and the reference §9.

## 11. Testing your commands

Give each test a fresh environment, so that registrations do not leak between tests: shut the
environment down after each test and initialize it again before the next.

```js
import init, * as liquers from "./liquers_web.js";

await init();
async function withEnvironment(test) {
  await liquers.init();
  try {
    await test();
  } finally {
    liquers.shutdown();
  }
}

await withEnvironment(async () => {
  liquers.registerCommand({ name: "hello", run: () => "hello" });
  assert.strictEqual(await liquers.evaluate("hello"), "hello");
});
await withEnvironment(async () => {
  await assert.rejects(liquers.evaluate("hello"), (e) => e.errorType === "action_not_registered");
});
```

A page-implemented store (§9) is the simplest way to give a test its input. The module runs under
Node as well as in a browser, as these examples do, except for `localStorage`.

## 12. Known limitations

What you will notice; the complete list, with the tracking issues, is
[`reference/JAVASCRIPT_INTEGRATION.md`](../reference/JAVASCRIPT_INTEGRATION.md) §10.

- **No templates.** The `ns-template` commands, template commands and programmatic rendering are
  not in the browser build (`WEB-HAS-NO-TEMPLATE-COMMANDS`).
- **No Polars or image commands**; the browser build has the core and record commands only.
- **No context in a command**: a JavaScript command cannot log, declare a dependency or check for
  cancellation.
- **Cancellation does nothing yet.**
- **One environment per page** for commands and stores; `new Environment()` is for isolated
  evaluation only.
- **Late registration** rebuilds the environment and forgets computed results.
- **Command lookup is by name only**: `describeCommand` sees the root namespace, and
  `unregisterCommand` removes every namespace's command of that name.
- An exception's class and stack are part of the error's `message`.
- No IndexedDB store, no read/write store of a Liquers server, no npm package and no generated API
  documentation beyond the `.d.ts`.

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-26 | Created from the implemented `liquers-web`; every example runs under Node. | `WEB-HAS-NO-API-REFERENCE-OR-USER-GUIDE` |
| 2026-10-26 | Nested values: `null` now returns as `null`; why nested bytes are refused. | `WEB-NESTED-NULL-CROSSES-AS-UNDEFINED`, `WEB-NESTED-BYTES-REFUSED-NOT-DEGRADED` |
