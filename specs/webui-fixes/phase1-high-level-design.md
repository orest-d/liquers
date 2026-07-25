# Phase 1: High-Level Design - webui-fixes

*Revised after the first Phase 1 review. Longer than the template's 30-line guideline by request:
the review asked for a per-issue demonstrating flow and an explicit answer to "does the `ui` module
design need to change to support retained-mode backends?".*

## Feature Name

webui-fixes — retained-mode support in the `ui` module: closing the interaction loop between the
backend and `AppState`

## Purpose

Three open `webui` issues make the browser backend behave differently from the egui reference
backend: Enter does not submit in the query console, the console forgets the query the user
submitted, and a state change with no pending async work leaves the page stale. They are not three
bugs — they are the three sides of one gap: the `ui` module was designed against an immediate-mode
backend and has no generic path for **user input to reach an element**, nor for **"the model
changed" to reach a backend**. The feature adds those paths, so the same widget behaves identically
in an immediate-mode and a retained-mode backend, and resolves W1–W3 as consequences rather than as
special cases. A fourth issue (the wasm async engine) is already fixed and only needs its record
closed.

## Scope

| ID | Issue (`specs/ISSUES.md`) | One-line symptom |
|----|---------------------------|------------------|
| W1 | WEBUI-QUERY-CONSOLE-ENTER-KEY-SUBMIT | Pressing Enter in the query input does nothing |
| W2 | WEBUI-SUBMIT-QUERY-STATE-NOT-PRESERVED | The input reverts to the previous query after submitting |
| W3 | WEBUI-REPAINT-AFTER-SYNC-MUTATION | A purely synchronous change does not update the page |
| W4 | webui async engine does not run on wasm | Already resolved by `async-wasm-refactor` — close the record |

