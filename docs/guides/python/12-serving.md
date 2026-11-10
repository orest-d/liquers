---
title: Serving and embedding
status: ready
sources: [guides/WEB_API_GUIDE.md, guides/PYTHON_GUIDE.md, reference/PYTHON_INTEGRATION.md]
reviewed: 2026-11-09
---
# 12. Serving and embedding

PURPOSE: The reader can use Liquers inside a Python application or service, call a Liquers server from Python, and share results between the two.

There are two ways to connect Liquers with the rest of your software. You can *embed* it: your
Python program creates an environment and evaluates queries itself. Or you can *call a server*:
a separate Liquers server evaluates queries, and your program asks it over HTTP. They combine
well, because both read the same queries and can share one store.

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

## 12.3 A Liquers server

Liquers has a ready-made HTTP server. It is a Rust program, built from the same core as the
Python package, and it is described in
[chapter 12 of *Liquers for Rust*](../rust/12-serving.md). From the outside, it offers four
interfaces:

| Interface | Path in the example server | What it does |
|---|---|---|
| Query API | `/liquer/q/<query>` | evaluates a query and returns the value |
| Store API | `/liquer/api/store/…` | reads and writes files as they are stored |
| Assets API | `/liquer/api/assets/…` | evaluates, watches and manages results; submits long computations |
| Recipes API | `/liquer/api/recipes/…` | reads the recipes |

A query goes into the URL as it is. This is the reason queries are written with URL-safe
characters only.

## 12.4 Calling a server from Python

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

## 12.5 Sharing a store

A server and a Python program can be pointed at the same storage: the same folder, or the same
bucket. Each side then reads what the other has written, as files with their metadata.

- A result your Python program computes and stores under a key can be served by the server as a
  stored file, although the server has no Python commands.
- A file the server's users upload can be the input of your queries.

This is the practical way to publish the results of Python commands today. The server cannot
run a Python function, but it can serve what the function produced.

Each process keeps its own view of what it has read. A running process does not see at once
that another process has changed a file; it notices when it next reads the file and finds the
content different from what it recorded (chapter 7).
