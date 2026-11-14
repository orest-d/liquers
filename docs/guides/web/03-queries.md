---
title: Queries
status: ready
sources: [reference/PROJECT_OVERVIEW.md, guides/QUERY_ESCAPING_GUIDE.md, guides/JAVASCRIPT_GUIDE.md, reference/PIPELINES.md]
reviewed: 2026-11-14
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

## 3.9 Building and editing a query

A value that goes into a query as an argument may contain `-`, `/` or a space, which have a
meaning in a query. Never paste such a value into the text by hand.

Use a `QueryEditor`. It knows the commands of the environment, so it sets arguments by name,
takes each value as the JavaScript value it is, writes it into the query correctly, and checks
every edit:

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();

const region = "North-West / 2";                    // from a form field, say
const editor = new liquers.QueryEditor("-R/data/sales.csv");
editor.action("to_record", "rec");                  // a command, and its namespace
let at = editor.action("eq", "rec");                // appends an action, returns its index
editor.set(at, "column", "region");
editor.set(at, "value", region);                    // a text as it is; the editor escapes it
at = editor.action("sort", "rec");
editor.set(at, "column", "amount");
editor.set(at, "ascending", false);                 // a boolean as a boolean
at = editor.action("head", "rec");
editor.set(at, "n", 10);                            // a number as a number
assert.strictEqual(editor.query(),
  "-R/data/sales.csv/-/ns-rec/to_record/eq-region-North~_West~.~/~.2/sort-amount-false/head-10");

const before = editor.query();
assert.throws(() => editor.set(at, "count", 3));    // head has no such argument
assert.throws(() => editor.action("nosuch"));       // no such command
assert.strictEqual(editor.query(), before);         // a refused edit changes nothing
editor.free();
```

Pass numbers and booleans as they are, not as text. You do not need to know how a query spells
`false` or a negative number; the editor does. `set` takes the name of an argument and puts the
value in the right position. `pushParameter(at, value)` appends the next positional argument
instead, which is what you use for a command that takes any number of arguments.

The same editor changes a query that already exists. Give the constructor the query, and edit
its steps by their position:

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();

const editor = new liquers.QueryEditor("-R/data/sales.csv/-/ns-rec/to_record/sort-amount/head-10");
// The steps are counted from 0: the file, to_record, sort, head.
editor.set(3, "n", 3);
editor.set(2, "ascending", false);
assert.strictEqual(editor.query(), "-R/data/sales.csv/-/ns-rec/to_record/sort-amount-false/head-3");

editor.unset(2, "ascending");                       // back to the default
assert.deepStrictEqual(editor.unsetArguments(2), ["ascending", "nulls_last"]);

editor.remove(3);                                   // drop the step
editor.setFilename("sorted.csv");
assert.strictEqual(editor.query(), "-R/data/sales.csv/-/ns-rec/to_record/sort-amount/sorted.csv");
editor.free();
```

The editor takes the index of an action rather than returning a sub-editor: `set`,
`pushParameter`, `link`, `unset` and `setNamespace` edit the action at that index, and
`resource`, `insert`, `remove`, `moveTo` and `setFilename` change the query's elements.
`unsetArguments(at)` names the arguments of a step that still have their defaults, which is
what a form for editing a query is built from. `editor.free()` releases the editor's memory
inside the module; call it when you are done (section 4.5).

For a single value, when you write the rest of the query yourself, there is the older way:
`liquers.encodeParam(text)` writes one argument with its escapes. It knows nothing about the
command, so it takes text only, and the position of the argument is yours to get right:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "echo", arguments: [{ name: "text", type: "string" }], run: (t) => t });

const text = "a-b c/d";
assert.strictEqual(liquers.encodeParam(text), "a~_b~.c~/d");
assert.strictEqual(await liquers.evaluate(`echo-${liquers.encodeParam(text)}`), text);
```

Prefer the editor.

API reference: [`QueryEditor`](https://orest-d.github.io/liquers/api/js/classes/QueryEditor.html) and
[`encodeParam`](https://orest-d.github.io/liquers/api/js/functions/encodeParam.html).

## 3.10 Pipelines: queries as data

{{> ../_shared/pipelines.md}}

### Running a pipeline in a page

In JavaScript a pipeline is a plain array. `evaluatePipeline` runs it as `evaluate` runs a
query:

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "data" }] });
const bytes = (s) => new TextEncoder().encode(s);
await env.store().set("data/sales.csv", bytes("region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n"), {});

const pipeline = [
  "-R/data/sales.csv/-/ns-rec/to_record",
  { action: "eq", ns: "rec", parameters: ["region", "EU"] },
  { action: "nrows", ns: "rec" },
];
assert.strictEqual(await liquers.evaluatePipeline(pipeline), 2);

// A pipeline turns into a query, and a query into a pipeline.
const query = liquers.pipelineToQuery(pipeline);
assert.strictEqual(query, "-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/nrows");
const steps = liquers.queryToPipeline(query).elements;      // how to look inside a query
assert.strictEqual(steps[0].resource, "data/sales.csv");
assert.strictEqual(steps[2].action, "eq");
assert.deepStrictEqual(steps[2].parameters.map((p) => p.value), ["region", "EU"]);
```

`queryToPipeline` answers an object with the steps in `elements`, each with its place in the
query text. Both forms, the bare array and this object, are accepted wherever a pipeline is.

Here is a pipeline that no query can replace. Its first step is a value written in place, and
one of its arguments is a list:

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();

const structured = [
  { value: [{ region: "EU", amount: 120 }, { region: "US", amount: 80 }] },
  { action: "to_record", ns: "rec" },
  { action: "is_in", ns: "rec", arguments: { column: "region", values: ["EU", "ASIA"] } },
  { action: "nrows", ns: "rec" },
];
assert.strictEqual(await liquers.evaluatePipeline(structured), 1);
assert.throws(() => liquers.pipelineToQuery(structured), /not representable as a query/);

// Editing: the same methods as the query editor, and values of any shape.
const editor = new liquers.PipelineEditor(structured);
editor.set(2, "values", ["US"]);
editor.remove(3);
const at = editor.action("select_columns", "rec");
editor.pushParameter(at, "amount");
editor.action("to_json", "rec");
assert.deepStrictEqual(await liquers.evaluatePipeline(editor.pipeline()), [{ amount: 80 }]);
editor.free();
```

A `PipelineEditor` is the query editor of section 3.9 without the limit to what a query can
hold. `new liquers.PipelineEditor()` starts an empty pipeline. Because a pipeline is plain data,
it can also be kept as JSON, sent to a server, or built by a form without any editor.

API reference: [`PipelineEditor`](https://orest-d.github.io/liquers/api/js/classes/PipelineEditor.html),
[`evaluatePipeline`](https://orest-d.github.io/liquers/api/js/functions/evaluatePipeline.html),
[`pipelineToQuery`](https://orest-d.github.io/liquers/api/js/functions/pipelineToQuery.html) and
[`queryToPipeline`](https://orest-d.github.io/liquers/api/js/functions/queryToPipeline.html).

{{> ../_shared/pipeline-or-query.md}}

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
