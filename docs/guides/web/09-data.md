---
title: Working with data
status: ready
sources: [guides/RECORD_STREAM_GUIDE.md, guides/JAVASCRIPT_GUIDE.md]
reviewed: 2026-11-10
---
# 9. Working with data

PURPOSE: The reader can load, transform and save tabular data and images with the command libraries.

For tables, the browser module has the *record* library: tables with named, typed columns, and
commands that read, filter, sort and reshape them. It is the same library a native program has.
What a native program has in addition, Polars data frames and images, is not in the browser;
section 9.5 says what to do instead.

The record library is an optional part of the module. The copy on the project's site has it. A
module you build yourself has it when you build with `--features records` (section 2.5). In a
module without it, the `ns-rec` commands do not exist, and the queries of this chapter fail with
`action_not_registered`.

## 9.1 A first table

A table can come from a CSV file in the store or from an array of objects that a command of
yours returns. `ns-rec/to_record` reads either as a table:

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
const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "data" }] });
const csv = "region,month,amount\nEU,1,10.5\nUS,1,20\nEU,2,5\n";
await env.store().set("data/sales.csv", new TextEncoder().encode(csv), {});

// From a file: the EU rows, smallest amount first.
const eu = await liquers.evaluate("-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/sort-amount/to_json");
assert.deepStrictEqual(eu, [
  { amount: 5, month: 2, region: "EU" },
  { amount: 10.5, month: 1, region: "EU" },
]);

// From a command: how many people are older than 30?
assert.strictEqual(await liquers.evaluate("people/ns-rec/to_record/gt-age-30/nrows"), 2);
assert.deepStrictEqual(await liquers.evaluate("people/ns-rec/to_record/shape"), { ncols: 3, nrows: 3 });
```

`ns-rec` selects the namespace of the record commands, and it stays selected for the actions
that follow. `to_record` makes the table: from CSV it reads the header line and works out the
type of each column, so `amount` is a number and `region` is text. `to_json` turns a table back
into plain data, an array with one object per row, which is the form a page can use directly.

## 9.2 The record commands

Every command of this table takes a table and returns a table, unless it says otherwise, so they
chain in any order:

| Command | What it does |
|---|---|
| `to_record` | reads the input (CSV bytes, parsed JSON, an array of objects) as a table |
| `head-N`, `tail-N`, `slice-OFFSET-LENGTH` | the first rows, the last rows, a range of rows |
| `select_columns-A-B`, `drop_columns-A` | keep or drop columns |
| `eq-COLUMN-VALUE`, `ne`, `gt`, `gte`, `lt`, `lte` | keep the rows whose column compares so with the value |
| `starts_with-COLUMN-TEXT`, `ends_with`, `contains` | keep the rows whose text column matches |
| `is_in-COLUMN-A-B`, `is_null-COLUMN`, `is_not_null-COLUMN` | keep the rows whose column is one of the values, is empty, is not empty |
| `sort-COLUMN`, `sort-COLUMN-false` | sort by a column, ascending or descending |
| `row-N` | one row, as a table of one row |
| `nrows`, `ncols`, `shape` | the size, as a number or as `{ nrows, ncols }` |
| `schema` | the columns and their types, as JSON text |
| `to_json` | the rows as an array of objects |
| `to_record_source`, `materialize` | a table in chunks, and back to one table (section 9.4) |

`commands_doc` lists them all with their arguments, and appendix B has the same list for the
whole library.

A value in a filter is an argument of a query, so it is escaped like one: build such queries
with `liquers.encodeParam` or a `QueryEditor` when the value comes from a user (section 3.9).

## 9.3 Tables in JavaScript

A table is not converted to a JavaScript value when it leaves the module: copying a million rows
into objects would be wasteful. `evaluate` resolves with a `RecordBatch`, a handle to the table
inside the module (section 4.5):

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "data" }] });
const csv = "region,month,amount\nEU,1,10.5\nUS,1,20\nEU,2,\n";
await env.store().set("data/sales.csv", new TextEncoder().encode(csv), {});

const batch = await liquers.evaluate("-R/data/sales.csv/-/ns-rec/to_record");
assert(batch instanceof liquers.RecordBatch);
assert.strictEqual(batch.numRows, 3);
assert.strictEqual(batch.numColumns, 3);

const schema = JSON.parse(batch.schemaJson());
assert.deepStrictEqual(schema.fields.map((f) => [f.name, f.data_type]),
  [["region", "Text"], ["month", "Int"], ["amount", "Float"]]);

assert.deepStrictEqual(batch.columnCopy(0), ["EU", "US", "EU"]);              // text: an array of strings
assert.deepStrictEqual(batch.columnCopy(1), new BigInt64Array([1n, 1n, 2n])); // integers: 64-bit
assert.deepStrictEqual(batch.columnCopy(2), new Float64Array([10.5, 20, 0])); // floats; the empty cell reads 0
assert.strictEqual(batch.column(2).validity.rows, 3);                         // a column with empty cells has a validity bitmap

batch.free();
```

