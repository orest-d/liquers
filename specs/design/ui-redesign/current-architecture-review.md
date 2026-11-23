---
id: UI-REDESIGN-CURRENT-ARCHITECTURE-REVIEW
kind: analysis
title: UI redesign — review of the current UI architecture, its designs and its issues
workflow: liquers-project
status: draft
area: [lib/ui, lib/egui, web, axum]
created: 2026-11-23
---
# Review of the current UI architecture

Input to the [master design](./master-design.md). Written 2026-11-23, true at `d0790d3`. It asks
one question: are the problems of the current UI local defects, or do they come from the
architecture? Answer in §4: most of the open issues trace back to three separations the
architecture never made.

## 1. What was reviewed

- **Code:** `liquers-lib/src/ui/` (element, app state, runner, messages, events, context, payload,
  commands, actions, shortcuts, native driver, the editor, the web renderer, the widgets) and
  `liquers-lib/src/egui/` (window, widgets, editor), about 24 000 lines; how `liquers-py` and
  `liquers-web` use them; the applications `orest-d/stockplottertest` and `orest-d/egui-midi-test`.
- **Designs, complete:** `webui`, `webui-fixes`, `ui-events`, `menu-pane-layout`,
  `keyboard-shortcuts`, `query-console-element`, `egui-native-app`, `query-pipeline-editor`,
  `query-editor-ergonomics`, `web-ui-use`, `web-ui-define`, `liquers-py-ui`,
  `liquers-py-ui-window`, `settable-value-accessor`, `ui-split-elements` (investigation); their
  Phase 5 "Important Learning" sections in particular.
- **Designs, in review:** `ui-asset-snapshot-boundary`, `ui-display-conversion`,
  `text-editor-save`, `template-editor-ui`, `template-syntax-highlighter`,
  `query-editor-live-validation`, `web-query-editor-live-preview`, `console-mounts-nested-element`.
- **Issues:** the 44 open ones of the [idea map](./idea-map.md) and the closed UI issues
  (`EGUI-HAS-NO-REUSABLE-NATIVE-APP`, `WEB-CANNOT-USE-A-LIQUERS-UI`,
  `WEB-CANNOT-DEFINE-UI-ELEMENTS`, `PY-CANNOT-USE-A-LIQUERS-UI`, the `WEBUI-*` event issues,
  `NO-COMMAND-PARAMETER-EDITOR`, the icon and theme issues).
- **Archive:** the March 2026 UI documents (`2026-03-02-ui-interface-phase1*.md`,
  `-ui-payload-design.md`, `-ui-web-`, `-ui-ratatui-`, `-ui-dioxus-design-notes.md`), the
  2026-07-07 project review, `2026-11-19-egui-craft-suite-survey.md`,
  `2026-11-22-ui-transport-spike.md`.
- **References:** `UI_INTERFACE_FSD.md`, `THEME.md`, `VALUE_ACCESSORS.md`, `VISION.md`.

## 2. The current architecture in one page

- **Element.** `UIElement` (`ui/element.rs`) is one typetag-serialised trait object holding
  state, behaviour (`init`, `update`) and the rendering of every toolkit as feature-gated methods
  (`show_in_egui`, `render_web`, `show_in_web`).
- **Tree.** `AppState` (`ui/app_state.rs`) is a tree of nodes, each with a handle, a parent,
  children, an `ElementSource` (none, query or recipe) and an optional element. It lives in an
  `Arc<tokio::sync::Mutex<dyn AppState>>` that the runner, the renderers, the commands and the
  hosts all hold.
- **Runner.** `AppRunner::run` (`ui/runner.rs`) is called once per frame or timer tick. It has
  seven phases: drain `AppMessage`s, start evaluations of pending nodes, poll finished
  evaluations, poll monitored assets, poll accessors, poll the theme, poll icons.
