---
title: Installing and setting up
status: ready
sources: [guides/JAVASCRIPT_GUIDE.md, reference/JAVASCRIPT_INTEGRATION.md]
reviewed: 2026-11-10
---
# 2. Installing and setting up

PURPOSE: The reader has Liquers installed for JavaScript in the browser and knows which optional parts exist.

There is nothing to install in the usual sense. Liquers for the browser is a few static files:
an ES module and the WebAssembly it loads. "Installing" means deciding where those files are
served from. This chapter covers the three answers, the project's site, your own site and a
build of your own, and what a web server must do to serve them well.

## 2.1 The files

| File | Size | What it is |
|---|---|---|
| `liquers_web.js` | about 150 kB | the ES module your page imports |
| `liquers_web_bg.wasm` | about 20 MB, about 5 MB compressed | Liquers itself, compiled to WebAssembly |
| `snippets/` | a few kB | small JavaScript helpers the module imports, in a `records` build |
| `liquers_web.d.ts` | about 80 kB | TypeScript declarations; for your editor, not for the browser |

The sizes are those of an optimized build with the record library. The files must stay
together, with these names: `liquers_web.js` loads `liquers_web_bg.wasm` and the `snippets/`
folder from its own location.

## 2.2 Loading the module in a page

A page imports the module and makes two calls, in this order:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();            // downloads and compiles the WebAssembly
await liquers.init();    // creates the Liquers environment
assert(liquers.isInitialized());
assert(liquers.version().startsWith("liquers-web"));
```

The default export, `init`, loads the WebAssembly. `liquers.init()` creates the environment.
Both return promises, and nothing else of the module may be used before the first has resolved.
The `import` must be in a module script, `<script type="module">`, and the page must come from a
web server: browsers do not load modules from a `file:` address.

`await` at the top level works in a module script. The rest of your page should not wait for
it. Start the page, show that something is loading, and use Liquers when the promise resolves:

<!-- example: not run — it needs a page's document -->
```js
const out = document.getElementById("out");
out.textContent = "Loading…";

const ready = (async () => {
  const liquers = await import("https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js");
  await liquers.default();          // the default export is `init`
  await liquers.init();
  return liquers;
})();

document.getElementById("run").addEventListener("click", async () => {
  const liquers = await ready;
  out.textContent = await liquers.evaluate("commands_doc");
});
```

A dynamic `import()` like this one also keeps the module out of the first load of the page: the
browser starts the download when the code runs, not when the page is parsed.

By default the `.wasm` file is fetched from the folder of `liquers_web.js`. To load it from
somewhere else, pass its address, or the bytes themselves, to `init`:
`await init({ module_or_path: "/static/liquers_web_bg.wasm" })`.

## 2.3 Where to host the files

### On the project's site, to try things out

The project publishes a current build of the module with its documentation, on GitHub Pages:

```text
https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js
```

The examples of this guide import it from there, so that each runs in any page without setup.
That address is right for trying Liquers, for a demonstration, and for a snippet in a
discussion. It is the wrong address for anything you publish, for two reasons:

- **It is not versioned.** The files are rebuilt whenever the project's main branch changes. A
  page that imports them can break on a day when you changed nothing.
- **It is a documentation site**, not a content delivery network. It makes no promise about
  availability or speed.

### On a CDN, in the future

The intended home of the module is a content delivery network, with a versioned address for
each release, such as an npm package served by one of the public CDNs. A page would then pin a
version in its import and get immutable files from a server near the user. This does not exist
yet: there is no npm package and no CDN address today. Until there is, the project's site is
the public copy, and for real use you host the files yourself.

### On your own site

Copy `liquers_web.js`, `liquers_web_bg.wasm` and the `snippets/` folder into your site, next to
each other, and import the module from there:

```html
<script type="module">
  import init, * as liquers from "./liquers/liquers_web.js";
  await init();
  await liquers.init();
