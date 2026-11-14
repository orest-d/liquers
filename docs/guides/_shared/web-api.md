EDITOR: shared fragment (the web API reference: every route of the standard server, the same in every guide), included by appendix E of all three guides. Keep it language-neutral: requests, JSON and YAML only, no host-language code. Written from reference/WEB_API_SPECIFICATION.md; the example answers were taken from a running server.

## E.1 Overview

A Liquers server offers four APIs. With the default prefix `/liquer` they are at:

| API | Base path | What it is for |
|---|---|---|
| Query API | `/liquer/q/` | evaluate a query and get the value, in one request |
| Store API | `/liquer/api/store/` | read and write the files of the store as they are |
| Assets API | `/liquer/api/assets/` | evaluate, submit, watch and manage results; live updates over a WebSocket |
| Recipes API | `/liquer/api/recipes/` | read the recipes |

The prefix and the set of APIs are settings of the server (chapter 12). The paths
in the tables below are relative to the base path of their API.

Two rules hold everywhere:

- **The address is in the path.** A route ends with a *query* (`greet-Ann/shout`,
  `-R/notes/a.txt/-/shout`) or with a *key* (`notes/a.txt`), written as it is. Each route takes
  one of the two, and the tables say which. A key is written bare, without `-R/`.
- **An answer is either a value or an envelope.** Routes that transfer a value answer its
  bytes. Every other route answers JSON in one shape, the *envelope* of section E.2.

## E.2 Answers

### Values

A route that returns a value answers the bytes of the value, in the format that the file name
at the end of the query or key selects. The headers say more:

| Header | Meaning |
|---|---|
| `Content-Type` | the media type of the value, `application/octet-stream` when it is not known |
| `X-Liquers-Status` | the status of the result, such as `Ready` or `Source` |
| `x-liquers-managed` | `true` when the server keeps the result and you can ask about it again; `false` when it was made for this request only (section E.4) |

End a query with a file name, such as `…/eu.csv` or `…/chart.png`, to choose the format and get
a useful `Content-Type`.

### The envelope

```json
{
  "status": "OK",
  "result": true,
  "message": "Contains check completed",
  "query": "-R/notes/a.txt",
  "key": "notes/a.txt"
}
```

| Field | Holds | Present |
|---|---|---|
| `status` | `"OK"` or `"ERROR"` | always |
| `result` | what the route answers; the tables below say what | on success |
| `message` | a sentence for people | always |
| `query`, `key` | what the request was about | when the route knows |
| `error` | the error (section E.3) | on failure |

### Entries

An *entry* is a value together with its metadata, in one answer or one request body:
`{"metadata": {…}, "data": …}`. Routes named `entry` use it. It has three encodings:

| Encoding | Chosen by | `data` is |
|---|---|---|
| CBOR, the default | `?_entry_format=cbor`, or `Accept: application/cbor` | raw bytes |
| JSON | `?_entry_format=json`, or `Accept: application/json` | base64 text |
| bincode | `?_entry_format=bincode`, or `Accept: application/x-bincode` | raw bytes |

For a request body, the encoding is taken from `?_entry_format=`, then from `Content-Type`.

## E.3 Errors

A failed request answers an envelope with `status: "ERROR"` and an HTTP status that follows the
type of the error:

```json
{
  "status": "ERROR",
  "message": "Failed to retrieve data",
  "error": {
    "type": "KeyNotFound",
    "message": "Key not found: 'notes/zzz.txt'"
  }
}
```

The outer `message` says what the route was doing, `error.message` what went wrong.
`error.type` is the error type of chapter 15. `error.query` and `error.key` are present when
the error names one, and `error.traceback`, a list of lines, when the error carries one, as an
error raised by a command usually does.

