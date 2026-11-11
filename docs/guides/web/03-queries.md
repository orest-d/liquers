---
title: Queries
status: ready
sources: [reference/PROJECT_OVERVIEW.md, guides/QUERY_ESCAPING_GUIDE.md, guides/JAVASCRIPT_GUIDE.md, reference/PIPELINES.md]
reviewed: 2026-11-10
---
# 3. Queries

PURPOSE: The reader can read and write a query, and run one from JavaScript in the browser.

The query is the one thing in Liquers that you cannot do without, and the one thing that is the
same in every language. This chapter first describes the query language itself, in the words
shared by all three guides. Sections 3.7 to 3.11 then show how to work with queries from
JavaScript.

The examples of the first sections use small commands of your own, `hello`, `greet` and `repeat`,
like those of chapters 1 and 5, and a few commands of the library. Some of them name commands of
the Polars library (`ns-pl`), which a browser does not have; the notation is the same for the
record commands (`ns-rec`) that it does have.

{{> ../_shared/queries.md}}

## 3.7 Running a query

There are four calls that run a query. They differ in what you pass and what you get:

| Call | Resolves with |
|---|---|
| `liquers.evaluate(query)` | the value, as a JavaScript value |
| `liquers.getAsset(query)` | an `Asset`: the result with its status and metadata (chapter 7) |
| `env.evaluate(query)`, `env.getAsset(query)` | the same, on an `Environment` handle (chapter 8) |
| `env.evaluateQuery(liquers.Query.parse(text))` | the value of a query that is already parsed |

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

const asset = await liquers.getAsset("hello/greet-Hi");
assert.strictEqual((await asset.get()).toJS(), "Hi, world!");

const env = liquers.Environment.global();
const query = liquers.Query.parse("hello/greet-Hi");
assert.strictEqual(await env.evaluateQuery(query), "Hi, world!");
```

All of them return a `Promise`. There is no synchronous evaluation, because a page must never
block its event loop. A query that fails rejects the promise with a `LiquersError`, which says
what kind of error it is and where in the query it happened:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "hello", run: () => "world" });

try {
  await liquers.evaluate("hello/nosuch");
  assert.fail("expected an error");
} catch (e) {
  assert(e instanceof liquers.LiquersError);
  assert.strictEqual(e.errorType, "action_not_registered");
  assert.strictEqual(e.position.column, 7);          // where `nosuch` starts
}
```

Chapter 15 describes the error object.

## 3.8 Looking inside a query

`liquers.Query.parse(text)` turns the text into a `Query` object without running anything. It
throws a `parse_error` when the text is not a query. `encode()` gives the text back in its
canonical form, and `filename` is the file name at the end, if there is one:

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
const query = liquers.Query.parse("-R/data/sales.csv/-/ns-rec/to_record/head-2/top.csv");
assert.strictEqual(query.encode(), "-R/data/sales.csv/-/ns-rec/to_record/head-2/top.csv");
assert.strictEqual(query.filename, "top.csv");

const key = liquers.Key.parse("data/sales.csv");
assert.strictEqual(key.filename, "sales.csv");
assert.strictEqual(key.parent().encode(), "data");

assert.throws(() => liquers.Query.parse("hello/greet-~"), (e) =>
  e.errorType === "parse_error" && e.position.column === 13);
```

Parsing needs only the first `init()`: no environment is involved. To see the steps of a query,
convert it to a *pipeline*, its form as data (section 3.10):

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
const pipeline = liquers.queryToPipeline("-R/data/sales.csv/-/ns-rec/head-2/top.csv");

assert.strictEqual(pipeline.filename, "top.csv");
const [file, head] = pipeline.elements;
assert.strictEqual(file.resource, "data/sales.csv");
assert.strictEqual(head.action, "head");
assert.strictEqual(head.ns, "rec");
assert.strictEqual(head.parameters[0].value, "2");
assert.deepStrictEqual(head.pos, { offset: 27, line: 1, column: 28 });   // where `head` is in the text
```

