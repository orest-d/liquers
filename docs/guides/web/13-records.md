---
title: Records
status: ready
sources: [reference/RECORD_STREAMS.md, guides/RECORD_STREAM_GUIDE.md, guides/SEARCH_GUIDE.md, reference/SEARCH.md, reference/AGENT_MEMORY_SERVICE.md, guides/JAVASCRIPT_GUIDE.md]
reviewed: 2026-11-14
---
# 13. Records

PURPOSE: The reader understands records: views, sources and streams, schemas and manifests, how a manifest relates to a recipe and records to Polars. The reader can use them in a page, and knows which of their applications, search and the agent memory, a page has.

Chapter 9 used tables without saying much about what they are. This chapter does. It describes
*records*, the table type that is built into Liquers, first in the words shared by all three
guides (sections 13.1 to 13.5) and then in a page (13.6). The last two sections are about what
records are used for in Liquers: search, and the memory for AI agents. A page has a part of the
first and none of the second.

Like chapter 9, this chapter needs a module with the record commands. The published module has
them; a module you build yourself has them when you build with `--features records`
(section 2.5).

A first look, to have something concrete in mind:

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();

const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "sales" }] });
const store = env.store();
const bytes = (s) => new TextEncoder().encode(s);
await store.set("sales/jan.csv", bytes("id,region,amount\na1,EU,120\na2,US,80\n"), {});

const table = await liquers.evaluate("-R/sales/jan.csv/-/ns-rec/to_record");
assert(table instanceof liquers.RecordBatch);              // a record view: a table in memory
assert.strictEqual(table.numRows, 2);
const fields = JSON.parse(table.schemaJson()).fields.map((field) => [field.name, field.data_type]);
assert.deepStrictEqual(fields, [["id", "Text"], ["region", "Text"], ["amount", "Int"]]);
table.free();
```

{{> ../_shared/records.md}}

## 13.6 Records in a page

Section 9.3 showed how to read a table in JavaScript and section 9.4 a first record source.
Here are the parts that belong to this chapter: schemas and manifests. In JavaScript a record
view is a `RecordBatch` and a record source is a `RecordSource`.

### Declaring a schema

`to_record` takes the schema as text, in YAML or JSON. A schema is too long and too structured
for a query, so this is a natural place for a pipeline (section 3.10), in which the argument is
simply a string:

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();

const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "sales" }] });
const store = env.store();
const bytes = (s) => new TextEncoder().encode(s);
const schema = `
fields:
  - {name: id, data_type: Text, key: Id}
  - {name: region, data_type: Text, nullable: false}
  - {name: amount, data_type: Float}
`;
await store.set("sales/jan.csv", bytes("id,region,amount\na1,EU,120\na2,US,80\n"), {});
await store.set("sales/schema.txt", bytes(schema), {});

// The schema as an argument of a pipeline step.
const table = await liquers.evaluatePipeline([
  "-R/sales/jan.csv",
  { action: "to_record", ns: "rec", arguments: { schema } },
]);
const fields = JSON.parse(table.schemaJson()).fields;
assert.strictEqual(fields[2].data_type, "Float");          // declared, not inferred
assert.strictEqual(fields[0].key, "Id");
table.free();

// The schema as a link: the argument is the result of another query.
const editor = new liquers.QueryEditor("-R/sales/jan.csv");
let at = editor.action("to_record", "rec");
editor.link(at, "schema", "-R/sales/schema.txt");
at = editor.action("rec_id", "rec");                       // look a row up by its Id
editor.set(at, "id", "a2");
editor.action("to_json", "rec");
const query = editor.query();
editor.free();
assert.strictEqual(query, "-R/sales/jan.csv/-/ns-rec/to_record--true-~X~-R/sales/schema.txt~E/rec_id-a2/to_json");
assert.deepStrictEqual(await liquers.evaluate(query), [{ amount: 80, id: "a2", region: "US" }]);
```

The second half stays with a query, and so with a cached result: the schema is stored under a
key and passed as a link, which is written `~X~` *query* `~E` in place of the argument. The
editor wrote the two arguments before `schema` with their defaults (an empty `format`, which
means "as the file says", and `header` true), because a query can only give arguments by
position. In a recipe, the same link is the entry `links: {schema: -R/./schema.txt}`.

The schema is stored as a `.txt` file on purpose. The argument wants text, and a file named
`.yaml` or `.json` is read as a structure, which the argument refuses.

### Sources and manifests

Manifests work in a page as section 13.4 describes. A manifest in the store is a record source:

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();

const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "sales" }] });
const store = env.store();
const bytes = (s) => new TextEncoder().encode(s);
await store.set("sales/jan.raw.csv", bytes("id,region,amount\na1,EU,120\na2,US,80\n"), {});
await store.set("sales/feb.raw.csv", bytes("id,region,amount\nb1,EU,95\n"), {});
await store.set("sales/monthly.manifest.yaml", bytes(`
manifest: record-stream
title: Sales by month
chunks:
  - query: -R/sales/jan.raw.csv/-/ns-rec/to_record/jan.csv
  - query: -R/sales/feb.raw.csv/-/ns-rec/to_record/feb.csv
uniform_schema:
  fields:
    - {name: id, data_type: Text, key: Id}
    - {name: region, data_type: Text, nullable: false}
    - {name: amount, data_type: Int}
`), {});

// Record commands read the manifest as one table.
assert.strictEqual(await liquers.evaluate("-R/sales/monthly.manifest.yaml/-/ns-rec/nrows"), 3);

