# Phase 1: High-Level Design - webui-fixes

## Feature Name

webui-fixes — closing the interaction loop between the browser DOM and `AppState`

## Purpose

Three open `webui` issues make the browser backend behave differently from the egui reference
backend: Enter does not submit in the query console, the console forgets the query the user
submitted, and a state change with no pending async work leaves the page stale. They look like
three unrelated bugs; they are three holes in the same loop (see *Common denominator*). A fourth
issue (the wasm async engine) is already fixed and only needs its record closed.

## Scope

| ID | Issue (`specs/ISSUES.md`) | One-line symptom |
|----|---------------------------|------------------|
| W1 | WEBUI-QUERY-CONSOLE-ENTER-KEY-SUBMIT | Pressing Enter in the query input does nothing |
| W2 | WEBUI-SUBMIT-QUERY-STATE-NOT-PRESERVED | The input reverts to the previous query after submitting |
| W3 | WEBUI-REPAINT-AFTER-SYNC-MUTATION | A purely synchronous change does not update the page |
| W4 | webui async engine does not run on wasm | Already resolved by `async-wasm-refactor` — close the record |

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
the same widget does.

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

### W4 — for the record

Previously, loading the page aborted immediately: the engine tried to spawn async work on a
runtime that does not exist in the browser, and nothing rendered at all. The `async-wasm-refactor`
work fixed this (inline asset manager, browser-compatible task handling) and proved it with a
browser test that clicks through the demo. Only the issue entry is stale.

## Common denominator

All three are the same design gap seen from three sides.

The web backend renders **the whole tree from `AppState` and replaces the page content wholesale**
(a pure `render_web` function of the model). That is a good, simple design — but it is only correct
if the loop around it is closed in all three directions, and today none of them is:

| Direction | Requirement created by wholesale re-render | Violation |
|---|---|---|
| Intent **in** | Every user gesture that should do something must be reachable as an action from the event's target | W1: Enter on an unmarked element is dropped |
| Value **in** | Every piece of user-editable state must be written back into the model, or the next render destroys it | W2: the typed query never becomes model state |
| Change **out** | Every model change must trigger a render, because nothing else can update the DOM | W3: rendering is triggered by "async work pending", not "state changed" |

The underlying cause is a porting artifact: the widgets were written for **egui, an immediate-mode
backend**, where all three requirements are satisfied for free — the widget mutates its own state
in place while drawing, the key press is examined inline in the same call, and the frame is redrawn
anyway. The web backend is retained-mode with full replacement, and inherited the immediate-mode
assumptions without adding the equivalents a DOM needs. W2 is the clearest case: in egui the text
box edits `query_text` directly, so the field cannot go stale; on the web the same field is a
*copy* rendered into HTML, and the copy is where the user types.

Two consequences worth deciding on in Phase 2:

- The same gap has more instances waiting: caret position, selection and scroll offset are all
  live DOM state destroyed by every wholesale re-render; any future editable widget (a text area,
  a checkbox, a slider) reproduces W2 exactly.
- The fixes can be point fixes (three small, safe changes) or one explicit **interaction contract**
  for the web backend that the widgets are held to. That trade-off is the main open question below.

## Core Interactions

- **Query system:** unchanged. Queries stay opaque strings carried by actions and messages.
- **Command system:** the `lui` namespace is involved (`submit`, `query_console`); whether
  `lui/submit` changes at all depends on the Phase 2 choice for W2.
- **Asset system:** the console monitors an asset and reacts to snapshots; W2's fix may route the
  submitted query through that channel. No change to the asset lifecycle itself.
- **UI:** the query console widget, the runner's repaint signalling, and the browser event
  dispatcher. The egui backend must keep behaving exactly as it does now.
- **Store / value types / web API:** not involved.

## Crate Placement

Entirely in `liquers-lib` (`src/ui/…`), plus tests and the browser example. `liquers-core` is
untouched, keeping `liquers-py` and `liquers-axum` out of the blast radius.

## Open Questions

1. **Point fixes or an interaction contract?** Three narrow fixes are small and low-risk but leave
   the pattern that produced them intact. A contract (declare each element's editable inputs; the
   driver round-trips their live values into the model before dispatch; rendering is driven by
   explicit invalidation) fixes the class instead of the instances, at a higher cost.
2. **Who owns editable state, the model or the DOM?** Either the model is authoritative and must be
   updated on every dispatch (and eventually on input), or the renderer must preserve live DOM
   values (and caret/selection) across re-renders. This decides the shape of W2's fix.
3. **Is Enter a special case or a keyboard binding?** There is already a `shortcuts` module that the
   egui backend uses. Should web keyboard handling route through it, so Enter-in-input is just one
   binding among many?
4. **How should "state changed" be detected for W3?** An explicit dirty flag set by the runner is
   the cheap option; alternatives are invalidation signalled by `AppState` mutations, or comparing
   the rendered HTML against the last render.
5. **Should the browser demo gain a query console?** Today's demo has none, so W1 and W2 cannot be
   exercised in a browser test at all.
6. **Does the wholesale re-render need to become a diff/patch?** Related to Q2 and Q4, and the
   larger of the questions here — worth an explicit yes/no even if the answer is "not now".

## References

- `specs/ISSUES.md` — W1–W4 records (W1–W3 filed from the PR #10 review)
- `specs/webui/` — the original webui feature design
- `specs/async-wasm-refactor/DESIGN.md` — evidence that W4 is resolved
- `specs/UI_WEB_DESIGN_NOTES.md`, `specs/UI_INTERFACE_FSD.md` — UI architecture
- `phase2-architecture.md`, `phase3-examples.md`, `phase4-implementation.md` in this folder —
  drafts from the earlier ungated pass, kept for reference; they assume the *point-fix* answer to
  Q1 and will be re-derived after this phase is approved
