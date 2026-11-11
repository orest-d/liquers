---
title: The environment
status: ready
sources: [reference/ENVIRONMENT_CONFIG.md, guides/JAVASCRIPT_GUIDE.md]
reviewed: 2026-11-10
---
# 8. The environment

PURPOSE: The reader can build an environment with the commands, stores and recipes an application needs.

The *environment* is the object that holds the commands, the store and the kept results, and in
which queries are evaluated. In a native program you build one with a builder, or several. In a
page there is one, and it is set up by a few calls in the right order.

## 8.1 One environment per page

`liquers.init()` creates the environment of the page. The functions of the module, `evaluate`,
`registerCommand`, `expand` and the rest, all act on it. `liquers.Environment.global()` returns a
handle to the same environment, which is where its store is configured:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
assert.strictEqual(liquers.isInitialized(), false);
await liquers.init();
assert.strictEqual(liquers.isInitialized(), true);
liquers.registerCommand({ name: "hello", run: () => "hello" });

const env = liquers.Environment.global();
assert.strictEqual(await env.evaluate("hello"), "hello");       // the same environment
assert(env.commandNames().includes("hello"));
assert(env.commandNames().includes("to_text"));                 // a command of the library
assert(liquers.version().startsWith("liquers-web"));
```

Calling `liquers.init()` again is harmless: it keeps the environment you have. Using Liquers
before `liquers.init()` fails with the error type `not_available`.

"One per page" means one per document that loaded the module: each tab and each `iframe` that
imports `liquers_web.js` has its own module instance, its own environment and its own copy of
the WebAssembly memory. They share nothing except what they read from the
same `localStorage` or the same server.

## 8.2 Setting it up

A page sets up its environment once, at the start, in this order:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

// 1. Load the module and create the environment.
await init();
await liquers.init();

// 2. Register every command, in one batch.
liquers.registerCommands([
  { name: "hello", run: () => "world" },
  { name: "shout", namespace: "app", state: "text", run: (text) => text.toUpperCase() },
]);

// 3. Configure the store.
const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "data" }] });

// 4. Only now evaluate.
assert.strictEqual(await liquers.evaluate("hello/ns-app/shout"), "WORLD");
```

Put these steps into one function of your application, `await` it before anything else uses
Liquers, and keep all registrations in it. The order matters because of what the next section
describes.

There is no configuration file for the environment of a page. What a native program reads from
`liquers.yaml`, a page says in code: the `store` section is the argument of `configureStore`,
and the commands are registered by the page. The execution settings of a native environment, the
number of parallel jobs and the size of the cache, have no counterpart here, since a page
evaluates inline on one thread (section 7.5).

## 8.3 Registering late rebuilds the environment

The environment is built when it is first needed: at the first evaluation. Commands registered
before that go straight in. A command registered *after* the first evaluation still works, but
the environment has to be built again to take it in:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
let calls = 0;
liquers.registerCommand({ name: "report", run: () => { calls += 1; return "the report"; } });
const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "data" }] });
await env.store().set("data/note.txt", new TextEncoder().encode("milk"), {});

await liquers.evaluate("report");
assert.strictEqual(calls, 1);

liquers.registerCommand({ name: "late", run: () => "late" });       // after the first evaluation

assert.strictEqual(await liquers.evaluate("late"), "late");          // it works
await liquers.evaluate("report");
assert.strictEqual(calls, 2);                                        // but kept results were forgotten
assert.strictEqual(await env.store().contains("data/note.txt"), false);   // and the memory store is new
```

A rebuild keeps the commands and the store *configuration*. It forgets every kept result, and
it creates the stores again from the configuration, so a `memory` store starts empty. Data in
`localstorage`, behind `http` or in a `js` store is not touched, because it lives outside the
environment.

So register first. When a page really must add commands later, for example when it loads a
plug-in, register them in one `registerCommands` batch, which rebuilds once, and keep data that
must survive out of `memory` stores.

## 8.4 Shutting down, and isolated environments

`liquers.shutdown()` releases the environment and every command registered on it. Afterwards
`evaluate` rejects with `not_available` until `liquers.init()` runs again, which creates a fresh
environment with only the library's commands. A page rarely needs this. Tests do: it is how
each test gets a clean start (chapter 14).

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "hello", run: () => "hello" });
assert.strictEqual(await liquers.evaluate("hello"), "hello");

liquers.shutdown();
await assert.rejects(liquers.evaluate("hello"), (e) => e.errorType === "not_available");

await liquers.init();
await assert.rejects(liquers.evaluate("hello"), (e) => e.errorType === "action_not_registered");
```

`new liquers.Environment()` makes a separate environment that has only the library's commands
and no store. It can evaluate queries, and nothing else: commands cannot be registered on it,
and its `configureStore` configures the page's environment, not itself. It is useful for
evaluating a query of library commands without the page's commands in the way, and not as a
second environment for an application.

## 8.5 Compared with a native environment

| | Native (Rust, Python) | Page |
|---|---|---|
| How many | any number | one, plus isolated ones for library-only evaluation |
| Built by | a builder, or a configuration file | `liquers.init()`, then registrations and `configureStore` |
| Commands | registered on the builder, by feature groups | the library's set is fixed by the build; yours are registered at run time |
| Late registration | not possible in Rust; a rebuild in Python | a rebuild |
| Execution | a queue with a number of parallel jobs | inline, one at a time |
| Store configuration | a `store` section, with file and cloud storage | the same section, with the browser's storage types |
| Default namespaces, cache settings | configurable | not configurable |
| Shutdown | stops running evaluations and releases stores | releases the environment and its commands |
