# Phase 1: High-Level Design - webui-fixes

*Narrowed after review. This feature originally covered all four open `webui` issues; the
interaction half (W1, W2, W5) moved to `specs/ui-events/`, which is its own phased design. What
remains here is the rendering half: making the browser show what the model says, and closing the
record of the already-fixed wasm issue.*

## Feature Name

webui-fixes — rendering and invalidation for the retained-mode (web) backend

## Purpose

The browser backend re-renders only when the runner reports *pending async work*, so a change that
has already completed leaves the page stale. Make rendering follow **what changed in the model**
instead, without destroying the user's focus and caret — the one property a retained-mode backend
has to protect and an immediate-mode backend never has to think about. This is what stands between
`examples-web/ui_spec_demo` working by luck and working by construction.

## Scope

| ID | Issue (`specs/ISSUES.md`) | Outcome |
|----|---------------------------|---------|
| W3 | WEBUI-REPAINT-AFTER-SYNC-MUTATION | Any model change reaches the DOM, without stealing focus |
| W4 | webui async engine does not run on wasm | Already resolved by `async-wasm-refactor` — close the record |

Explicitly **not** here (moved to `specs/ui-events/`): W1 (Enter does not submit), W2 (submitted
query never reaches the element), W5 (accelerators are egui-only). They need an inbound-interaction
vocabulary; nothing in this feature does.

## Demonstrating flow (W3)

The app's menu has an entry that only rearranges the UI tree — close a panel, or make a panel
active. The user clicks it. The panel stays on screen. Some seconds later, apparently at random, it
disappears — right after some unrelated part of the app receives an update.

What happens underneath: the click is dispatched, the runner processes the message, the command
removes the node from `AppState`. The render loop then asks one question before re-rendering: *is
there work in flight?* — are there evaluations running or monitored assets that might publish
later. For a change that has already completed there is nothing in flight, so the re-render is
skipped and the DOM keeps showing state that no longer exists. It corrects itself only when some
*other* activity happens to make that question true again.

This is not exotic. In the browser the engine evaluates inline (`ImmediateAssetManager`), so even
query-driven mutations typically finish before the loop asks.

**Corrected during implementation.** This document originally claimed `ui_spec_demo` was saved by an
accident of its one menu action — that `dashboard/q/ns-lui/add-child` leaves a pending node in
flight long enough to trigger a repaint. That is wrong: measured against the pre-fix build, clicking
*Add Dashboard* produced **no DOM change at all**, because the inline asset manager finishes the
evaluation inside the same `run()` call that started it. The demo's existing Playwright test passed
only because its assertion (`#app` contains "Dashboard") is satisfied by the *menu label* "Add
Dashboard" before any click. So W3 affected the demo's only action, not merely hypothetical
inline-resolving ones.

### W4 — for the record

Previously, loading the page aborted immediately: the engine tried to spawn async work on a runtime
that does not exist in the browser, and nothing rendered. `async-wasm-refactor` fixed this (inline
asset manager, browser-compatible task handling) and proved it with a browser test that clicks
through the demo. Only the issue entry is stale.

## Why this is a design gap, not a missing `if`

The web backend renders the whole tree from `AppState` and installs the result — `render_web` is a
pure function of the model, shared by SSR and the browser. That is a good design, but it makes one
requirement absolute: **every model change must reach the backend, because nothing else updates the
DOM.** An immediate-mode backend has this for free (it redraws every frame regardless), so the
module never grew a way to say "this changed". The runner's `needs_repaint()` is a proxy for
"async work may land later", not a statement about state.

The fix is therefore an *invalidation* concept in the `ui` module, not a patch in the browser loop.

## Decisions carried from review

1. **A dirty signal is definitely in** — but a single global flag would re-render the whole page for
   any change, throwing away focus, caret and scroll everywhere: a stale-DOM bug traded for a
   typing bug.
2. **Invalidation is per element.** State modifications are already element-scoped (`lui/add`,
   `lui/remove`, `activate`, an element updating itself from a snapshot all name a handle), and
   every element renders into a container with a stable `ui-element-{handle}` id. So the signal is
   a **set of dirty handles**, and the backend's response is the DOM operation corresponding to the
   state change: re-render that element's markup into its own container, or drop its node. The
   element the user is typing in is simply never touched.
3. **The global flag is the fallback**, for changes that cannot be attributed to a handle (the root
   set changing, deserialization) — with focus and caret saved and restored around the whole-tree
   re-render.
4. **No diff/patch.** The tree plus stable ids already give the granularity that matters. Comparing
   freshly rendered markup against what is installed, and skipping identical work, is a later
   optimization.
5. **Immediate-mode backends are unaffected.** egui may ignore the signal (it redraws anyway) or use
   it to skip work; its behaviour must not change.

## Core Interactions

- **Query system:** unchanged.
- **Command system:** unchanged in signature. The `lui` commands that mutate the tree become the
  natural producers of per-handle invalidation.
- **Asset system:** unchanged. Snapshot delivery already reports a per-element repaint verdict
  (`UpdateResponse::NeedsRepaint`) that is currently discarded — a ready-made invalidation source.
- **UI:** `AppState`/`AppRunner` (the invalidation signal), the browser render loop (consuming it,
  targeted rendering, focus/caret preservation). Widgets are untouched.
- **Store, value types, web API:** not involved.

## Crate Placement

`liquers-lib` only: `src/ui/runner.rs`, `src/ui/app_state.rs`, `src/ui/web/app.rs`, plus tests and
`examples-web/`. `liquers-core` untouched.

## Open Questions

1. **Where does the dirty set live** — on `AppRunner` (which already sees every message and every
   snapshot delivery) or on `AppState` (which is where mutation actually happens, including
   mutations made directly by commands)? `AppState` catches more, at the cost of putting
   backend-facing state into the model.
2. **Granularity of a change to the tree.** Removing a node invalidates its parent (the child list
   changed); adding a child does too. Is "invalidate the parent" enough, or is an explicit
   structural-change signal needed?
3. **How is focus/caret restored** after a whole-tree fallback re-render — record the focused
   element id, field name and selection offsets, and reapply? This is straightforward for declared
   fields, which arrive with `ui-events`; before then it can only be best-effort by DOM id.
4. **Should the render loop stop polling** once invalidation is explicit (render on signal instead
   of every 16 ms tick), or is that a later optimization?

## References

- `specs/ISSUES.md` — W3, W4 records
- `specs/ui-events/` — the interaction half of the original design (W1, W2, W5)
- `specs/webui/` — the original webui feature design
- `specs/async-wasm-refactor/DESIGN.md` — evidence that W4 is resolved
- `phase2-architecture.md`, `phase3-examples.md`, `phase4-implementation.md` in this folder —
  drafts from the earlier ungated pass, covering the *old, wider* scope; reference only
