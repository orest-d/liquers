---
title: Stores
status: ready
sources: [reference/STORE_SEMANTICS.md, guides/JAVASCRIPT_GUIDE.md]
reviewed: 2026-11-10
---
# 6. Stores

PURPOSE: The reader can configure where Liquers reads and writes data, and knows what a store guarantees.

The *store* is where Liquers keeps data: the files your queries start from, and the results that
have a key. A native program keeps them on a disk or in cloud storage. A page has neither, so
this chapter is mostly about what a page has instead.

## 6.1 Keys, data and metadata

A store holds *entries*. Each entry has:

- a **key**, which looks like a path: `data/sales.csv`. It has no leading slash, and its last
  part usually has an extension, which says what format the data is in;
- its **data**, a sequence of bytes;
- its **metadata**, a record that describes the data: its type, its format, and, for a computed
  result, the query that made it.

Keys form folders: `data/sales.csv` is in the folder `data`. A query reads a key with
`-R/data/sales.csv` (section 3.4).

A new environment has no store at all. Until you configure one, every `-R/` query fails with
`asset_not_found`.

## 6.2 Configuring the store

`env.configureStore(config)` gives the environment its stores. The configuration lists one or
more *mounts*. Each has a `type`, the kind of storage, and a `prefix`, the first part of the
keys it holds:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
const env = liquers.Environment.global();
await env.configureStore({
  stores: [
    { type: "memory", prefix: "work" },
    { type: "memory", prefix: "fixed", read_only: true },
  ],
});

const store = env.store();
await store.set("work/note.txt", new TextEncoder().encode("milk"), {});
assert.strictEqual(await liquers.evaluate("-R/work/note.txt"), "milk");

await assert.rejects(store.set("fixed/note.txt", new Uint8Array([1]), {}),
  (e) => e.errorType === "not_supported");                         // the mount is read-only
await assert.rejects(liquers.evaluate("-R/elsewhere/note.txt"),
  (e) => e.errorType === "asset_not_found");                       // no mount has this prefix
```

A key goes to the first mount whose prefix it starts with. The mount keeps the prefix: the key
`work/note.txt` is the entry `work/note.txt` of that mount.

The format is the one native Liquers uses for its `store` configuration, so one document can
describe the stores of a page and of a server. It can be passed as an object, as above, or as
YAML text:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
const env = liquers.Environment.global();
await env.configureStore(`
stores:
  - {type: memory, prefix: work}
  - {type: memory, prefix: fixed, read_only: true}
`);
await assert.rejects(
  env.configureStore({ stores: [{ type: "filesystem", prefix: "data", config: { path: "." } }] }),
  (e) => e.errorType === "not_supported");          // a browser has no file system
```

Configure the store once, before the first query. Calling `configureStore` again replaces the
whole configuration and discards the results computed against the old one.

## 6.3 Kinds of storage in a browser

Four types work in a page:

| `type` | Where the data is | Writes | Lives until |
|---|---|---|---|
| `memory` | in the module's memory | yes | the page is reloaded, or the environment is rebuilt (section 8.3) |
| `localstorage` | the browser's `localStorage`, under `config.namespace` | yes | the user clears the site's data |
| `http`, `https` | files fetched from `config.url_prefix` + the key | no | the server changes them |
| `js` | an object your page implements (section 6.5) | as implemented | as implemented |

The types of native Liquers that need a file system or a cloud SDK, such as `filesystem` and
`s3`, are refused in the browser with a `not_supported` error that names the type.

**`memory`** needs no settings. Use it for scratch data, for input the user drops onto the page,
and in tests.

**`localstorage`** keeps data across visits. The browser gives each site a small budget for it,
a few megabytes, and stores text, so binary data takes more room than its size. It suits
settings, notes and small results, not data files. `config.namespace` separates the entries of
your application from other uses of `localStorage` on the same site, and `config.quota_bytes`
sets a budget of your own. This example needs a browser, so the tests do not run it:

<!-- example: not run — localStorage and files served beside the page exist only in a browser -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
const env = liquers.Environment.global();
await env.configureStore({
  stores: [
    { type: "http", prefix: "data", config: { url_prefix: "./data/", keys: ["sales.csv", "recipes.yaml"] } },
    { type: "localstorage", prefix: "local", config: { namespace: "myapp" } },
  ],
});

await env.store().set("local/note.txt", new TextEncoder().encode("milk"), {});
console.log(await liquers.evaluate("-R/local/note.txt"));                       // milk, also after a reload
console.log(await liquers.evaluate("-R/data/sales.csv/-/ns-rec/to_record/nrows"));
```

**`http`** reads the files of a web site. The key, without the mount's prefix, is appended to
`url_prefix`: with the configuration above, `data/sales.csv` is fetched from `./data/sales.csv`,
relative to the page. Three things to know:

- HTTP cannot list a folder, so `keys` must name every file the store has. A key that is not
  listed does not exist for Liquers, even when the server would answer for it. A `recipes.yaml`
  must be listed too (section 7.8).
- The store is read-only.
- The files are fetched with `fetch`, so the browser's rules apply. Files on the page's own site
  just work. Files on another site need that site to allow it with CORS headers
  (`Access-Control-Allow-Origin`).

This is the natural store for a page that presents data published with it: put the CSV files
next to the page, list them in `keys`, and let recipes describe what is derived from them.

There is no store for IndexedDB and none that reads and writes the store of a Liquers server.
Both can be written as a `js` store, since its methods may return promises (section 6.5).

## 6.4 Using the store from the page

`env.store()` is the configured store. Its methods all return promises:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "data" }] });
const store = env.store();
const bytes = (s) => new TextEncoder().encode(s);

await store.set("data/sales.csv", bytes("region,amount\nEU,10\n"), {});
await store.set("data/notes/a.txt", bytes("first"), {});

assert.deepStrictEqual((await store.listdir("data")).sort(), ["notes", "sales.csv"]);
assert.strictEqual(await store.contains("data/sales.csv"), true);
assert.strictEqual(await store.isDir("data/notes"), true);
assert.strictEqual(new TextDecoder().decode(await store.get("data/notes/a.txt")), "first");

const info = await store.getAssetInfo("data/sales.csv");
assert.strictEqual(info.media_type, "text/csv");
assert.strictEqual(info.filename, "sales.csv");

await store.remove("data/notes/a.txt");
assert.strictEqual(await store.contains("data/notes/a.txt"), false);
```