Found while analysing W1 and pulled into scope by review decision 3 (*"shortcuts should be
supported"*):

| ID | Finding | Symptom |
|----|---------|---------|
| W5 | Menu accelerators are egui-only | `Ctrl+N` declared in a `UISpec` menu does nothing in the browser: the shortcut registry and its matching live inside `show_in_egui`, and the web backend never consults them |

## Demonstrating flows

### W1 — Enter does nothing

A page shows a query console: a toolbar with a text input, a "Go" control and a status chip, and
a content pane below.

The user clicks into the input — the caret appears, as expected. They type a query and press
Enter. Nothing happens at all: no evaluation, no error, no visible reaction. They press Enter
again, then click "Go" instead — and the very same text evaluates correctly and the result
appears.

What happens underneath: the browser fires the key event on the `<input>`. The single delegated
listener takes the event's target and walks *up* the DOM looking for the nearest ancestor carrying
a `data-lq-action` attribute. The input has none, and neither does the toolbar that contains it —
the only marked node in that toolbar is the "Go" control, which is the input's *sibling*, not its
ancestor. The walk finds nothing and the handler returns. The user's intent was real and arrived
in the browser; the backend had no way to recognise it as an action.

The same widget in the egui backend does submit on Enter, so the two backends disagree about what
one widget does.

### W2 — the input forgets what was submitted

The console currently shows a query and the result of evaluating it. The user selects the input,
replaces the text with a different query, and clicks "Go".

The content pane updates correctly — the new query really was evaluated. But the input snaps back
to the *previous* query. Three things follow:

1. The visible query no longer describes the visible result, which is exactly the thing a query
   console exists to show.
2. The back/forward history has no record of the new query, so navigating history walks a list
   that is missing the user's last step.
3. Worst: if the result is volatile or expires, the console refreshes itself by re-submitting the
   query it believes it is showing — the stale one. The pane silently reverts to the earlier
   result, with no user action at all.

What happens underneath: the typed text is read from the live DOM by the driver, sent as a message,
consumed by the `lui/submit` command, and turned into a request to evaluate and monitor that query.
Every step handles the text correctly — and none of them writes it back into the console element's
own state. The element still holds the old query. Because the page is re-rendered from element
state, the next render regenerates the input from that old value and overwrites what the user
typed.

A sharper variant of the same flow needs no submit at all: the user is halfway through typing a
long query when a monitored asset publishes an update. The page re-renders, and the half-typed text
is gone.

### W3 — the page does not react to a synchronous change

The app's menu has an entry that only rearranges the UI tree — close a panel, or make a panel
active. The user clicks it. The panel stays on screen. Some seconds later, apparently at random,
it disappears — right after some unrelated part of the app receives an update.

What happens underneath: the click is dispatched, the runner processes the message, the command
removes the node from `AppState`. The render loop then asks a single question before re-rendering:
"is there work in flight?" — that is, are there evaluations running or monitored assets that might
publish later. For a change that has already completed there is nothing in flight, so the loop
skips the re-render, and the DOM keeps showing state that no longer exists. It corrects itself
only when some *other* activity happens to make that question true again.

This is not exotic: in the browser the engine evaluates inline (`ImmediateAssetManager`), so even
query-driven mutations typically finish before the loop asks the question.

### W5 — accelerators are silently egui-only

A `UISpec` menu declares *New Console* with `Ctrl+N`. In the native app the accelerator works and
the label documents it. The same spec in the browser renders the same menu, and `Ctrl+N` does
nothing — it reaches the browser's own "new window" instead. Nothing warns the author; the YAML
looks honoured because the menu entry is there.

Underneath: the shortcut registry is built in `UISpecElement::init`, but it is only *matched*
inside `show_in_egui` via egui's `consume_shortcut`. The shared `KeyboardShortcut` type has an
egui conversion and no web counterpart, and the web driver looks only for Enter.

### W4 — for the record

Previously, loading the page aborted immediately: the engine tried to spawn async work on a
runtime that does not exist in the browser, and nothing rendered at all. The `async-wasm-refactor`
work fixed this (inline asset manager, browser-compatible task handling) and proved it with a
browser test that clicks through the demo. Only the issue entry is stale.

## Common denominator

The web backend renders **the whole tree from `AppState` and replaces the page content wholesale**
(`render_web` is a pure function of the model, shared by SSR and the browser). That is a good
design — but it is only correct if the loop around it closes in three directions, and today none of
them does:

| Direction | What the design requires | Violation |
|---|---|---|
| Intent **in** | Every gesture that should act must be reachable as an action from the event target | W1, W5 |
| Value **in** | Every user-editable value must reach the model, or the next render destroys it | W2 |
| Change **out** | Every model change must reach the backend, because nothing else updates the DOM | W3 |

The root cause is a porting artifact. The widgets were written for **egui, an immediate-mode
backend**, where all three directions are satisfied for free: `show_in_egui(&mut self, …)` runs
every frame and *is* both the renderer and the input handler, so the text box writes straight into
`query_text`, the Enter key is examined inline in the same call, and the frame redraws regardless
of whether anything changed. A **retained-mode backend** separates those: rendering is a pure
`&self` function producing markup, input arrives later as events aimed at DOM nodes, and the DOM
keeps user-authored state (field values, caret, selection, scroll, focus) between renders that a
wholesale re-render destroys. The module never grew the pieces that separation requires.

## Does the `ui` module design need to change?

**Yes — and the three issues are the evidence.** Each can be patched locally (that is what the
superseded Phase 2–4 drafts do), but every patch is a web-specific special case bolted onto a
generic module, and none of them would carry over to a second retained-mode backend. Concretely,
today the module offers a backend exactly one mutation entry point into an element —
`update(&mut self, &UpdateMessage, &UIContext)` — and every `UpdateMessage` variant
(`AssetNotification`, `AssetUpdate`, `Timer`, `Custom`) travels *engine → element*. **There is no
user → element path at all.** egui does not need one; every retained backend does. The current
`ApplyToInput` message is precisely the missing path improvised in the web driver: it carries the
user's text from the DOM to the *command system*, bypassing the element that owns it — which is
W2 in one sentence.

Three capabilities are missing, all backend-neutral:

1. **Inbound interaction.** A backend must be able to deliver "the user changed field *f* of
   element *e* to value *v*" and "the user triggered *e*'s field default action" into the element,
   so the element's own state stays true in any backend. (In immediate mode the widget does this
   itself while drawing; the same field ends up written either way.)
2. **A declared interaction surface.** An element must be able to say which of its state is
   user-editable: a stable *name*, its current value, and what the field's default action is.
   A retained backend uses that declaration to emit stable ids, route values back, and know what
   Enter means *in that field*; an immediate-mode backend uses the same declaration as the thing it
   draws. Without it, ids like `qc-input-{handle}` are private conventions between one widget and
   one driver. What the name is *bound to* is a separate layer — see "Declared fields and
   value-accessor" below.
