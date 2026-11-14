---
title: Serving and embedding
status: ready
sources: [guides/WEB_API_GUIDE.md, guides/PYTHON_GUIDE.md, reference/PYTHON_INTEGRATION.md]
reviewed: 2026-11-14
---
# 12. Serving and embedding

PURPOSE: The reader can use Liquers inside a Python application or service, call a Liquers server from Python, and share results between the two.

There are two ways to connect Liquers with the rest of your software. You can *embed* it: your
Python program creates an environment and evaluates queries itself. Or you can *serve* it: a
Liquers server evaluates queries, and other programs ask it over HTTP. Your program can start that
server for its own environment, Python commands included, or call a server running elsewhere.
They combine well, because both read the same queries and can share one store.

## 12.1 Embedding in a script or a notebook

For a script, the default environment is enough. Register commands with `@lq.command`, evaluate
with `lq.evaluate`.

In a notebook, cells are run again and again. Running a cell that defines a command registers
it again, replaces the old definition with a warning, and drops the cached results, so the next
evaluation uses the new code. That is the behaviour you want while exploring. Put the
definitions of commands in cells of their own, so that re-running an analysis cell does not
discard the cache.

## 12.2 Embedding in a service

In a long-running program, create one environment at startup and use it for the life of the
process:

```python
import liquers as lq

def create_environment(data_folder: str) -> lq.Environment:
    env = lq.Environment(config={
        "store": {"stores": [
            {"type": "filesystem", "prefix": "data", "config": {"path": data_folder}},
        ]},
        "assets": {"job_capacity": 8},
    })

    @env.command
    def shout(text) -> str:
        return text.upper()

    return env

if __name__ == "__main__":
    import tempfile
    env = create_environment(tempfile.mkdtemp())
    try:
        env.store_set("data/note.txt", "milk")
        print(env.evaluate("-R/data/note.txt/-/shout"))      # MILK
    finally:
        env.shutdown()
```

The points to keep in mind:

- **One environment per process.** The cache of results lives in the environment. Two environments
  share only what is written to a common store.
- **Register everything first.** A registration after the first evaluation rebuilds the
  environment and empties the cache (chapter 8).
- **Share the environment between threads.** It is safe to use from many threads, and
  `job_capacity` bounds the work (chapter 8).
- **In an async web framework**, call `await env.evaluate_async(query)` in your handlers, so
  that a long evaluation does not block the event loop.
- **Shut down on exit**, so that running evaluations are stopped and stores are released.

A handler that evaluates a query taken from a request has turned your service into a query
server. Think about who may send queries before you do that: a query can call any registered
command and read any key of the store. Parsing and planning check that a query is well formed.
They do not check that the caller is allowed to ask for it.

## 12.3 Serving an environment

An environment can answer other programs over HTTP: a browser, `curl`, a service written in
another language. `env.serve()` starts the standard Liquers server for it. The server runs beside
your Python code, so a notebook or a script keeps working while it serves.

### Starting a server

```python
env = lq.Environment()

@env.first_command
def greet(name: str = "world") -> str:
    return f"Hello, {name}!"

server = env.serve()            # http://127.0.0.1:8080
```

Open `http://127.0.0.1:8080/liquer/q/greet-Ann` in a browser and it answers `Hello, Ann!`.
`serve` returns once the server listens; the server then runs in the background until you stop it.
A command you register after `serve` is served at once.

`server` is an `lq.Server`: `server.address` and `server.port` say where it listens,
`server.url` is its base address, and `server.running` says whether it still runs. With `port=0`
the server picks a free port, and `server.port` reports the one it got, which suits tests. As a
`with` block, the server stops at the block's end:

```python
import urllib.request
import liquers as lq

env = lq.Environment()

@env.first_command
def greet(name: str = "world") -> str:
    return f"Hello, {name}!"

with env.serve(port=0) as server:
    answer = urllib.request.urlopen(f"{server.url}/liquer/q/greet-Ann").read()
    assert answer == b"Hello, Ann!"
assert not server.running
env.shutdown()
```

The module-level `lq.serve()` serves the default environment, the one `@lq.command` registers in.

### Serving in the foreground

A script whose only job is to serve passes `background=False`. `serve` then runs until you press
Ctrl-C, which stops the server and raises `KeyboardInterrupt`:

