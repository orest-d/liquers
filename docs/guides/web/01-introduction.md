---
title: Introduction
status: ready
sources: [reference/PROJECT_OVERVIEW.md, guides/JAVASCRIPT_GUIDE.md]
reviewed: 2026-11-10
---
# 1. Introduction

PURPOSE: The reader has run a first query from JavaScript in the browser and knows the words query, command, store and asset.

Liquers is a universal toolkit for working with data. Think of a Swiss Army knife: many tools,
each useful on its own, folded into one handle so that they are always at hand and fit together.
Liquers has a tool for turning your functions into reusable building blocks, a tool for storing
data, a tool for caching results, a tool for describing which files a folder should contain, a
tool for writing reports, a tool for showing results in a user interface, and more.

The handle that holds these tools together is the *query*: a short piece of text that says how
to make a value. Every tool in Liquers either makes queries possible or does something useful
with them. So the plan of this chapter is simple. You run a query first. Then you meet the tools
one at a time, each as a new thing the same query can do.

In the browser, all of this runs inside the page. The module you import contains Liquers itself,
compiled from Rust to WebAssembly, and your JavaScript functions become commands next to the
ones that come with it.

## 1.1 A first query

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();

liquers.registerCommand({ name: "hello", run: () => "world" });
liquers.registerCommand({
  name: "greet",
  state: "text",
  arguments: [{ name: "greeting", type: "string", default: "Hello" }],
  run: (text, greeting) => `${greeting}, ${text}!`,
});

assert.strictEqual(await liquers.evaluate("hello/greet-Hi"), "Hi, world!");
assert.strictEqual(await liquers.evaluate("hello/greet"), "Hello, world!");
```

Two ordinary JavaScript functions became *commands*: `hello`, which makes a value from nothing,
and `greet`, which takes a value and makes another one. `registerCommand` registers each under
its name, with a small description: `state: "text"` says that `greet` takes the value before it
as text, and `arguments` names its one argument and gives it a default. The text
`hello/greet-Hi` is a query. Read it from left to right, like a path: run `hello`, pass its
result to `greet`, and give `greet` the argument `Hi`.

The two `init` calls at the top are needed once per page. The first loads the WebAssembly, the
second creates the *environment*: the object that holds the commands, the store and the cache,
and in which queries are evaluated. `liquers.evaluate(query)` asks the environment for the result
of a query and resolves with the value, as a JavaScript value.

The same code as a complete page looks like this. Save it as an `.html` file and open it through
any web server, or paste the script into the console of a page:

```html
<!doctype html>
<meta charset="utf-8">
<title>A first query</title>
<pre id="out">loading…</pre>
<script type="module">
  import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

  await init();
  await liquers.init();
  liquers.registerCommand({ name: "hello", run: () => "world" });
  liquers.registerCommand({
    name: "greet",
    state: "text",
    arguments: [{ name: "greeting", type: "string", default: "Hello" }],
    run: (text, greeting) => `${greeting}, ${text}!`,
  });
  document.getElementById("out").textContent = await liquers.evaluate("hello/greet-Hi");
</script>
```

That is the whole mechanism. A query is a chain of commands, and a command is a function. What
makes it worth having is everything that follows from writing the chain down as text.

## 1.2 One text, many uses

A function call disappears once it returns. A query does not, because it is a piece of data. The
same text is, at the same time:

- **an instruction**: Liquers can evaluate it;
- **a name**: the result of `hello/greet-Hi` can be remembered under exactly that text, so asking
  again does not compute again;
- **a link**: a query contains only characters that are safe in a URL, so it fits into the
  address of a page, into `location.hash`, into a bookmark;
- **a record**: the result carries the query that made it, so you can always say where a value
  came from;
- **something you can check**: a query can be parsed and inspected without running anything.

The last three matter in a page more than anywhere else. A view whose content is the value of a
query can be shared by sharing its address, and a server that runs Liquers answers the very same
text at `/liquer/q/hello/greet-Hi` (chapter 12).

## 1.3 The commands that are already there

You do not have to write every command. The module comes with a set of its own, written in Rust,
and they mix freely with yours:

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({
  name: "people",
  run: () => [
    { name: "Ann", age: 31, city: "Oslo" },
    { name: "Bob", age: 25, city: "Rome" },
    { name: "Cy", age: 40, city: "Oslo" },
  ],
});

const names = await liquers.evaluate("people/ns-rec/to_record/eq-city-Oslo/sort-age/select_columns-name/to_json");
assert.deepStrictEqual(names, [{ name: "Ann" }, { name: "Cy" }]);

const listing = await liquers.evaluate("commands_doc");   // Markdown: every command you can use
assert(listing.startsWith("# Commands"));
```

