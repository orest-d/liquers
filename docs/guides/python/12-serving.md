---
title: Serving and embedding
status: draft
sources: [guides/WEB_API_GUIDE.md, guides/PYTHON_GUIDE.md, reference/PYTHON_INTEGRATION.md]
reviewed:
---
# 12. Serving and embedding

PURPOSE: The reader can make Liquers available to other programs: over HTTP, or embedded in an application.

An environment can answer other programs over HTTP: a browser, `curl`, a service written in
another language. `env.serve()` starts the standard Liquers server for it. The server runs beside
your Python code, so a notebook or a script keeps working while it serves.

## 12.1 Starting a server

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def greet(name: str = "world") -> str:
    return f"Hello, {name}!"

server = env.serve()
print(server.url)               # http://127.0.0.1:8080
```

Open `http://127.0.0.1:8080/liquer/q/greet-Ann` in a browser and it answers `Hello, Ann!`.
`serve` returns once the server listens; the server then runs in the background until you stop it.
A command you register after `serve` is served at once.

`server` is an `lq.Server`: `server.address` and `server.port` say where it listens,
`server.url` is its base address, and `server.running` says whether it still runs. With `port=0`
the server picks a free port, and `server.port` reports the one it got, which suits tests:

```python
with env.serve(port=0) as server:
    ...                         # the server runs inside the block and stops at its end
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

## 12.2 Settings

The server's settings come from the `server` section of the environment's configuration file
([8. The environment](08-environment.md)), and each keyword of `serve` overrides one of them:
`host`, `port`, `prefix`, `apis`, `read_only` and `web`.

```python
env = lq.Environment(config=pathlib.Path("liquers.yaml"))
server = env.serve(port=9000)   # the file's settings, except the port
```

{{> ../_shared/serving.md}}

A wrong setting raises `lq.ConfigurationError` from `serve`, naming it; a port in use raises too.

## 12.3 Stopping a server

`server.shutdown()` stops the server in two steps. It stops accepting connections at once, gives
requests already running up to `timeout` seconds to finish (5 by default), and cuts off the rest.
A client following an asset over the WebSocket receives a close message first. When `shutdown`
returns, the port is free. `await server.shutdown_async()` is the same for asyncio code, and a
second call does nothing.

Dropping the `Server` object does not stop the server. It stops when you call `shutdown`, when
you leave its `with` block, when `env.shutdown()` stops the environment (which stops its servers
first), or when Python exits. `env.servers` lists the environment's running servers.

## 12.4 Embedding in notebooks and services

TBD: 12.4 Embedding in notebooks and services: one environment per process; threads.
SOURCE: guides/PYTHON_GUIDE.md §6

## 12.5 Calling a Liquers server

TBD: 12.5 Calling a Liquers server: the HTTP API from Python.
SOURCE: guides/WEB_API_GUIDE.md

The same in Rust: [12. Serving and embedding](../rust/12-serving.md).