- **Messages.** Elements send `AppMessage` to the runner; the runner answers through
  `UIElement::update(UpdateMessage)`. User input is a `UIEvent` (`Submit`, `Named`, `Change`).
  Status goes to hosts as `RunnerEvent`. Tree changes are recorded as `UIChange` in an
  `Invalidation`.
- **Commands.** The `lui` commands reach the tree through the payload (`UIPayload` →
  `UIContext` → the mutex) and mutate it as a side effect of evaluation.
- **Hosts.** egui: `run_window` calls `block_on(driver.tick())` in every frame, then
  `render_element` locks the state, takes the element out, calls `show_in_egui(&mut self)` and
  puts it back. Browser: a loop runs the runner every 16 ms, takes the invalidation and replaces
  the HTML of each changed element; delegated DOM listeners read `data-lq-*` attributes and send
  `UIEvent`s.

## 3. Fundamental problems

Each has its evidence in the code and the issues it produces. They are ordered by how much else
depends on them.

**F1. One object is model, controller and view for every toolkit.**
`UIElement` carries `show_in_egui`, `render_web` and `show_in_web` beside its state. Adding a
toolkit means adding a method to the trait and a feature to `liquers-lib` (the March ratatui notes
propose exactly `show_in_ratatui`). Every element must be written once per toolkit, and a client
cannot render an element without linking its type, and with it polars and the image codecs.
*Issues:* `UI-NOT-SEPARATED-BY-CRATE-AND-TECHNIQUE`, `WEB-RELEASE-MODULE-IS-20-MB`, no TUI.

**F2. Behaviour lives in the renderers, and the two renderers have different event paths.**
In egui an element mutates itself inside `show_in_egui(&mut self)` while it draws
(`QueryConsoleElement::show_in_egui` applies commands to its own fields). In the browser the
driver scrapes the values of `data-lq-field` nodes from the DOM and sends a `UIEvent` to `update`.
The same element therefore has two implementations of its behaviour that can disagree
(`egui/editor.rs` is 1 772 lines, `ui/web/editor.rs` 970). `UIEvent` has three kinds and is shaped
like an HTML form. *Issues:* `UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS`,
`WEB-EDITOR-HAS-NO-KEYBOARD-MODEL`, `WEB-QUERY-EDITOR-NOT-LIVE`, the closed `WEBUI-*` issues.
The one place that avoided this is the editor (`ui/editor/`): a backend-free `EditorState` that
applies `EditorCommand`s, returns `EditorEffect`s and builds an `EditorTree` which both renderers
only draw. Its Phase 5 says so: "one layout, two renderers ... made egui and the browser agree by
construction and made the layout testable without a renderer".