| HTTP status | Error types | In words |
|---|---|---|
| 400 | `ParseError`, `ParameterError`, `ArgumentMissing`, `TooManyParameters`, `UnknownCommand`, `ActionNotRegistered`, `KeyNotAbsolute` | the request is wrong: the query does not parse, names a command that does not exist, or has wrong arguments |
| 404 | `KeyNotFound`, `AssetNotFound`, `KeyNotSupported`, `AssetNotSupported`, `NotAvailable` | there is nothing at that address |
| 409 | `StatusConflict`, `DependencyVersionMismatch`, `DependencyCycle`, `CommandAlreadyRegistered` | the operation exists, but not for the state the result is in |
| 422 | `ConversionError`, `SerializationError` | a value could not be converted or written in the format asked for |
| 499 | `Cancelled` | the evaluation was cancelled |
| 500 | `General`, `ExecutionError`, `UnexpectedError`, `KeyReadError`, `KeyWriteError`, `ConfigurationError` | a command failed, or the server could not read or write |
| 501 | `NotSupported`, `CacheNotSupported` | the server does not do this |

`KeyNotFound` and `AssetNotFound` are both 404. The first says the store holds nothing at the
key, though a recipe may still make it. The second says nothing is stored and no recipe makes
it.

A path that is no route at all answers 404 with an empty body, and a method the route does not
serve answers 405. On a read-only server the routes that change data are absent, so they answer
in this way.

## E.4 The Query API

| Request | Does |
|---|---|
| `GET /liquer/q/<query>` | evaluates the query, waits, and answers the value |
| `POST /liquer/q/<query>` | the same, with arguments in a JSON body |

```text
GET /liquer/q/greet-Ann            →  200  Hello, Ann!
GET /liquer/q/nosuch               →  400  {"status":"ERROR", …, "error":{"type":"ActionNotRegistered", …}}
```

The request waits for the evaluation, 30 seconds unless the server was set up otherwise. A
longer evaluation answers 500 `ExecutionError` with a message that names the limit. For long
work, submit the query through the Assets API and ask for the result later (section E.6).

### Arguments outside the query

The arguments of the **last action** of a query can be given outside the query text, by name.
This is the way to pass a value that would need escaping, or that is not text:

```text
GET  /liquer/q/greet?name=Bob                          →  Hello, Bob!
POST /liquer/q/greet/shout      body: {"times": 2}     →  HELLO, WORLD!!HELLO, WORLD!!
GET  /liquer/q/report?_link_limits=-R/config/limits.json
```

| Rule | |
|---|---|
| Names | the name of an argument of the last command. An unknown name is 400 `ParameterError`, before anything runs |
| In the URL | text, converted as the same text in the query would be: `?times=2` is the number 2 |
| In a JSON body | `POST` only; the body is one JSON object. Values are passed as they are, so a list or a map can be an argument |
| An argument that takes many values | a repeated name in the URL (`?f=a&f=b`), or a JSON list |
| A link | `_link_<argument>=<query>` gives the argument the result of another query |
| The same name twice | in the URL and in the body, or as a value and as a link: 400 |
| Other names starting with `_` | reserved; 400 |

An argument given this way replaces the one written in the query. The Query API and the routes
`q/data`, `q/entry` and `q/submit` of the Assets API take arguments in this way.

A request with such arguments is evaluated for itself. Its result is not kept under the query,
because the query alone no longer says what was computed, and the answer carries
`x-liquers-managed: false`. The same is true of a volatile query and of a command that declares
that its result is not cached. Nothing is wrong with such a result; you only cannot ask for its
status or cancel it by its query afterwards.

## E.5 The Store API

The Store API reads and writes the store directly. Nothing is evaluated, and no recipe runs: a
key that only a recipe can make is "not found" here. Every route takes a key.

