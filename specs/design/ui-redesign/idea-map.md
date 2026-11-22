---
id: UI-REDESIGN-IDEA-MAP
kind: analysis
title: UI redesign — idea map, existing work and issue coverage
workflow: liquers-project
status: draft
area: [lib/ui, lib/egui, web, axum]
created: 2026-11-22
---
# UI redesign: idea map

Input to [`plan.md`](./plan.md). Written 2026-11-22 from Orest's goal list of the same day, the
open issues and designs in `specs/index.csv`, the investigative design
[`ui-split-elements`](../ui-split-elements/), the theme work of
[`query-editor-ergonomics`](../query-editor-ergonomics/) (reference `specs/reference/THEME.md`), the
archive survey `specs/archive/2026-11-19-egui-craft-suite-survey.md`, the March 2026 frontend notes
(`specs/archive/2026-03-02-ui-{ratatui,web,dioxus}-design-notes.md`) and the
`orest-d/egui-midi-test` and `orest-d/stockplottertest` repositories (the latter read on
2026-11-22 at `5d7bdb5`, after access was granted). GitHub has no open UI issue (the three open issues are
#232, #233, #237, none about the UI); every issue below is a local `specs/issues/` file.

Each area **G** is one of Orest's goals; each **B** is a brainstormed addition. Per area: what the
goal means, what already exists, the ideas, and the open issues it absorbs.

## 0. Starting point (true at HEAD, 2026-11-22)

- The whole UI is `liquers-lib/src/ui/` (~19 500 lines: element tree, `AppState`, `AppRunner`,
  events, shortcuts, editors, theme, web renderer) plus `liquers-lib/src/egui/` (~12 400 lines with
  `ui/`). One trait, `UIElement` (`ui/element.rs`), carries every technique as feature-gated methods
  (`show_in_egui`, `render_web`, `show_in_web`).
- Built and worth keeping: handles and the element tree, `UIEvent` delivery (`ui-events`),
  invalidation for retained rendering (`webui-fixes`), `UISpec` with menus, layouts and shortcuts
  (`menu-pane-layout`, `keyboard-shortcuts`), the query console, the query/pipeline/action/resource
  editors (`query-pipeline-editor`, `query-editor-ergonomics`), the native driver and egui window
  (`egui-native-app`), the theme token model with YAML layers, CSS generation, icons and the `ui:`
  configuration section (`THEME.md`), JavaScript mounting and JavaScript-defined elements
  (`web-ui-use`, `web-ui-define`), the Python UI model and window (`liquers-py-ui*`), settable value
  accessors with an HTTP and WebSocket API (`liquers-axum/src/accessor/`).
- Not built: any client-server UI, any crate outside `liquers-lib`, ratatui, charts (`egui_plot` is
  a dependency nothing uses), authentication, sessions beyond `SimpleSession`.
- Decided already and carried over (`ui-split-elements`, Orest 2026-11-14/17): model/view pair with
  typed `Down`/`Up` messages (option C); one client trait per frontend; table pages are
  `RecordView`s (Arrow IPC on the wire); several asset subscriptions per element; the envelope
  `[u32 header length][JSON][8-byte aligned attachments]`; the server is authoritative, with
  per-element revisions and resync; session separate from connection; old and new UI in parallel
  until the new one matures.

## G1. UI in its own crate(s), independent of `liquers-lib`

**Means.** The UI model compiles against `liquers-core` (and optionally `liquers-records`) only.
`liquers-lib` becomes a *user* of the UI crates: it contributes element models and value
projections for its own types (DataFrame, image, polars), never the other way round.

**Ideas.**
- `liquers-ui`: element identity and tree, handles, `AppState`, runner, events, messages, the
  model/view traits, protocol types, theme and icon model, `UISpec`, function-mapping and MIDI
  *data* models. No renderer, no network code. Builds on wasm and native.
- One crate per frontend: `liquers-ui-egui`, `liquers-ui-web`, `liquers-ui-tui`.
- Transport and session crates for split mode: `liquers-ui-server` (sessions, WebSocket handler,
  mounted by `liquers-axum` behind a feature) and the client driver (in `liquers-ui`, transport
  behind a trait).
- `liquers-lib` keeps a thin glue module (`liquers-lib/src/ui/` shrinks to "elements and
  projections for `ExtValue`" plus the `lui` commands), or the glue moves to a
  `liquers-ui-lib` crate if the `lui` commands should not force the UI on every `liquers-lib` user.
- Value access without `liquers-lib`: the UI needs "can this value be shown as text / table /
  image / chart, and give me that form" without knowing `ExtValue`. That is a capability lookup
  (`VALUE-INTERFACE-CAPABILITY-SPLIT`, `realm-type-registry`) plus conversion by automatic edge
  (`ui-display-conversion`).
- Migration: the current `UIElement` stays as a *fused element* with an adapter, so both UIs run
  in parallel (Orest, 2026-11-17).

**Issues.** `UI-NOT-SEPARATED-BY-CRATE-AND-TECHNIQUE` (P1, the umbrella), `UI-INTERFACE-FSD-DESCRIBES-UNBUILT-API`,
`EGUI-ASSET-MANAGER-INTEGRATION`, `VALUE-INTERFACE-CAPABILITY-SPLIT` (dependency),
`UI-WIDGET-DOES-NOT-CONVERT-ITS-VALUE` / design `ui-display-conversion`,
`WEB-RELEASE-MODULE-IS-20-MB` (a client without polars is the main lever).

## G2. A clean split for several frontends: egui, web, ratatui first-class

**Means.** Each frontend is a separate trait, and a frontend can be added without touching the
model crate. Three first-class frontends validate the traits from the start: immediate-mode GUI
(egui), retained DOM (web), character cells (ratatui).

**Ideas.**
- Per-frontend view traits speaking the model's messages: `EguiView`, `WebView`, `TuiView`. The
  `mount` message names the model type and protocol version; each client picks its own registered
  view for it, and a missing view falls back to a generic one (status + text projection).
- Frontend capability profile: which value forms it renders (image, SVG, HTML, Markdown, chart
  kinds), input devices, colour depth, icon kinds (the TUI draws Unicode only). Sent in `hello`, so
  a model can choose a projection.
- A built-in element set every frontend must implement (asset view, text, table page, form field
  kinds, layout containers, menu, status bar) and a **frontend conformance suite**, like the store
  conformance suite: the same `UISpec` documents rendered by each frontend's test backend
  (egui `egui_kittest`, ratatui `TestBackend`, web Playwright / DOM snapshot).
- ratatui: immediate mode like egui, so the extract-render-replace pattern of the March notes
  carries over; input via crossterm. `egui_ratatui` (used by `egui-midi-test`) can host a TUI view
  inside egui, handy for tests and demos. A TUI in the browser (ratzilla / xterm.js) is a later
  option.
- Other desktop GUIs (iced, Slint, Dioxus, GTK) are possible later frontends; the March Dioxus
  notes are the reference. Not first-class here.

**Issues.** `UI-NOT-SEPARATED-BY-CRATE-AND-TECHNIQUE`, `EGUI-EXAMPLES-HAND-WRITE-THE-FRAME-LOOP`,
`WEBUI-SHORTCUTS-RUN-IN-EVERY-MOUNT`, `WEB-ONEVENT-SHOW-GUARD-IGNORES-THE-MOUNT`,
`PY-UI-CHANGE-DOES-NOT-WAKE-AN-OPEN-WINDOW`.

## G3. Monolithic and split client-server, for every frontend

**Means.** Monolithic: engine, models and views in one process (egui desktop, ratatui, wasm in a
browser via `liquers-web`). Split: engine and models on a server, views in a client. Any frontend
can be a split client (a browser, an egui desktop, a terminal attached to a remote server).

**Ideas.**
- Monolithic is split with an in-process transport: the same messages, values passed as
  `Arc<Value>` attachments, nothing encoded (`ui-split-elements` AC-4/AC-5).
- Fused elements (today's `UIElement`) work monolithically only, or in split mode by snapshot,
  refusing values that cannot travel (AC-7).
- Several clients per session (a supporter watching a user's session, a phone and a desktop):
  model state shared, view state (scroll, selection) per client.
- The current runner blocks on awaits (`WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`); a split server must
  not, so the non-blocking runner is a prerequisite, not an extra.

**Issues.** `WEB-UI-CANNOT-BE-DRIVEN-BY-A-REMOTE-ENGINE`, `AXUM-CANNOT-SERVE-A-LIQUERS-UI`,
`WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`, `UI-ELEMENT-KEEPS-AN-EXPIRED-VALUE-AND-NEVER-RE-REQUESTS`,
design `ui-asset-snapshot-boundary` (the runner owns subscriptions; to be absorbed).

## G4. Tauri for desktop and mobile apps

**Means.** A Tauri 2 shell: the Rust side hosts the environment and the UI server half, the
webview runs the web frontend as a split client. Mobile (iOS, Android) through Tauri 2.

**Ideas.**
- Transport choice: Tauri IPC (commands + events, binary payloads through `Channel`) versus a
  loopback WebSocket to an embedded server. IPC needs no port and no auth; WebSocket reuses the
  browser client unchanged. The transport trait should allow both; a spike decides the default.
- Single session, trivial auth (G7), app-local store paths, packaging template (`cargo tauri`),
  native menus and dialogs mapped from `UISpec` menus where possible.
- Mobile constraints: no polars on the device is not required (the server half runs natively in the
  app), but memory and binary size are; touch input feeds the function mapping (G13: pinch = zoom).

**Issues.** None filed. New design.

## G5. Protocol, server API and clients; WebSocket for UI events

**Means.** A documented UI protocol over WebSocket, alongside the existing Query, Store, Assets and
Accessor APIs, which the client keeps using for data.

**Ideas.**
- Messages from `ui-split-elements` §5: `hello`/`welcome`, `mount`/`update`/`unmount`, `status`,
  `up`, `resync`; plus `capabilities` (G2), `theme` (push the composed theme), `input` (G13/G14
  control events), `ping`, and an error message.
- Versioning: protocol version in `hello`, per-model message version in `mount`.
- Binary attachments: Arrow IPC pages, PNG/SVG, later a compact binary value format
  (`NO-COMPACT-BINARY-DATA-FORMAT`).
- Data side-channels the client may use directly: Query API (`/liquer/q/…`), Assets API events
  (`asset-events-over-websocket`), accessor API (`/api/acc`) for form fields; the UI channel then
  carries only UI state and events. Which data goes which way is a design decision (D5 in the plan).
- Hardening: connection limits, keep-alive, idle and write timeouts (`axum-websocket-connection-limits`),
  origin checks, message size limits, rate limits.
- Server API: create/list/close sessions, attach to a session, list UI documents.

**Issues.** `AXUM-WEBSOCKET-HARDENING`, designs `asset-events-over-websocket`,
`axum-websocket-connection-limits`, `http-request-body-limit`, `NO-COMPACT-BINARY-DATA-FORMAT`.

## G6. Processing on the client, within what the client supports

**Means.** Light work (formatting, sorting a page, filtering small data, rendering templates,
evaluating cheap commands) runs on the client; heavy types (polars, large data) stay on the server.

**Ideas.**
- The client is itself a Liquers environment (wasm `liquers-web` or native) with its own realm;
  queries for server-only commands dispatch to the server realm
  (`multi-realm-dispatch`, `registry-publication`, `remote-asset-manager`,
  `remote-store-and-asset-manager`).
- Capability negotiation: the client publishes its type registry (`realm-type-registry`); the model
  sends a value as is when the client has the type, a projection otherwise (DataFrame → `RecordView`
  page, large image → downscaled PNG).
- Thick-view elements: a view may evaluate client-side queries over data it already holds (sort a
  page locally, re-render a chart with another style) without a round trip.
- Accessors on the client: transfer a descriptor, not state (`ACCESSOR-CANNOT-BE-SENT-TO-A-CLIENT`),
  so a form can read and write a server YAML file through `/api/acc` directly.

**Issues.** `ACCESSOR-CANNOT-BE-SENT-TO-A-CLIENT`, `WEB-HAS-NO-READ-WRITE-REMOTE-STORE`, designs
`multi-realm-dispatch`, `registry-publication`, `realm-type-registry`, `remote-asset-manager`,
`remote-store-and-asset-manager`, `language-bridge-extended-scalars`.

## G7. Sessions, authentication and authorisation

**Means.** Sessions from trivial (one local session, Tauri) to multi-session and multi-user, with
pluggable authentication that integrates with existing infrastructure (SSO) without Liquers
shipping every auth system.

**Ideas.**
- Split identity from policy: an `Authenticator` trait turns a request (headers, cookie, token,
  WebSocket handshake) into a `Principal`; the access policy of `access-policy-at-entry-points`
  (principals + key patterns + command rules) decides. Liquers core knows only the trait and the
  principal.
- Built-in authenticators: `none` (single local user), static token / API keys, trusted
  reverse-proxy header (oauth2-proxy, Authelia, Keycloak gatekeeper, corporate SSO gateways), and
  the Tauri "local app" principal. OIDC/JWT verification as a separate optional crate
  (`liquers-auth-oidc`), so the default build carries no IdP code.
- Session store: in memory first, trait for a persistent one; session lifetime, reattach after
  reconnect, session per user versus per tab; joining another user's session is an explicit
  permission.
- Evaluation sees the principal (`Context`), so commands and stores can check per key; cached
  assets stay shared across principals (AC-5 of the access-policy design).
- Configuration under `server:` / `ui:` sections; secrets only through `${VAR}`.

**Issues.** `CORE-SESSION-AND-KEY-ACL`, design `access-policy-at-entry-points`,
`SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED`, `AXUM-WEBSOCKET-HARDENING`.

## G8. Themes: dark/light, portable, icons, icons in command metadata

**Means.** One theme serves egui, the web and the TUI; dark and light modes; icons usable
everywhere (buttons, palettes, menus); commands and namespaces declare icons.

**Exists.** Token model, YAML layers, `to_css()`, egui `Visuals`, icon definitions with Unicode
fallback, Phosphor set, `ui:` section, command `hint icon` (`THEME.md`, `THEME_GUIDE.md`).

**Ideas.**
- Move the theme model to `liquers-ui`; renderers map it (egui `Visuals`, CSS custom properties,
  ratatui `Style` with the `terminal.*` ANSI fallbacks already in the token set).
- Tooling: `liquers-theme` CLI — validate, compose, export CSS (and DTCG JSON, per
  `query-editor-ergonomics/theme-formats.md`), preview page; a theme editor element (B5).
- Style queries: a renderer-neutral selector path resolved by `selectors` + our cascade
  (`NO-RENDERER-NEUTRAL-STYLESHEET`), so `UISpec` theme overrides can target element kinds and
  states, not only namespaces and commands.
- Icon sets beyond Phosphor, icon assets by key, namespace metadata (icon, hue, label, doc), icons
  from Python and JavaScript commands; icons for menu items and palette entries in `UISpec`.
- High-contrast and colour-blind-safe variants; theme per UI document.

**Issues.** `NO-RENDERER-NEUTRAL-STYLESHEET`, `ICON-SETS-OTHER-THAN-PHOSPHOR-ARE-NOT-DRAWN`,
`NAMESPACES-HAVE-NO-METADATA`, `UI-DOCUMENT-CANNOT-NAME-ITS-THEME`, `WEB-MOUNTUI-HAS-NO-THEME-OPTION`,
`PY-COMMAND-CANNOT-SET-ICON-PRESETS-OR-NEXT`, `WEB-EDITOR-HAS-NO-KEYBOARD-MODEL` (icon images part).

## G9. Charts and graphing

**Means.** Good charts in egui (`egui_plot`), Apache ECharts on the web behind a compatibility
layer for common chart types, frontend-specific extras, basic ratatui charts.

**Ideas.**
- A `ChartSpec` value in `liquers-ui` (or a small `liquers-chart` crate): data by reference to a
  record table (columns), marks (line, scatter, bar, area, histogram, box, heatmap, candlestick for
  stocks), axes (linear, log, time, category), legends, multiple series, secondary axis, annotations.
  The common subset maps to all three backends; each backend declares which kinds it supports.
- Backends: egui `egui_plot` (interactive zoom/pan, linked axes); web ECharts option JSON generated
  from the spec (loaded from a CDN or bundled); ratatui `Chart`, `BarChart`, `Sparkline` (braille).
- **Direction (Orest, 2026-11-22):** our own `ChartSpec` plus per-backend extensions, built on
  Liquers queries and theming, possibly reusing `UISpec` layout for multi-panel figures, shaped
  after ECharts for an easy web port. Backends are not only the UI frontends: matplotlib (a
  `ChartSpec` rendered from Python), later plotly and others, even XMGrace. hvPlot's high-level,
  data-first API (`df.plot.line(x=…, y=…)`, one call for many backends) is a model. Liquers is
  both an integration layer and "batteries included", so the variety of backends is used to find
  the patterns that work across them.
- Escape hatch per frontend: an `echarts` or `egui_plot` extension block for what only that backend
  has, ignored elsewhere with a fallback to the common spec.
- Static render for reports and the TUI-less contexts: SVG/PNG (`plotters` or ECharts SSR) so a
  chart is also an ordinary value in a store.
- Commands: `ns-chart/line-x-y`, `…/bar`, applied to a record table or DataFrame; charts become
  query results, cacheable and shareable.
- Large data: server-side downsampling (LTTB) and windowing, a chart page like a table page.
- Interaction: zoom/pan/selection feed the function mapping (G13), so a chart's zoom can be a MIDI
  knob and a selection can drive a filter query.

**Lessons from `orest-d/stockplottertest`** (`5d7bdb5`; egui 0.33, `egui_plot` 0.34, polars 0.53,
`liquers-lib` by path). A stock-chart app composed from a `UISpec` (`main_spec.yaml`: a menu of
queries, `layout: windows`, each example opened in a query console with `ns-lui/add-child`). Data
commands return polars DataFrames with `date`, `timestamp` (i64 seconds), `open`, `high`, `low`,
`close`, `adjclose`, `volume`: `yahoo_get_quotes`, `yahoo_intraday_quotes` (`expires: "in 1 min"`),
`merge_historical_data` over CSVs from a recipe provider that generates one recipe per ticker
(`src/recipes.rs`), and `brownian_quotes` (synthetic OHLC, `expires: "in 100 ms"`, so a live,
self-refreshing series). The chart is `simple_plot-close` (`src/commands.rs`): it reads the two
columns and returns `Value::from_ui(closure)` drawing an `egui_plot` `Line` with a fixed blue
colour, a fixed plot id `"stock_plot"`, zoom and drag on, and an x-axis formatter turning
timestamps into dates. `simple_table` does the same with an `egui::Grid` over every row.

What the chart design takes from it:
- **A chart must be data, not a closure.** `Value::from_ui` is an opaque egui closure: it cannot
  be stored, sent to a client, drawn by the web or the TUI, restyled by a theme or edited. The
  replacement is `…/simple_plot-close` producing a `ChartSpec` value that each frontend renders.
- **The first target use case is financial time series:** a time x axis with date formatting,
  OHLC candlesticks with a volume panel below on a shared x axis, line and area overlays (moving
  averages), several tickers compared (a normalised scale), and zoom and pan over a long history.
  `egui_plot` has `BoxPlot` and bars but no candlestick, so the egui backend draws candles itself
  (bars for bodies, segments for wicks); ECharts has `candlestick` natively; ratatui gets a line of
  closes in braille plus a volume sparkline.
- **Live data:** `brownian_quotes` expires every 100 ms. A chart element must re-request its
  query when the value expires (today's elements do not:
  `UI-ELEMENT-KEEPS-AN-EXPIRED-VALUE-AND-NEVER-RE-REQUESTS`) and keep the zoom window across
  updates; in split mode only new points should travel (append updates to a chart page).
- **Colours and ids from the theme and the element:** a fixed `Color32::BLUE` and a fixed plot id
  break with dark mode and with two charts open. Series colours come from the theme's hues; plot
  identity comes from the element handle.
- **Columns by name, not by convention:** `simple_plot` hard-codes `timestamp`; the spec names x,
  y, OHLC and volume columns, with defaults detected from the schema.
- **Version coupling:** the app pins egui 0.33 while `liquers-lib` is on 0.36. An app that writes
  egui code against `liquers-lib`'s egui breaks on every upgrade; a chart spec rendered by
  `liquers-ui-egui` removes that coupling, and the egui crate should re-export `egui` for the apps
  that still need custom drawing.
- **Tables too:** `simple_table` renders every row of the DataFrame with `unwrap` per cell; the
  paged `RecordView` table of D16 replaces it.
- It is also a reference application for G12 (a `UISpec` with menus of queries, windows and
  consoles) and for G10's recipes editor (generated recipes per ticker from `config.yaml`).

**Issues.** None filed. New design.

## G10. Composite tools and editors

**Means.** Asset manager, orthodox two-pane asset/store manager, process monitor, text editor, code
editor, rich text (Markdown) editor, template editor with an integrated query editor, recipes
editor, tabular viewer/editor (records, SQL databases with SQL highlighting, DataFrames,
spreadsheet-like), charts (G9).

**Ideas, per tool.**
- **Asset manager:** list assets by key and query, status, progress, log, metadata, dependencies,
  cancel, re-evaluate, remove (`assets-api-admin-operations`), follow unmanaged evaluations
  (`unmanaged-asset-registry`).
- **Orthodox store manager:** two panes over stores/mounts, copy/move/rename between them, preview,
  metadata panel, recipes in a folder — the FSD's "Use case 1: Orthodox Commander".
- **Process monitor:** asset-manager queue, running and waiting jobs, cancellation, throughput, the
  event stream (`asset-events-over-websocket`); a server view and a session view.
- **Text and code editor:** one editor element with syntax modes (query, template, YAML, JSON,
  SQL, Markdown, Python, Rust), diagnostics from the validators, save through the key or an
  accessor (`text-editor-save`). egui: `egui_extras::syntax_highlighting` or a highlighter on
  `syntect`/tree-sitter; web: CodeMirror 6 or Monaco; TUI: a ratatui text area.
- **Rich text editor:** Markdown with preview first (`egui_commonmark` is already a dependency;
  `liquers-markdown` gives sections and addresses); WYSIWYG on the web (e.g. Milkdown/TipTap) later.
- **Template editor:** live diagnostics, preview, query editor for the queries in the template
  (`template-editor-ui`, `template-syntax-highlighter`).
- **Recipes editor:** `recipes.yaml` as a form and as text, each recipe's query in the query
  editor, validation through `liquers-validate` logic, plan and dependency preview, run.
- **Tables:** paged viewer over `RecordView` for records, DataFrames and SQL results; sort, filter,
  column statistics; editing through accessors; SQL editor with highlighting and a schema browser
  over `relational-database-access`; a spreadsheet-like grid (cell editing, formulas as queries?)
  as a research item.
- **Query editor:** live validation and preview in every frontend (existing designs).

**Issues.** `TEXT-EDITOR-EDITS-ARE-NEVER-SAVED` / `text-editor-save`, `NO-TEMPLATE-EDITOR-UI` /
`template-editor-ui`, `TEMPLATE-HAS-NO-HIGHLIGHTER` / `template-syntax-highlighter`,
`UI-QUERY-EDITOR-LACKS-LIVE-VALIDATION` / `query-editor-live-validation`,
`WEB-QUERY-EDITOR-NOT-LIVE` / `web-query-editor-live-preview`,
`CORE-QUERY-EDITOR-ACCEPTS-AN-UNKNOWN-COMMAND`, `EDITOR-CANNOT-EDIT-INSIDE-A-NESTED-PIPELINE`,
`WEB-PIPELINE-EDITOR-HAS-NO-DEFINITION-MODE`, `PIPELINE-COMMAND-OPERAND-TYPES-UNCHECKED`,
`UI-ELEMENT-NESTED-IN-CONSOLE-HAS-NO-HANDLE` / `console-mounts-nested-element`; data-side
dependencies `relational-database-access`, `sql-over-tables`, `sql-several-tables`,
`assets-api-admin-operations`, `unmanaged-asset-registry`, `asset-events-over-websocket`.

## G11. Standard UI configuration in the environment configuration

**Exists.** The `ui:` section (`UiConfig`: theme, mode, density), taken by the egui window, the
native driver, the browser mount and Python.

**Ideas.** Extend `UiConfig`: default frontend options, icon set, keymap overrides, MIDI device
profiles and mappings (G14), function-mapping defaults (G13), the start document (`UISpec` key),
session and auth settings for split mode (or under `server:`), chart backend options (ECharts
source), per-frontend sections (`ui.egui`, `ui.web`, `ui.tui`). One typed reader for every host
(`HOSTS-READ-CONFIGURATION-DOCUMENTS-DIFFERENTLY`).

**Issues.** `HOSTS-READ-CONFIGURATION-DOCUMENTS-DIFFERENTLY`, `WEB-STORE-CONFIG-NOT-APPLIED-THROUGH-ENVIRONMENT-CONFIG`,
`WEB-MOUNTUI-HAS-NO-THEME-OPTION`, `UI-DOCUMENT-CANNOT-NAME-ITS-THEME`.

## G12. A better UI spec: compose applications from elements, accessors, theme overrides

**Means.** `UISpec` grows from "menus, layouts and init queries" to a small application language:
compose elements, bind them to data through accessors, override the theme.

**Ideas.**
- Bindings: a field element bound to an accessor (`-R/config/app.yaml` + path `server/port`), with
  validation from the type and `gui_info`; a form is a list of bound fields, generated from a
  schema or written by hand. Read/write through accessors (`settable-value-accessor`), live
  updates from accessor events, conflict handling (`VALUE-ACCESSOR-CANNOT-LOCK`), YAML comments
  kept (`YAML-DOCUMENT-EDIT-DROPS-COMMENTS`).
- Actions: buttons and events run queries or set accessor values; simple derived values (a label
  showing a query of another field).
- Layout: existing horizontal/vertical/grid/tabs/windows plus docking (`egui_dock` or `craft-ui`'s
  docking, per the craft survey) and responsive rules for the web and mobile.
- Theme overrides per document and per element (style queries, G8).
- Reuse: components (a `UISpec` fragment with parameters), includes by key.
- Validation of a `UISpec` (like `liquers-validate` for queries), and a `UISpec` editor element.
- Portability: a `UISpec` declares the frontends it needs, or degrades (a chart in the TUI as a
  sparkline).

**Issues.** `ACCESSOR-CANNOT-BE-SENT-TO-A-CLIENT`, `VALUE-ACCESSOR-CANNOT-LOCK`,
`YAML-DOCUMENT-EDIT-DROPS-COMMENTS`, `ACCESSOR-CANNOT-BE-IMPLEMENTED-IN-PYTHON-OR-JAVASCRIPT`,
`UI-DOCUMENT-CANNOT-NAME-ITS-THEME`, `UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS`,
`COMMAND-COMPOSITE-VARIADIC-ARGUMENTS` (form editing of variadic arguments).

## G13. High-level function mapping

**Means.** Abstract *functions* (zoom/scale, rotation, 2D position, 3D position, selection,
previous/next, …) that an input device, a widget or a command argument can be bound to. A command
may map zoom to one argument and 2D position to two (x, y). Needs research and experiments.

**Ideas.**
- A small vocabulary of **controls** with value shapes: `scalar` (zoom, scale, opacity, gain),
  `angle` (rotation), `vec2` (position, pan), `vec3`, `range` (from–to), `choice` (selection among
  N), `step` (previous/next, ±1), `trigger` (apply, reset), `toggle`. Each with a range, a scale
  (linear, log), a default, absolute or relative updates.
- Command metadata: a command-level or argument-level hint binding arguments to a control, e.g.
  `hint control zoom: "factor"`, `hint control position: "x,y"`; ranges from `gui_info` or the
  hint. `register_command!` already has `hint <key>: "…"`; the editor and the MIDI layer read it.
- Element-level functions: an image viewer exposes `zoom` and `position`, a table `selection` and
  `step`, a chart `zoom`/`position`/`range`. Focus decides which element receives a function.
- Sources: mouse wheel, pinch, keyboard, gamepad, MIDI (G14), OSC; all produce the same control
  events, which the protocol carries up in split mode.
- Research plan: catalogue functions across the existing commands (`ns-img`, `ns-pl`, charts), try
  them on two or three real applications (image processing, a stock chart, a parameter sweep), and
  only then fix the vocabulary.

**Issues.** `UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS`, `WEB-EDITOR-HAS-NO-KEYBOARD-MODEL`,
`WEBUI-SHORTCUTS-RUN-IN-EVERY-MOUNT` (keyboard is one source). New design.

## G14. MIDI mapping hints

**Means.** MIDI devices give inputs to the GUI, particularly to commands: knobs and sliders set
values, buttons select presets.

**Lessons from `orest-d/egui-midi-test`** (egui 0.32, `midir` 0.10, `liquers-core` from git):
`listen_midi` runs `midir` on its own thread and sends `UserEvent`s (`Zoom`, `SetImage`,
`SetParameter(i, value)`, `RecalculateAll`, `QuitApp`) over a channel that `MyApp::update` drains
each frame. Mapping is hard-coded: CC 80–87 set the *i*-th numeric argument of the current query
(`ActionEditorRecord::find_numeric_parameter` counts integer and float arguments;
`set_numeric_parameter_value_in_query` writes 0–127 raw for integers and `/127` for floats, then
re-encodes the query and re-evaluates); note-off 48–60 select one of eight queries; CC 74 on
channel 16 sets zoom ×5. It connects to the last port found and prints to stdout. What a design
must add: a mapping file and device profiles, learn mode, ranges and scales from argument metadata,
relative encoders and 14-bit CC, soft takeover / pick-up, feedback to the device (LEDs, motor
faders), hot-plug, routing by focus, debouncing and rate limiting of re-evaluation (a knob turn
must not queue 127 evaluations — the asset manager's cancellation helps), presets on buttons
(`preset:` metadata already exists), and transport: native `midir`; browser Web MIDI API; Tauri
via the native side.

**Ideas.** MIDI is one *source* of G13 controls: `cc 1/74 → zoom`, `note 36..43 → choice`,
`cc 80..87 → the focused command's controls in order`. Mapping hints in command metadata
(`hint midi …`) stay optional; the default comes from G13 controls, so most commands need no MIDI
hint at all.

**Issues.** None filed. New design.

## Brainstormed additions

- **B1. Non-blocking runner and async discipline.** The runner must start evaluations and continue
  (`WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`); stale-result rules per element; repaint wake-up from any
  thread (`PY-UI-CHANGE-DOES-NOT-WAKE-AN-OPEN-WINDOW`). Prerequisite for G3/G5.
- **B2. Input and keymap model.** One keymap across frontends (chords, scopes, user overrides, a
  shortcut cheat sheet), merged with G13 controls; focus scopes per mount
  (`WEBUI-SHORTCUTS-RUN-IN-EVERY-MOUNT`, `WEB-EDITOR-HAS-NO-KEYBOARD-MODEL`). `craft-keymap` is a
  reference.
- **B3. Command palette and tool palettes.** Ctrl+K fuzzy search over commands, UI actions and
  documents; tool palettes of icon buttons (`COMMANDS-HAVE-NO-TOOL-PALETTE`); both read command
  icons, labels and presets.
- **B4. Notifications, logs and errors.** Toasts, an error panel with structured errors
  (`ERROR-TYPE-TAXONOMY`, `ERROR-WITH-KEY-FIELD`), per-asset logs, a status bar.
- **B5. Developer tooling.** `UISpec` validator and editor, theme editor and preview, icon catalogue
  browser, a protocol inspector (record and replay a session's messages), `liquers-theme` CLI.
- **B6. Workspace persistence.** Save and restore a UI document with its layout, open elements,
  theme name and view state; reopen a session; "share this view" as a link (a UI document key).
- **B7. Accessibility and i18n.** AccessKit in egui, ARIA roles in the web frontend, keyboard-only
  operation, screen-reader names from labels; translatable labels in command metadata and `UISpec`.
- **B8. Undo/redo.** Per-editor history, accessor-level undo for forms.
- **B9. Image and canvas tools.** Image viewer with zoom/pan (a G13 showcase), before/after compare,
  drawing tools driven by tool palettes; the first consumer of `vec2`/`scalar` controls.
- **B10. Graph views.** Dependency graph of assets and recipes, plan visualisation (the VISION's
  "visual query representation"), useful for the recipes editor and the asset manager.
- **B11. Agent workbench UI.** Inspect and approve agent-made queries, browse an `ns-mem` memory
  (`liquers-agent`), the VISION's "oversight of queries by a human".
- **B12. Language integration.** Python and JavaScript mount, define and drive elements of the new
  crates; themes and icons from Python/JS commands; a rule per design on whether `liquers-py` /
  `liquers-web` are in scope (CLAUDE.md).
- **B13. Testing and conformance.** Frontend conformance suite (G2), protocol conformance tests
  (a recorded client against the server), snapshot tests, an executable example per element.
- **B14. Performance and size.** Virtualised lists and tables, incremental DOM updates, binary
  frames, backpressure for slow clients, a wasm client without polars or image codecs
  (`WEB-RELEASE-MODULE-IS-20-MB`), egui upgrade policy (0.36 → 0.37 moves `egui_plot` and
  `egui_commonmark` with it).
- **B15. Offline and reconnect.** The client keeps showing its last state while disconnected,
  queues events or refuses them visibly, resyncs on reconnect (AC-6).
- **B16. Plugins and third-party elements.** Registering element models and views from other
  crates, Python and JavaScript (`web-ui-define` today); versioned element type names.

## Coverage: every open UI-related issue

Open issues and features (status not closed) that touch the UI, and the plan design that takes
each (design ids from [`plan.md`](./plan.md)).

| Issue | P | Size | Area | Plan design |
|---|---|---|---|---|
| `UI-NOT-SEPARATED-BY-CRATE-AND-TECHNIQUE` | P1 | XL | G1/G2 | D1 (umbrella of D1–D3) |
| `UI-INTERFACE-FSD-DESCRIBES-UNBUILT-API` | P2 | M | G1 | D1 (rewrite the reference for the new crate) |
| `WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS` | P2 | L | B1 | D1 |
| `UI-ELEMENT-KEEPS-AN-EXPIRED-VALUE-AND-NEVER-RE-REQUESTS` | P3 | S | G3 | D1 (fix now on the old UI if eligible) |
| `EGUI-ASSET-MANAGER-INTEGRATION` | P2 | M | G1 | D1 (with `ui-asset-snapshot-boundary`) |
| `UI-WIDGET-DOES-NOT-CONVERT-ITS-VALUE` | P3 | M | G1/G6 | D2 (with `ui-display-conversion`) |
| `WEB-UI-CANNOT-BE-DRIVEN-BY-A-REMOTE-ENGINE` | P3 | XL | G3 | D3 |
| `AXUM-CANNOT-SERVE-A-LIQUERS-UI` | P3 | L | G3/G5 | D3 |
| `AXUM-WEBSOCKET-HARDENING` | P3 | M | G5 | D3 (with `axum-websocket-connection-limits`) |
| `NO-COMPACT-BINARY-DATA-FORMAT` | P3 | L | G5 | D3 (wire form), later standalone |
| `CORE-SESSION-AND-KEY-ACL` | P2 | L | G7 | D4 (with `access-policy-at-entry-points`) |
| `SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED` | P2 | L | G7 | D4 (the auth settings part) |
| `ACCESSOR-CANNOT-BE-SENT-TO-A-CLIENT` | P3 | L | G6/G12 | D5 |
| `WEB-HAS-NO-READ-WRITE-REMOTE-STORE` | P3 | M | G6 | D5 (via `remote-store-and-asset-manager`) |
| `EGUI-EXAMPLES-HAND-WRITE-THE-FRAME-LOOP` | P3 | S | G2 | D6 |
| `PY-UI-CHANGE-DOES-NOT-WAKE-AN-OPEN-WINDOW` | P3 | S | B1 | D6 |
| `WEBUI-SHORTCUTS-RUN-IN-EVERY-MOUNT` | P3 | S | B2 | D7 (fix now if eligible) |
| `WEB-ONEVENT-SHOW-GUARD-IGNORES-THE-MOUNT` | P3 | S | G2 | D7 (fix now if eligible) |
| `WEB-RELEASE-MODULE-IS-20-MB` | P3 | M | B14 | D7 |
| `WEB-MOUNTUI-HAS-NO-THEME-OPTION` | P3 | S | G8/G11 | D7 (fix now if eligible) |
| `NO-RENDERER-NEUTRAL-STYLESHEET` | P3 | L | G8 | D10 |
| `ICON-SETS-OTHER-THAN-PHOSPHOR-ARE-NOT-DRAWN` | P3 | M | G8 | D10 |
| `NAMESPACES-HAVE-NO-METADATA` | P3 | M | G8 | D10 |
| `UI-DOCUMENT-CANNOT-NAME-ITS-THEME` | P3 | S | G8/G12 | D10 |
| `PY-COMMAND-CANNOT-SET-ICON-PRESETS-OR-NEXT` | P3 | S | G8/B12 | D10 (fix now if eligible) |
| `HOSTS-READ-CONFIGURATION-DOCUMENTS-DIFFERENTLY` | P3 | L | G11 | D11 |
| `WEB-STORE-CONFIG-NOT-APPLIED-THROUGH-ENVIRONMENT-CONFIG` | P3 | M | G11 | D11 |
| `VALUE-ACCESSOR-CANNOT-LOCK` | P3 | L | G12 | D12 |
| `YAML-DOCUMENT-EDIT-DROPS-COMMENTS` | P3 | M | G12 | D12 |
| `ACCESSOR-CANNOT-BE-IMPLEMENTED-IN-PYTHON-OR-JAVASCRIPT` | P3 | L | G12/B12 | D12 (later part) |
| `UI-ELEMENT-NESTED-IN-CONSOLE-HAS-NO-HANDLE` | P3 | M | G12 | D12 (with `console-mounts-nested-element`) |
| `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS` | P3 | L | G12 | D12 (form side only) |
| `UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS` | P3 | L | G13 | D13 |
| `WEB-EDITOR-HAS-NO-KEYBOARD-MODEL` | P3 | M | B2/G13 | D13 (keyboard), D10 (icon images) |
| `COMMANDS-HAVE-NO-TOOL-PALETTE` | P3 | M | B3 | D20 |
| `TEXT-EDITOR-EDITS-ARE-NEVER-SAVED` | P3 | M | G10 | D18 (with `text-editor-save`) |
| `TEMPLATE-HAS-NO-HIGHLIGHTER` | P3 | L | G10 | D18 (with `template-syntax-highlighter`) |
| `NO-TEMPLATE-EDITOR-UI` | P3 | L | G10 | D19 (with `template-editor-ui`) |
| `UI-QUERY-EDITOR-LACKS-LIVE-VALIDATION` | P2 | L | G10 | D19 (with `query-editor-live-validation`) |
| `WEB-QUERY-EDITOR-NOT-LIVE` | P3 | M | G10 | D19 (with `web-query-editor-live-preview`) |
| `CORE-QUERY-EDITOR-ACCEPTS-AN-UNKNOWN-COMMAND` | P3 | S | G10 | D19 (fix now if eligible) |
| `EDITOR-CANNOT-EDIT-INSIDE-A-NESTED-PIPELINE` | P3 | M | G10 | D19 |
| `WEB-PIPELINE-EDITOR-HAS-NO-DEFINITION-MODE` | P3 | S | G10 | D19 |
| `PIPELINE-COMMAND-OPERAND-TYPES-UNCHECKED` | P3 | S | G10 | D19 (via `composite-command-definition`) |

Related, not UI-owned (dependencies the plan consumes, not covers): `VALUE-INTERFACE-CAPABILITY-SPLIT`
(D2), designs `realm-type-registry`, `registry-publication`, `multi-realm-dispatch`,
`remote-asset-manager`, `remote-store-and-asset-manager` (D5), `relational-database-access`,
`sql-over-tables` (D16), `assets-api-admin-operations`, `unmanaged-asset-registry`,
`asset-events-over-websocket` (D17), `error-type-taxonomy` (B4), `library-no-panic-lint`
(`LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` names `lib/egui`; the new crates start lint-clean).

In-review designs on the old UI (`ui-asset-snapshot-boundary`, `ui-display-conversion`,
`text-editor-save`, `template-editor-ui`, `template-syntax-highlighter`,
`query-editor-live-validation`, `web-query-editor-live-preview`, `console-mounts-nested-element`)
are listed with the plan design that would absorb or re-target them; what to do with each is
plan decision 2: absorbed into D1/D2 or re-targeted at the new crates in M4.