**F3. The tree is shared mutable state behind a lock that everybody holds.**
The mutex is reached by the renderer (`try_sync_lock`; when it is busy the egui frame draws
"Loading..." and the browser drops the tick's invalidation until later), the runner, commands
running inside evaluations, hosts and Python threads. To call an element without holding the lock
the code uses extract-update-replace (`take_element` / `put_element`), so an element is absent
from the tree while it is updated or drawn. The mutation contract has two clauses "the compiler
cannot enforce" (`put_element` versus `set_element`; `get_element_mut` records a change eagerly).
*Lessons recorded:* "a callback that calls back into the mount deadlocks" (`web-ui-use`), "any
blocking wait for a UI from inside a command is a deadlock risk" (`liquers-py-ui`), "a definition
made while the UI lock is held cannot mark the tree at once" (`web-ui-define`), "clone before
writing back" (`query-pipeline-editor`).

**F4. The lifecycle is implicit and has several entry paths.**
An element comes to exist in at least four ways: (a) a pending node's source is evaluated and the
runner mounts the result; (b) a submitted query is evaluated and a command mutates the tree
through the payload (the runner comments "do NOT set element from result — commands did it");
(c) a host edits the `AppState` directly; (d) an element nested in a console is rendered without
ever being initialised and has no handle. `init` is called by whoever inserted the element
("the caller is responsible"). There is no unmount: the runner finds dead subscriptions by
polling `node_exists`, and `AssetViewElement::from_asset_ref` spawns a task that lives as long as
the asset. *Issues:* `UI-ELEMENT-NESTED-IN-CONSOLE-HAS-NO-HANDLE`,
`UI-ELEMENT-KEEPS-AN-EXPIRED-VALUE-AND-NEVER-RE-REQUESTS`, `EGUI-ASSET-MANAGER-INTEGRATION`,
`WEB-ONEVENT-SHOW-GUARD-IGNORES-THE-MOUNT`.

**F5. There is no single event model, and the two message enums are closed.**
Seven vocabularies carry events: `AppMessage` (ten variants), `UpdateMessage` (seven),
`UIEvent`, `AccessorUpdate`, asset notifications, `RunnerEvent` and `UIChange`. Each new
capability added a variant to both closed enums and a phase to the runner: accessors
(`Accessor` / `Accessor`, phase 5), the file selector (`ListDirectory` / `Directory`), icons
(`LoadIcon`, phase 7), the theme (phase 6). An element in another crate cannot ask for anything
the runner does not already know, and nobody can see, in one place, what happened to an element
and in what order.

**F6. The loop polls, and it waits for evaluations.**
`run` is driven by a timer (16 ms in the browser, every frame plus a 250 ms idle poll in egui)
and polls every watcher on every pass; nothing wakes it. `SubmitQuery` and `ApplyToInput` call
`evaluate_immediately` and await the whole evaluation, and on wasm `evaluate` itself runs inline,
so one slow query stops every element. A subscription map keyed by handle allows one asset per
element. *Issues:* `WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`,
`PY-UI-CHANGE-DOES-NOT-WAKE-AN-OPEN-WINDOW`, `EGUI-EXAMPLES-HAND-WRITE-THE-FRAME-LOOP`.

**F7. Nothing describes a rendered element as data.**
egui gets a `&mut` element and a `Ui`; the browser gets an HTML string whose behaviour is encoded
in `data-lq-*` attributes, and replaces the element's whole subtree whenever it reports
`NeedsRepaint` (which loses focus and caret, hence special cases). There is no representation of
"what this element shows" that could be compared, diffed, tested or sent to another process.
*Issues:* `WEB-UI-CANNOT-BE-DRIVEN-BY-A-REMOTE-ENGINE`, `AXUM-CANNOT-SERVE-A-LIQUERS-UI`,
`NO-RENDERER-NEUTRAL-STYLESHEET`.

**F8. Elements are code, not data.**
An element is a typetag trait object. 24 fields are `#[serde(skip)]`, among them the value an
asset view shows, so saving a UI silently drops data. A language integration cannot add a typetag
type at run time, hence `DelegatedElement` (a kind, JSON props and behaviour registered per
thread). `ExtValue::UIElement` holds a live `Arc<dyn UIElement>` that is cloned on mounting.
Such a value cannot be validated, cached meaningfully or sent to a client.

**F9. The UI is typed to `liquers-lib`'s `Value`.**
Every element, message and snapshot uses `liquers_lib::value::Value`, and views match on
`ExtValue` variants. That is why the UI cannot leave `liquers-lib`, and why a view shows a value
only by its own variant. *Issues:* `UI-WIDGET-DOES-NOT-CONVERT-ITS-VALUE`,
`ACCESSOR-CANNOT-BE-SENT-TO-A-CLIENT`.

**F10. No object owns a running UI.**
What makes "one UI" is spread over an `AppState`, a runner, a channel, a `UIContext` (which grew a
registry, a theme cell and an icon cache) and host bookkeeping. Focus, shortcuts and the
"element being updated" guard are therefore global or per handle number rather than per UI.
*Issues:* `WEBUI-SHORTCUTS-RUN-IN-EVERY-MOUNT`, `WEB-ONEVENT-SHOW-GUARD-IGNORES-THE-MOUNT`; no
place to attach a session.

**F11. The reference describes an API that was never built**
(`UI-INTERFACE-FSD-DESCRIBES-UNBUILT-API`), so the lifecycle and the message order are known only
from the code and its tests.

## 4. Root causes

The eleven problems come from three separations that were never made:

1. **State from presentation** (F1, F2, F7, F8, F9). The element is both what the UI *is* and how
   it is *drawn*. Separate them and the frontends become independent of the elements.
2. **Deciding from doing** (F4, F5, F6). Elements and commands *perform* their effects (lock the
   tree, spawn tasks, await evaluations) instead of *describing* them to one runtime that performs
   them in a known order.
3. **One owner from shared access** (F3, F10). The tree has no single owner, so correctness
   depends on a locking discipline spread over every caller.

None of this is a criticism of the individual designs: each solved its problem inside the frame
it was given, and several of them (`webui-fixes`, `ui-asset-snapshot-boundary`,
`query-pipeline-editor`) are steps towards these separations.

## 5. What is sound and is kept

| Kept | From | Becomes |
|---|---|---|
| Handles, the tree, a source per node: "every view is a query" | `UI_INTERFACE_FSD`, VISION | The document tree |
| The editor's `State` + `Command` + `Effect` + `Tree` | `query-pipeline-editor`, `query-editor-ergonomics` | The pattern of every element |
| `UIChange` / `Invalidation` records | `webui-fixes` | View patches |
| The runner owns subscriptions and pushes `AssetSnapshot`s | `ui-asset-snapshot-boundary`, the console | Subscriptions as effects |
| Stale results are dropped; the settled-tick rule | `web-ui-use`, `egui-native-app` | Turn and revision rules |
| An outbox drained after the update, with no lock held | `web-ui-use` | The session's update stream |
| Elements as data plus registered behaviour | `web-ui-define` (`DelegatedElement`) | Every element's saved form |
| `UiAction`, menus, shortcuts, layouts | `menu-pane-layout`, `keyboard-shortcuts` | View nodes and the keymap |
| The theme token model, layers, icons, `ui:` section | `query-editor-ergonomics`, `THEME.md` | Unchanged, moved to the core |
| Value accessors with `set_if` and events | `settable-value-accessor` | Bindings |
| Model/view halves, revisions, resync, session ≠ connection | `ui-split-elements` | The protocol |
| One connection shape for every transport | spike S2 | The transport layer |

## 6. Lessons from the earlier designs

| Design | Lesson that shapes the master design |
|---|---|
| `2026-03-02` web notes | "Elements manage their own DOM subtrees" kept DOM handles inside elements; the later `webui` design replaced it with HTML strings. Both tie the element to the DOM. |
| `webui-fixes` | Retained rendering needs change records produced by the model, not by the renderer. |
| `ui-events` | Input must reach the element through `update`, not through the renderer; but only the browser followed it. |
| `web-ui-define` | Behaviour cannot be serialised; data can. A DOM lifecycle belongs to the driver, keyed by node identity. |
| `web-ui-use` | Deliveries to the host happen after the update with nothing locked. On wasm a node replaced during an evaluation is the normal case. |
| `liquers-py-ui*` | Never wait for the UI from inside an evaluation. Other threads need a wake-up, not a poll. |
| `egui-native-app` | A host needs a race-free definition of "idle". |
| `query-pipeline-editor` | Edits are validated commands; a refused edit changes nothing. One write in flight; a change elsewhere is a reload or a conflict. |
| `query-editor-ergonomics` | One backend-free layout, two renderers that only draw. Keymaps need scopes and an order. |
| `ui-split-elements` | The server is authoritative; the view asks for what it can show (a page, not the DataFrame). |
| spike S2 | Flow control, ping, coalescing per tick and large values by reference belong to the protocol. |
