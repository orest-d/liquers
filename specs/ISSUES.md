# Issues and Open Problems

## Open

### Issue: ASSET-MESSAGE-LIFECYCLE-ROBUSTNESS
Status: Partially Resolved (WP-2)
Priority: High

#### Problem
Asset execution currently assumes that only `Context` sends service messages (`LogMessage`, `UpdatePrimaryProgress`, `UpdateSecondaryProgress`, etc.) and that no new messages are sent after plan execution completes.

This assumption needs thorough verification and explicit handling. In future, additional producers may appear (for example websocket/user-originated messages), which can introduce late or concurrent messages after execution finalization.

#### Risks
1. Late progress/control messages may mutate metadata after execution is finished.
2. Message-order races may cause inconsistent status/progress transitions.
3. Additional producers can break current single-producer assumptions and reintroduce deadlocks or blocked completion paths.

#### Scope of investigation
1. Audit all `AssetServiceMessage` producers and sender ownership/lifetime.
2. Verify end-of-execution guarantees for `Context` and plan evaluation.
3. Define and enforce post-finish message policy (ignore/reject/log/error) per message kind.
4. Define behavior for future external producers (e.g. websocket messages), including authorization and allowed message subset.
5. Add tests covering:
   1. late message arrival after `JobFinishing`/`JobFinished`,
   2. concurrent producers,
   3. cancellation + error + completion race ordering.

#### Expected outcome
A formalized message lifecycle contract for assets, with implementation and tests ensuring correctness under current and future multi-source message scenarios.

#### Implemented policy (WP-2)
Post-finish message policy, by kind × phase (see `specs/ASSETS.md` → Terminal Outcome Contract
and `specs/wp2-terminal-outcome/`):

| Message kind | Before finish | After finish |
|---|---|---|
| `UpdatePrimaryProgress` / `UpdateSecondaryProgress` | apply + notify | drop (debug-logged) |
| `JobSubmitted` / `JobStarted` | status transition | drop |
| `Cancel` | → `Status::Cancelled` (no stored error) | drop (no-op) |
| `ErrorOccurred(e)` | `fail_asset(e)` (→ `Status::Error`, metadata-preserving) | drop |
| `LogMessage` | append to metadata log | tolerated (at most one late entry) |
| `JobFinishing` / `JobFinished` | end the service loop | idempotent |

Also resolved: the terminal-outcome side (`get()` returns `Ok(error_state)` and consults status
rather than lossy notification content, so an overwritten `ErrorOccurred` cannot lose the error),
the unified metadata-preserving `fail_asset` routine, and deletion of the dead "meaningless"
post-finalization `JobFinished` service send. Remaining for a future WP: authorization and the
allowed message subset for genuinely external/multi-source producers.

### Issue: EXPIRATION-RECOVERY-WEB-API
Status: Open
Priority: P2 (Medium)
Source: WP-3 `expiration-safety` (see `specs/expiration-safety/`) — deferred follow-up.

#### Problem
WP-3 added two keyed-asset recovery operations as shared default methods on the `AssetManager<E>`
trait (`liquers-core/src/assets.rs`), inherited by both `DefaultAssetManager` and
`ImmediateAssetManager`:

- `get_any_status(key) -> Result<Option<State>, Error>` — read a keyed asset's current value
  regardless of status, **including `Status::Expired`**, without triggering evaluation (for
  inspection / download / audit of an expensive expired result).
- `to_override(key) -> Result<(), Error>` — pin a keyed asset's current value as
  `Status::Override`, preserving it without recomputation (`PersistenceStatus`-aware: no
  double-serialization).

These are only reachable in-process today. There is **no web API surface**, so a browser/HTTP
client cannot inspect an expired keyed asset or promote it to `Override` — exactly the
user-directed recovery flows the feature exists to enable. This support should be added to the web
API.

#### Fix direction
Expose both operations through `liquers-axum` (the assets router, `liquers-axum/src/assets/`):
1. A **recovery read** endpoint that resolves via `AssetManager::get_any_status` instead of the
   normal `get` (which treats `Expired` as a cache miss) — returning the expired/any-status state
   (data + metadata), with a clear indication in the response/metadata that the value is expired.
   It must NOT trigger evaluation and must not be the default `get` path.
