# Phase 1: High-Level Design - ui-events

*Split out of `specs/design/webui-fixes/` after review: that feature grew from "three browser bugs" into
"how does user input reach a widget", which deserves its own phased design. `webui-fixes` keeps the
rendering/invalidation half (W3); this feature owns the interaction half (W1, W2, W5).*

## Feature Name

ui-events — how user interaction reaches a `UIElement`, in any backend

## Purpose

The `ui` module has exactly one mutation entry point into an element —
`update(&mut self, &UpdateMessage, &UIContext)` — and every existing `UpdateMessage` variant
(`AssetNotification`, `AssetUpdate`, `Timer`, `Custom`) travels *engine → element*. **There is no
user → element path.** An immediate-mode backend does not need one, because `show_in_egui` is
renderer and input handler in a single call; every retained-mode backend does, because rendering
and input are separated in time and the DOM keeps user-authored state between renders. This feature
defines that path: an event vocabulary, how an element declares what it reacts to, and how a
handler is expressed — natively where that is the right tool, and as a Liquers query where the
action should be data.

## Motivating defects

These are observable consequences of the missing path — this feature's acceptance criteria, not its
scope.

| ID | Issue (`specs/archive/2026-08-08-issues.md`) | Symptom |
|----|---------------------------|---------|
| W1 | WEBUI-QUERY-CONSOLE-ENTER-KEY-SUBMIT | Enter in the query input does nothing; only clicking "Go" works |
| W2 | WEBUI-SUBMIT-QUERY-STATE-NOT-PRESERVED | The submitted query never reaches the element that owns it, so the input reverts and refreshes re-run the stale query |
| W5 | Accelerators are egui-only | `Ctrl+N` from a `UISpec` menu silently does nothing in the browser |

W2 is the sharpest illustration: the web driver reads the typed text from the DOM and sends it to
the *command system* (`ApplyToInput` → `lui/submit`), bypassing the element that owns the field.
That message is the missing path, improvised for one widget.

## Core Interactions

### Query System

Query-based handlers are actions represented as data: a `UiAction` carrying a query, evaluated by
the runner. This feature generalises what already exists for `UISpec` menu buttons so the same
representation can cover other controls and other event kinds.

### Store System

Not involved.

### Command System

`lui` namespace. Whether `lui/submit` survives in its current form is a Phase 2 question; the
mechanism it stands for — "take these live values, apply this query" — is kept and generalised.

### Asset System

Unchanged. Assets remain the authority for data; this feature concerns widget-internal state and
the *triggering* of evaluation, not the asset lifecycle.

### Value Types

No new `ExtValue` variants. Event payloads and field values travel as `Value` (see *Relationship to
`value-accessor`*).

### Web/API

No HTTP surface. The web backend gains event routing (which event types are observed, how a target
resolves to element + field), keyboard handling with HTML-native precedence, and the `data-lq-*`
markup conventions that carry the declarations.

### UI

The `UIElement` trait (event vocabulary, declared interaction surface), `UIContext`/`AppRunner`
(delivery), `shortcuts` (a backend-neutral matcher usable from the web), `UiAction`, and
`QueryConsoleElement`/`UISpecElement` as first adopters.

## Design axes

### 1. Vocabulary: what an element receives

Modelled on the two backends that exist:

- **HTML** — typed events (`click`, `input`, `change`, `keydown`, `focusin`), a target, bubbling
  and delegation, a *default action* per control, and an explicit way to suppress it.
- **egui** — a `Response` per widget with predicates (`clicked()`, `changed()`, `lost_focus()`),
  plus explicit consumption of accelerators.

Candidate variants for Phase 2 (names indicative):

- `UserInput { field, value }` — a declared field's value changed (the "listen and visualise" case).
- `Event { kind, target, payload }` — a general typed event: activate, submit, focus, key, or
  custom-by-name. One general variant versus several specific ones is a Phase 2 decision.
- `Custom(Box<dyn Any + Send>)` stays as the escape hatch for genuinely backend-specific payloads.

Agreed constraints: the vocabulary is *shared* (not backend-private, so a widget behaves the same
in egui and the browser), it is delivered through `update()`, and adding variants is expected to
break exhaustive matches — which this project treats as the signal to decide what a widget does
with a new event kind.

### 2. Declared interaction surface: what an element reacts to

A retained backend must know, before any event arrives, which nodes participate and which fields
exist: a delegated listener has to be told which event *types* to observe, and `focus`/`blur` do
not bubble (`focusin`/`focusout` do), while `scroll` does not bubble at all.

- **Declarative (normal path):** the element's rendered markup names its fields and marks the
  participating nodes, extending today's `data-lq-action` convention. Nothing to register, nothing
  to re-register after a re-render, and it survives server-side rendering because the markup *is*
  the declaration.
- **Imperative (opt-in extension):** a per-element mount/unmount hook in the web backend where a
  widget attaches its own JS listeners — the right tool for canvas interaction, drag-and-drop,
  third-party components, `scroll`. It costs a lifecycle: invisible to SSR, and re-established
  whenever a re-render replaces the node.

