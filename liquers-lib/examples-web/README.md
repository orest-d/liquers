# Liquers web (wasm) examples

Browser examples for the **`webui`** backend of `liquers-lib` — the string-first,
framework-independent web renderer described in `specs/design/webui/` and `specs/archive/2026-03-02-ui-web-design-notes.md`.

Each example compiles `liquers-lib` (and `liquers-core`) to `wasm32-unknown-unknown` and runs the
**whole Liquers evaluation engine inside the browser**: queries are parsed, commands are executed
and assets are evaluated by the `ImmediateAssetManager` in the page itself. There is no server
component — `trunk serve` only serves static files.

Every example here is the browser counterpart of a native egui example in `../examples`, so the
pair is a readable diff of what a backend has to provide. Run the native one with
`cargo run -p liquers-lib --example <name>`.

---

## Available examples

Each runs on its own port, so several can be served at once.

| Example | Port | What it shows | Native counterpart |
|---|---|---|---|
| [`ui_hello`](./ui_hello) | 8081 | The smallest complete pipeline: a root node carrying `ElementSource::Query("hello")` starts *pending*, `AppRunner` evaluates it, and the result renders. Nothing is interactive. | `ui_hello.rs` |
| [`ui_spec_simple`](./ui_spec_simple) | 8082 | A `UISpecElement` built in Rust (not YAML) with two static children, showing how a layout choice becomes a CSS class. | `ui_spec_simple.rs` |
| [`ui_spec_interactive`](./ui_spec_interactive) | 8083 | A menu built in Rust: *Add Hello* appends a child, *Clear All* removes the last one. The same `UiAction` values drive egui and the browser. | `ui_spec_interactive.rs` |
| [`ui_spec_demo`](./ui_spec_demo) | 8080 | A YAML-defined menu-driven dashboard: add panels, remove the last, or add a nested group whose own `init` query adds a child. | `ui_spec_demo.rs` |
| [`ui_payload_app`](./ui_payload_app) | 8084 | The payload-carrying environment (`DefaultEnvironment<Value, SimpleUIPayload>`): an `init` query populates the tree at startup and a menu action re-runs it. Commands reach `AppState` only through the payload. | `ui_payload_app.rs` |
| [`ui_button_app`](./ui_button_app) | 8085 | A **custom `UIElement` defined in the example crate**, implementing `render_web` instead of `show_in_egui`, which replaces itself with its query result via `add-instead`. | `ui_button_app.rs` |
| [`ui_query_console_app`](./ui_query_console_app) | 8086 | The interactive query console opened from a YAML menu. **Two of its interactions are known-broken in the browser** — see *Known gaps* below. | `ui_query_console_app.rs` |

`../examples/egui_async_prototype.rs` has no browser counterpart by design: it exercises egui's own
async integration rather than the UI framework.

### Known gaps

The query console's browser behaviour is incomplete, and the tests say so out loud rather than
avoiding the subject:

- **W1** — pressing Enter in the query input does nothing; only clicking **Go** submits.
- **W2** — the submitted query never reaches the element, so the input reverts to the previous
  query after a submit, and volatile/expired refreshes re-run the stale one.

Both are designed in [`specs/design/ui-events/`](../../specs/design/ui-events/). `tests/ui_query_console_app.spec.ts`
contains a `known gaps` block asserting the *current* broken behaviour — when `ui-events` lands
those tests will start failing, which is the signal to rewrite them as the desired behaviour.

---

## Prerequisites

### 1. Rust + the wasm target

```bash
rustup target add wasm32-unknown-unknown
```

### 2. Trunk (wasm bundler / dev server)

```bash
cargo install --locked trunk
```

Trunk builds the crate for `wasm32-unknown-unknown`, runs `wasm-bindgen`, injects the loader into
`index.html`, and serves the result. It downloads a matching `wasm-bindgen-cli` on first use
(0.2.126 at the time of writing); `wasm-opt` is disabled in the examples (`data-wasm-opt="0"`) to
keep builds fast.

If `trunk` is not on your `PATH` after installing, add `~/.cargo/bin` to it.

### 3. Node.js + Playwright (only for the tests)

```bash
npm ci                            # in this directory — installs @playwright/test
npx playwright install chromium   # once per machine, downloads the browser
```

In dev containers that pre-install Chromium (`PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers`), skip
`npx playwright install`.

---

## Running an example

```bash
cd ui_spec_demo        # or any other example directory

trunk serve            # build + serve with live reload, on the port listed above
trunk serve --open     # …and open a browser
trunk build            # one-off debug build into dist/
trunk build --release  # optimized build into dist/
```

