# Phase 1: High-Level Design — the UI core crate

## Purpose

Give Liquers one UI model that does not depend on `liquers-lib` or on any toolkit. Every frontend
(egui, ratatui and the browser first) is then a separate crate that only draws and turns input into
events. This is milestone M0/M1 of the UI redesign ([`../ui-redesign/plan.md`](../ui-redesign/plan.md),
design D1). It serves the VISION's "every view is a query" desktop and web uses, and VISION
problem 5 (one UI, not one per host).

## Problem Example

A small application: a `UISpec` document with a menu, a query console, a Markdown help page, and
an asset view of `-/brownian_quotes/ns-pl/describe` (from `orest-d/stockplottertest`, a value
that expires every 100 ms).

**Today.**
- It runs in egui (`NativeDriver`, `run_window`) and in a page (`mount_web`), both inside
  `liquers-lib`. A terminal cannot show it. Adding a terminal means adding a `show_in_ratatui`
  method to `UIElement` (`liquers-lib/src/ui/element.rs`) and a feature to `liquers-lib`, as the
  March notes (`specs/archive/2026-03-02-ui-ratatui-design-notes.md`) proposed.
- The browser build links `liquers-lib`, and with its default features polars and the image
  codecs (`WEB-RELEASE-MODULE-IS-20-MB`).
- Logic is spread across the renderers. The egui and web query editors each turn input into
  edits (`egui/editor.rs`, `ui/web/editor.rs`), so the two can drift.
- `AppRunner::run` awaits evaluations, so the whole UI stops while one query runs
  (`WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`). The asset view keeps the expired describe table and
  never asks again (`UI-ELEMENT-KEEPS-AN-EXPIRED-VALUE-AND-NEVER-RE-REQUESTS`). An element can
  watch only one asset.

**Wanted.** The same document opens in egui, in a terminal and in the browser, each through its
own frontend crate over `liquers-ui`. The tree, the titles, the statuses, the menu and the edits
are computed once in `liquers-ui`. The describe table refreshes while the menu stays responsive.
The old UI in `liquers-lib` keeps working beside it.

## Scope and Acceptance Criteria

- **AC-1** The core stands alone
  WHEN `liquers-ui` is built for native and for `wasm32-unknown-unknown`
  THEN it builds, and its dependency tree contains `liquers-core` but no `liquers-lib`, polars,
  egui, ratatui, crossterm, `web-sys` or Slint
- **AC-2** One document, three frontends
  WHEN the example document is opened by the egui host, the ratatui host and the browser mount
  THEN each shows the same element tree, with the same titles, statuses and menu entries
- **AC-3** Logic lives in the core
  WHEN the same sequence of UI events (open the menu, run a query, edit a query parameter) is fed
  to the document under each frontend's test backend
  THEN the element states in `liquers-ui` are identical after each event
- **AC-4** A frontend is added without changing the core
  WHEN a new frontend crate implements only the required level of the view vocabulary
  THEN it compiles with no change to `liquers-ui`, and every portable element is shown, with the
  node kinds the frontend lacks degraded by the core's rules (master design §10.7); an element
  that has only native views for other frontends shows a generic view
- **AC-5** Immediate and retained frontends
  WHEN one element changes state
  THEN an immediate-mode frontend (egui, ratatui) draws the new state on its next frame, and a
  retained-mode frontend (the browser) receives a change set naming only that element
- **AC-6** The UI never waits for a query
  WHEN one element's query takes five seconds
  THEN the other elements, the menu and the input stay responsive and keep updating
- **AC-7** Expired values are fetched again
  WHEN the value shown by an asset view expires
  THEN the element asks for it again and shows the new value, without user action
- **AC-8** An element watches several assets
  WHEN an element subscribes to a value query and to a second query (its log or a related value)
  THEN it receives updates for both, and removing the element stops both subscriptions
- **AC-9** Queries create elements
  WHEN a command returns an element (for example `ns-lui/query_console/q/add-child`)
  THEN the element is mounted in the tree, as with `liquers-lib`'s `lui` commands today
- **AC-10** The old UI runs alongside
  WHEN the existing `liquers-lib` UI (egui window, web mount, Python window) is used
  THEN it behaves as before, and an existing `UIElement` can be mounted in a new tree as a fused
  element for monolithic use