### 3. Handlers: native where possible, query-based where it should be data

Two tiers, chosen per interaction by the widget:

- **Native/local handling** — the widget (in the browser, possibly a small JS handler it installs)
  does the work directly: toggling a disclosure, switching a tab, moving a caret, live-filtering a
  list. No engine round trip, lowest latency; the natural choice for anything that only affects
  presentation.
- **Query-based handling** — the action is a `UiAction` carrying a query, evaluated by the runner,
  able to touch `AppState`, assets and the store. It is *data*: serializable, declarable in a
  `UISpec`, inspectable, and usable from HTML/JavaScript in the same places an event handler or
  callback would be. Today only `UISpec` menu buttons use it; the aim is that any control — and
  eventually other event kinds — can carry one.

The framework's job is to make the second tier first-class without forcing it on the first.
Guiding rule for Phase 2: *presentation-only interactions default to native; anything that changes
model state, evaluates, or must be expressible in a spec goes through a query action.*

### 4. Keyboard: accelerators without breaking native behaviour

- **Application accelerators** (`Ctrl+N` from a menu spec) use the shared `KeyboardShortcut`
  matcher, so both backends agree on what a shortcut means. In the browser they are matched at the
  root and consume the key *only* on a match (W5).
- **A field's default action** (Enter in a text input) is not an accelerator: it is the control's
  own behaviour, declared on the field and dispatched from that declaration — the HTML-native
  concept (W1).
- Precedence: a focused editable field with a declared default action wins; otherwise accelerators;
  otherwise the browser keeps its behaviour. Never suppress a key nothing consumed; leave IME
  composition, Tab traversal and browser shortcuts alone.

### 5. Timing and payload shape

- Field values reach the model **on dispatch**: the action carries the live values of the fields it
  declares. Per-keystroke syncing stays available as a per-field opt-in, so it is a tweak rather
  than a redesign.
- Two shapes, one mechanism: *value only* (the element updates itself, nothing runs) and *values +
  action* (the action declares which fields it collects). Bundling values with the action removes
  the hazard of a value arriving after the action that needed it. `ApplyToInput` is this shape,
  hardcoded to a single field.

## Relationship to `value-accessor`

`specs/value-accessor` (design only — there is no `liquers-core/src/accessor.rs`) gives values a
uniform read/write handle, explicitly motivated by two-way binding for widgets. It is the natural
*binding* layer beneath this feature's *naming* layer: a declared field says "element *e* has an
editable field named *f*"; an accessor says what *f* reads and writes — a struct field today, a
store key, a query parameter or an asset later.

**Accessors are not required to fix W1, W2 or W5.** The console's `query_text` is a plain struct
field; what is missing is delivery, not binding. So `value-accessor` need not be prioritised for
this feature. Two cheap rules keep the door open:

1. Field values travel as `Value` (a text value in practice), so a future `set(Value)` slots in
   without changing the vocabulary or the markup.
2. Writes that may become async are dispatched as messages and performed by the runner, so
   `update()` stays synchronous — which is how elements already delegate async work.

Accessors become worth prioritising when a *generic* widget must edit something it does not own —
an editor bound to a store key, a form bound to query parameters. That is a good follow-up feature,
not a prerequisite.

## Crate Placement

`liquers-lib/src/ui/` — trait and vocabulary (`element.rs`), delivery (`runner.rs`,
`ui_context.rs`), web routing and markup conventions (`web/`), keyboard matching (`shortcuts.rs`),
and the first adopting widgets (`widgets/`). `liquers-core` is untouched unless accessors are
adopted, keeping `liquers-py` and `liquers-axum` out of the blast radius.

## Open Questions

1. **One general `Event` variant or several specific ones?** A single typed event with a `kind` is
   extensible without touching the enum; specific variants make exhaustive matches meaningful.
2. **Where does the field/interest declaration live?** A trait method returning declarations that
   every backend consumes, versus `render_web` emitting attributes plus a setter method.
3. **Does egui adopt the vocabulary in this feature, or keep handling input inline?** Not needed
   for W1/W2/W5; deferring means two ways of writing the same field for a while.
4. **How far is the native/JS handler tier specified now?** Naming the extension point costs
   nothing; a real mount/unmount lifecycle is its own effort, and nothing in scope needs it.
5. **Do events need propagation semantics** — bubbling from a child element to its parent element
   and a way to stop it — or is delivery to the target element enough for now?
6. **Does a query-based handler need its own error channel**, or does a failing handler report
   through the element that triggered it?

## References

- `specs/design/webui-fixes/` — the rendering/invalidation half (W3) and the review discussion this
  feature was split out of
- `specs/archive/2026-08-08-issues.md` — W1, W2, W5 records
- `specs/design/value-accessor/phase1-high-level-design.md` — the binding layer this composes with
- `specs/reference/UI_INTERFACE_FSD.md`, `specs/archive/2026-03-02-ui-web-design-notes.md`, `specs/archive/2026-03-02-ui-ratatui-design-notes.md`
- `liquers-lib/src/ui/{element,action,runner,shortcuts}.rs`, `liquers-lib/src/ui/web/app.rs`