3. **Invalidation.** A backend must learn that the model changed, ideally *which part* changed.
   Immediate mode may ignore this (it redraws anyway) or use it to skip work; retained mode cannot
   function without it.

What deliberately does **not** change: `render_web(&self, …) -> String` stays pure (SSR depends on
it), `AppState` stays the non-generic tree, `AssetManager` and the asset lifecycle are untouched,
`liquers-core` is untouched, and the egui backend keeps behaving exactly as it does today — it may
adopt the declared-field path later, but it is not required to.

## Decisions from the Phase 1 review — round 1

1. **Clean design over point fixes.** The three capabilities above are the deliverable; W1–W3 are
   its acceptance criteria, not its scope.
2. **Authority is layered.** Assets in the `AssetManager` are the authoritative source of truth for
   *data*. For *widget-internal* state the model is authoritative for what the element declares as
   editable (so it survives re-render and serialization in every backend); everything else —
   caret, selection, scroll, hover, backend-internal ids — belongs to the backend and is not
   modelled. Rule of thumb: **the model owns what the user entered, the backend owns how it is
   being shown.** Divergence between backends is allowed where a backend can do better, but each
   divergence must be a deliberate, documented choice.
3. **Shortcuts are supported, and they do not replace native HTML behaviour.** See the discussion
   below.
4. **Invalidation is per element, with a dirty flag as fallback.** See the discussion below.
5. **The browser example is `examples-web/ui_query_console_app`** — a web counterpart of the
   existing native `liquers-lib/examples/ui_query_console_app.rs`, not a console bolted onto
   `ui_spec_demo`. The two examples then differ only by backend, which is itself a test of the
   design: the same spec and the same widgets, driven two ways.
6. **No diff/patch.** Rendering stays "produce the element's markup and install it". Per-element
   invalidation already gives the granularity that matters, and comparison-based skipping is a
   later optimization.

### On decision 3: what "route through the shortcuts module" should mean

There are three readings, and only one is right:

- **(a) Shared matching, application accelerators only.** The web driver listens for `keydown` at
  the root, builds the shared `KeyboardShortcut` value from the event, and asks the same registry
  the egui backend uses (built from the `UISpec` menu). A key is consumed — and the browser's
  default suppressed — *only* if it matches a registered accelerator. Everything else falls through
  untouched. This is what W5 needs, and it makes `Ctrl+N` mean the same thing in both backends
  because both consult one registry and one matcher.
- **(b) All keys become shortcuts,** including Enter/Tab/Escape inside fields. Rejected: it breaks
  native text editing (Enter in a multi-line field, IME composition, Tab focus traversal, browser
  shortcuts, assistive technology), and makes a text field's basic behaviour depend on a global
  registry that has nothing to do with it.
- **(c) Hybrid — the recommendation.** Accelerators exactly as in (a). Enter-in-a-field is *not* an
  accelerator: it is the **field's default action**, an HTML-native concept (a form's implicit
  submission), declared by the element on the field itself and dispatched from that declaration.
  Precedence: focus in an editable field with a declared default action wins; otherwise consult the
  accelerator registry; otherwise let the browser do its thing.

Guard rails that follow from (c): never suppress a key's default unless something actually consumed
it; ignore key events during IME composition; leave Tab/Shift-Tab to the browser; a field with no
declared action must let Enter behave natively. The equivalent split already exists in egui —
accelerators go through `consume_shortcut`, while the text box handles Enter itself — so the
hybrid *aligns* the backends rather than adding a web special case.

### On decision 4: invalidation that does not steal focus

A single global dirty flag fixes W3 but re-renders the whole page for any change, which throws away
focus, caret and scroll everywhere — trading a stale-DOM bug for a typing bug. The review's point
is the way out: **state modifications are already element-scoped** (`lui/add`, `lui/remove`,
`activate`, an element updating itself from a snapshot all name a handle), and every element
already renders into a container with a stable id. So invalidation should be a **set of dirty
handles**, and the backend's response is the DOM operation that corresponds to the state change:
re-render that element's markup into its own container, or drop its node. Elements the user is
typing in are simply not touched.

That leaves three cases, in order of preference:

1. **Targeted:** the change names a handle → re-render that subtree only.
2. **Fallback:** something changed that cannot be attributed to a handle (e.g. the root set) → a
   whole-tree re-render, with focus and caret saved and restored around it. This is where the plain
   dirty flag lives.
3. **Optimization (later):** compare freshly rendered markup with what is installed and skip
   identical work.

