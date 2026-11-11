---
title: Testing
status: ready
sources: [guides/JAVASCRIPT_GUIDE.md, guides/UNITTEST_GUIDE.md]
reviewed: 2026-11-10
---
# 14. Testing

PURPOSE: The reader can test commands and queries in JavaScript in the browser.

The browser module also runs under Node. So most tests of a page's commands, queries, recipes
and templates need no browser: they are ordinary Node tests, fast and easy to run anywhere. A
browser is needed only for what Node does not have, the document and `localStorage`.

## 14.1 Three levels

| Level | What it checks | Needs |
|---|---|---|
| Your functions alone | the logic of a command, called as a plain function | nothing from Liquers |
| Queries in an environment | commands registered and combined, recipes, templates, stores | the module, under Node |
| The page | mounting a user interface, `localStorage`, the files as you host them | a browser, driven by a tool such as Playwright |

Write most tests at the first two levels. A command is a function, so the first level is plain
unit testing. The second is where this chapter helps.

## 14.2 A test file for Node

Node cannot fetch a file URL, so under Node the WebAssembly is passed to `init` as bytes. The
rest is the same as in a page. This is a complete test file for Node's built-in test runner,
with the module's files copied next to it:

<!-- example: not run — a test file for Node's test runner, with imports of its own -->
```js
import assert from "node:assert";
import { readFile } from "node:fs/promises";
import { afterEach, beforeEach, test } from "node:test";
import init, * as liquers from "./liquers_web.js";

// Node cannot fetch a file, so the WebAssembly is passed as bytes.
await init({ module_or_path: await readFile(new URL("./liquers_web_bg.wasm", import.meta.url)) });

// The commands under test. In a real project they are imported from the application.
function registerAppCommands() {
  liquers.registerCommand({ name: "shout", state: "text", run: (text) => text.toUpperCase() });
}

beforeEach(async () => {
  await liquers.init();          // a fresh environment for each test
  registerAppCommands();
});
afterEach(() => liquers.shutdown());

test("shout makes capital letters", async () => {
  liquers.registerCommand({ name: "hello", run: () => "hello" });
  assert.strictEqual(await liquers.evaluate("hello/shout"), "HELLO");
});

test("a command of another test is gone", async () => {
  await assert.rejects(liquers.evaluate("hello"), (e) => e.errorType === "action_not_registered");
});
```

Run it with `node --test`. The file must be an ES module: name it `.mjs`, or set
`"type": "module"` in `package.json`. Import the module from a local copy in tests, not from a
web address: a test should not depend on the network, and Node does not import from `https:`
without extra flags.

To make this possible, write the registration of your commands as a function that the page and
the tests both call, as `registerAppCommands` stands for here.

## 14.3 A fresh environment for each test

Commands and kept results belong to the environment, and a page has one. Without care, a
command registered by one test is visible in the next, and a result computed by one test is
reused by another. `liquers.shutdown()` followed by `liquers.init()` gives a clean start:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

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

`init()`, the loading of the WebAssembly, is done once for the whole test file. Only the
environment is created anew.

## 14.4 Giving a test its input

A `memory` store is the simplest way to give a test its files, and to look at what a query
wrote:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "shout", state: "text", run: (text) => text.toUpperCase() });

const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "data" }] });
const bytes = (s) => new TextEncoder().encode(s);
await env.store().set("data/note.txt", bytes("milk"), {});
await env.store().set("data/recipes.yaml", bytes("recipes:\n  - query: -R/data/note.txt/-/shout/loud.txt\n"), {});

assert.strictEqual(await liquers.evaluate("-R/data/loud.txt"), "MILK");
assert.strictEqual(await env.store().contains("data/loud.txt"), true);     // the recipe stored its result
liquers.shutdown();
```

Register the commands before you configure and fill the store. A registration after the first
evaluation rebuilds the environment and empties a `memory` store (section 8.3), which in a test
shows up as a confusing "not found".

To test code that reads from an `http` store or from a store of your own, a `js` store over a
`Map` (section 6.5) stands in for it, and lets the test count the calls.

## 14.5 Useful assertions

A test of a query usually asserts one of four things:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
let calls = 0;
liquers.registerCommand({ name: "report", run: () => { calls += 1; return { total: 3 }; } });
liquers.registerCommand({ name: "fails", run: () => { throw new RangeError("too large"); } });

// 1. The value.
assert.deepStrictEqual(await liquers.evaluate("report"), { total: 3 });

// 2. The kind of error, not its text.
await assert.rejects(liquers.evaluate("fails"), (e) => e.errorType === "execution_error");
await assert.rejects(liquers.evaluate("report/nosuch"), (e) => e.errorType === "action_not_registered");

// 3. What the result is and what it was made from.
const state = await (await liquers.getAsset("report")).get();
assert.strictEqual(state.metadata().type_identifier, "Object");
assert(state.metadata().dependencies.length > 0);

// 4. That a result was reused.
await liquers.evaluate("report");
assert.strictEqual(calls, 1);
```

Assert on `errorType` and `categories` rather than on the message. The messages are written for
people and change between versions.

## 14.6 Checking without running

Three checks run nothing, so they are cheap enough to apply to every query, declaration and
template an application contains:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "count", run: () => 3 });

// Every query the application uses parses.
for (const query of ["count", "count/to_text", "-R/data/sales.csv/-/ns-rec/to_record/head-5"]) {
  liquers.Query.parse(query);
}

// Every declaration is well formed.
const report = liquers.validateCommands([{ name: "ok", run: () => 1 }, { name: "broken" }]);
assert.strictEqual(report.errorCount, 1);
assert.strictEqual(report.issues[0].command.name, "broken");
```

{{=<% %>=}}
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "count", run: () => 3 });

// Every template names only commands that exist.
assert.deepStrictEqual(await liquers.validateTemplate("{{$ count}} items"), []);
const problems = await liquers.validateTemplate("{{$ cuont}} items");
assert.strictEqual(problems.length, 1);
```
<%={{ }}=%>

`validateTemplate` checks the queries of a template against the commands that are registered,
so it finds a misspelled command without rendering anything.

## 14.7 Tests that need a browser

Three things need a real browser: a mounted user interface, `localStorage`, and the loading of
the module itself, with the server, the addresses and the headers you really use. A browser
automation tool such as Playwright opens the page and reads what it shows.

Keep these tests few. One that loads the page as it is deployed and evaluates one query checks
the whole chain of section 2.3: the script is found, the `.wasm` file is found next to it and is
served with the right type, and the environment starts. It is the test that catches a broken
deployment, which no test under Node can.

## 14.8 How the examples of this guide are tested

The examples of this guide are tested the way this chapter recommends. A script of the
repository reads every chapter, takes each JavaScript example, and runs it under Node in a fresh
instance of the module, with `init`, `liquers` and `assert` supplied in place of the import
line. An example that needs a page is marked in the source of the chapter and skipped, and the
text says so. That is why you can trust an example to run as printed.
