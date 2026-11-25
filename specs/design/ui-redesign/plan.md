---
id: UI-REDESIGN-PLAN
kind: analysis
title: UI redesign — designs, dependencies and milestones
workflow: liquers-project
status: draft
area: [lib/ui, lib/egui, web, axum]
created: 2026-11-22
---
# UI redesign: plan

A sketch for review, written 2026-11-22 from the [idea map](./idea-map.md). Nothing here is
approved. Each **D** below becomes its own `liquers-project` design (`specs/design/<slug>/`) when
its milestone starts; the slugs are proposals. Sizes use `DOCS_STRUCTURE_GUIDE.md` §4.5.

## 1. Target architecture (sketch)

> **Superseded in part, 2026-11-23.** The [master design](./master-design.md) refines this
> section: one runtime for every element, native views per frontend and a portable view both
> first-class (its §3.4, §5), clients from very thin to thick (§16), crate layout in §22. Its §29
> lists the effect on D1–D21 and the spikes, and adds D22 (very thin web client).

```text
                   liquers-core ── liquers-records (optional, table pages)
                        │
                   liquers-ui            model crate: tree, handles, AppState, runner, events,
                        │                ElementModel/ElementView traits, protocol types, theme,
                        │                icons, UISpec, controls (function mapping), client driver
      ┌─────────┬───────┼──────────┬───────────────┐
 liquers-ui-egui  liquers-ui-web  liquers-ui-tui  liquers-ui-server ── liquers-axum (feature `ui`)
  (egui views,     (DOM views,     (ratatui views,  (sessions, WebSocket,      │
   native host,     CSS, ECharts,   crossterm host)  auth hook)          liquers-ui-tauri
   egui_plot)       wasm client)                                         (IPC transport, shell)
      └─────────┴───────┬──────────┴───────────────┘
                   liquers-lib            models and projections for ExtValue (DataFrame → RecordView
                                          page, image → PNG), the `lui` commands; depends on the UI
                                          crates, never the reverse
```

- **Model and views.** A splittable element is an `ElementModel` (authoritative state, asset
  subscriptions, `Down`/`Up` messages) plus one view per frontend (`EguiView`, `WebView`,
  `TuiView`). Monolithic = in-process transport with values by pointer; split = an encoding
  transport (WebSocket, Tauri IPC). Today's `UIElement` survives as a *fused element* behind an
  adapter, so the old and new UI run side by side (decided 2026-11-17).
