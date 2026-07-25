# Liquers web (wasm) examples

Browser examples for the **`webui`** backend of `liquers-lib` — the string-first, framework-independent
web renderer described in `specs/webui/` and `specs/UI_WEB_DESIGN_NOTES.md`.

Each example compiles `liquers-lib` (and `liquers-core`) to `wasm32-unknown-unknown` and runs the
**whole Liquers evaluation engine inside the browser**: queries are parsed, commands are executed
and assets are evaluated by the `ImmediateAssetManager` in the page itself. There is no server
component — `trunk serve` only serves static files.

> Native, egui-based examples live in `../examples` and are run with
> `cargo run -p liquers-lib --example <name>`. This directory is the browser counterpart.

---

## Available examples

| Example | What it shows | Run |
|---------|---------------|-----|
| [`ui_spec_demo`](./ui_spec_demo) | A YAML-defined (`UISpec`) menu-driven dashboard. *Add Dashboard* submits `dashboard/q/ns-lui/add-child`, which evaluates in the browser and appends a panel; *Remove Last Panel* submits `ns-lui/remove-last`, which resolves fully inline. Exercises `mount_web` → delegated DOM listener → `UiAction` → `AppRunner` → inline evaluation → invalidation → DOM update. | `cd ui_spec_demo && trunk serve` → <http://127.0.0.1:8080> |

`ui_spec_demo` also carries the Playwright end-to-end suite
(`ui_spec_demo/tests/webui.spec.ts`) that is the acceptance test for running the engine in a real
browser (see `specs/async-wasm-refactor/`).

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
(0.2.126 at the time of writing); `wasm-opt` is disabled in the examples
(`data-wasm-opt="0"` in `index.html`) to keep builds fast.

If `trunk` is not on your `PATH` after installing, add `~/.cargo/bin` to it.

### 3. Node.js + Playwright (only for the e2e tests)

```bash
cd ui_spec_demo
npm ci                            # installs @playwright/test
npx playwright install chromium   # once per machine, downloads the browser
```

In dev containers that pre-install Chromium (`PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers`), skip
`npx playwright install`.

---

## Running an example

```bash
cd ui_spec_demo

trunk serve            # build + serve with live reload at http://127.0.0.1:8080
trunk serve --open     # …and open a browser
trunk build            # one-off debug build into dist/
trunk build --release  # optimized build into dist/
```

`dist/` is a plain static bundle — any static file server can host it.

Open the browser devtools console if something does not appear: the examples install
`console_error_panic_hook`, so a Rust panic shows up there with a readable stack.

## Running the browser tests

```bash
cd ui_spec_demo
npx playwright test              # headless Chromium
npx playwright test --headed     # watch it run
npx playwright test --debug      # step through
```

`playwright.config.ts` starts `trunk serve` itself (`webServer`, with `reuseExistingServer: true`),
so a server you already have running on port 8080 is reused instead of being started twice.

## Checking the wasm build without a browser

```bash
# from the repository root — type-checks the library for the browser target
cargo check -p liquers-lib --no-default-features --features webui --target wasm32-unknown-unknown

# from an example directory — full wasm build of the example crate
cd liquers-lib/examples-web/ui_spec_demo && cargo check --target wasm32-unknown-unknown
```

The **server-side rendering** half of the same backend is testable natively (no wasm, no browser):

```bash
cargo test -p liquers-lib --no-default-features --features webui,image-support --test webui_ssr
```

---

## Anatomy of an example crate

```
ui_spec_demo/
├── Cargo.toml           # standalone crate: `[workspace]` + crate-type = ["cdylib"]
├── Trunk.toml           # build target + dev-server address/port
├── index.html           # `<link data-trunk rel="rust"/>`, page styling, `<div id="app">`
├── src/lib.rs           # `#[wasm_bindgen(start)]` → build env + AppState → `mount_web`
├── package.json         # @playwright/test (e2e only)
├── playwright.config.ts # webServer = `trunk serve`, baseURL 127.0.0.1:8080
└── tests/webui.spec.ts  # end-to-end assertions
```

Two things are worth knowing:

- **Each example is its own workspace.** `Cargo.toml` contains an empty `[workspace]` table so the
  crate does not inherit the repository workspace (whose dev-dependencies do not build for wasm).
  Consequences: `cargo build`/`cargo test` at the repository root ignore these crates, and each has
  its own `Cargo.lock` and `target/`.
- **`liquers-lib` is used with `default-features = false, features = ["webui"]`.** The default
  features (`egui`, `polars`, `image-support`) pull in crates that do not compile for wasm.

## Adding a new example

1. Copy `ui_spec_demo` to `examples-web/<your_example>` and rename the package in `Cargo.toml`
   (keep the empty `[workspace]` table and the relative `path` dependencies).
2. Write `src/lib.rs`: register your commands, build a `DirectAppState`, then call
   `mount_web(root_element, envref, app_state, tx, rx, initial_query)` and `std::mem::forget` the
   returned `MountHandle` so the DOM listeners stay alive.
3. Add any element-specific CSS to `index.html` (the backend emits stable `lq-*` class names and
   `ui-element-{handle}` ids).
4. `trunk serve` and iterate; add Playwright cases under `tests/` when the behaviour is worth
   locking down.
5. Add a row to the table at the top of this file.

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `error: no such command: trunk` | `cargo install --locked trunk`, ensure `~/.cargo/bin` is on `PATH` |
| `can't find crate for 'core' … wasm32-unknown-unknown` | `rustup target add wasm32-unknown-unknown` |
| Page stays blank | Check the devtools console; a Rust panic is reported there via `console_error_panic_hook` |
| `Address already in use (127.0.0.1:8080)` | Another `trunk serve` is running — reuse it, or change `[serve] port` in `Trunk.toml` (and `baseURL` in `playwright.config.ts`) |
| Playwright: `browserType.launch: Executable doesn't exist` | `npx playwright install chromium` |
| A `polars` / `mio` / `openssl` crate fails to build for wasm | The example enabled default features of `liquers-lib`; use `default-features = false, features = ["webui"]` |

## References

- `specs/webui/` — design of the web backend (Phases 1–4)
- `specs/async-wasm-refactor/` — what made the engine run on wasm (`ImmediateAssetManager`)
- `specs/UI_WEB_DESIGN_NOTES.md`, `specs/UI_INTERFACE_FSD.md` — UI architecture
- `liquers-lib/tests/webui_ssr.rs` — native SSR tests for the same renderer