Each element and each argument carries its position in the text (`pos`), which is what an editor
needs to underline the right part of a query.

## 3.9 Building a query from values

A value that goes into a query as an argument may contain `-`, `/` or a space, which have a
meaning in a query. Never paste such a value into the text by hand.
`liquers.encodeParam(text)` writes one argument with its escapes, for any string:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "echo", arguments: [{ name: "text", type: "string" }], run: (t) => t });

const text = "a-b c/d";                       // from a form field, say
assert.strictEqual(liquers.encodeParam(text), "a~_b~.c~/d");
assert.strictEqual(await liquers.evaluate(`echo-${liquers.encodeParam(text)}`), text);
```

For more than one argument, a `QueryEditor` is safer. It knows the commands of the environment,
so it sets arguments by name, checks every edit, and escapes for you:

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

const editor = new liquers.QueryEditor("hello");
const at = editor.action("greet");                  // appends an action, returns its index
editor.set(at, "greeting", "Good morning - all");
assert.strictEqual(editor.query(), "hello/greet-Good~.morning~.~_~.all");

assert.throws(() => editor.set(at, "greting", "Hi"));       // no such argument
assert.throws(() => editor.action("nosuch"));               // no such command
assert.strictEqual(editor.query(), "hello/greet-Good~.morning~.~_~.all");   // a refused edit changes nothing
editor.free();

assert.strictEqual(await liquers.evaluate("hello/greet-Good~.morning~.~_~.all"), "Good morning - all, world!");
```

The editor takes the index of an action rather than returning a sub-editor: `set`,
`pushParameter`, `link`, `unset` and `setNamespace` edit the action at that index, and
`resource`, `insert`, `remove`, `moveTo` and `setFilename` change the query's elements.
`editor.free()` releases the editor's memory inside the module; call it when you are done
(section 4.5).

## 3.10 Pipelines: queries as data

A *pipeline* is the structured form of a query: a plain array with one entry per step. An entry
is a piece of query text, or an object naming an action and its arguments. In a pipeline an
argument is a JSON value, so nothing needs escaping, and an argument can be something a query
cannot spell, such as an object.

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

const pipeline = ["hello", { action: "greet", arguments: { greeting: "a-b c" } }];
assert.strictEqual(await liquers.evaluatePipeline(pipeline), "a-b c, world!");
assert.strictEqual(liquers.pipelineToQuery(pipeline), "hello/greet-a~_b~.c");
```

`evaluatePipeline` runs a pipeline as `evaluate` runs a query. `pipelineToQuery` gives the query
text when there is one, and throws a `parameter_error` when an argument has no query form.
`queryToPipeline` goes the other way (section 3.8). A pipeline is plain data, so it can be kept
as JSON, sent to a server, or built by a form. `new liquers.PipelineEditor(pipeline)` edits one
with the same methods as the `QueryEditor`.

A pipeline can also define a command (section 5.9).

## 3.11 Queries in the address of a page

A query fits into a URL, so the state of a view can live in the address of the page. This sketch
shows the value of the query after the `#`, and evaluates again when the address changes. It
needs a page, so the tests do not run it:

<!-- example: not run — it needs the page's location and document -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "hello", run: () => "world" });

async function show() {
  const query = location.hash.slice(1) || "hello";
  const out = document.getElementById("out");
  try {
    liquers.Query.parse(query);                       // refuse a malformed address early
    out.textContent = String(await liquers.evaluate(query));
  } catch (e) {
    out.textContent = `${e.errorType}: ${e.message}`;
  }
}
addEventListener("hashchange", show);
await show();
```

A link to `page.html#hello` now shows the same view to whoever opens it. Remember what that
means: whoever writes the link chooses the query, and a query can call any registered command
and read any key of the store. In a page this is the user's own browser and data, so the risk is
small, but do not register a command that does something a link should not be able to trigger.