- **Frontends are crates; logic is in the core** (decided 2026-11-22). Adding a frontend adds a
  crate and view impls; `liquers-ui` never learns about it. As much logic as possible lives in
  `liquers-ui`: state, validation, editing, paging, focus, keymaps, layout decisions, theme
  resolution. A frontend crate only draws and turns native input into `UIEvent`s. The core also
  defines the common rendering patterns, so each frontend picks one instead of inventing its own:
  an **immediate-mode** pattern (egui, ratatui: draw the whole view each frame from the view state)
  and a **retained-mode** pattern (DOM, later Slint or GTK: build once, then apply the change set
  of `webui-fixes`' invalidations). Three first-class frontends validate the traits from the start:
  egui (immediate), web (retained DOM), ratatui (immediate, character cells).
- **Data stays where it lives.** The client receives projections (pages, PNG, chart data windows)
  unless it declares the type (`realm-type-registry`); light work may run in a client environment
  (realms), heavy work on the server.
- **Identity is pluggable.** An `Authenticator` produces a `Principal`; the access policy of
  `access-policy-at-entry-points` decides; IdP-specific code lives in optional crates.

### 1.1 The web from the start

In the old UI the web came last (`render_web` beside `show_in_egui`). Here it shapes the core
from D1 on. What the core and D7 must account for:

- **One thread, never block:** wasm in a page has a single thread and no `block_on`; the runner,
  subscriptions and every element API are async-safe and non-blocking (B1).
- **Retained DOM:** views build nodes once and apply change sets (`webui-fixes` invalidations);
  element identity and keys must be stable across updates; no full re-render per event.
- **CSS is native:** the theme is emitted as custom properties, and styling stays in CSS rather
  than inline drawing; style queries (D10) map to selectors.
- **URLs and history:** a view state has an address (deep links into a `UISpec` document, a
  selected tab, a table page); back/forward work.
- **Responsive layout and touch:** layouts adapt to width; touch and pointer events feed the
  controls of D13.
- **Accessibility:** semantic elements and ARIA roles, focus order, keyboard-only operation.
- **Native text input:** browser text fields, IME and spell-check instead of drawn text, so
  editors use a web editor engine (CodeMirror or Monaco) rather than a port of the egui one.
- **Browser services:** clipboard permissions, file upload and download, Web MIDI, Web Workers
  for heavy client work, storage quotas.
- **Size and loading:** the client wasm stays small (no polars, no image codecs by default:
  `WEB-RELEASE-MODULE-IS-20-MB`); lazy loading of large views (charts, editors).
- **Embedding:** a Liquers UI mounts into an existing page and its framework (custom elements,
  `liquers-web` `mountUI`), several mounts per page with separate focus scopes
  (`WEBUI-SHORTCUTS-RUN-IN-EVERY-MOUNT`).
- **SSR:** server-rendered first paint stays possible (`webui`'s `render_app_ssr`), then the
  client takes over.

## 2. Designs

| # | Proposed slug | Scope | Size | Covers (issues, absorbed designs) | Depends on |
|---|---|---|---|---|---|
| D1 | `ui-core-crate` | `liquers-ui` crate: tree, handles, `AppState`, events; `ElementModel`/view traits; non-blocking runner with several subscriptions per element; fused-element adapter; migration rules; rewrite of `UI_INTERFACE_FSD.md` | XL | `UI-NOT-SEPARATED-BY-CRATE-AND-TECHNIQUE`, `UI-INTERFACE-FSD-DESCRIBES-UNBUILT-API`, `WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`, `UI-ELEMENT-KEEPS-AN-EXPIRED-VALUE…`, `EGUI-ASSET-MANAGER-INTEGRATION`; absorbs `ui-asset-snapshot-boundary`, reuses `ui-split-elements` | spikes S1, S2 |
| D2 | `ui-value-projection` | How a model shows a value without `liquers-lib`: display capabilities, projections (text, `RecordView` page, PNG/SVG, chart data), conversion by automatic edge, what may travel | L | `UI-WIDGET-DOES-NOT-CONVERT-ITS-VALUE`; absorbs `ui-display-conversion` | D1; `value-interface-capability-split`, `realm-type-registry` |
| D3 | `ui-protocol-and-transport` | Protocol messages, envelope, revisions, resync, session vs connection, capabilities in `hello`; in-process and WebSocket transports; `liquers-ui-server`; `liquers-axum` route; client driver | XL | `WEB-UI-CANNOT-BE-DRIVEN-BY-A-REMOTE-ENGINE`, `AXUM-CANNOT-SERVE-A-LIQUERS-UI`, `AXUM-WEBSOCKET-HARDENING`; uses `axum-websocket-connection-limits` | D1, D2 |
| D4 | `ui-auth-and-sessions` | `Authenticator` trait and `Principal`; built-ins (none, token, trusted proxy header, local app); optional OIDC crate; session store, lifetime, reattach, joining another user's session | L | `CORE-SESSION-AND-KEY-ACL`, `SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED` (auth part) | D3; `access-policy-at-entry-points` |
| D5 | `ui-client-side-processing` | Client as an environment with its own realm; which commands run where; accessor descriptors; remote store from the client | L | `ACCESSOR-CANNOT-BE-SENT-TO-A-CLIENT`, `WEB-HAS-NO-READ-WRITE-REMOTE-STORE` | D3; `multi-realm-dispatch`, `registry-publication`, `remote-store-and-asset-manager` |
| D6 | `ui-egui-frontend` | `liquers-ui-egui`: views for the built-in elements, native host (move `NativeDriver`/`run_window`), theme → `Visuals`, egui as split client | L | `EGUI-EXAMPLES-HAND-WRITE-THE-FRAME-LOOP`, `PY-UI-CHANGE-DOES-NOT-WAKE-AN-OPEN-WINDOW` | D1, D2 (D3 for client mode) |
| D7 | `ui-web-frontend` | `liquers-ui-web`: DOM views (from today's `ui/web`), CSS, monolithic wasm mount and the default split client, `liquers-web` `mountUI` on top, a client build without polars | L | `WEBUI-SHORTCUTS-RUN-IN-EVERY-MOUNT`, `WEB-ONEVENT-SHOW-GUARD-IGNORES-THE-MOUNT`, `WEB-MOUNTUI-HAS-NO-THEME-OPTION`, `WEB-RELEASE-MODULE-IS-20-MB` | D1, D2, D3 |
| D8 | `ui-tui-frontend` | `liquers-ui-tui`: ratatui views and crossterm host, theme → ANSI styles, Unicode icons; split client over WebSocket | L | — (new) | D1, D2 |
| D9 | `ui-tauri-shell` | Tauri 2 desktop shell: IPC or loopback transport, single local session, packaging template; mobile as a later phase | M | — (new) | D3, D7; S2 |
| D10 | `ui-theme-portable` | Theme model moved to `liquers-ui`; TUI mapping; `liquers-theme` CLI (validate, compose, CSS/DTCG export); more icon sets and icon assets; namespace metadata; style queries for element kinds and states | L | `NO-RENDERER-NEUTRAL-STYLESHEET`, `ICON-SETS-OTHER-THAN-PHOSPHOR-ARE-NOT-DRAWN`, `NAMESPACES-HAVE-NO-METADATA`, `UI-DOCUMENT-CANNOT-NAME-ITS-THEME`, `PY-COMMAND-CANNOT-SET-ICON-PRESETS-OR-NEXT` | D1 (the move); style queries after D12 Phase 1 |
| D11 | `ui-configuration` | `UiConfig` extended (frontend sections, start document, keymap, MIDI, session settings); one typed reader for every host | M | `HOSTS-READ-CONFIGURATION-DOCUMENTS-DIFFERENTLY`, `WEB-STORE-CONFIG-NOT-APPLIED-THROUGH-ENVIRONMENT-CONFIG` | D1 |
| D12 | `ui-spec-v2` | `UISpec` as an application language: accessor bindings, forms, actions, components and includes, docking, per-element theme overrides, validation; likely splits into *bindings and forms* and *layout and components* | XL | `VALUE-ACCESSOR-CANNOT-LOCK`, `YAML-DOCUMENT-EDIT-DROPS-COMMENTS`, `UI-ELEMENT-NESTED-IN-CONSOLE-HAS-NO-HANDLE`, later `ACCESSOR-CANNOT-BE-IMPLEMENTED-IN-PYTHON-OR-JAVASCRIPT`; absorbs `console-mounts-nested-element` | D1, D2; D5 for split forms |
| D13 | `ui-function-mapping` | Controls vocabulary (scalar, angle, vec2, vec3, range, choice, step, trigger, toggle); command and element hints; sources (pointer, keyboard, touch, gamepad); unified keymap and focus scopes | L (research first) | `UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS`, `WEB-EDITOR-HAS-NO-KEYBOARD-MODEL` (keyboard) | D1; spike S4 |
| D14 | `ui-midi-mapping` | MIDI as a control source: device profiles, mapping file, learn mode, ranges, relative encoders, soft takeover, feedback; `midir`, Web MIDI, Tauri | M | — (new; lessons from `egui-midi-test`) | D13, D11 |
| D15 | `ui-charts` | `ChartSpec` value (not an egui closure) with a common subset: time axes, line, area, bar, scatter, OHLC candlestick with a volume panel; egui `egui_plot` (candles drawn by us), web ECharts, ratatui braille lines and sparklines, matplotlib from Python (plotly later); an hvPlot-like high-level API; per-frontend extensions; theme hues for series; live updates on expiry with append-only pages; static SVG/PNG; `ns-chart` commands; downsampling | L | — (new); replaces `stockplottertest`'s `simple_plot` | D2, D6, D7, D8; spike S3 |
| D16 | `ui-table-tools` | Paged table viewer/editor over `RecordView` for records, DataFrames and SQL; SQL editor and schema browser; spreadsheet-like grid (research) | L | — (new) | D2, D18; `relational-database-access`, `sql-over-tables` |
| D17 | `ui-asset-store-tools` | Asset manager, orthodox two-pane store manager, process monitor (queue, jobs, events, cancel) | L | — (new) | D3 for the server view; `assets-api-admin-operations`, `asset-events-over-websocket`, `unmanaged-asset-registry` |
| D18 | `ui-text-editors` | One editor element with syntax modes (query, template, YAML, JSON, SQL, Markdown, code), diagnostics, save; Markdown editor with preview; per-frontend editor engines | L | `TEXT-EDITOR-EDITS-ARE-NEVER-SAVED`, `TEMPLATE-HAS-NO-HIGHLIGHTER`; absorbs `text-editor-save`, `template-syntax-highlighter` | D1, D6, D7 |
| D19 | `ui-query-template-recipe-editors` | Query editor with live validation on the new crates in every frontend; template editor with integrated query editor; recipes editor with validation and plan preview | L | `UI-QUERY-EDITOR-LACKS-LIVE-VALIDATION`, `WEB-QUERY-EDITOR-NOT-LIVE`, `NO-TEMPLATE-EDITOR-UI`, `CORE-QUERY-EDITOR-ACCEPTS-AN-UNKNOWN-COMMAND`, `EDITOR-CANNOT-EDIT-INSIDE-A-NESTED-PIPELINE`, `WEB-PIPELINE-EDITOR-HAS-NO-DEFINITION-MODE`, `PIPELINE-COMMAND-OPERAND-TYPES-UNCHECKED`; absorbs `query-editor-live-validation`, `web-query-editor-live-preview`, `template-editor-ui` | D18, D12 |
| D20 | `ui-palettes` | Command palette (fuzzy search) and tool palettes of icon buttons from command hints | M | `COMMANDS-HAVE-NO-TOOL-PALETTE` | D10, D13 |
| D21 | `ui-frontend-conformance` | Built-in element set every frontend implements; conformance suite over `UISpec` fixtures per frontend test backend; protocol replay tests | M | — (new) | D1; grows with D6–D8 |

Later, not yet designs: workspace persistence (B6), accessibility and i18n (B7), undo/redo (B8),
graph views (B10), agent workbench UI (B11), other desktop frontends (iced, Slint, Dioxus),
TUI in the browser.

## 3. Spikes before the architecture is fixed

Throwaway branches whose findings go to `specs/archive/` and into D1/D3 Phase 2. Each answers one
question.

| Spike | Question | Feeds |
|---|---|---|
| S1 ratatui | Do the draft `ElementModel` + view traits work for an immediate GUI, a DOM and a terminal? Render asset view, text and a list in all three | D1, D8 |
| S2 transports | The transport abstraction over WebSocket (required) and Tauri IPC (`invoke` + `Channel`): binary throughput, latency, startup, mobile background behaviour; a paper check that MQTT (topics per session, MQTT 5 response topics, broker auth, message size limits) fits the same abstraction | D3, D9 |
| S3 charts | Survey ECharts, `egui_plot`, matplotlib, plotly and hvPlot for common patterns (series, axes, panels, styling, interaction); then port `stockplottertest`'s `simple_plot` to a draft `ChartSpec`: `brownian_quotes` (live, 100 ms expiry) and `merge_historical_data-AAPL` as candlesticks + volume + close line, rendered by `egui_plot`, ECharts and ratatui | D15 |
| S5 Slint | A second retained-mode frontend, Slint (chosen 2026-11-22; check its licence first), renders the M1 element set, so the core's retained pattern is checked against a toolkit without a DOM | D1 |
| S4 controls | Catalogue functions across `ns-img`, `ns-pl` and charts; drive three of them from keyboard, wheel and a MIDI controller (`egui-midi-test` as base) | D13, D14 |

## 4. Dependencies

```mermaid
graph LR
  S1 --> D1; S5 --> D1; S2 --> D3; S2 --> D9; S3 --> D15; S4 --> D13
  D1 --> D2 --> D3
  D1 --> D6; D1 --> D8; D1 --> D10; D1 --> D11; D1 --> D21
  D2 --> D6; D2 --> D7; D2 --> D8; D3 --> D7
  D3 --> D4; D3 --> D5; D3 --> D9; D7 --> D9
  D2 --> D12; D5 --> D12
  D1 --> D13 --> D14; D11 --> D14
  D2 --> D15; D6 --> D15; D7 --> D15; D8 --> D15
  D2 --> D16; D18 --> D16
  D3 --> D17
  D6 --> D18; D7 --> D18; D18 --> D19; D12 --> D19
  D10 --> D20; D13 --> D20
```

The critical path is **S1 → D1 → D2 → D3 → D7 → D9**: the core crate, the value boundary, the
protocol, the web client, Tauri. Everything else hangs off D1/D2 and can run in parallel once they
are approved.

## 5. Milestones (proposed priority)

Each milestone ends with something runnable. The old UI keeps working throughout; only fixes land
on it (decision 2).

**M0 — Architecture decided** (no product change). Spikes S1, S2, S5. D1 and D2 through Phase 2, D3
through Phase 1. Exit: the trait signatures and crate layout approved; the "proceed" gates of D1/D2
passed.

**M1 — New core, monolithic egui, ratatui and web.** D1 and D2 implemented; D6, D8 and D7's
monolithic part (wasm in the page) for the basic element set (asset view, text, Markdown, table
page, image, layouts, menu); D10 part 1 (theme moved to `liquers-ui`, TUI mapping, CSS); D11; D21
skeleton. Exit: one `UISpec` document runs natively in egui, in a terminal and in a browser on the
new crates, next to the old UI. The web is in M1 on purpose (decided 2026-11-22): egui and ratatui
are both immediate-mode, and the web is the retained-mode frontend that must shape the core.

**M2 — Split mode.** D3 implemented; D7's split client; D4 part 1
(single session; `none` and token authenticators; loopback bind by default); D9 desktop. Exit: a
browser, the egui app and a Tauri app attach to a `liquers-axum` UI session and show the M1
document; `ui-split-elements` AC-1..AC-9 pass.

**M3 — Data tools.** D16 (tables, SQL viewer), D15 (charts in three frontends, after S3; `stockplottertest` ported as the acceptance app),
D17 (asset manager, store manager, process monitor), D5 (client-side processing). Exit: the
2-million-row example of `ui-split-elements` with a chart over it, in egui and the browser.

**M4 — Composition and editors.** D12 (bindings and forms first), D18, D19, D20, D10 part 2 (style
queries, icon assets, CLI). Exit: a form over a YAML config file through accessors, a template and
recipes editor, all in a `UISpec` with theme overrides.

**M5 — Inputs.** S4, then D13 and D14; touch gestures. Exit: zoom and pan of an image viewer and a
chart from a MIDI knob, presets on MIDI buttons, without per-command MIDI code.

**M6 — Multi-user and mobile.** D4 part 2 (multi-session, trusted proxy, OIDC crate, joining
sessions), WebSocket hardening, D9 mobile phase. Exit: two users on one server with separate
sessions and key policies; an Android build of the Tauri app.

**M7 — Retire the old UI.** Port the remaining elements, move the `lui` commands onto the new
crates, deprecate `UIElement` and the feature-gated render methods, update the user guides.

Rationale for the order: M0–M2 carry the structural goals (crates, frontends, split, Tauri) that
everything else depends on; doing tools and editors first would mean writing them twice. M3 comes
before M4 because tables and charts are what the split model is for (data on the server) and they
test the projection rules hardest. Inputs (M5) need the controls research, which can start as S4
during M1–M3 without blocking. Multi-user (M6) is last because a single local session covers
desktop, Tauri and development; D4 Phase 1–2 can still be written early so M2 does not preclude it.

## 6. Cross-cutting rules for every design

- **Language integration** (CLAUDE.md): each design records Python (`liquers-py`) and JavaScript
  (`liquers-web`) as in scope, later or not needed. Expected: D1, D7, D10, D12 touch both;
  `liquers-web` exposes D7's mount and client; `liquers-py` keeps its window on D6.
- **User guides:** the UI chapters of `docs/guides/{rust,python,web}/` are rewritten at M2 and M4,
  not per design.
- **New crates start clean:** no `unwrap`/`expect` (the `library-no-panic-lint` rule), explicit
  matches, module docs per `API_DOCUMENTATION_GUIDE.md`, a `check-build-matrix.sh` row each
  (wasm32 for `liquers-ui` and `liquers-ui-web`).
- **Issues:** a design without a source issue files a `kind: feature` issue when it starts (D8,
  D9, D13–D17, D21); small eligible bugs listed "fix now" in the idea map are fixed on the old UI
  in their own branches rather than waiting.

## 7. Decisions (questions answered by Orest on 2026-11-22)

1. **Crate split — answered 2026-11-22:** one core crate plus one crate per frontend (and
   `liquers-ui-server`). The core holds as much logic as possible and defines the common patterns,
   among them support for both immediate-mode and retained-mode frontends (§1).
2. **In-review designs on the old UI** (`ui-asset-snapshot-boundary`, `ui-display-conversion`,
   `text-editor-save`, `template-editor-ui`, `template-syntax-highlighter`,
   `query-editor-live-validation`, `web-query-editor-live-preview`,
   `console-mounts-nested-element`): finish them on the old UI, or absorb them into D1/D2/D18/D19
   and implement only on the new crates? **Answered 2026-11-22: absorb and re-target.** The first
   two fold into D1/D2, the editor designs move to the new crates at M4, and only bug fixes land on
   the old UI.
3. **Milestone order — answered 2026-11-22:** egui, ratatui and web together in M1 (monolithic),
   split mode in M2. The web must be designed in from the start, not added afterwards (§1.1). A
   second retained-mode frontend triangulates the architecture: spike S5.
4. **Chart specification — answered 2026-11-22:** our own `ChartSpec` with per-backend
   extensions. It uses Liquers queries (data, and charts as query results) and theming, and may
   reuse `UISpec` mechanisms such as layout for multi-panel figures. Its shape takes inspiration
   from ECharts, to make the web port easy. Backends beyond the UI frontends are in view:
   matplotlib (from Python), later plotly and others (even XMGrace). hvPlot is a model for the
   high-level API. Liquers is an integration layer that should play well with other systems, and
   "batteries included" with the major libraries, so D15 surveys these backends to find the
   patterns that work across them.
5. **Transports — answered 2026-11-22:** one transport abstraction below the protocol. WebSocket
   is required; Tauri IPC is investigated in S2 as a second implementation; other channels are
   possible, MQTT in particular (interesting for a future embedded Liquers, "lqe"). D3 defines the
   abstraction as an ordered, bidirectional channel of envelope frames with connect, close and
   reconnect, so sessions, revisions and resync sit above it and do not change per transport.
   Candidates: in-process (monolithic), WebSocket, Tauri IPC, MQTT, a Web Worker `postMessage`
   (engine in a worker, view in the page), stdio (a TUI or test client in a subprocess).
6. **Inputs versus multi-user — answered 2026-11-22:** inputs first (M5), multi-user after (M6).
7. **`stockplottertest` as the chart acceptance app — answered 2026-11-22: yes to both.** S3 and
   D15 use it as the reference application (its stock queries rewritten to return a `ChartSpec`),
   and it moves to the new crates at M3 as the first real app on them.
