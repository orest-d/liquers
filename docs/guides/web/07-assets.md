---
title: Recipes, assets and caching
status: ready
sources: [reference/ASSETS.md, reference/RECIPES_AND_PLANS.md, guides/JAVASCRIPT_GUIDE.md, reference/PIPELINES.md]
reviewed: 2026-11-14
---
# 7. Recipes, assets and caching

PURPOSE: The reader knows when a result is computed, cached, reused and recomputed, and can use that from JavaScript in the browser.

Sections 7.1 to 7.4 describe assets and recipes as they are in every Liquers, in the words
shared by all three guides. They describe the full model, as a native program has it. The
browser module has the same model but not yet all of its behaviour: results are not refreshed
when their inputs change, and nothing can be cancelled. Sections 7.5 to 7.9 say exactly what a
page gets.

{{> ../_shared/assets.md}}

## 7.5 The asset in a page

`liquers.getAsset(query)` resolves with an `Asset`. Its methods all return promises:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "hello", run: () => "world" });

const asset = await liquers.getAsset("hello/hello.txt");
assert.strictEqual(await asset.status(), "ready");

const state = await asset.get();                 // the value with its metadata
assert.strictEqual(state.toJS(), "world");

const info = await asset.getAssetInfo();         // a summary, without the value
assert.strictEqual(info.status, "Ready");
assert.strictEqual(info.media_type, "text/plain");
assert.strictEqual(info.filename, "hello.txt");
```

Section 7.1 says that an asset exists before its value does, so that you can watch it being
computed. In a native program that is so. In the browser it is not, yet: the module evaluates a
query *inline*, as part of the call that asked for it. By the time `getAsset` resolves, the
evaluation is over. So in a page:

- the status you read is the final one, `ready`; you do not see `submitted` or `processing`;
- a query that fails rejects `getAsset` itself, with the error, as it rejects `evaluate`;
- `asset.cancel()` has nothing left to cancel (section 7.9);
- two queries do not run in parallel. They can overlap when their commands are asynchronous and
  await something, but only one piece of JavaScript or WebAssembly runs at a time.

For a page this is usually what you want: `await liquers.evaluate(query)` and show the value. To
show progress while a slow command runs, let the command report through your own code, a callback
or an event, or mount a Liquers user interface, whose elements show their own status
(chapter 11).

## 7.6 What is kept, and for how long

The environment keeps each result it computes, in memory, under its query. Ask again and you get
the kept result:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
let reports = 0;
let ticks = 0;
let failures = 0;
liquers.registerCommand({ name: "report", run: () => { reports += 1; return "the report"; } });
liquers.registerCommand({ name: "shout", state: "text", run: (text) => text.toUpperCase() });
liquers.registerCommand({ name: "tick", volatile: true, run: () => { ticks += 1; return ticks; } });
liquers.registerCommand({ name: "fails", run: () => { failures += 1; throw new Error("no"); } });

await liquers.evaluate("report");
await liquers.evaluate("report");
await liquers.evaluate("report/shout");
assert.strictEqual(reports, 1);                  // computed once, reused as a part of a longer query too

assert.strictEqual(await liquers.evaluate("tick"), 1);
assert.strictEqual(await liquers.evaluate("tick"), 2);          // volatile: never reused

await liquers.evaluate("fails").catch(() => {});
await liquers.evaluate("fails").catch(() => {});
assert.strictEqual(failures, 2);                 // a failure is not kept
```

How long a result is kept:

| Event | What happens to kept results |
|---|---|
| the page is reloaded or closed | all gone: they live in the page's memory |
| a command is registered after the first evaluation | all forgotten (section 8.3) |
| `env.configureStore(…)` is called again | all forgotten |
| `liquers.shutdown()` | all forgotten, with the environment |
| a command is `volatile: true` | its results are never kept |
| an evaluation fails | nothing is kept; the next request tries again |

Results are kept until one of these happens. Nothing removes them to save memory, so a page that
evaluates an unbounded number of different queries, one per keystroke for example, grows. Keep
the set of queries bounded, or make the command at the varying end `volatile`.

Only results that have a recipe are written to the store (section 7.8). Everything else lives in
memory only.

## 7.7 Results are not refreshed yet

Section 7.3 says that a result becomes stale when something it was made from changes, and is
then computed again. The browser module records the dependencies, and you can read them, but it
does not act on them yet:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "shout", state: "text", run: (text) => text.toUpperCase() });
const env = liquers.Environment.global();
const mounts = { stores: [{ type: "memory", prefix: "data" }] };
await env.configureStore(mounts);
const bytes = (s) => new TextEncoder().encode(s);

await env.store().set("data/note.txt", bytes("milk"), {});
assert.strictEqual(await liquers.evaluate("-R/data/note.txt/-/shout"), "MILK");

