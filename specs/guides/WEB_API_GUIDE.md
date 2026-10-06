---
title: Web API Guide
kind: guide
audience: both
area: [axum, web]
reviewed: 2026-10-06
---
# Using the Liquers web API

This guide shows how to use a Liquers server over HTTP and WebSocket: evaluating queries, running
long evaluations in the background, reading and writing files, working with assets, and following
progress live. Every example was run against the example server described below.

The exact routes, result shapes and status codes are in the reference,
[`WEB_API_SPECIFICATION.md`](../reference/WEB_API_SPECIFICATION.md). This guide is the *how*; the
specification is the *what*.

## 1. The example server

```bash
mkdir -p /tmp/liquers-store
LIQUERS_STORE_PATH=/tmp/liquers-store cargo run -p liquers-axum --example assets_recipes_basic
```

It listens on `http://localhost:3000`, keeps its files in `$LIQUERS_STORE_PATH`, and registers five
small commands: `text-<string>` (makes a text value), `upper`, `reverse`, `count`, and
`wait-<seconds>` (waits, then passes its input through — handy for trying long evaluations). The
examples use two shell variables:

```bash
B=http://localhost:3000/liquer          # the server's base
A=$B/api/assets                          # the Assets API
```

A real deployment chooses its own base paths and commands; see
[§8 of the specification](../reference/WEB_API_SPECIFICATION.md#8-assembling-a-server).

## 2. How the API is organized

A server mounts up to four APIs, each under its own base path:

| API | Base (in the example) | Use it to |
|---|---|---|
| Query API | `/liquer/q` | evaluate a query and get the value in one request |
| Store API | `/liquer/api/store` | read and write files, exactly as stored |
| Assets API | `/liquer/api/assets` | evaluate, cache, observe and manage values; submit long evaluations; get notifications |
| Recipes API | `/liquer/api/recipes` | read the recipes that define computed files |

**Queries and keys.** Liquers addresses data in two ways:

- A **key** is a path in the store: `notes/todo.txt`.
- A **query** is a Liquers query: a pipeline of actions such as `text-hello/upper`, optionally
  starting from a stored resource: `-R/notes/todo.txt/-/upper`. A query ending in a file name
  (`text-hello/upper/greeting.txt`) gives its result that name, which also decides its format and
  media type.

The Store API and the Assets API's `key/` routes take keys. The Query API and the Assets API's
`q/` routes take queries. Giving a query to a key route is an error that tells you to use `/q/`.

**Responses.** A route that returns *data* returns the bytes themselves, with the media type in
`Content-Type` and the asset's status in `X-Liquers-Status`. Every other route returns a JSON
envelope:

```json
{"status":"OK","result":{"contains":true},"message":"Contains","query":"-R/notes/todo.txt","key":"notes/todo.txt"}
```

An error has `"status":"ERROR"`, a matching HTTP status code, and an `error` object whose `type`
names the problem:

```json
{"status":"ERROR","message":"Query evaluation failed","error":{"type":"ActionNotRegistered","message":"Action 'nosuch' not registered in namespaces '', 'root'"}}
```

The common codes: 400 for a malformed query, key or parameter; 404 for something that does not
exist (or a query nobody has submitted); 409 when an operation does not apply to the asset's
current state; 422 when a value cannot be converted or serialized; 500 for execution and storage
failures. The full table is in
[§3 of the specification](../reference/WEB_API_SPECIFICATION.md#3-errors).

## 3. Evaluating a short query

Use the Query API: the request waits for the evaluation and returns the value.

```bash
curl $B/q/text-hello/upper
# HELLO

curl -i $B/q/text-hello/upper/greeting.txt
# HTTP/1.1 200 OK
# content-type: text/plain
# x-liquers-status: Ready
#
# HELLO
```

Without a file name the value is served as `application/octet-stream`; add one
(`…/greeting.txt`, `…/data.json`) to choose the format.

The wait is bounded (30 seconds by default, `QueryApiBuilder::with_timeout` on the server). For
anything that may take longer, submit it instead (§5).

## 4. Parameters

Parameters are part of the query, not of the URL's query string: an action's parameters follow its
name, separated by `-`.

```bash
curl $B/q/text-hello/wait-1/upper     # text("hello"), wait(1), upper
# HELLO
```

Characters that the query language uses — `/`, `-`, space, `~`, and anything outside ASCII — are
escaped with `~`. A space is `~.`:

```bash
curl "$B/q/text-hello~.world/upper"
# HELLO WORLD
```

`~_` is `-`, `~/` is `/`, `~~` is `~`, and any character can be written as `~U<hex>~`. Programs
should not escape by hand: build the query and encode it (the Rust `ActionRequest::encode`, see the
[Query Escaping Guide](QUERY_ESCAPING_GUIDE.md)). To check what a query means without running it,
use `liquers-validate` (see `CLAUDE.md`, "Validating queries").

A `?name=value` query string is **not** passed to commands; the Query API's `POST` accepts a JSON
body but does not use it yet.

## 5. Long evaluations: submit, then poll

The Assets API evaluates in the background. `submit` starts the evaluation and answers at once
with the asset's status; `info` reports the status without ever starting anything; `data` returns
the value (waiting if it is not ready).

```bash
curl -X POST $A/q/submit/text-hi/wait-3/upper
# {"status":"OK","result":{"status":"Processing", …},"message":"Submitted", …}

curl $A/q/info/text-hi/wait-3/upper
# {"status":"OK","result":{"status":"Dependencies", …}, …}
#   … poll until "status":"Ready" …

curl $A/q/data/text-hi/wait-3/upper
# HI
```

`info` of a query that was never submitted (or has been evicted) is a 404 that says so, and
polling it never starts an evaluation:

```bash
curl $A/q/info/text-other/wait-3/upper
# {"status":"ERROR","message":"Asset info not available (submit the query first)","error":{"type":"NotAvailable", …}}
```

The statuses you will see while polling: `Submitted`, `Processing`, `Dependencies` (waiting for an
input), then a final one — `Ready` (the value is available), `Error` (the `result.message` and
`result.error_data` say why), `Cancelled`, or `Expired`. `result.progress` carries `message`,
`done` and `total` for commands that report progress. `GET q/version/…` returns 32 zeros until the
value exists. `POST q/cancel/…` cancels a submitted query.

The same in Python, with `requests`:

```python
"""Submit a long-running query, poll its status, then fetch the result."""
import sys
import time

import requests  # pip install requests

ASSETS = "http://localhost:3000/liquer/api/assets"


def run(query: str, interval: float = 0.5, timeout: float = 600) -> bytes:
    info = requests.post(f"{ASSETS}/q/submit/{query}").json()
    if info["status"] != "OK":
        raise RuntimeError(info["error"])
    deadline = time.monotonic() + timeout
    while True:
        status = info["result"]["status"]
        progress = info["result"]["progress"]
        print(f"{status:<12} {progress['done']}/{progress['total']} {progress['message']}")
        if status in ("Ready", "Source", "Override"):
            return requests.get(f"{ASSETS}/q/data/{query}").content
        if status in ("Error", "Cancelled", "Expired"):
            raise RuntimeError(f"{status}: {info['result']['message']}")
        if time.monotonic() > deadline:
            raise TimeoutError(query)
        time.sleep(interval)
        info = requests.get(f"{ASSETS}/q/info/{query}").json()
        if info["status"] != "OK":  # e.g. 404 NotAvailable: evicted meanwhile
            raise RuntimeError(info["error"])


print(run(sys.argv[1] if len(sys.argv) > 1 else "text-hello/wait-2/upper"))
```

```text
$ python3 poll.py 'text-polling/wait-2/upper'
Processing   0/0
Dependencies 0/0
…
Ready        0/0
b'POLLING'
```

Keyed assets work the same way under `key/`: `POST $A/key/submit/notes/shout.txt`, then
`GET $A/key/info/notes/shout.txt`.

## 6. Files: the Store API

The Store API reads and writes the store exactly as stored, with no evaluation.

```bash
# write, read, delete
curl -X PUT --data-binary 'Hello from the store' $B/api/store/data/docs/readme.txt
# {"status":"OK","result":"docs/readme.txt","message":"Data stored successfully"}
curl $B/api/store/data/docs/readme.txt
# Hello from the store
curl -X DELETE $B/api/store/data/docs/readme.txt

# navigate
curl $B/api/store/listdir/docs          # {"status":"OK","result":["docs/readme.txt"], …}
curl $B/api/store/keys                  # the keys at the root: {"result":["docs"], …}
curl $B/api/store/is_dir/docs           # {"result":true, …}
curl $B/api/store/contains/docs/readme.txt

# directories
curl -X PUT $B/api/store/makedir/tmp
curl -X DELETE $B/api/store/removedir/tmp      # removes everything in it

# metadata, and data + metadata together
curl $B/api/store/metadata/docs/readme.txt
curl "$B/api/store/entry/docs/readme.txt?format=json"
# {"metadata":{ … },"data":"SGVsbG8gZnJvbSB0aGUgc3RvcmU="}   (base64 in JSON; CBOR by default)

# upload files (multipart)
curl -F "file=@data.csv" $B/api/store/upload/uploads
# {"status":"OK","result":{"uploaded":["uploads/data.csv"]}, …}
```

Writes are `PUT`. A file written with plain `PUT data` carries no media type of its own and is
served as `application/octet-stream`; to write a value with a type, a title or a description, use
the Assets API (§7). Servers built with `with_destructive_gets()` also accept
`GET remove|removedir|makedir/{key}`, for clients that can only issue GETs.

## 7. Assets: values with a lifecycle

The Assets API's key family works on the same store, through the asset manager: it knows statuses,
versions, recipes and dependencies, and it never evaluates on an observe route.

### Writing and reading a value

```bash
curl -X POST --data-binary 'Buy milk' \
  "$A/key/data/notes/todo.txt?type_identifier=Text&data_format=txt&title=Todo&description=Shopping%20list"
# 201 {"status":"OK","result":{"key":"notes/todo.txt","status":"Source","title":"Todo",
#      "type_identifier":"Text","media_type":"text/plain", …},"message":"Value set"}

curl -i $A/key/data/notes/todo.txt
# content-type: text/plain
# x-liquers-status: Source
#
# Buy milk
```

Only five descriptive fields can be set — `type_identifier` (default `Bytes`), `data_format`,
`media_type`, `title`, `description` — as query parameters of `POST key/data`, or in the
`metadata` object of a `POST key/entry` body (a `DataEntry`, like the Store API's). Anything else
is ignored and listed in the response `message`. Status, version and the rest belong to the asset
manager: a value you write becomes a `Source`, or an `Override` if a recipe also defines the key.

### Navigating and describing without evaluating

```bash
curl $A/key/listdir/notes                 # {"result":{"assets":[ AssetInfo, … ]}, …}
curl "$A/key/listdir?deep=true"           # every key: {"result":{"keys":["docs","docs/readme.txt", …]}}
curl $A/key/info/notes/todo.txt           # status, title, description, media type, …
curl $A/key/metadata/notes/todo.txt       # the full metadata record
curl $A/key/version/notes/todo.txt        # {"result":{"version":"51fc1b88…"}}
curl $A/key/contains/notes/todo.txt       # {"result":{"contains":true}}  stored, or listed by a recipe provider
curl $A/key/can_make/notes/todo.txt       # {"result":{"can_make":true}}  stored, or producible (a template chunk is, though unlisted)

curl -X POST -H 'Content-Type: application/json' -d '{"description":"Weekly shopping"}' \
  $A/key/description/notes/todo.txt       # title/description of a Source; data and version unchanged
```

`listdir` returns `AssetInfo` for each entry — status, title, description, type, size — so a
client can show a directory with its descriptions in one request, without reading any data.

### Computed files: recipes

A recipe defines a key by a query. Recipes live in a `recipes.yaml` in the directory they define;
write one with the Store API:

```yaml
recipes:
- query: -R/notes/todo.txt/-/upper/wait-2/shout.txt
  title: Shouted todo
  description: The todo list, upper-cased (takes two seconds)
```

```bash
curl -X PUT --data-binary @recipes.yaml $B/api/store/data/notes/recipes.yaml

curl $A/key/info/notes/shout.txt          # status "Recipe", title "Shouted todo" — not evaluated
curl -X POST $A/key/submit/notes/shout.txt    # starts it: status "Processing"
curl $A/key/info/notes/shout.txt          # … "Ready"
curl $A/key/data/notes/shout.txt          # BUY MILK
curl $B/api/recipes/data/notes/shout.txt  # the recipe itself
```

The file name at the end of the recipe's query (`shout.txt`) is the key it defines, in the
directory of the `recipes.yaml`.

### Dependencies, recovery and removal

When a value changes, whatever was computed from it expires, and the next read recomputes it:

```bash
curl -X POST --data-binary 'Buy bread' "$A/key/data/notes/todo.txt?type_identifier=Text"
curl $A/key/info/notes/shout.txt          # "status":"Expired"
curl "$A/key/recover/notes/shout.txt?format=json"   # the last value anyway: "BUY MILK" (base64)
curl $A/key/data/notes/shout.txt          # recomputes (two seconds): BUY BREAD
```

- `POST key/override/{key}` keeps the current value — even an expired one — as an `Override`, so
  it is no longer recomputed.
- `POST key/expire/{key}` expires a computed value and its dependents (a `Source` cannot expire:
  409).

`DELETE key/data/{key}` does what fits the key, and tells you what the key is afterwards:

```bash
curl -X DELETE $A/key/data/notes/shout.txt
# {"result":{"removed":true,"new_status":"Recipe"}, …}    computed value dropped; the recipe stays
curl -X DELETE $A/key/data/notes/todo.txt
# {"result":{"removed":true,"new_status":"None"}, …}      your value deleted; dependents expire
curl -X DELETE $A/key/data/notes
# 409 {"error":{"type":"StatusConflict","message":"Cannot remove 'notes': asset status is Directory; use key/removedir …"}}
curl -X DELETE $A/key/removedir/notes     # the directory, its files and its recipes
curl -X PUT $A/key/makedir/archive        # 201, a new directory
```

Queries work the same way on the `q/` family: `q/data`, `q/entry`, `q/info`, `q/metadata`,
`q/version`, `q/submit`, `q/cancel`. A keyed query there (`q/info/-R/notes/todo.txt`) is answered
like the key.

A server may be built `read_only()` (no writes or deletes through the Assets API) or without the
`admin/` routes; a route that is switched off answers 404 or 405.

## 8. Live updates: the WebSocket

Instead of polling, subscribe on the WebSocket: `ws://…/api/assets/ws/q` for queries,
`…/ws/key` for keys. (In the example server the base is `ws://localhost:3000/liquer/api/assets/ws`.)

Send JSON messages:

```json
{"action": "subscribe", "query": "text-hello/wait-2/upper"}
{"action": "subscribe", "key": "notes/todo.txt"}
{"action": "unsubscribe", "query": "text-hello/wait-2/upper"}
{"action": "unsubscribe_all"}
{"action": "ping"}
```

`query` goes to `ws/q`, `key` to `ws/key`; a path in the URL (`…/ws/key/notes/todo.txt`) subscribes
on connect. Subscribing *requests* the asset, like `submit`.

Each notification has a `type` (`Initial`, `JobSubmitted`, `JobStarted`, `StatusChanged`,
`ValueProduced`, `ErrorOccurred`, `LogMessage`, `PrimaryProgressUpdated`, `JobFinished`,
`Expired`, `Removed`, …) and an `info` object: the asset's status re-read at that moment. Rely on
`info.status` rather than on the sequence of types, because quick successive changes can be merged.
A subscription follows one asset and ends when that asset finishes badly (`Error`, `Cancelled`,
`Expired`) or is removed or replaced (`Removed`); subscribe again to follow its successor. A request
that cannot be handled gets an `Error` message.

```python
"""Follow a query's evaluation over the Assets API WebSocket."""
import asyncio
import json
import sys

import websockets  # pip install websockets

WS = "ws://localhost:3000/liquer/api/assets/ws"
TERMINAL = {"Error", "Cancelled", "Expired", "Volatile"}


async def follow(query: str) -> None:
    async with websockets.connect(f"{WS}/q") as ws:
        await ws.send(json.dumps({"action": "subscribe", "query": query}))
        async for text in ws:
            msg = json.loads(text)
            info = msg.get("info") or {}
            print(f'{msg["type"]:<24} status={info.get("status")}')
            if msg["type"] == "Error":  # the subscribe itself was refused
                print(msg["error"])
                return
            # A subscription ends with its asset; stop when it is finished or gone.
            if msg["type"] in ("JobFinished", "Removed") or info.get("status") in TERMINAL:
                return


asyncio.run(follow(sys.argv[1] if len(sys.argv) > 1 else "text-hello/wait-2/upper"))
```

```text
$ python3 follow.py 'text-websocket/wait-2/upper'
Initial                  status=Processing
StatusChanged            status=Dependencies
…
ValueProduced            status=Ready
JobFinished              status=Ready

$ python3 follow.py 'nosuch'
Error                    status=None
{'type': 'ActionNotRegistered', 'message': "Action 'nosuch' not registered in namespaces '', 'root'", 'query': ''}
```

Notifications are a convenience, not a guarantee: for a definitive answer, `GET …/info`. A
runnable Rust client that also follows a key through its deletion is
`liquers-axum/examples/websocket_client.rs`.

## 9. Things to know

- There is no authentication or access control in the server; put it in front of the routers, or
  build them `read_only()`. See [§9 of the specification](../reference/WEB_API_SPECIFICATION.md#9-access-control).
- The Query API's timeout ends the request, not the need for the value: submit long work through
  the Assets API.
- With an inline asset manager (`EvalMode::Inline`, not the default server setup), `submit`
  evaluates before it answers and returns the final status.
- Known gaps are filed under `specs/issues/`: the Recipes API's `metadata` is a placeholder
  (`AXUM-RECIPES-METADATA-AND-ENTRY-ARE-PLACEHOLDERS`), Store API `keys` lists only one level
  (`AXUM-STORE-KEYS-LISTS-ONLY-DIRECT-CHILDREN`), and cancelling a running command may still end
  `Ready` (`ASSET-CANCEL-DURING-PROCESSING-FINISHES-READY`).

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-06 | Added a `key/can_make` example beside `key/contains`. | phase-5 |
| 2026-09-29 | Created: organization and conventions, short queries, parameters, submit and poll, the Store API, the Assets API (writing, navigating, recipes, dependencies, removal), and the WebSocket, with curl and Python examples run against `examples/assets_recipes_basic.rs`. | `design/axum-assets-endpoints/` |