`columnCopy(i)` copies one column out as the natural JavaScript form for its type: a typed array
for numbers, an array for text. Integers are 64-bit, so they arrive as a `BigInt64Array`; use
`Number(x)` on a cell when you know it is small. An empty cell has no value in a typed array and
reads as `0`. A column that has empty cells says so with a `validity` entry in its description,
`batch.column(i)`. When empty cells matter, the simplest course is to filter them out in the
query (`is_not_null-amount`) or to read the rows with `to_json`, where an empty cell is `null`.

`columnView(i)` gives the column without copying, as a view into the module's memory. It is the
fast path for large numeric columns, and it is valid only until the handle is freed or the
module's memory grows, so read it at once and do not keep it.

A command of yours receives a table the same way. Declare `state: "value"`, and the first
parameter is the `RecordBatch`:

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({
  name: "people",
  run: () => [{ name: "Ann", age: 31 }, { name: "Bob", age: 25 }, { name: "Cy", age: 40 }],
});
liquers.registerCommand({
  name: "mean",
  state: "value",
  arguments: [{ name: "column", type: "int", default: 0 }],
  run: (batch, column) => {
    let sum = 0;
    for (const value of batch.columnCopy(column)) sum += Number(value);
    return sum / batch.numRows;
  },
});

// The columns of a table made from objects are in alphabetical order: age, name.
assert.strictEqual(await liquers.evaluate("people/ns-rec/to_record/mean"), 32);
assert.strictEqual(await liquers.evaluate("people/ns-rec/to_record/gt-age-30/mean"), 35.5);
```

A command can also return the handle it received, or pass it on: Liquers gets the same table
back, not a copy.

## 9.4 Large tables in chunks

A `RecordSource` is a table that comes in *chunks*, so that it never has to be in memory as a
whole. `to_record_source` makes one from a table, and other sources read their chunks on demand:

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({
  name: "people",
  run: () => [{ name: "Ann", age: 31 }, { name: "Bob", age: 25 }, { name: "Cy", age: 40 }],
});

const source = await liquers.evaluate("people/ns-rec/to_record_source");
assert(source instanceof liquers.RecordSource);
assert.strictEqual(source.bounded, true);                 // it has an end

let rows = 0;
for await (const batch of await source.stream()) {         // one chunk at a time
  rows += batch.numRows;
  batch.free();
}
assert.strictEqual(rows, 3);

const all = await source.materialize(10000);               // one table; refused beyond 10 000 rows
assert.strictEqual(all.numRows, 3);
all.free();
source.free();
```

`stream()` gives the chunks one after another, and only one is held at a time when you free
each. `materialize(maxRows)` puts them together into one table and refuses when there are more
rows than you allow, which protects the page from a table that is larger than you thought.
`source.chunks()` lists the chunks without reading any.

Keep the scale in mind. Everything is in the page's memory and on its one thread. Tens of
thousands of rows are comfortable; for millions, let a server do the work and send the page the
result (chapter 12).

## 9.5 What is not in the browser

Two libraries of native Liquers are left out of the browser module, because they would add many
megabytes to the download:

| Not in the browser | What a native program uses it for |
|---|---|
| Polars (`ns-pl`) | fast data frames: grouping, joins, aggregation, Parquet |
| Images (`ns-img`) | reading, resizing and converting images |

Queries that use them fail in a page with `action_not_registered`. There are three ways around
it:

- **Use the record commands** for what they cover: reading, filtering, sorting, selecting.
- **Write the missing step in JavaScript.** A grouping or a sum over a few thousand rows is a
  short command over `to_json` or `columnCopy`. For images, the browser has its own tools: a
  command can draw on a canvas and return the bytes of a PNG, or pass a bitmap on as an opaque
  object (section 4.2).
- **Ask a server.** A Liquers server has the whole library. The page sends it the query and gets
  the result, as chapter 12 shows. This is the right place for heavy work in any case.

## 9.6 Saving results

A page cannot write files on the user's disk on its own. What it can do with a result:

- **Keep it in the store**, with `env.store().set(key, bytes, {})` or through a recipe
  (section 7.8). In `localstorage` it survives a reload.
- **Offer it as a download.** This needs a page, so the tests do not run it:

<!-- example: not run — it needs the page's document to offer a download -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "people", run: () => [{ name: "Ann", age: 31 }] });

const rows = await liquers.evaluate("people/ns-rec/to_record/to_json");
const blob = new Blob([JSON.stringify(rows, null, 2)], { type: "application/json" });
const link = document.createElement("a");
link.href = URL.createObjectURL(blob);
link.download = "people.json";
link.click();
URL.revokeObjectURL(link.href);
```

In a native program, a file name at the end of a query (`…/eu.csv`) gives the result its byte
form, and a server sends those bytes. In a page, `evaluate` resolves with the value whatever the
file name says, so your code makes the bytes, as `JSON.stringify` does here.