2. A **promote-to-override** endpoint (mutating; POST/PUT) calling `AssetManager::to_override(key)`
   for a keyed asset, returning the resulting `Override` status.
3. Keep these on the keyed (`&Key`) surface only — there is no query-based counterpart (mirrors the
   core API, which is keyed-only by signature).
4. Consider whether the WebSocket asset stream should surface `Status::Expired` distinctly (ties
   into the WP-2 outcome contract already used there).

#### Verification
`tower::ServiceExt::oneshot` handler tests in `liquers-axum`: evaluate a keyed resource, expire it,
then (a) the recovery-read route returns the stale value with expired metadata while the normal
`get` route treats it as a cache miss / recomputes, and (b) the promote route flips the asset to
`Override` so a subsequent normal `get` serves it without recomputation.

## webui: async evaluation engine does not run on wasm (browser)

**Status: Resolved** by `async-wasm-refactor` (2026-07-23) — see
`specs/async-wasm-refactor/DESIGN.md`.

The engine called `tokio::spawn` on paths reachable from the browser, which panics on wasm because
there is no tokio runtime there. Resolved by option (A) plus an inline asset manager: conditional
`Send` across the async-trait hierarchy, `ImmediateAssetManager` evaluating inline with no spawn,
and wasm tokio reduced to `["sync"]`.

**Evidence (re-verified 2026-07-25 against current `HEAD`):** `trunk build` produces the wasm
bundle and the Playwright suite for `examples-web/ui_spec_demo` passes in headless Chromium with
zero `pageerror` — the engine parses, evaluates and renders inside the browser.

Remaining work from that effort is tracked below under *async-wasm-refactor follow-ups*.

## async-wasm-refactor follow-ups (out of scope, tracked)

The `async-wasm-refactor` (2026-07-23) made `liquers-core` run in the browser
(`ImmediateAssetManager` + target-gated conditional-`Send`; wasm tokio → `["sync"]`;
`ui_spec_demo` passes Playwright in headless Chromium). Deliberately **out of scope**, for a
future effort:

- **Full tokio removal / executor-agnostic core.** wasm still uses `tokio::sync` (channels/locks
  in `AssetData`/`DependencyManager`). Replacing it with framework-neutral primitives
  (`async-lock`/`async-channel`/`event-listener`/`async-once-cell`) would let the core run under any
  executor (embassy/smol/futures-executor) — the embedded angle. See
  `specs/async-wasm-refactor/phase2-architecture.md` → "Tokio Dependency Reduction".
- **Tier 2 browser-native I/O.** The conditional-`Send` groundwork permits a future
  `BrowserEnvironment` with an IndexedDB/`fetch` `AsyncStore` and a JS-closure command backend
  (`!Send` closures — the core already does not preclude them). Not implemented.

> **Note.** The two issues below are the *interaction* half of the browser backend — user input
> reaching a widget — and are designed together in `specs/ui-events/`, along with a third finding
> (menu accelerators are egui-only, so `Ctrl+N` from a `UISpec` menu silently does nothing in the
> browser). `specs/webui-fixes/` covered the rendering half and is complete.

### Issue: WEBUI-QUERY-CONSOLE-ENTER-KEY-SUBMIT
Status: Open — design in `specs/ui-events/` (W1)
Priority: P2 (Medium)
Source: PR #10 review (chatgpt-codex-connector, 2026-07-22) — `liquers-lib/src/ui/widgets/query_console_element.rs:461`

#### Problem
In the browser, Enter-key events originate on the `<input>`, and `dispatch_dom_event` looks only at
the target's closest `[data-lq-action]` ancestor. The current markup puts `data-lq-action` on the
sibling `<span>` (the "Go" button) instead of the input or one of its ancestors, so pressing Enter in
the query console returns without sending `ApplyToInput` — only clicking "Go" works.

#### Fix direction
Put the action on the input (or a shared toolbar ancestor of both the input and the button), or
special-case the input element on Enter in `dispatch_dom_event`.