| Request | Answers |
|---|---|
| `GET data/<key>` | the bytes of the file |
| `PUT data/<key>` | stores the body; `result` is the key |
| `DELETE data/<key>` | removes the file; `result` is the key |
| `GET metadata/<key>` | the metadata, as JSON |
| `PUT metadata/<key>` | stores the body, a JSON document, as the metadata |
| `GET entry/<key>` | the file and its metadata as an entry (section E.2) |
| `PUT entry/<key>` | stores both from an entry |
| `DELETE entry/<key>` | as `DELETE data` |
| `GET listdir/<key>` | a list of the keys directly in the folder |
| `GET keys?prefix=<key>` | a list of all keys under the prefix, at any depth; the whole store without `prefix` |
| `GET contains/<key>` | `true` or `false` |
| `GET is_dir/<key>` | `true` or `false` |
| `PUT makedir/<key>` | makes a folder |
| `DELETE removedir/<key>` | removes a folder and everything in it |
| `POST upload/<key>` | stores the files of a `multipart/form-data` body in the folder, each under its file name; `result` is `{"uploaded": [keys]}` |

```text
GET /liquer/api/store/listdir/notes
→  {"status":"OK","result":["notes/a.txt","notes/recipes.yaml"],"message":"Directory listed successfully"}
```

A write through the Store API does not tell the results made from that key that their input
changed. To replace a value so that everything depending on it is computed again, write it
through the Assets API (`POST key/data`, section E.6).

On a read-only server the whole Store API is absent.

## E.6 The Assets API

The Assets API is the web face of what chapter 7 describes: results with a status, which are
computed when asked for, cached, and kept up to date. Its routes come in two families. A route
under `q/` takes a query, and a route under `key/` takes a key. `key/data/notes/loud.txt` and
`q/data/-R/notes/loud.txt` ask for the same result.

Each family has three ways to ask, and only the first two start an evaluation:

| Way | Routes | Starts an evaluation | Answers |
|---|---|---|---|
| ask and wait | `data`, `entry` | yes, when the value is not there | the value, or the error of the evaluation |
| submit | `submit` | yes | at once, with the status |
| look | `info`, `metadata`, `version`, and for keys `contains`, `can_make`, `listdir`, `recover` | never | what is there now |

To run something long: `submit`, then ask `info` from time to time until `status` is `Ready`,
`Error` or `Cancelled`, then get `data`. Or follow it over the WebSocket (section E.7).

Most routes answer an *asset info*, a JSON object describing one result. Its main fields:

| Field | Holds |
|---|---|
| `query`, `key` | what the result is of |
| `status` | `None`, `Recipe`, `Submitted`, `Dependencies`, `Processing`, `Partial`, `Storing`, `Ready`, `Error`, `Cancelled`, `Expired`, `Source`, `Override`, `Volatile` or `Directory` |
| `type_identifier`, `data_format`, `media_type`, `filename`, `file_size` | what kind of value it is |
| `title`, `description`, `unicode_icon` | text for people |
| `message` | the last line of the log |
| `progress` | `message`, `done`, `total` and `eta`, while it is being computed |
| `is_error`, `error_data` | whether it failed, and how |
| `is_dir`, `is_volatile`, `expires`, `expiration_time`, `updated` | further facts |

### Routes that take a query

| Request | Answers |
|---|---|
| `GET q/data/<query>`, `POST q/data/<query>` | the value, after waiting for it |
| `GET q/entry/<query>`, `POST q/entry/<query>` | the value and its metadata as an entry. When the evaluation fails, `error.metadata` holds the metadata of the failed result, with its log |
| `POST q/submit/<query>`, `GET q/submit/<query>` | starts the evaluation and answers the asset info at once, with a field `managed` |
| `GET q/info/<query>` | the asset info; 404 `NotAvailable` when nobody has asked for the query yet |
| `GET q/metadata/<query>` | the whole metadata, with the log |
| `GET q/version/<query>` | `{"version": "…"}`, 32 hexadecimal digits; all zeros while it is not known |
| `POST q/cancel/<query>` | asks a running evaluation to stop and answers the asset info |

`q/data`, `q/entry` and `q/submit` take arguments outside the query as the Query API does
(section E.4). Cancelling never starts anything, and it always answers 200: read the outcome
from the status. A command that cannot be interrupted finishes, and the result is then `Ready`.

