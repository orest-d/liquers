---
title: Serving and embedding
status: ready
sources: [guides/WEB_API_GUIDE.md, reference/WEB_API_SPECIFICATION.md]
reviewed: 2026-11-14
---
# 12. Serving and embedding

PURPOSE: The reader can make Liquers available to other programs: over HTTP, or embedded in an application.

A page does not serve anything: a browser cannot listen for requests. In this guide the chapter
is therefore about the other side of a server. A page can talk to a Liquers server over HTTP,
and it can do so with or without the browser module. The server itself is started by a Rust or
Python program ([12. Serving and embedding](../rust/12-serving.md) in the Rust guide,
[12. Serving and embedding](../python/12-serving.md) in the Python guide). This chapter covers
what a page sees of it, and how to divide the work between the two.

The examples of this chapter need a running server, so the tests of this guide do not run them.

## 12.1 Pages served by the server

The standard server serves files from its store's `web/` folder at `/liquer/web/`, and its start
page, `/`, sends the browser there. A page served this way calls the APIs with relative
addresses, on the same origin, without any cross-origin setup. With a server whose program
registered a `greet` command:

<!-- example: not run — it needs a running Liquers server -->
```js
const response = await fetch("/liquer/q/greet-Ann");
console.log(await response.text());       // Hello, Ann!
```

Such a page needs no Liquers in the browser at all: the query is evaluated on the server, with
the server's commands, and the page receives the bytes.

## 12.2 The server's settings

{{> ../_shared/serving.md}}

## 12.3 Calling a server from a page

This section shows the requests a page makes most often. Every route, with what it takes and
what it answers, is listed in [appendix E](e-web-api.md).

**The Query API** evaluates a query and returns the value in one request. The query goes into
the URL as it is, after `/liquer/q/`:

<!-- example: not run — it needs a running Liquers server -->
```js
const BASE = "/liquer";                     // the page is served by the same server

async function fetchQuery(query) {
  const response = await fetch(`${BASE}/q/${query}`);
  if (!response.ok) {
    const body = await response.json();     // { status: "ERROR", message, error: { type, message } }
    throw new Error(`${body.error.type}: ${body.error.message}`);
  }
  return response;
}

const response = await fetchQuery("-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.json");
console.log(response.headers.get("Content-Type"));        // application/json
console.log(response.headers.get("X-Liquers-Status"));    // Ready
const rows = await response.json();
```

What to know about the answers:

- **End the query with a file name.** It chooses the format of the bytes: `eu.json`, `eu.csv`,
  `chart.png`. Without one the value is sent as `application/octet-stream`.
- **An error** has an HTTP error status and a JSON body whose `error.type` names the kind, with
  the same names as chapter 15, written `ActionNotRegistered` instead of
  `action_not_registered`. The usual codes are 400 for a malformed query, 404 for something that
  does not exist, 422 for a value that cannot be converted to the requested format, and 500 for
  a failing command.
- **A query is already safe in a URL.** Build it with `liquers.encodeParam` or a `QueryEditor`
  when it contains values from users (section 3.9), and do not pass it through
  `encodeURIComponent` as well. Without the module, the escapes of section 3.6 are simple enough
  to write by hand for known values.
- **Because a query is a URL**, a result can be used wherever the browser takes one:
  `<img src="/liquer/q/…/chart.png">`, a link to download a CSV file, the `src` of a frame.

**Long evaluations.** A request to the Query API waits for the result, up to a limit the server
sets, 30 seconds by default. For longer work, submit the query through the Assets API, which
answers at once, and ask for its status until it is ready:

<!-- example: not run — it needs a running Liquers server -->
```js
const ASSETS = "/liquer/api/assets";

async function run(query, interval = 500) {
  let info = await (await fetch(`${ASSETS}/q/submit/${query}`, { method: "POST" })).json();
  for (;;) {
    if (info.status !== "OK") throw new Error(info.error.message);
    const { status, progress, message } = info.result;
    console.log(status, `${progress.done}/${progress.total}`, progress.message);
    if (status === "Ready") return fetch(`${ASSETS}/q/data/${query}`);
    if (["Error", "Cancelled", "Expired"].includes(status)) throw new Error(`${status}: ${message}`);
    await new Promise((resolve) => setTimeout(resolve, interval));
    info = await (await fetch(`${ASSETS}/q/info/${query}`)).json();
  }
}

const response = await run("-R/data/big.csv/-/ns-rec/to_record/sort-amount/sorted.csv");
```

`POST …/q/cancel/<query>` stops a submitted evaluation. On a server, cancelling works.

**The Store API** reads and writes the server's files as they are stored, without evaluating
anything:

<!-- example: not run — it needs a running Liquers server -->
```js
const STORE = "/liquer/api/store";

await fetch(`${STORE}/data/notes/todo.txt`, { method: "PUT", body: "buy milk" });
const text = await (await fetch(`${STORE}/data/notes/todo.txt`)).text();
const listing = await (await fetch(`${STORE}/listdir/notes`)).json();     // { status, result: [...] }
await fetch(`${STORE}/data/notes/todo.txt`, { method: "DELETE" });
```

A server started with `read_only: true` has no writing routes (section 12.2).

