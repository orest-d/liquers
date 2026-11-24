# UI runner spike (S1, stage 2): the same runner in a browser, over WebSocket, and behind a thin HTML client

A spike run on 2026-11-24, true on that date. It is the second stage of spike S1 of the UI
redesign ([plan](../design/ui-redesign/plan.md) §3). [Stage 1](./2026-11-23-ui-runner-spike.md)
built one core runner under egui, a terminal, a retained host and an in-process split pair. This
stage checks the web claims of the [master design](../design/ui-redesign/master-design.md): a
DOM host, an idiomatic DOM view and a reactive framework as native web idioms, a thick, a mixed
and a thin generic client over a real WebSocket, a very thin HTML client, and client sizes.

The code is throwaway and is not in the repository. It is kept in the project files under
`spikes/s1-ui-runner/`. Stage 2 added two crates and a web folder, about 1,000 lines with tests.

| Added | Contents |
|---|---|
| `lui-web` | DOM host adapter (333 lines), the browser application (238), a portable view as HTML (42), a Leptos view (45, feature `leptos`) |
| `lui-server` | A WebSocket server (171 lines): one session per connection, endpoints `/ws/thick` and `/ws/html` |
| `web/` | The page, the very thin client `thin.js` (68 lines), and nine browser tests |

The core crate `lui` was **not changed** for this stage: the runner, the runner protocol and the
demo elements are the ones stage 1 tested.

**Limits of this stage.** The asynchronous engine is still simulated (browser timers in the
page, threads on the server); no Liquers crate is linked, so the sizes below are the UI's own
share, not a whole client. The tests run in headless Chromium only. The portable view is still
replaced whole per element and morphed positionally in the DOM; a keyed diff from the core is
not tested. Reconnect, resync, flow control and removal of native views are not tested. Dioxus
was not tried. A real window loop and a real terminal loop are still open from stage 1.

## 1. Verdict

- **The runner needs nothing new for the web.** A browser host is the retained adapter of
  stage 1 with DOM nodes as widgets. It is fed by the change list and wakes through one timer
  callback (§2).
- **Both native web idioms fit one small contract**: `mount(node, cx)` returns an update
  function. Hand-written DOM code and a Leptos component were both registered for the counter,
  nested in a portable panel (§3).
- **One page, four client forms.** The same wasm module ran monolithic, thick, mixed and thin
  generic, the last three against `lui-server` over WebSocket, with the stage 1 protocol
  unchanged (§4).
- **The very thin web client is 68 lines of JavaScript** and no wasm. The server runs the same
  monolithic runner and sends one HTML fragment per changed element (§5).
- **The UI's own share of a client is small**: 316 KB of wasm (110 KB gzipped); Leptos adds
  105 KB (30 KB gzipped); the thin client is 3 KB (§6).
- **Seven problems were found**, one of them a correctness rule the protocol must state: who
  owns a text field's content while the user types (§7).

## 2. The DOM host

- One container node per handle (`<div data-lq="handle">`), created and removed from the change
  list. The host keeps a map from handle to node, as the made-up retained toolkit of stage 1
  did.
- A portable view is rendered to an HTML string and **morphed** into the container: a node of
  the same tag is updated in place, anything else is replaced. Focus, caret and scroll survive.
- A child is shown where its parent's view names it: `View::Child(h)` becomes an empty
  `data-lq-slot` node, and the host moves the child's container into it. After a parent is
  updated the host places its children again, because a slot may be a new node.
- Input from portable nodes takes two delegated listeners on the root (`click`, `input`). They
  find the key (`data-lq-key`) and the owning handle and call `runner.input`.