Restoring focus/caret is well-defined precisely because of capability 2: the field has a stable
name and the model holds its value, so "focus field *f* of element *e*, caret at offset *n*" is
expressible. No diff/patch is required for any of this — decision 6 holds.

## Decisions from the Phase 1 review — round 2

### `update()` is the inbound path; the open choice is the vocabulary

Nothing is wrong with `update(&mut self, &UpdateMessage, &UIContext) -> UpdateResponse` as the
inbound entry point: it takes `&mut self`, it gets a context it can send messages through, and it
returns a repaint verdict. The earlier framing was misleading — the missing piece is not a method
but a **shared vocabulary**: every existing variant means "the engine has news for you", and none
means "the user did something to you".

Three ways to add it:

| Option | Cost | Consequence |
|---|---|---|
| `UpdateMessage::Custom(Box<dyn Any + Send>)` — exists today | none | Each backend invents its own payload type and every widget downcasts; interaction becomes backend-private, which is exactly the divergence decision 2 wants to avoid |
| New typed variant(s) | breaks the exhaustive `match` in ~5 elements | One vocabulary every backend speaks; the compile errors are the project's intended "new variant, decide what it means" signal |
| New trait methods with defaults | no breakage | A second mutation path alongside `update`, and nothing forces a widget to think about it |

**Decision: typed variants for the shared vocabulary, `Custom` kept as the escape hatch** for
genuinely backend-specific extras.

### Event interest: declarative first, imperative subscription as an opt-in extension

A subscription mechanism *is* needed, and for a concrete reason: the web driver installs delegated
listeners at the root, so it must know which event *types* to listen for, and some events do not
bubble (`focus`/`blur` need `focusin`/`focusout`; `scroll` does not bubble at all). Two flavours:

- **(A) Declarative, carried by the rendered markup** — the element's own markup marks the nodes
  that participate and names the fields, extending today's `data-lq-action` convention. The driver
  listens for a fixed, extensible set of types and routes by attribute. Nothing to register,
  nothing to re-register after a re-render, and it works for server-rendered HTML because the
  markup *is* the declaration.
- **(B) Imperative, per widget, at initialisation** — the widget registers interest against live
  DOM nodes (JS/`addEventListener`) as suggested in review. This buys things attributes cannot
  express: canvas interaction, drag-and-drop, third-party JS components, `scroll`. It costs a
  lifecycle: the subscription is invisible to SSR and must be re-established every time the node is
  replaced by a re-render.

**Decision: (A) is the normal path; (B) is a documented, opt-in extension point** (a per-element
mount/unmount hook in the web backend) that no part of W1–W3/W5 depends on. In backend-neutral
terms the element declares interest as an abstract list — activate, field change, field submit,
focus — and each backend maps it: web to delegated listeners plus attributes, egui to its inline
input checks, ratatui to its key routing.

### Field values reach the model on dispatch, with per-keystroke kept open

**Decision (review answer 2):** on-dispatch is the primary behaviour — when an action fires, the
live values of the declared fields travel with it. Per-keystroke syncing stays possible as a
per-field opt-in (a "live" flag on the declaration that makes the backend also emit a value-only
message as the user types), so it is a tweak rather than a redesign. The known cost of
on-dispatch-only is named in the W2 flow: an asset-driven re-render during typing still discards a
half-typed value, until the field opts into live syncing or the re-render restores it.

### Two message shapes, one action model

**Decision (review answer 3):** both shapes are needed and they are the same mechanism seen twice.

- **Value only** — the user changed a field; the element receives it and may simply visualise it.
  No action runs. (This is the "listen to field values" case.)
- **Field values + action** — the action declares which fields it collects; the backend gathers
  their live values and delivers them together with the action. Today's `ApplyToInput` is this
  shape, hardcoded to one field.

The analogy to keep honest is the HTML/JS one: a `UiAction` in an attribute plays the role of an
event handler, and an action that declares its inputs plays the role of a submit handler that
receives the form's values. Bundling values with the action also removes the ordering hazard — the
value cannot arrive after the action that was supposed to use it.

### Declared fields and `value-accessor`

"Declared field" means: *this element has a piece of user-editable state called `query`; its
current value is X; its default action is Y.* It is a **naming and routing** concept, needed
because the DOM (and server-rendered markup) can only carry strings, and an incoming event must be
resolvable to "element *e*, field *f*".