```text
POST /liquer/api/assets/q/submit/greet-Ann/shout
→  {"status":"OK","result":{"query":"greet-Ann/shout","status":"Processing", …,"managed":true}, …}
GET  /liquer/api/assets/q/info/greet-Ann/shout
→  {"status":"OK","result":{"query":"greet-Ann/shout","status":"Ready","type_identifier":"Text", …}, …}
GET  /liquer/api/assets/q/data/greet-Ann/shout
→  HELLO, ANN!!
```

### Routes that take a key: reading

| Request | Answers |
|---|---|
| `GET key/data/<key>` | the value: the stored file, or the result of the recipe, computed if need be |
| `GET key/entry/<key>` | the same as an entry |
| `POST key/submit/<key>`, `GET key/submit/<key>` | starts the recipe and answers the asset info at once |
| `GET key/info/<key>` | the asset info of the result, of the stored file, or of the recipe, whichever exists first |
| `GET key/metadata/<key>` | the metadata |
| `GET key/version/<key>` | `{"version": "…"}` |
| `GET key/contains/<key>` | `{"contains": true}` when the key is stored or listed by a recipe |
| `GET key/can_make/<key>` | `{"can_make": true}` when the key is stored or some recipe can make it |
| `GET key/listdir/<key>` | `{"assets": [asset info, …]}` for the folder, folders first, with the keys that recipes declare. With `?deep=true`, `{"keys": […]}` for everything below |
| `GET key/recover/<key>` | the last known value as an entry, whatever its status, an expired one included |

`key/info` of a key that has a recipe and no result yet answers `status: "Recipe"`. This is how
a file browser shows what a folder *can* contain.

### Routes that take a key: changing

| Request | Does |
|---|---|
| `POST key/data/<key>` | stores the body as the value of the key; answers 201 and the asset info |
| `POST key/entry/<key>` | the same from an entry |
| `DELETE key/data/<key>`, `DELETE key/entry/<key>` | removes the value; answers `{"removed": true, "new_status": "Recipe"}`, or `"None"` when no recipe makes the key |
| `PUT key/makedir/<key>` | makes a folder; 201 |
| `DELETE key/removedir/<key>` | removes a folder and everything in it |
| `POST key/description/<key>` | sets the title and the description of a `Source` from a body `{"title": …, "description": …}` |
| `POST key/expire/<key>` | marks the result expired, and with it everything made from it |
| `POST key/override/<key>` | pins the current value: it becomes an `Override` and is no longer computed again |
| `POST key/cancel/<key>` | asks a running evaluation to stop |

A value written with `POST key/data` is described by parameters in the URL: `type_identifier`
(`Bytes` when left out), `data_format`, `media_type`, `title` and `description`. Nothing else
can be set from outside; the status is decided by the server. A key without a recipe becomes a
`Source`, and a key with one becomes an `Override`, a value that stands in for the recipe's
result. Either way, everything made from the key is marked expired and is computed again when
it is next asked for.

```text
POST /liquer/api/assets/key/data/notes/c.txt?type_identifier=Text&data_format=txt&title=C    body: cheese
→  201  {"status":"OK","result":{"key":"notes/c.txt","status":"Source","type_identifier":"Text","title":"C", …}, …}
```

Removing depends on what the key is. A `Source` or an `Override` is deleted, and a key with a
recipe falls back to it. A computed result is dropped and will be computed again. A folder is
refused with 409; use `key/removedir`.

### Administration

| Request | Does |
|---|---|
| `POST admin/audit/<key>`, `POST admin/audit` | checks the recorded dependencies of the key, or of everything, and expires what no longer holds; answers `{"checked": […], "expired": […]}` |
| `POST admin/refresh_command_versions` | notices changed commands and expires the results made by them |

On a read-only server the routes that change data, and the administration routes, are absent.
Reading, submitting and cancelling stay.

## E.7 Live updates: the WebSocket

The Assets API has two WebSocket addresses, one for each family:

| Address | Follows |
|---|---|
| `/liquer/api/assets/ws/q`, or `…/ws/q/<query>` | queries |
| `/liquer/api/assets/ws/key`, or `…/ws/key/<key>` | keys |

