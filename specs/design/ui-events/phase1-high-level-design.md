# Phase 1: High-Level Design - ui-events

*Split out of `specs/design/webui-fixes/` (complete), which kept rendering and invalidation. This
feature owns the interaction half: user input reaching a widget. Resumed 2026-10-19 under
`specs/guides/autonomous_bulk_design.md` §11: trimmed, scenarios added, questions tiered.*

## Feature Name

ui-events: how user interaction reaches a `UIElement`, in any backend.

## Purpose

`UIElement::update` (`liquers-lib/src/ui/element.rs`) is the only mutation entry point into an
element, and every `UpdateMessage` variant (`AssetNotification`, `AssetUpdate`, `Timer`,
`Custom`) travels engine to element. **There is no user-to-element path.** egui does not need one,
because `show_in_egui` renders and handles input in one call; the retained web backend does. This
feature defines it: a shared event vocabulary delivered through `update()`, declared in markup, with
native handling by the element and query handling (`UiAction`) where the action should be data.

## Problem Example

In `liquers-lib/examples-web/ui_query_console_app` the menu item `New Console`
(`hello/q/ns-lui/query_console/add-child`, shortcut `Ctrl+N`) opens a console showing `hello`.

| Step | Today | Expected |
|---|---|---|
| Type `about`, press Enter | nothing happens (W1) | the `about` result is shown |
| Type `about`, click Go | `about` result shown, then the input reverts to `hello`; history and volatile refresh keep `hello` (W2) | input, history and refresh use `about` |
| Press the menu shortcut | nothing (W5) | a new console is added, as in egui (keys the browser reserves, such as Chrome's Ctrl+N, excepted) |

W2 is the sharpest case: the driver reads the typed text and sends it to the *command system*
(`UiAction::Apply` to `AppMessage::ApplyToInput` to `lui/submit` to `RequestAssetUpdates`),
bypassing the element that owns the field. The Playwright "known gaps" cases in
`liquers-lib/examples-web/tests/ui_query_console_app.spec.ts` assert today's W1 and W2 behaviour.

## Scope and Acceptance Criteria

- **AC-1** Enter submits the console
  WHEN the user types a query into the browser console input and presses Enter
  THEN the console evaluates that query and shows its result, as clicking Go does
- **AC-2** Submitted query owned by the element
  WHEN a query is submitted from the browser console (Enter or Go)
  THEN the element's `query_text` and history end with it, and the re-rendered input shows it
- **AC-3** Refresh uses the submitted query
  WHEN the submitted query's asset is volatile or expires
  THEN the refresh requests the submitted query, not the previous one
- **AC-4** Clicking into a field is not an activation
  WHEN the user clicks inside an editable field that declares an event
  THEN no event is delivered and no action runs
- **AC-5** Menu shortcuts in the browser
  WHEN a key press matches a `shortcut` declared in a rendered `UISpec` menu
  THEN that item's `UiAction` runs once and the key's default is suppressed
- **AC-6** Native keys left alone
  WHEN a key press matches no declared field action and no shortcut, is Tab, is part of IME
  composition, or is typed into an editable field without Ctrl/Cmd or Alt
  THEN nothing is dispatched and the browser default is not prevented
- **AC-7** Vocabulary is backend-neutral
  WHEN a `UIEvent` is sent as `AppMessage::UIEvent` on a native `AppRunner`
  THEN the target element receives `UpdateMessage::UI` and its repaint verdict is recorded
- **AC-8** egui and web share the submit logic
  WHEN the console's egui Enter or Go fires
  THEN it calls the same `handle_ui_event` with `UIEventKind::Submit` that `update()` calls for the
  web, and `query_text`, history and the requested query match the web outcome

Non-goals: routing egui input through the message channel (egui calls the element's handler
inline, see below); event kinds beyond `Submit` and `Named` (`Change`, `Drop`, focus, pointer:
`UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS`); per-keystroke field sync;
the imperative per-element mount hook (canvas, drag-and-drop, `scroll`); propagation between
elements; generalising `UiAction::Apply` to several fields; retiring `lui/submit`; `value-accessor`.

## Principle: platform-independent events

A `UIEvent` describes what the user meant (`Submit`, `Named("reset")`) plus the owning element's
field values as `Value`, never how a device produced it. Each backend's driver translates its own
native input (DOM events, egui responses, a future ratatui/crossterm loop) into a `UIEvent`
addressed by `UIHandle`; no `web_sys`, egui or terminal type appears in it. Handles belong to the
shared `AppState`, so the event reaches its element whichever backend drew it, including nested
UIs (ratatui inside egui inside a web page).

Platform-dependent and therefore outside the event: how interest is declared (`data-lq-*` markup
is the web backend's form; other backends declare in their own rendering), key routing between
nested hosts (which layer sees a key first; a driver rule, designed when nesting is real), and raw
low-level input. Pointer or drag input, if ever needed, becomes a neutral kind with element-local
logical coordinates; `UpdateMessage::Custom` stays the escape hatch for anything truly
backend-specific.

egui shares the handler, not the channel. One `UIElement` implements both `show_in_egui` and
`render_web` when both features are on, and `update()` is backend-neutral. `show_in_egui` takes
`&mut self`, so egui calls the same private `handle_ui_event` inline when its Enter or Go fires
(no round trip, no frame of delay); the web driver reaches it through `AppMessage::UIEvent` and
`update()`. Both backends then run one submit logic. Menu shortcuts are not `UIEvent`s in either
backend: they run the menu item's `UiAction` (egui through `consume_shortcut`, web through the
`data-lq-shortcut` scan).

Naming: `UIEvent` / `UIEventKind`, following `UIElement`, `UIHandle`, `UIContext`, `UIChange`.
"User event" was rejected (Orest, 2026-10-30): it reads as a user-defined event, which here is the
`Named` kind.

## Core Interactions

- **UI** (`liquers-lib/src/ui/`): `UpdateMessage` gains one variant; `AppMessage` and `AppRunner`
  deliver it; the web driver routes `click`/`keydown` by markup; `QueryConsoleElement` and
  `UISpecElement` adopt it.
- **Commands:** none added, removed or changed; `lui/submit` and `UiAction::Apply` stay as the
  query tier.
- Query, Store, Assets, Value types, HTTP: untouched. Field values travel as `Value`.

## Crate Placement

`liquers-lib` only (`ui/` with `cfg(feature = "webui")` and the wasm-only driver). `liquers-core`,
`liquers-py` and `liquers-axum` are untouched.

## Documentation Intent

- Reference: extend `specs/reference/UI_INTERFACE_FSD.md` §Event Handling with the vocabulary,
  the `data-lq-*` markup conventions and keyboard precedence.
- Guide: none; no guide covers UI widgets yet.
- Other documents: rewrite the "known gaps" Playwright cases as desired behaviour.
- Documents to update: the three source issues (closed on completion).

## Design Readiness

- **Readiness:** ready (vocabulary shape decided 2026-10-30: one variant, closed kind enum)
- **Automatic fixing:** not-eligible: adds a public `UpdateMessage` variant, `AppMessage` variant
  and the `UIEvent` / `UIEventKind` types (rules 3 and 4).
- **Leading issue:** **Open design - vocabulary shape:** one `UpdateMessage::UI(UIEvent)`
  variant whose closed `UIEventKind` enum (`Submit`, `Named(String)`) grows by need, versus one
  variant per event kind. Every `UIElement` matches `UpdateMessage` exhaustively, so the choice
  sets what each future event costs. Recommended: one variant with a closed kind enum.
- **Explanation:** Both defects and W5 are confirmed at HEAD and a working, testable solution is
  specified in Phases 2-4 under the recommended answers; only the public vocabulary shape awaits
  confirmation.
- **Open questions:**
  1. **Open design - vocabulary shape:** as the leading issue.
  2. **Proposed resolution - declarations in markup:** `data-lq-field`, `data-lq-event` and
     `data-lq-shortcut` attributes written by `render_web`; no new trait method. SSR-safe and
     nothing to re-register after a re-render.
  3. **Proposed resolution - query tier unchanged:** keep `UiAction::Apply`, `ApplyToInput` and
     `lui/submit` (registry unchanged); the console stops using them. Retiring or generalising them
     is a later decision.
  4. **Proposed resolution - keyboard precedence:** a field's declared Enter action, then menu
     shortcuts (inside an editable field only with Ctrl or Alt), then the browser. Shortcuts the
     browser reserves (Chrome Ctrl+N/T/W) are a documented limit, not worked around.
  5. **Proposed resolution - delivery:** to the owning element only, no propagation; a native
     handler reports errors through its own state, the query tier through the runner as today.

## Design Dependencies

- `webui-fixes` (complete): **requires**, already satisfied. Relies on its `UIChange::Replaced`
  recording and focus/caret restore in `liquers-lib/src/ui/web/app.rs`.
- `keyboard-shortcuts` (complete): **requires**, satisfied. Reuses `KeyboardShortcut` and
  `Key::from_name`.
- `value-accessor` (abandoned): **overlaps** historically. The binding layer beneath this naming layer; not a
  prerequisite. Field values travel as `Value`, so an accessor can slot in later.
- `ui-asset-snapshot-boundary` (in review, written in parallel on 2026-10-19): **overlaps**. Both
  edit `AssetViewElement::update` and `ui/runner.rs` (it: monitoring and polling; here:
  `deliver_snapshot` becomes `deliver_update` plus one match arm). Independent contracts; whichever
  lands second rebases a small textual conflict.
- Issue `UI-QUERY-EDITOR-LACKS-LIVE-VALIDATION` (no design): weak overlap; browser live validation
  will need per-keystroke field events, a non-goal here that the vocabulary leaves room for.

## Consolidated Findings

- All three defects reproduce at HEAD; the Playwright "known gaps" cases pin today's behaviour and
  must be rewritten, not loosened, in the same change.
- Fixing W2 needs a user-to-element message: any fix "inside" `lui/submit` would need downcasting
  `dyn UIElement` or a console-specific `Custom` payload, a private copy of this contract. That is
  why the issues stay here rather than in a separate compact design.
- Adding `UpdateMessage::UI` breaks exhaustive matches in `element.rs`, two widgets, two test
  files and two examples (listed in Phase 4); that is the intended signal.
- Keydown Enter no longer dispatches `data-lq-action`: native `<button>` activation already turns
  Enter into a click, and `.lq-go` becomes a `<button>` so it is keyboard reachable.
- Browser behaviour is proven only by Playwright; native tests cover vocabulary, delivery, element
  handling, markup and the pure keyboard routing function.

## Open Questions

Tiered in **Design Readiness** above.