`specs/value-accessor` is the complementary layer, and the review is right that they meet. Its
motivation is literally two-way binding for widgets: an accessor is a cheap, clonable read/write
handle to a value that may live in a widget, a store, an asset, or inside a query. So:

- **Layer A (this feature, `liquers-lib::ui`)** — the *name* and its route: markup identity, event
  delivery, default action.
- **Layer B (`value-accessor`, `liquers-core`)** — what the name is *bound to*. Today the binding
  is implicit and trivial: a plain struct field on the widget. Later a field can be bound to a
  `ValueAccessor`, and the same text input edits a store value, a query parameter or an asset
  without the widget knowing which.

`value-accessor` is Phase-1 design only — there is no `liquers-core/src/accessor.rs` — so this
feature must not depend on it, but must compose with it. Two shaping requirements follow, both
cheap now:

1. **Field values travel as `Value`, not `String`** (a text value in practice), so a future
   accessor `set(Value)` slots in without changing the message vocabulary or the markup.
2. **Writes that can be async must not need an async `update()`.** An accessor's get/set are async
   while `update` is sync, so an accessor-backed write is dispatched as a message and performed by
   the runner — which is exactly how elements already delegate async work. Nothing about Layer A
   needs to change when Layer B arrives.

### Focus feeds `active_handle`

**Decision (review answer 5):** yes — a focus event inside an element's subtree sets
`AppState::active_handle`, so "current element" means the same thing in both backends. The one
hazard to respect in Phase 2: focus restored programmatically after a targeted re-render must not
be mistaken for the user navigating.

## Core Interactions

- **Query system:** unchanged; queries stay opaque strings carried by actions and messages.
- **Command system:** `lui` namespace (`submit`, `query_console`, navigation/`add`/`remove`).
  Whether `lui/submit` changes depends on how inbound interaction is modelled in Phase 2.
- **Asset system:** unchanged lifecycle. The console monitors an asset and reacts to snapshots;
  assets remain the authority for data.
- **UI module:** the `UIElement` trait (declared fields + inbound interaction), `AppState`/runner
  (invalidation), the web driver (event → interaction, accelerators, targeted rendering),
  `shortcuts` (a web conversion alongside the egui one), and `QueryConsoleElement` as the first
  widget to declare a field.
- **Store, value types, web API:** not involved.

## Crate Placement

Entirely in `liquers-lib` (`src/ui/…`), its tests, and `examples-web/`. `liquers-core` untouched,
so `liquers-py` and `liquers-axum` stay out of the blast radius.

## Open Questions

Settled in review: the inbound path (`update` with typed variants), event interest (declarative,
with an opt-in mount hook), sync timing (on dispatch, live opt-in kept open), message shapes
(value-only and values+action), the field/accessor layering, and focus → `active_handle`.

Remaining:

1. **Where the field declaration lives on the trait.** A method returning declarations that both
   backends consume, versus letting `render_web` emit the attributes and pairing it with a setter.
   The first is the honest shared contract; the second is less code today. Phase 2 decides.
2. **Does the egui backend adopt declared fields in this feature, or later?** Not required for
   W1–W3; deferring means the two backends keep two ways of writing the same field for a while,
   which decision 2 (round 1) tolerates only as a documented divergence.
3. **Scope check on W5.** Browser accelerators need a `KeyboardEvent` → `Key`/`Modifiers`
   conversion, a registry lookup at the root, and the precedence rules described above. It is the
   right thing to do and it is real extra work — confirm it belongs in this feature rather than an
   immediate follow-up.
4. **How far does the mount/unmount hook (B) get specified now?** Naming it as an extension point
   costs nothing; designing it properly is its own effort, and nothing in scope needs it.

## References

- `specs/ISSUES.md` — W1–W4 records (W1–W3 filed from the PR #10 review)
- `specs/webui/` — the original webui feature design
- `specs/async-wasm-refactor/DESIGN.md` — evidence that W4 is resolved
- `specs/UI_INTERFACE_FSD.md`, `specs/UI_WEB_DESIGN_NOTES.md`, `specs/UI_RATATUI_DESIGN_NOTES.md`
  — UI architecture and the other backends the module is meant to host
- `phase2-architecture.md`, `phase3-examples.md`, `phase4-implementation.md` in this folder —
  drafts from the earlier ungated pass, kept for reference only; they implement the point-fix
  answer that decision 1 has now rejected