A query or key in the address is subscribed when the connection opens. All messages are JSON
text. Subscribing *asks for* the result, as `submit` does; there is no way to only watch.

**What the client sends:**

```json
{"action": "subscribe", "query": "greet-Ann/shout"}
{"action": "subscribe", "key": "notes/loud.txt"}
{"action": "subscribe", "query": "report", "arguments": {"limit": 20}, "request_id": "r1"}
{"action": "unsubscribe", "query": "greet-Ann/shout"}
{"action": "unsubscribe", "subscription_id": "7c9e…"}
{"action": "cancel", "subscription_id": "7c9e…"}
{"action": "unsubscribe_all"}
{"action": "ping"}
```

Use `query` on `ws/q` and `key` on `ws/key`. `arguments` are arguments outside the query
(section E.4), on `ws/q` only. A subscription with arguments is answered at once by a
`Subscribed` message with a `subscription_id`, and that id is how you unsubscribe from it or
cancel it, since its query alone does not name it.

**What the server sends.** Every notification about a result has these fields:

| Field | Holds |
|---|---|
| `type` | what happened; see below |
| `query`, `key` | what you subscribed to |
| `info` | the asset info as it is *after* the change; `null` after a removal |
| `timestamp` | when |
| `managed` | whether the server keeps the result |
| `subscription_id` | for a subscription that has one |

| `type` | Means | Also carries |
|---|---|---|
| `Initial` | sent once, when you subscribe: the state now | |
| `JobSubmitted`, `JobStarted`, `ValueProduced`, `JobFinished` | steps of the evaluation | |
| `StatusChanged` | the status changed; a cancellation arrives so | `status` |
| `PrimaryProgressUpdated`, `SecondaryProgressUpdated` | progress | `progress` |
| `LogMessage` | a line was added to the log; its text is `info.message` | |
| `ErrorOccurred` | the evaluation failed | `error` |
| `Expired` | the result expired | |
| `Removed` | the value was removed or replaced | |
| `Subscribed` | the answer to a subscription with arguments | `subscription_id`, `request_id` |
| `Pong`, `UnsubscribedAll` | answers to `ping` and `unsubscribe_all` | |
| `Error` | a message of yours could not be handled | `error` |

Three things to know when you write a client:

- **Read `info`, not the sequence.** When notifications come faster than they can be sent, the
  intermediate ones are dropped. `info` always holds the current state, and the last
  notification of an evaluation is always delivered.
- **A subscription follows one result and ends with it**: after a notification whose
  `info.status` is `Error`, `Cancelled`, `Expired` or `Volatile`, or after `Removed`. To keep
  following a key through its changes, subscribe again when that happens.
- **Limits.** A connection may hold 256 subscriptions, and a message of yours may be 64 KiB,
  unless the server was set up otherwise. Closing the connection ends its subscriptions and
  does not cancel the evaluations.

## E.8 The Recipes API

Read-only. Every route but the first takes a key.

| Request | Answers |
|---|---|
| `GET listdir` | the names in the root folder that have a recipe |
| `GET data/<key>` | the recipe as YAML text |
| `GET metadata/<key>` | the metadata of the recipe: title, description, file name |
| `GET entry/<key>` | both, as an entry |
| `GET resolve/<key>` | `{"key": …, "query": …, "plan": {…}}`: the plan of the recipe, with its steps and dependencies |

```text
GET /liquer/api/recipes/data/notes/loud.txt
→  query: -R/notes/a.txt/-/shout/loud.txt
   title: Loud
   cwd: notes
   …
```

The plan is meant for inspection: its form may change between versions. To list the recipes of
a folder other than the root, use `key/listdir` of the Assets API, whose entries have the
status `Recipe` for keys that only a recipe makes.

## E.9 The web folder

Not an API, but served by the same server: `GET /liquer/web/<path>` answers the file stored at
`web/<path>`, a path ending in `/` answers that folder's `index.html`, and `/` sends the browser
to `/liquer/web/`. It serves stored files only and never evaluates a query. Chapter 12 describes
it with the server's settings.