- The waker is a flag-guarded `setTimeout(0)`: any number of wake-ups before the callback run
  one tick. A tick is `run_pending()` (or the client link's pump), then the change list into the
  DOM. An idle page runs no tick; a test checks it.

Checked in the browser: a click changes one element; typing "hello world", moving the caret to
the start and typing again leaves the text, the focus and the caret where a user expects them,
although every key changes the model and re-renders the element; adding a counter creates one
container and the first counter's field keeps its text when the panel is re-rendered.

## 3. Native web views

| Idiom | Code | What it does |
|---|---|---|
| DOM | 35 lines | Creates three nodes once, adds listeners that change the part with `cx.with(...)`, returns a closure that sets the label and the field |
| Leptos 0.8 | 45 lines | Mounts a `view!` component into the container; two signals; the update closure copies the part's state into the signals, event handlers call `cx.with(...)` |

- Both nest in a portable panel and beside portable siblings, in the monolithic page and in the
  mixed client.
- **A retained native view does not have stage 1's problem 3** ("a native view must say that it
  changed something"). The state is reached through `cx.with`, which reports the change itself.
  The problem belongs to immediate mode only, where the view holds `&mut` state for a frame.
- The host uses a native view only where the client holds the part. In the thin generic client
  the same element type is drawn from the server's views, with the native view registered or
  not.
- With Leptos the state exists twice, in the part and in the signals. That is the price of a
  part that is independent of the frontend. An implementation written for Leptos alone could
  keep its state in signals and would then be a native-only implementation.

## 4. Client forms over WebSocket

`lui-server` builds a session per connection. `/ws/thick` speaks the runner protocol; the client
says which element types it has in `Hello`, and the server builds its tree after that.

| Form | Client holds | Tested |
|---|---|---|
| Monolithic | everything, engine on timers | no connection at all |
| Thick | all four element types | the counter sends no frame; the asset view subscribes through the connection; the tally's two parts exchange messages; "Add counter" is a request the server answers |
| Mixed | the counter only, as a native DOM or Leptos view | a local native element inside a panel drawn from the server's views, beside remote siblings |
| Thin generic | no element type | everything drawn from views; input goes up, views come down |

The browser tests are the same walk for every form, so the forms are checked against one
expected behaviour. For a whole walk the server sent 13 frames (0.9 KB) to the thick client,
14 frames (2.0 KB) to the mixed one and 33 frames (5.2 KB) to the thin generic one.

## 5. The very thin HTML client

`/ws/html` runs the monolithic runner on the server, with every element in it. A change becomes
`{"op":"add"|"html"|"rm","h":handle,"html":fragment}`; several updates of one element between
two sends become one fragment. `thin.js` keeps a map from handle to node, morphs the fragment
in, places children into slots, and sends `{h, key}` or `{h, key, change}`. It passes the same
walk, with 24 frames and 4.6 KB for the whole walk.

The server side is a third retained adapter over the same change list. It shares the HTML
renderer with the DOM host, so a portable element looks the same in the wasm clients and in the
thin client.

## 6. Sizes

Release build, `opt-level = "s"`, LTO, stripped, after `wasm-bindgen`; no `wasm-opt`.

| Artifact | Raw | Gzipped |
|---|---|---|
| `lui-web` wasm: runner, protocol, DOM host, four demo elements, JSON | 316 KB | 110 KB |
| The same with Leptos and one Leptos view | 421 KB | 140 KB |
| JavaScript glue from `wasm-bindgen` | 24 KB (39 KB with Leptos) | not measured |
| `thin.js` | 3.1 KB | 1.2 KB |
| For comparison, stage 1: the runner with egui | 2.8 MB | 0.9 MB |

The 5 MB goal for the standard web client will be decided by what Liquers code the client
carries (`liquers-core`, records, value types), not by the UI runtime.

## 7. Problems found

1. **A text field needs an ownership rule, in the protocol.** With the view level, the echo of
   an older value arrives while the user is still typing. A control run without a guard turned
   "hello world" plus three more keys into `>hello ol>`. The spike's rule is "the frontend owns
   the content while the field has focus"; then the server cannot correct a focused field. The
   portable vocabulary must state the rule, and an input sequence number in `Input` and `View`
   would let the client accept a correction that is newer than its last key.
2. **The waker must not require `Send`.** `set_waker` asks for `Send + Sync`. A browser callback
   is neither, and the spike needed an `unsafe impl Send` wrapper. The core needs a waker that is
   `Send` only on targets with threads.
3. **Placing children is the host's work and must be defined.** A parent's update may replace a
   slot, which detaches the child's container; the host has to place children again after every
   parent update. With a positional morph a child can be detached and re-attached, which loses
   focus inside it. Keyed slots in the core's diff would make placement exact. A native parent
   has no slots at all; the spike appends children to the container. A native view needs a way
   to say where each child goes.
4. **Listeners and native views are never released.** The spike leaks every closure and the
   Leptos mount handle. A real host owns them per node and drops them on `Removed`; this is the
   DOM form of the lifecycle's "the runner cleans up" and was not tested.
5. **Re-entrancy.** The runner sits in a `RefCell` and is borrowed by event listeners. A DOM
   event that fires synchronously while the host changes the DOM (`blur` when a focused node is
   removed) would borrow it twice. The spike listens to no such event, so it was not hit.
   Listeners should post to the mailbox instead of borrowing the runner.
6. **The server end has no waker.** `ServerLink` and the HTML adapter are polled every 10 ms,
   because updates for a client's subscriptions arrive on a channel nobody is woken for. The
   real server end has to be woken by the engine as a host is.
7. **`Hello` is still needed with an endpoint per form.** The endpoint names the form, but the
   element types the client was built with come in `Hello`, and the tree is built after it. This
   confirms stage 1's problem 1 and its answer (a profile known at connect).

## 8. What is still open for S1

A keyed diff and patches from the core instead of whole views; reconnect and resync; the Liquers
engine and real values in place of the simulation, with the client's size then; a real window
and a real terminal loop; removal and replacement of native views; Dioxus, if a second reactive
framework is wanted; a native parent with children in the DOM.
