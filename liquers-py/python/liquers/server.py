"""The web server: serve an environment over the Liquers HTTP API.

Rust counterpart: the `liquers-axum` crate. Every name here is also available directly from `liquers`
(`liquers.serve is liquers.server.serve`); `import liquers as lq` and `lq.serve` stays
the usual style.

# Main components
- `serve(**settings)`, `serve_async`: serve the default environment.
- `Server`: a running server, with its URL, stopped by `stop()`.

# Where it is used
- `liquers.environment.Environment.serve` returns a `Server`.

# Related concepts
- The server settings can come from a configuration document; see the Python guide.

# Examples
>>> from liquers.server import Server, serve
>>> Server.__module__
'liquers.server'
"""

from liquers._env import (
    serve,
    serve_async,
)
from liquers._backend import Server

__all__ = [
    "serve",
    "serve_async",
    "Server",
]
