EDITOR: shared fragment (the standard server's settings, web folder and security), included by chapter 12 of all three guides. Language-neutral: settings are shown as the `server` section of a configuration file.

### The server settings

The `server` section of a configuration file sets up the standard server. Every key is optional;
a file without the section, or with an empty one, gives the defaults below.

```yaml
server:
  host: 127.0.0.1          # this machine only
  port: 8080               # 0 picks a free port
  prefix: /liquer          # every route is under it
  apis: [query, store, assets, recipes]
  read_only: false         # writes enabled
  web: true                # serve the store folder web/ at /liquer/web/
```

| Key | Default | What it does |
|---|---|---|
| `host` | `127.0.0.1` | The address to listen on. The default answers only programs on this machine. |
| `port` | `8080` | The port. With `0` the server picks a free one and reports it. |
| `prefix` | `/liquer` | The path every route is under. `/` puts them at the root. |
| `apis` | all four | Which APIs to offer: `query` (`{prefix}/q/…`), `store` (`{prefix}/api/store/…`), `assets` (`{prefix}/api/assets/…`, with the WebSocket) and `recipes` (`{prefix}/api/recipes/…`). |
| `read_only` | `false` | With `true`, the store API and every route of the assets API that changes data are left out. |
| `web` | `true` | `true` serves the store folder `web/`; a folder name such as `site` serves that folder instead; `false` turns the web folder and the start page off. |

A misspelled key is an error that names it, so a typo never leaves a setting at its default
without notice.

### The web folder

A server serves files stored under `web/` to a browser: the file stored at `web/app/main.js` is
at `{prefix}/web/app/main.js`. A path ending in `/` serves that folder's `index.html`, and the
server's start page, `/` or `/index.html`, sends the browser to `{prefix}/web/`. So a small web
application is a set of files you put in the store.

The web folder serves stored files only. It never evaluates a query; a file nobody has stored is
"not found" even when a recipe could make it. Each answer names its content type (from the file
name), tells the browser to check back before reusing a copy, and lets it skip the download when
the file has not changed.

To serve a folder of your project without copying it into the store, mount it read-only under
`web`. The mount keeps the prefix in its paths, so with `path: ./site` the key `web/index.html` is
the file `./site/web/index.html`:

```yaml
store:
  stores:
    - {type: filesystem, prefix: web, read_only: true, config: {path: ./site}}   # serves ./site/web/
```

### Security

The server has no users, passwords or permissions: anyone who can reach it can do everything its
APIs offer, with all the rights of your environment. Three settings decide who that is and what
they can do. Check them before you let anything but your own programs connect.

- **`host`.** The default, `127.0.0.1`, accepts connections from this machine only. That is not
  the same as from you only: every program on the machine can connect, and so can a web page you
  visit, by tricking the browser (DNS rebinding). `0.0.0.0` accepts connections from the network.
  Put an authenticating proxy in front of a server you expose.
- **`read_only`.** Writes are enabled by default, so a client can store, replace and delete data
  through the store and assets APIs. Set `read_only: true` when clients should only read and
  evaluate. To protect only some data, make those mounts read-only in the `store` section instead.
- **`web`.** A page in the web folder runs in the visitor's browser with the server's own address,
  so it can call every API in the visitor's name. While the server accepts writes, anyone who can
  reach it can store such a page. Serve pages from a read-only mount (above), set
  `read_only: true`, or turn the web folder off with `web: false`.