- **AC-11** Several mounts on one page
  WHEN two documents are mounted in one browser page
  THEN each has its own tree, focus and shortcuts, and an event in one never reaches the other

Non-goals: the client-server protocol, sessions and transports (D3, D4); how values become
displayable forms (D2); moving the theme (D10, though the core reads it); charts, tables, editors
and `UISpec` v2 (D12-D19); the Slint frontend (spike S5 only); removing the old UI (M7).

## Core Interactions

- **UI:** new element model, tree, session, messages, view tree and patches, rendering patterns,
  degrading rules.
- **Assets:** the runner subscribes per element (several each), reacts to expiry, never blocks.
- **Commands:** the `lui` commands also return elements of the new model.
- **Value types:** held without knowing `ExtValue`; display forms come from D2.

## Crate Placement

New crate `liquers-ui`, depending on `liquers-core` only, and building on native and wasm32. Three
frontend crates, created by D6/D7/D8 but scaffolded here: `liquers-ui-egui`, `liquers-ui-web` and
`liquers-ui-tui`. `liquers-lib` depends on `liquers-ui` (behind a feature) to provide elements for
its own value types and the `lui` commands. Nothing in `liquers-ui` depends on `liquers-lib`.
Dependency flow: `liquers-core` ← `liquers-ui` ← `liquers-ui-*` and `liquers-lib`.

## Language Integration

- Integration guide: no new feature ID in D1. The UI features `UIUSE`/`UIDEF` are extended when
  D6/D7 expose the new crates.
- Python (`liquers-py`): not needed in D1. Its window keeps using the old UI (AC-10); moving it
  to `liquers-ui-egui` belongs to D6.
- JavaScript (`liquers-web`): not needed in D1. `mountUI` keeps using the old UI; moving it to
  `liquers-ui-web` belongs to D7.

## Documentation Intent

- Reference: new `specs/reference/UI_CORE.md` (the model, the runner, the view trait, the
  patterns). `UI_INTERFACE_FSD.md` is rewritten to point to it and keep only the old UI
  (`UI-INTERFACE-FSD-DESCRIBES-UNBUILT-API`).
- Guide: new `specs/guides/UI_FRONTEND_GUIDE.md`: how to write a frontend crate and how to write
  an element.
- Other documents: `CLAUDE.md` project structure and dependency flow; `specs/README.md`.
- Documents to update: `specs/reference/PROJECT_OVERVIEW.md` (the new crate).
- API docs: `liquers-ui/src/lib.rs` and each of its modules; the scaffolded frontend crates'
  `lib.rs`.
- User guides: none in D1. The UI chapters are rewritten at M2 and M4 (plan §6).

## Open Questions

All four were answered by Orest on 2026-11-23 ("I agree with recommendations"):

1. **Name.** `liquers-ui` for the core and `liquers-ui-<frontend>` for the frontends.
2. **Value type.** The core is generic over `E: Environment`, with display through D2. The
   master design narrows this to the engine side; views and frontends are not generic.
3. **Element as value.** The `ExtValue` variant stays, so core is unchanged. The master design
   makes its content an `ElementSpec` (data).
4. **The spikes.** Phase 2 waits for spikes S1 and S5.

The architecture this design is built on is the
[master design](../ui-redesign/master-design.md), proposed 2026-11-23 and awaiting approval.

## Design Dependencies

- **Builds on:** `ui-split-elements` (model/view pair, messages, several subscriptions) and
  `ui-redesign` (plan decisions 1-3).
- **Absorbs:** `ui-asset-snapshot-boundary` (the runner owns subscriptions; no per-element
  tasks), reusing its AC-5..AC-7 rules.
- **Develops with:** D2 `ui-value-projection` (Phase 2 together), D3 `ui-protocol-and-transport`
  (its messages are the model/view messages of this design).
- **Feeds:** D6, D7 and D8 (frontends), D10, D11, D21.

## References

The issues in `DESIGN.md`, plus `WEBUI-SHORTCUTS-RUN-IN-EVERY-MOUNT` and `WEB-RELEASE-MODULE-IS-20-MB`;
`specs/reference/UI_INTERFACE_FSD.md`.