`people` is yours. `to_record`, `eq`, `sort`, `select_columns` and `to_json` are commands of the
*record* library: they read the array as a table, keep the rows whose `city` is `Oslo`, sort
them and keep one column. `ns-rec` selects their *namespace*, the group they are registered in.
`commands_doc` lists every command the environment has.

The browser module has a smaller library than a native program. It has the core commands, the
record commands, the template commands and the user-interface commands. Polars data frames and
images are not in it (section 1.9, and chapter 9 for what to do instead).

## 1.4 The store

A query can start from stored data instead of a command. The *store* is where Liquers keeps
data: a set of files, each under a *key* that looks like a path.

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "data" }] });

const csv = "region,amount\nEU,10\nUS,20\nEU,5\n";
await env.store().set("data/sales.csv", new TextEncoder().encode(csv), {});

const rows = await liquers.evaluate("-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/nrows");
assert.strictEqual(rows, 2);
```

`-R/data/sales.csv` reads the key `data/sales.csv` from the store, and `/-/` starts the commands
that are applied to it. A browser has no file system, so a store in a page is one of four
things: memory, the browser's `localStorage`, files fetched over HTTP, or an object your page
implements. Chapter 6 covers them.

## 1.5 Assets and caching

Each result Liquers computes is an *asset*: the value together with its status and a description
of it, the *metadata*. The environment keeps assets, so a query evaluated twice is computed
once:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
let calls = 0;
liquers.registerCommand({ name: "slow_report", run: () => { calls += 1; return "the report"; } });

assert.strictEqual(await liquers.evaluate("slow_report"), "the report");
assert.strictEqual(await liquers.evaluate("slow_report"), "the report");
assert.strictEqual(calls, 1);                       // computed once

const asset = await liquers.getAsset("slow_report");
const state = await asset.get();
assert.strictEqual(await asset.status(), "ready");
assert.strictEqual(state.metadata().query, "slow_report");
assert.strictEqual(state.metadata().type_identifier, "Text");
```

`getAsset` gives you the asset instead of only its value. Its metadata says which query made the
value, what type it has, what it depends on and what was logged while it was made (chapter 7).

## 1.6 Recipes

A *recipe* gives the result of a query a key, so that the result reads like a file. Recipes are
written in a file called `recipes.yaml`, in the folder they describe:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "shout", state: "text", run: (text) => text.toUpperCase() });

const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "data" }] });
const text = (s) => new TextEncoder().encode(s);
await env.store().set("data/note.txt", text("milk"), {});
await env.store().set("data/recipes.yaml", text("recipes:\n  - query: -R/data/note.txt/-/shout/loud.txt\n"), {});

assert.strictEqual(await liquers.evaluate("-R/data/loud.txt"), "MILK");
```

Nobody stored `data/loud.txt`. The recipe says that the file `loud.txt` of the folder `data` is
the result of a query, so reading the key computes it. Whoever reads the key does not need to
know whether it is a stored file or a computed one (chapter 7).

## 1.7 Templates

A *template* is text with queries in it. Rendering replaces each tag by a value:

{{=<% %>=}}
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "count", run: () => 3 });

const text = await liquers.expand("{{name}} has {{$ count}} items.", { name: "Ann" });
assert.strictEqual(text, "Ann has 3 items.");
```

