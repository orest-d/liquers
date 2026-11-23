# UI runner spike (S1, stage 1): one core runner under egui, a terminal, a retained host and a split pair

A spike run on 2026-11-23, true on that date. It is the first stage of spike S1 of the UI
redesign ([plan](../design/ui-redesign/plan.md) §3), widened after the review of the
[master design](../design/ui-redesign/master-design.md): before more is designed, check by
building it that one runner can serve immediate-mode and retained-mode frontends, native and
portable views, and thin and thick clients.

The code is throwaway and is not in the repository. It is kept in the project files under
`spikes/s1-ui-runner/`: a Cargo workspace of five crates, about 2,600 lines with tests.

| Crate | Contents |
|---|---|
| `lui` | The core: runner (tree, lifecycle, mailbox, effects), shared services, a seven-node portable view, the runner protocol |
| `lui-egui` | egui host adapter: native views and the portable interpreter |
| `lui-tui` | ratatui host adapter with a focus ring; a thin terminal client |
| `lui-demo` | Four demo elements, registries per role, native views, 16 tests |
| `lui-wasm` | The runner and the egui host as a wasm32 module, run under Node |

**Limits of this stage.** The asynchronous engine is simulated (updates pushed by the test, or
from other threads); no Liquers crate is linked. There is no real window, no real terminal and
no browser: egui runs under `egui_kittest`, the terminal draws into ratatui buffers, and the
retained toolkit is made up. Views are replaced whole per element; a keyed diff is not tested.
Reconnect, resync and flow control are not tested. The web (DOM, Leptos or Dioxus, sizes) is
stage 2.

## 1. Verdict

- **The runner can be frontend-independent.** `lui` names no toolkit. Five consumers were built
  on four entry points: `run_pending()`, lending a part out and taking it back, `view(handle)`,
  and a list of changes since the last call (§2).
- **Relating handles to widgets is the host adapter's job, and it is small.** Immediate mode
  keeps no map. Retained mode keeps a map fed by the change list. The server end of the
  protocol is that same retained adapter with remote widgets (§3).
- **Asynchronous work meets a synchronous main loop through a mailbox and a waker, with one
  thread.** It ran with engine threads natively and as wasm32 under Node (§4).
- **Idiomatic native views and portable views nest in both directions**, in egui and in the
  terminal (§5).
- **Thick, thin and mixed clients are one protocol** with three parts, and the same element
  code runs as a monolithic implementation and as a client part (§6).
- **A thin terminal client is three lines over ratatui's buffer diff** (§7).
- **Eight problems were found** that the master design did not answer, two of them structural:
  the server must know the client form before it creates parts, and state held only in a client
  part is invisible to the server (§8).

## 2. The core runner

`Runner` owns a map of nodes (handle, parent, children, element type, properties, generation,
the part, its subscriptions). A part is `update(msg, cx)` plus an optional portable `view()`.
`cx` records effects (subscribe, add or remove a child, send to the other part); the runner
performs them when `update` returns.

| Entry point | Used by |
|---|---|
| `run_pending()`: process the mailbox, never block | every host, once per frame or after a wake-up |
| `take_part` / `put_part`: lend a part and its `cx`, then perform its effects | `update` itself; native immediate-mode views |
| `view(handle)`: the portable view, local or sent by the server | portable interpreters |
| `drain_changes()` / `snapshot()`: added, replaced, removed, updated | retained adapters; the server end of the protocol |
| `RunnerHandle` (cloneable, `Send`): post a message, add or remove a node | other threads, commands |

Checked by tests: a parent mounts before its children and unmounts after them; `Mount` is the
first message; removing or replacing a node cancels its subscriptions without the part doing
anything; a message addressed to an older generation is dropped and traced; another thread
reserves a handle and returns before the node exists.

**Lending without a lock.** To give a native view `&mut` access, the runner takes the part out
of its node for the duration of the call. The runner itself stays usable, so the view can draw
its children through it. No `Mutex`, no `RefCell`, no extract-render-replace of the whole tree.

## 3. Host adapters

| Host | Kind | Handle to widget | Lines |
|---|---|---|---|
| egui | immediate | none: `ui.push_id(handle)` per node, state in egui's own memory | 134 |
| ratatui | immediate, cells | none; a focus ring rebuilt each frame | 224 |
| made-up toolkit | retained | a `HashMap<Handle, Widget>` driven by the change list | 60, in a test |
| server link | remote, retained | the client is the widget set: changes become protocol frames | part of `split.rs` |
| thin terminal | remote, immediate | none: the terminal adapter runs on the server | 82 |

The retained test counts creations, updates and destructions: a click in one element updates one
widget and creates none; an idle sync does nothing.

## 4. Asynchronous engine, synchronous loop, one thread

Anything asynchronous holds a sink that posts into the runner's mailbox and calls the waker. The
host sets the waker to its repaint request and calls `run_pending()` at the start of a frame.

- Native: a simulated engine on other threads reports progress, a value and expiry; the test's
  "main loop" sleeps until woken, and receives every update in order.
- wasm32: the runner, the demo elements and the egui host build for `wasm32-unknown-unknown` and
  run under Node (`run() = 0`): frames, an asset update between frames that requests a repaint,
  a message round, a native state change, cleanup. One thread, nothing blocks.