</script>
```

You can take the files from the project's site, at the address above with the three names, or
build them (section 2.5). Either way you now have a fixed version that changes when you replace
it. Put it into a folder whose name says which version it is, `liquers-0.1.0/`, so that a new
version gets a new address and browsers never mix files of two builds.

Any static hosting works: there is no server-side part. What the server must get right is in
the next section.

## 2.4 What the web server must do

**Serve `.wasm` as `application/wasm`.** With that media type the browser compiles the module
while it downloads. With another type the module still loads, but more slowly, and the console
shows a warning about `WebAssembly.instantiateStreaming`. Most servers and hosting services do
this by default, GitHub Pages among them. If yours does not, add the type to its configuration.

**Serve `.js` as JavaScript.** Browsers refuse a module script with the wrong media type.
Every web server does this by default.

**Compress.** The WebAssembly shrinks to about a quarter with gzip, and further with Brotli.
Turn compression on for `application/wasm`; many servers compress text types only until told
otherwise.

**Cache.** The files are large and change only when you deploy a new build. In a versioned
folder they can be cached for a long time (`Cache-Control: public, max-age=31536000,
immutable`). Without a version in the address, let the browser check back
(`Cache-Control: no-cache`), which costs one small request when nothing has changed.

**Allow other sites, if they are to import your copy.** A page on one site that imports a module
from another is making a cross-origin request, for the script and for the `.wasm` file. The
server of the files must answer with `Access-Control-Allow-Origin`. GitHub Pages sends it for
every file, which is why the import from the project's site works in your page. If the files
are on the same site as the page, nothing is needed.

**Content Security Policy.** A site that sends a `Content-Security-Policy` header must allow
WebAssembly to be compiled: add `'wasm-unsafe-eval'` to `script-src`. If the files come from
another origin, that origin must be in `script-src` and `connect-src` as well.

When loading fails, section 15.7 lists the symptoms and their causes.

A word about size. Twenty megabytes is a lot for a web page, and about five travel over the
network when the server compresses. The browser caches the file, so the cost is paid on the
first visit. Still, load the module only on the pages that use it, load it after the page has
appeared (section 2.2), and show the user that something is loading.

## 2.5 Building the module yourself

Building needs the repository, a Rust toolchain with the `wasm32-unknown-unknown` target, and
the `wasm-bindgen` command-line tool in the same version as the `wasm-bindgen` crate the project
uses:

```bash
git clone https://github.com/orest-d/liquers.git
cd liquers
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version <the wasm-bindgen version in Cargo.lock>
./liquers-web/examples-web/quickstart/build.sh --release --features records
```

The script compiles the crate `liquers-web` for WebAssembly and runs `wasm-bindgen --target web`
over the result. Its output folder, `liquers-web/examples-web/quickstart/dist/`, holds the files
of section 2.1 and a small demonstration page. Serve the folder with any static server to try
it:

```bash
python3 -m http.server 8090 --directory liquers-web/examples-web/quickstart/dist
```

The options that matter:

| Option | Effect |
|---|---|
| `--release` | an optimized build. Always use it for files you publish: a debug build is more than twice as large and much slower |
| `--features records` | includes the record library: tables, the `ns-rec` commands, `RecordBatch` and `RecordSource` (chapter 9). Without it the module is a few megabytes smaller and has no table commands |

A reason to build yourself, apart from pinning a version, is to leave out what you do not use.
The build decides which commands of the library are in the module; a page cannot add or remove
Rust commands afterwards. What a page adds are commands in JavaScript (chapter 5).

## 2.6 Bundlers and TypeScript

The module is built for direct use by a browser and needs no bundler. In a project that has one,
the simplest arrangement is to keep the three files out of the bundle: put them into the folder
your tool copies as it is (often called `public` or `static`) and import the module by its
address at run time, as in section 2.2. The bundler then never has to understand WebAssembly.

`liquers_web.d.ts` declares every function and class of the module, with the shapes of the plain
objects they take and return (`LiquersCommandSpec`, `LiquersStoreConfig`, `LiquersStoreObject`,
…). Keep it next to `liquers_web.js` and an editor with TypeScript support checks your calls and
shows the documentation. The same documentation, as pages, is the
[API reference](https://orest-d.github.io/liquers/api/js/index.html).

## 2.7 What is in the module

| Purpose | Names |
|---|---|
| Lifecycle | `init` (the default export), `liquers.init`, `isInitialized`, `shutdown`, `version` |
| Running queries | `evaluate`, `getAsset`, `Environment`, `Asset`, `State`, `Value` |
| Writing commands | `registerCommand`, `registerCommands`, `validateCommands`, `unregisterCommand`, `describeCommand`, `opaque` |
| Errors | `LiquersError` |
| The query language | `Query`, `Key`, `encodeParam` |
| Pipelines | `evaluatePipeline`, `pipelineToQuery`, `queryToPipeline`, `PipelineEditor`, `QueryEditor` |
| Templates | `render`, `expand`, `validateTemplate` |
| Stores | `Environment.configureStore`, `Environment.registerStoreObject`, `Store` |
| Record streams, in a `records` build | `RecordBatch`, `RecordSource`, `RecordStream` |
| User interfaces | `mountUI`, `mountRenderer`, `defineElement`, `undefineElement`, `element`, `UIElement`, `UIMount` |

The commands the module brings, as opposed to its JavaScript functions, are listed by the query
`commands_doc` and in appendix B.

## 2.8 Things to know before you start

- **One environment per page.** The functions of the module act on it (chapter 8).
- **Register commands first, then evaluate.** A command registered after the first evaluation
  works, but it rebuilds the environment and forgets every computed result (section 8.3).
- **Everything is asynchronous.** `evaluate` returns a promise, and there is no synchronous
  form.
- **One thread.** A command that computes for a second freezes the page for a second
  (section 5.7).
- **The module runs under Node too**, which is how you test (chapter 14).
- **Liquers writes its own progress messages to the browser's console**, where a warning about
  a replaced command or a slow load also appears.