| Method | What it does |
|---|---|
| `get(key)` | the data, a `Uint8Array` |
| `set(key, bytes, metadata)` | writes data; pass `{}` for the metadata unless you have a record to keep |
| `remove(key)`, `removedir(key)`, `makedir(key)` | remove an entry or a folder, make a folder |
| `contains(key)`, `isDir(key)` | whether a key exists, whether it is a folder |
| `listdir(key)` | the names in a folder |
| `getMetadata(key)`, `setMetadata(key, metadata)` | the stored metadata record |
| `getAssetInfo(key)` | what Liquers makes of the key: media type, file name, format, whether it is a folder |
| `isReadOnly()` | whether every write is refused |

To decide how to show a key, read `getAssetInfo`. `getMetadata` returns the raw record, in which
most fields are empty for a file that was simply written.

`store.get` gives bytes, always. A query that reads the same key gives a value: text for a text
file, parsed data for JSON (section 4.4).

One thing does not happen yet: a write does not refresh results that were already computed from
the old data (section 7.7).

## 6.5 A store your page implements

A `js` store is an object with a few methods, registered under a name and then mounted like any
other store. It can stand for anything the page can reach: a `Map`, files the user picked,
IndexedDB, an API of your own.

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
const files = new Map([["notes/a.txt", new TextEncoder().encode("first")]]);

const env = liquers.Environment.global();
env.registerStoreObject("notes", {
  get: (key) => (files.has(key) ? { data: files.get(key) } : null),
  contains: (key) => files.has(key),
  isDir: (key) => [...files.keys()].some((k) => k.startsWith(key + "/")),
  listdir: (key) => [...new Set([...files.keys()]
    .filter((k) => k.startsWith(key + "/"))
    .map((k) => k.slice(key.length + 1).split("/")[0]))],
  getMetadata: (key) => (files.has(key) ? {} : null),
  set: (key, data) => { files.set(key, data); },
  setMetadata: () => {},
  remove: (key) => { files.delete(key); },
});
await env.configureStore({ stores: [{ type: "js", prefix: "notes", config: { object: "notes" } }] });

assert.strictEqual(await liquers.evaluate("-R/notes/a.txt"), "first");
await env.store().set("notes/b.txt", new TextEncoder().encode("second"), {});
assert(files.has("notes/b.txt"));
assert.deepStrictEqual((await env.store().listdir("notes")).sort(), ["a.txt", "b.txt"]);
await assert.rejects(liquers.evaluate("-R/notes/missing.txt"), (e) => e.categories.includes("not_found"));
```

The rules of the object:

- **Keys arrive whole**, with the mount's prefix: `notes/a.txt`, not `a.txt`.
- **`get` returns `{ data }`** for a key, with the data as a `Uint8Array`, and `null` for a key
  that does not exist. Throwing means a failure, which is a different thing: "absent" becomes
  `key_not_found`, a thrown error becomes `key_read_error`. Throw a `LiquersError` to choose the
  type yourself.
- **Any method may return a `Promise`**, so a store can await `fetch` or IndexedDB.
- **Only `get` is required**, but a store that queries read needs `contains` and `isDir` too.
  Without them a missing key cannot be told from a folder.
- **A method you leave out is reported as unsupported** (`key_not_supported`) when Liquers needs
  it, rather than answering "empty". Recipes need `listdir`.
- **`getMetadata` returns `{}`** for a key that exists and has no metadata to report, and `null`
  for a key that does not exist.

The complete list of methods is the interface `LiquersStoreObject` in the
[API reference](https://orest-d.github.io/liquers/api/js/index.html).

## 6.6 Read-only mounts and metadata sections

Any mount may say `read_only: true`, which refuses every write before it reaches the storage, as
section 6.2 showed. `read_only: true` at the top of the configuration does it for all mounts.

A mount may also carry a `metadata` section, which decides where the metadata records are kept
and how they are filled in. For example, `metadata: { storage: "per_entry" }` keeps each record
as an entry of its own beside the data. The section is the same as in a native store
configuration, described in
[6. Stores](../rust/06-stores.md) of the Rust guide. `localstorage` supports all of it, `http`
the reading half, and a `js` store needs five more methods for it (`getData`, `setData`,
`removeData`, `listdirData`, `dataFacts`). Most pages never need a `metadata` section.

## 6.7 What a store guarantees in a page

A store in a page is weaker than a disk, and it is good to be clear about it:

- **`memory` is gone on reload.** It is also emptied when the environment is rebuilt, which a
  late registration of a command causes (section 8.3).
- **`localStorage` belongs to the user.** They can clear it, a private window starts without it,
  and the browser refuses writes beyond its budget. Treat what is in it as a cache of something
  the page can make again, or as something the user can afford to lose.
- **`http` shows the files as they are on the server**, through the browser's HTTP cache. A file
  that changes on the server is seen when the browser fetches it again.
- **Nothing is shared between tabs.** Each tab has its own environment. Two tabs that write the
  same `localStorage` namespace see each other's entries, but neither is told when the other
  writes.