// A lazy filter gives a source; materialize makes the table.
const lazy = await liquers.evaluate("-R/sales/monthly.manifest.yaml/-/ns-rec/eq-region-EU");
assert(lazy instanceof liquers.RecordSource);
assert.deepStrictEqual(await lazy.chunks(), [              // listed, not read
  { kind: "key", query: "-R/sales/jan.csv" },
  { kind: "key", query: "-R/sales/feb.csv" },
]);
lazy.free();
const eu = await liquers.evaluate("-R/sales/monthly.manifest.yaml/-/ns-rec/eq-region-EU/materialize/to_json");
assert.deepStrictEqual(eu.map((row) => row.id), ["a1", "b1"]);

// Each chunk has a key, and after its first use a stored file.
assert.deepStrictEqual(await liquers.evaluate("-R/sales/feb.csv/-/ns-rec/to_json"), [{ amount: 95, id: "b1", region: "EU" }]);
assert.strictEqual(await store.contains("sales/feb.csv"), true);
```

| To | Use |
|---|---|
| list the chunks without reading any | `await source.chunks()`; each has `kind` (`"key"` or `"query"`) and `query` |
| walk the source | `for await (const batch of await source.stream())` |
| get everything as one table | `await source.materialize(maxRows)` |
| know whether there is an end | `source.bounded` |

API reference: [`RecordBatch`](https://orest-d.github.io/liquers/api/js/classes/RecordBatch.html) and
[`RecordSource`](https://orest-d.github.io/liquers/api/js/classes/RecordSource.html).

What is particular to a page is what section 7.8 says of recipes, for the same reasons:

- **The store must be able to list a folder**, because Liquers finds a manifest by listing.
  With an `http` store, name the manifest in `keys`.
- **A chunk with a key is written to the store** when the store accepts writes, and with
  `localstorage` it survives a reload.
- **Chunks are not refreshed yet** (section 7.7). A stored chunk is served as it is even after
  its input has changed.

With an `http` store, a manifest is a good way to publish a table that is too large to
download at once: the server holds the monthly files and the manifest, and the page reads only
the chunks that a visitor's question needs. `head-10` reads one.

### Polars

Polars is not in the browser module (section 9.5), so the crossing of section 13.5 cannot
happen in a page. Use the record commands for what they cover, and for statistics over a large
table ask a server (chapter 12): the same query, with its `ns-pl` part, runs there.

## 13.7 Search: records put to work

Native Liquers has a search library, `ns-search`, that finds things in a store. It is built
entirely from records: one command turns what you want to search into a table, one row for each
thing, and another keeps the rows that match. A page has the second half. `ns-rec/filter` keeps
the rows of any table that match a *search expression*. A bare word matches in any column, and
`column:word` in one:

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
    { name: "Oslo Olsen", age: 40, city: "Bergen" },
  ],
});

// The expression comes from a search box: let the editor write it into the query.
const search = (expression) => {
  const editor = new liquers.QueryEditor("people/ns-rec/to_record");
  const at = editor.action("filter", "rec");
  editor.set(at, "query", expression);
  const names = editor.action("select_columns", "rec");
  editor.pushParameter(names, "name");
  editor.action("to_json", "rec");
  const query = editor.query();
  editor.free();
  return liquers.evaluate(query);
};

assert.deepStrictEqual(await search("Oslo"), [{ name: "Ann" }, { name: "Oslo Olsen" }]);
assert.deepStrictEqual(await search("city:Oslo"), [{ name: "Ann" }]);
assert.deepStrictEqual(await search("age:>30 -city:Bergen"), [{ name: "Ann" }]);
```

A search expression contains characters such as `:` and spaces, so build the query with a
query editor (section 3.9), as here, whenever the expression comes from a search box.

### Search expressions

| Write | It matches when |
|---|---|
| `word` | the word occurs in one of the searched columns |
| `a b` | both occur |
| `a \| b` | either occurs |
| `-a` | `a` does not occur |
| `"a phrase"` | the words occur in this order |
| `/re/` | the regular expression matches |
| `field:value` | the column equals the value; `*` and `?` are wildcards, `field:*` means "has a value" |
| `field:>5`, `>=`, `<`, `<=` | the column compares so, for numbers and dates |
| `( … )` | grouping |

Which columns a bare word is looked for in is decided by the schema (section 13.2): the fields
whose role says they hold text to search, and when no field has such a role, the text columns.
`filter` is lazy on a record source, like the other filters, so it also searches the manifest
of section 13.6 chunk by chunk.

### What a page does not have

| Native Liquers | In the browser |
|---|---|
| `ns-search`: list and search the files of a folder, by text and by fields, with ranking | not available; the queries fail with `action_not_registered` |
| search over Markdown documents, their sections and front matter | not available |

The search library reads folders of a file system or of cloud storage, which a page does not
have. When a page needs search over documents, run it on a server and ask from the page. A
Liquers server with the search library answers a search as it answers any query (chapter 12),
so the page fetches a query whose result is a table of matches and shows it. The search queries
themselves are described in [13. Records](../rust/13-records.md) of the Rust guide, and in
[the same chapter](../python/13-records.md) of the Python guide.

## 13.8 The agent memory

The second application of records in native Liquers is a *memory* for AI agents: a folder of
Markdown documents with a schema, and commands that list, search, read and change them. Listing
and searching a memory return record views. Neither the memory nor the program that serves it
is in the browser module:

| Native Liquers | In the browser |
|---|---|
| the agent memory (`liquers-agent`): documents with versions, served to AI agents as tools over the Model Context Protocol | a native program; a page cannot run it |
| commands served as tools to an agent (`liquers-mcp`) | a native library; a tool server must listen for connections |

Both are described in the chapters of the other two guides named above.