- The module is 2.8 MB raw and 0.9 MB gzipped (`opt-level = "s"`, LTO), almost all of it egui
  with its fonts. The runner's own share was not measured separately.

## 5. Native and portable views together

- **egui, idiomatic.** A registered closure gets `&mut Counter`, `&mut egui::Ui` and a context:
  `if ui.button("+1").clicked() { counter.count += 1 }`, `ui.text_edit_singleline(&mut
  counter.note)`. No message is involved; the trace records "changed during frame".
- **Nesting.** A native panel draws native and portable children with `cx.show_child(ui, h)`;
  a portable panel's `Child(h)` node draws a native child. Both orders pass under `egui_kittest`
  with real clicks and typing.
- **Portable text field in immediate mode.** The interpreter keeps the draft in egui's memory
  while the field has focus and sends `Change`; typed text arrives in the model intact.
- **Terminal.** The same portable views draw as lines; a native view writes into the buffer.
  The terminal has no focus handling of its own, so the adapter builds a focus ring of the
  buttons and fields it drew, moves it with Tab, and routes Enter and characters.

## 6. Split mode

Two runners, roles `Server` and `Client`, joined by two queues of JSON frames.

| Part | Frames | Tested |
|---|---|---|
| A, tree | `Hello` (client form, element types the client has); `Added`, `Replaced`, `Removed`; `RequestAdd`, `RequestRemove` | The client's outline equals the server's, same handles and generations; lifecycle order holds on the client; a client part asks for a child, the server adds it, the client mirrors it; a server-side removal unmounts on the client and cancels the client's subscription on the server |
| B, services | `Subscribe`, `Unsubscribe`, `Asset` | The asset view's client part subscribes through the connection; progress and value are relayed |
| C, element | `ToPart` (handle, generation, payload) | The tally's client part and server part exchange their own messages |
| View level | `View` (whole portable view), `Input` | Nodes the client has no part for are drawn from views the server sends; input goes up |

- **One piece of code, two roles.** `AssetView` is registered as the monolithic implementation
  and as the client part. Only the `Services` behind `cx.subscribe` differ: in process, or the
  connection. This supports "one message shape, several carriers" for asset monitoring.
- **Thick.** With every element type in the client, a click on the counter causes no frame at
  all; the server node of an element without a server part is a record with no behaviour.
- **Thin generic.** With no element type in the client, the same client runner draws everything
  from received views.
- **Mixed.** With only the counter in the client, one screen shows a remote panel, the local
  counter inside it, and remote siblings. It works because handles are the same on both sides.
- **The split shape runs monolithically.** `Joined { client, server }` routes the two parts'
  messages in process; the tally then needs no connection.

## 7. Thin terminal client

The server runs the runner and the terminal adapter and keeps the previous buffer. Each frame it
sends `prev.diff(&next)` as (x, y, symbol, style) cells; the client applies them and sends keys.
The client's buffer equals a local terminal's; an idle frame sends nothing; the update after a
key press was under a third of the first frame. This is the terminal counterpart of HTML
fragments: the frontend runs on the server and only its device is remote.

## 8. Problems found

1. **The server must know the client form before it creates parts.** For an element the client
   has, the server runs only the server part (or nothing). For an element the client lacks, it
   runs the whole portable implementation. The spike picks the server registry from the client's
   list of element types, before building the tree. Two consequences: the client form has to be
   known at connect (an endpoint per form, or `Hello` before the tree exists), and two clients of
   different forms cannot share one server tree as built here.
2. **State held only in a client part is invisible to the server.** The thick client's counter
   lives in the client alone. The server cannot save it, show it to a second device, or serve
   it to a thin client. A standard "saved state" message from client part to server is needed if
   sessions are to be restored or shared.
3. **A native immediate-mode view must say that it changed something.** The demo calls
   `mark_changed()`. Forgetting it does not affect egui (it redraws anyway) but starves anything
   that listens for changes: a retained observer, a view-level client, persistence.
4. **Messages between parts can be sent before the other part exists.** The server part's
   `Mount` sent its first message before any client had connected; the spike held the outbox
   until `Hello`. Parts need "peer connected" and "peer gone" in their lifecycle, also for
   reconnects.
5. **A client cannot reserve a handle.** `cx.add_child` returns a handle at once in the
   monolithic and server roles; in the client role it is a request, and the real handle arrives
   later with `Added`. Client-side ranges or request ids are needed.
6. **Focus has to be in the core.** The terminal adapter had to invent a focus order. Portable
   views need a defined one, shared with the keymap.
7. **Service subscriptions made for a client are not tied to its nodes on the server.** The
   server link cancels them on `Unsubscribe` only; a dropped connection would leak them. They
   must be owned per connection.
8. **One `Part` trait served all three roles.** The spike needed no separate client-part trait,
   only different registries and services. Whether that survives a real `Environment` type
   parameter is open.

## 9. What stage 2 should cover

A real DOM host in a browser, and one reactive framework (Leptos or Dioxus) as a native web
idiom; a thin HTML client; module sizes against the 5 MB goal; a real window and a real terminal
loop; the Liquers engine in place of the simulated one; a keyed diff; reconnect and resync.