```python
env.serve(port=8000, background=False)
```

### Serving from asyncio

`await env.serve_async()` serves until the task awaiting it is cancelled, which stops the server
gracefully. To serve beside other work, run it as a task:

```python
task = asyncio.create_task(env.serve_async(port=0))
...
task.cancel()
```

### Settings

The server's settings come from the `server` section of the environment's configuration file
([8. The environment](08-environment.md)), and each keyword of `serve` overrides one of them:
`host`, `port`, `prefix`, `apis`, `read_only` and `web`.

```python
env = lq.Environment(config=pathlib.Path("liquers.yaml"))
server = env.serve(port=9000)   # the file's settings, except the port
```

{{> ../_shared/serving.md}}

A wrong setting raises `lq.ConfigurationError` from `serve`, naming it; a port in use raises too.

### Stopping a server

`server.shutdown()` stops the server in two steps. It stops accepting connections at once, gives
requests already running up to `timeout` seconds to finish (5 by default), and cuts off the rest.
A client following an asset over the WebSocket receives a close message first. When `shutdown`
returns, the port is free. `await server.shutdown_async()` is the same for asyncio code, and a
second call does nothing.

Dropping the `Server` object does not stop the server. It stops when you call `shutdown`, when
you leave its `with` block, when `env.shutdown()` stops the environment (which stops its servers
first), or when Python exits. `env.servers` lists the environment's running servers.

## 12.4 What a server offers

The server `env.serve()` starts is the standard Liquers server, the same one a Rust program runs
([chapter 12 of *Liquers for Rust*](../rust/12-serving.md)). From the outside, it offers four
interfaces:

| Interface | Path with the default prefix | What it does |
|---|---|---|
| Query API | `/liquer/q/<query>` | evaluates a query and returns the value |
| Store API | `/liquer/api/store/…` | reads and writes files as they are stored |
| Assets API | `/liquer/api/assets/…` | evaluates, watches and manages results; submits long computations |
| Recipes API | `/liquer/api/recipes/…` | reads the recipes |

A query goes into the URL as it is. This is the reason queries are written with URL-safe
characters only.

Every route of the four interfaces, with what it takes and what it answers, is listed in
[appendix E](e-web-api.md).

## 12.5 Calling a server from Python

Any HTTP client will do. With the standard library:

```python
import urllib.request

BASE = "http://localhost:3000/liquer"       # a running Liquers server

def fetch(query: str) -> bytes:
    with urllib.request.urlopen(f"{BASE}/q/{query}") as response:
        print(response.headers["Content-Type"], response.headers["X-Liquers-Status"])
        return response.read()

data = fetch("-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.csv")
```

This example needs a server at that address, so it is not complete on its own.

The response is the result as bytes. The file name at the end of the query chooses the format,
`eu.csv` here, and the server reports the media type in `Content-Type` and the status of the
asset in `X-Liquers-Status`. Without a file name, the value is sent as
`application/octet-stream`, so always end a query for a server with a file name.

An error comes back with an HTTP error status and a JSON body that names the type of the error,
the same types as in chapter 15:

```json
{"status": "ERROR", "message": "Query evaluation failed",
 "error": {"type": "ActionNotRegistered", "message": "Action 'nosuch' not registered"}}
```

The usual codes are 400 for a malformed query, 404 for something that does not exist, 422 for a
value that cannot be converted to the requested format, and 500 for a failing command.

The request waits for the result, up to a limit set by the server, 30 seconds by default. For
longer computations, submit the query through the Assets API and ask for its status later.

Build the query with a query editor or `lq.ActionRequest.from_arguments` (chapter 3) and use
`query.encode()` in the URL. An encoded query needs no further URL quoting.

## 12.6 Sharing a store

A server and a Python program can be pointed at the same storage: the same folder, or the same
bucket. Each side then reads what the other has written, as files with their metadata.

- A result your Python program computes and stores under a key can be served by the server as a
  stored file, although the server has no Python commands.
- A file the server's users upload can be the input of your queries.

A server written in Rust cannot run your Python functions, but it can serve what they produced.
To serve the Python commands themselves, serve their environment (12.3).

Each process keeps its own view of what it has read. A running process does not see at once
that another process has changed a file; it notices when it next reads the file and finds the
content different from what it recorded (chapter 7).