#### Verification
Playwright: type a query, press Enter, assert the result renders (currently only a click works).

### Issue: WEBUI-SUBMIT-QUERY-STATE-NOT-PRESERVED
Status: Open — design in `specs/ui-events/` (W2)
Priority: P2 (Medium)
Source: PR #10 review (chatgpt-codex-connector, 2026-07-22) — `liquers-lib/src/ui/commands.rs:367`

#### Problem
When the web QueryConsole's "Go" control emits `ApplyToInput`, `lui/submit` only forwards
`RequestAssetUpdates`; it bypasses `QueryConsoleElement::submit_query`, so `query_text` and history are
never updated with the live DOM input. After the result triggers a re-render, the input is rebuilt
from the old `self.query_text`, and volatile/expired refresh paths also resubmit that stale query.

#### Fix direction
Update the console element's state (or carry the submitted query through the snapshot) before
requesting asset updates, so `query_text`/history reflect the live input.

#### Verification
Type a new query, submit, trigger a re-render; assert the input retains the submitted query and a
volatile refresh uses it (not the previous value).

### Issue: WEBUI-REPAINT-AFTER-SYNC-MUTATION
Status: **Resolved** by `webui-fixes` (2026-07-25) — see `specs/webui-fixes/`
Priority: P2 (Medium)
Source: PR #10 review (chatgpt-codex-connector, 2026-07-22) — `liquers-lib/src/ui/web/app.rs:165`

#### Problem
After the initial paint, the browser loop only re-rendered while `AppRunner::needs_repaint()`
reported active evaluations or monitoring — a proxy for "async work may land later", not a statement
about state. A web action that mutates `AppState` and leaves no pending asset was processed by
`runner.run`, but `needs_repaint()` was false immediately afterward, so the DOM stayed stale until
some unrelated async asset update occurred.

**Worse than recorded.** Measured against the pre-fix build, the demo's *Add Dashboard* action
produced no DOM change at all: with `ImmediateAssetManager` the evaluation completes inside the same
`run()` that starts it, so nothing is ever in flight when the loop asks. The existing Playwright
test passed only because its assertion (`#app` contains "Dashboard") was already satisfied by the
"Add Dashboard" menu label. This affected every menu action in the browser, not just
inline-resolving ones.

#### Resolution
Invalidation became a property of the model. `AppState`'s mutating methods record a `UIChange`
(`Inserted` / `Removed` / `Replaced`) into an `Invalidation` (`None` / `Changes` / `All`), and the
renderer takes it and applies it:

- `Replaced` re-renders that element's markup in place (stable `ui-element-{handle}` ids).
- `Inserted` / `Removed` perform the corresponding DOM operation when the parent declares a child
  container (`data-lq-children="{handle}"`), so siblings keep their DOM identity — and with it
  scroll position, selection and node-local state. Otherwise they degrade to re-rendering the
  parent.
- Anything unattributable (a deserialized state, a change log past `MAX_CHANGES`, an
  implementation that does not track) escalates to a whole-tree render. Focus and caret are
  captured and restored around replacements.

`needs_repaint()` remains, but only to decide whether to keep polling. The five egui example apps
consume the same signal, so they get the fix too. No `liquers-core`, macro, `liquers-py` or
`liquers-axum` changes.

#### Verification
- Unit: 17 tests in `liquers-lib/src/ui/app_state.rs` — one per recording site, the absorbing state
  machine, the `MAX_CHANGES` escalation, the serialization contract, and a deliberately
  non-tracking `AppState` proving the conservative default degrades to a full render, never to
  stale.
- Integration: `liquers-lib/tests/ui_invalidation.rs` — 6 tests including both runner delivery
  paths (`NeedsRepaint` records, `Unchanged` does not).
- Browser: `examples-web/ui_spec_demo/tests/webui.spec.ts` — a *Remove Last Panel* entry
  (`ns-lui/remove-last`) that resolves fully inline, plus a node-identity case. Both were checked
  in the failing direction as well: each fails against the behaviour it replaces.