`{{name}}` reads a name from the data you pass, and `{{$ count}}` is replaced by the value of the
query `count`. Templates produce Markdown, HTML or any other text from the results of queries
(chapter 10).
<%={{ }}=%>

## 1.8 User interfaces, servers and more

- **User interfaces.** `liquers.mountUI(element, { query })` puts a Liquers user interface into
  an element of your page. What it shows is the value of a query, and you can add kinds of
  elements written in JavaScript (chapter 11).
- **A server.** A Liquers server, started from a Rust or a Python program, answers queries over
  HTTP. A page can use it for the work a browser should not do: large data, Polars, images,
  files on disk (chapter 12).
- **The other languages.** A query means the same thing in a Rust program, in a Python notebook
  and in your page. The sister guides, *Liquers for Rust* and *Liquers for Python*, have the
  same chapters as this one.

## 1.9 What is different in the browser

The browser module is the same code as native Liquers, built for a different machine. The
differences follow from that machine: one thread, no file system, and a download before anything
runs.

| | Native Liquers (Rust, Python) | In the browser |
|---|---|---|
| Where it runs | a program on a computer or a server | inside the page, as WebAssembly |
| Environments | as many as you create | one per page (chapter 8) |
| Execution | a queue of jobs on several threads | one thread: a query runs to its end before `evaluate` resolves, and nothing runs in parallel (chapter 7) |
| Cancelling | stops a running evaluation | does nothing yet |
| Commands in the library | core, records, templates, Polars, images, search, user interface | core, records, templates, user interface |
| Stores | file system, memory, cloud storage | memory, `localStorage`, files over HTTP (read-only), an object the page implements (chapter 6) |
| Cached results | in memory and in the store, kept fresh when inputs change | in memory, for the life of the page; a write to the store does not refresh a computed result yet |
| Registering a command after the first evaluation | Rust: not possible, commands go on the builder. Python: rebuilds the environment | rebuilds the environment and forgets computed results |
| Values | Rust types | converted to and from JavaScript values (chapter 4) |
| User interface | egui on the desktop, HTML in the browser | HTML, with elements you can define in JavaScript |
| Serving queries over HTTP | yes | no: a page is a client of a server, not a server |
| Search and agent memory | yes | no (chapter 13) |
| Installing | a crate, a Python package | two files to host: a script and a `.wasm` file of several megabytes (chapter 2) |

Section 15.6 lists the known limitations one by one.

## 1.10 Where Liquers fits in a page, and where it does not

Liquers in the browser is at its best when a page derives many small results from a few inputs
and shows them: a report over a CSV file, a dashboard, an interactive document, a tool that works
on the user's own data without sending it anywhere. The page gets caching, a record of how each
value was made, and views that can be shared as links.

It is the wrong tool for some jobs:

- large data: everything is held in the page's memory, and one thread does all the work;
- computations that take seconds: the page does not repaint while a query runs, so move them
  to a server, or make the command asynchronous and let it wait on something else;
- pages where every kilobyte counts: the module is a download of several megabytes.

Liquers is at version 0.1. It works and it is tested, but its interfaces can still change
between releases.

## 1.11 Where to go next

Read chapters 2 and 3 next: loading and hosting the module, and the query language. After that
the chapters can be read in any order.

| If you want to | Read |
|---|---|
| Host the module with your own page | [2. Installing and setting up](02-installing.md) |
| Write your own commands | [5. Commands](05-commands.md) |
| Keep data in the browser, or read it from your site | [6. Stores](06-stores.md) |
| Understand caching and write recipes | [7. Recipes, assets and caching](07-assets.md) |
| Work with tables | [9. Working with data](09-data.md) |
| Produce text and HTML from results | [10. Templates](10-templates.md) |
| Show results in the page | [11. User interfaces](11-ui.md) |
| Call a Liquers server | [12. Serving and embedding](12-serving.md) |
| Find out why something fails | [15. Troubleshooting](15-troubleshooting.md) |