**Pages on another site.** The browser lets a page call a server on another origin only when the
server allows it with CORS headers, and the standard Liquers server does not send any. The
simple arrangement is the one of section 12.1: let the server serve the page too, from its web
folder. When the page must live elsewhere, put the server behind a reverse proxy that serves
both under one origin, or that adds the headers. The same proxy is the place for
authentication, which the server does not have either.

## 12.4 Live updates

A page that wants to follow an evaluation does not have to ask again and again. The Assets API
has a WebSocket, at `/liquer/api/assets/ws/q` for queries and `…/ws/key` for keys:

<!-- example: not run — it needs a running Liquers server -->
```js
const scheme = location.protocol === "https:" ? "wss" : "ws";
const socket = new WebSocket(`${scheme}://${location.host}/liquer/api/assets/ws/q`);
const query = "-R/data/big.csv/-/ns-rec/to_record/sort-amount/sorted.csv";

socket.addEventListener("open", () => {
  socket.send(JSON.stringify({ action: "subscribe", query }));
});
socket.addEventListener("message", (event) => {
  const notification = JSON.parse(event.data);
  if (!notification.info) return;                 // an answer to ping, for example
  const { status, progress } = notification.info;
  console.log(notification.type, status, progress?.message);
  if (status === "Ready") {
    socket.close();
    // now fetch /liquer/api/assets/q/data/<query>
  }
});
```

Subscribing asks for the asset, so it also starts the evaluation. The server then sends a
notification for each change, with a `type` (`JobStarted`, `StatusChanged`, `LogMessage`,
`PrimaryProgressUpdated`, `JobFinished`, …) and an `info` object with the state of the asset at
that moment. Rely on `info.status` and not on the order of the types: changes that follow each
other quickly can be merged into one notification. The other messages a client can send are
`unsubscribe`, `cancel` and `ping`.

A subscription follows one asset. It ends when the asset ends in `Error`, `Cancelled` or
`Expired`; subscribe again to follow its successor. Notifications are a convenience and not a
guarantee. For a definitive answer, ask for the asset's info.

## 12.5 The module and a server together

The browser module and a server understand the same queries, so a page can use both and decide,
query by query, where the work is done:

| Do it in the page | Do it on the server |
|---|---|
| small data, already in the page | large data, files on disk, databases |
| the user's own data, which should not leave the browser | data shared by all users |
| instant reactions to input | work that takes seconds or longer |
| commands written in JavaScript | Polars, images, search, commands in Python or Rust |
| must work offline | results cached for everybody, and kept fresh |

Two ways to connect them:

**A command that asks the server.** A JavaScript command fetches a query from the server and
returns the answer. To the rest of the page it is a command like any other: its result is kept,
templates and user-interface elements can use it, and the server's work appears as one step of
a page's query.

<!-- example: not run — it needs a running Liquers server -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({
  name: "remote",
  namespace: "app",
  doc: "The JSON result of a query evaluated by the server.",
  async: true,
  arguments: [{ name: "query", type: "string" }],
  run: async (query) => {
    const response = await fetch(`/liquer/q/${query}`);
    if (!response.ok) throw new liquers.LiquersError("execution_error", `server: ${response.status}`);
    return response.json();
  },
});

// The server filters two million rows; the page sorts the answer and shows ten.
const inner = "-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.json";
const top = await liquers.evaluate(
  `ns-app/remote-${liquers.encodeParam(inner)}/ns-rec/to_record/sort-amount-false/head-10/to_json`);
```

The inner query is an argument of `remote`, so it is escaped with `encodeParam`, and it arrives
in the command as the original text.

**The server's files as a store of the page.** An `http` mount can point at the Store API, which
answers a `GET` for a key with the file's bytes. The page's queries then read the server's files
with `-R/`:

<!-- example: not run — it needs a running Liquers server -->
```js
const env = liquers.Environment.global();
await env.configureStore({
  stores: [{
    type: "http",
    prefix: "data",
    read_only: true,
    config: { url_prefix: "/liquer/api/store/data/data/", keys: ["sales.csv", "recipes.yaml"] },
  }],
});
const rows = await liquers.evaluate("-R/data/sales.csv/-/ns-rec/to_record/nrows");
```

The mount is read-only and must be told its keys (section 6.3). A store that also writes to the
server can be written as a `js` store whose methods call the Store API (section 6.5).

Remember that a result kept in the page is not refreshed when the server's data changes
(section 7.7). Declare a command like `remote` as `volatile: true` when the page must always
show what the server has now.

## 12.6 Embedding the module in an application

"Embedding" for a page means using the module inside a larger front end. A few points help it
fit:

- **Set Liquers up in one place**, in an `async` function that loads the module, registers the
  commands and configures the store (section 8.2), and have the rest of the application await
  that one promise.
- **Keep Liquers behind your own functions.** Components call `getSales(region)`, which builds
  the query and evaluates it. The query language stays in one file, and components do not
  depend on it.
- **Load the module when it is needed.** It is a download of several megabytes (section 2.3). A
  dynamic `import()` on the page or the route that needs it keeps the rest of the site fast.
- **Give Liquers the state that is worth sharing.** A view that is the value of a query can be
  put into the address of the page (section 3.11). State that is not worth sharing stays in your
  framework.
- **With a framework's own rendering**, use `mountRenderer` (section 11.5) or plain `evaluate`,
  and let the framework draw.