`dist/` is a plain static bundle — any static file server can host it.

Open the browser devtools console if something does not appear: the examples install
`console_error_panic_hook`, so a Rust panic shows up there with a readable stack.

## Running the browser tests

All examples share one Playwright harness in this directory. Each example is a Playwright
*project* whose spec file is named after it, so a test's `page.goto('/')` lands on the right app.

```bash
npx playwright test                        # every example
npx playwright test --project ui_hello     # one example
npx playwright test -g "menu actions"      # one test by name
npx playwright test --headed               # watch it run
npx playwright test --debug                # step through
```

Playwright starts a `trunk serve` per example (`reuseExistingServer: true`, so a server you already
have running is reused). **The first run compiles every example to wasm**; they share one target
directory, so cargo serialises the builds and the first run takes a few minutes.

## Checking the wasm build without a browser

```bash
# from the repository root — type-checks the library for the browser target
cargo check -p liquers-lib --no-default-features --features webui --target wasm32-unknown-unknown

# from this directory — builds every example
cargo build --target wasm32-unknown-unknown
```

The **server-side rendering** half of the same backend is testable natively (no wasm, no browser):

```bash
cargo test -p liquers-lib --no-default-features --features webui,image-support --test webui_ssr
```

---

## Anatomy

```
examples-web/
├── Cargo.toml            # workspace: all examples share one target/ and Cargo.lock
├── shared.css            # one stylesheet; the backend emits stable lq-* class names
├── package.json          # @playwright/test
├── playwright.config.ts  # one project + one dev server per example
├── tests/<example>.spec.ts
└── <example>/
    ├── Cargo.toml        # workspace member, crate-type = ["cdylib"]
    ├── Trunk.toml        # this example's port
    ├── index.html        # `<link data-trunk rel="rust"/>`, shared.css, `<div id="app">`
    └── src/lib.rs        # `#[wasm_bindgen(start)]` → build env + AppState → `mount_web`
```

Two things are worth knowing:

- **`examples-web` is its own workspace**, deliberately not a member of the repository root: the
  root pulls in dev-dependencies and crates that do not build for wasm. Grouping the examples
  together means one shared `target/` rather than seven copies of every dependency — the
  difference is gigabytes. Consequences: `cargo build` at the repository root ignores these
  crates, and they have their own `Cargo.lock`.
- **`liquers-lib` is used with `default-features = false, features = ["webui"]`.** The default
  features (`egui`, `polars`, `image-support`) pull in crates that do not compile for wasm.

## Adding a new example

1. Copy the closest existing example to `examples-web/<your_example>` and rename the package in
   its `Cargo.toml`.
2. Add it to `members` in `examples-web/Cargo.toml`, and to `examples` in `playwright.config.ts`
   with a free port; set the same port in its `Trunk.toml`.
3. Write `src/lib.rs`: register commands, build a `DirectAppState`, call
   `mount_web(root_element, envref, app_state, tx, rx, initial_query)` and `std::mem::forget` the
   returned `MountHandle` so the DOM listeners stay alive.
4. Add `tests/<your_example>.spec.ts`.
5. Add a row to the table at the top of this file.

If your example builds a root element by hand, remember that `init` is only called automatically
when `AppRunner` installs an element — a hand-built root with `init` queries must call it itself
(see `ui_payload_app`).

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `error: no such command: trunk` | `cargo install --locked trunk`, ensure `~/.cargo/bin` is on `PATH` |
| `can't find crate for 'core' … wasm32-unknown-unknown` | `rustup target add wasm32-unknown-unknown` |
| `Cannot find module '@playwright/test'` | `npm ci` in this directory (not in an example subdirectory) |
| Page stays blank | Check the devtools console; a Rust panic is reported there via `console_error_panic_hook` |
| `Address already in use` | Another `trunk serve` holds that example's port — reuse it, or change the port in both `Trunk.toml` and `playwright.config.ts` |
| Playwright: `browserType.launch: Executable doesn't exist` | `npx playwright install chromium` |
| A `polars` / `mio` / `openssl` crate fails to build for wasm | The example enabled default features of `liquers-lib`; use `default-features = false, features = ["webui"]` |

## References

- `specs/design/webui/` — design of the web backend (Phases 1–4)
- `specs/design/webui-fixes/` — rendering and invalidation (why the DOM follows model changes)
- `specs/design/ui-events/` — the interaction half, including the query console's known gaps
- `specs/design/async-wasm-refactor/` — what made the engine run on wasm (`ImmediateAssetManager`)
- `liquers-lib/tests/webui_ssr.rs` — native SSR tests for the same renderer