const state = await (await liquers.getAsset("-R/data/note.txt/-/shout")).get();
assert(state.metadata().dependencies.some((d) => d.key === "-R/data/note.txt"));   // recorded

await env.store().set("data/note.txt", bytes("bread"), {});
assert.strictEqual(await liquers.evaluate("-R/data/note.txt/-/shout"), "MILK");    // still the old result
```

After the write, the store holds `bread`, and the kept result still says `MILK`. Until this is
fixed, a page that changes its inputs has three ways to get current results:

- **Put what changes into the query.** A result is kept under its query, so a different argument
  is a different result. A command that takes the user's text as an argument, or as the state
  from a volatile command, never serves stale data.
- **Declare the reading command `volatile`** when its input changes behind Liquers' back anyway:
  a command that reads a form field, the clock or a server.
- **Forget everything.** Calling `env.configureStore` again with the same configuration discards
  all kept results. It also recreates the stores, so a `memory` store starts empty; with a `js`
  store or `localstorage` the data is still there.

## 7.8 Recipes in a page

Recipes work in a page as section 7.4 describes: a `recipes.yaml` in a folder of the store names
the files that folder should contain and the query that makes each.

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "shout", state: "text", run: (text) => text.toUpperCase() });

const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "data" }] });
const store = env.store();
const bytes = (s) => new TextEncoder().encode(s);
await store.set("data/note.txt", bytes("milk"), {});
await store.set("data/recipes.yaml", bytes(`
recipes:
  - query: -R/data/note.txt/-/shout/loud.txt
    title: Loud note
    description: The note in capital letters
`), {});

assert.strictEqual(await store.contains("data/loud.txt"), false);        // nothing is computed yet
assert.strictEqual(await liquers.evaluate("-R/data/loud.txt"), "MILK");  // the first request runs the query

const state = await (await liquers.getAsset("-R/data/loud.txt")).get();
assert.strictEqual(state.metadata().title, "Loud note");

assert.strictEqual(await store.contains("data/loud.txt"), true);         // and the result was stored
assert.strictEqual(new TextDecoder().decode(await store.get("data/loud.txt")), "MILK");
```

A recipe with a pipeline (section 7.4) is written the same way. Here the pipeline passes a
list of regions, which a query could not hold, and the query finishes the work:

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();

const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "data" }] });
const store = env.store();
const bytes = (s) => new TextEncoder().encode(s);
await store.set("data/sales.csv", bytes("region,month,amount\nEU,jan,120\nUS,jan,80\nASIA,jan,300\nEU,feb,95\n"), {});
await store.set("data/recipes.yaml", bytes(`
recipes:
  - pipeline:
      - -R/./sales.csv/-/ns-rec/to_record
      - action: is_in
        ns: rec
        arguments: {column: region, values: [EU, US]}
    query: ns-rec/sort-amount-false/head-2/top.csv
    title: The two largest sales in the EU and the US
`), {});

const top = await liquers.evaluate("-R/data/top.csv/-/ns-rec/to_json");
assert.deepStrictEqual(top.map((row) => row.amount), [120, 95]);
// The result is stored under its key, like the result of any recipe.
assert.strictEqual(new TextDecoder().decode(await store.get("data/top.csv")), "region,month,amount\nEU,jan,120\nEU,feb,95\n");
```

The result has a key, so it is cached and stored, which a pipeline run with `evaluatePipeline`
is not (section 3.10).

What is particular to a page:

- **The store must be able to list a folder**, because Liquers looks for `recipes.yaml` by
  listing. `memory` and `localstorage` can. An `http` store lists what its `keys` name, so put
  `recipes.yaml` there. A `js` store needs `listdir`.
- **The result is written to the store** when the store accepts writes. With `localstorage`
  that makes a computed file survive a reload: the next visit reads it instead of computing it.
  With a read-only store, such as `http`, the result is kept in memory like any other.
- **A stored result stays until you remove it.** Because results are not refreshed yet
  (section 7.7), a recipe result in `localstorage` is served as it is even after its input or
  its recipe has changed. Remove the key with `store.remove(key)` when you publish a new version
  of the page's data, or keep recipe folders on a store that does not persist.

With an `http` store, recipes give a published folder of data a set of derived files at no cost:
the server holds `sales.csv` and `recipes.yaml`, and each visitor's page computes `eu.csv` the
first time it is asked for.

## 7.9 Cancelling

`asset.cancel()` exists and does nothing yet. An evaluation is finished before your code holds
the asset, so there is nothing left to stop, and `context.isCancelled()` inside a command always
answers `false`. A command that may run long needs its own way to stop, such as an
`AbortController` your page keeps and the command's `fetch` listens to.
