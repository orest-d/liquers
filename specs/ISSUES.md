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

## webui: async evaluation engine does not run on wasm (browser)

**Status:** Open — tracked follow-up from the `webui` feature (see `specs/webui/DESIGN.md`).

The `webui` backend renders server-side (SSR) and **compiles** to
`wasm32-unknown-unknown`, but the browser example does not yet **run**: the async
evaluation engine calls `tokio::spawn` (in `liquers-core` `AssetManager::with_capacity`,
`Context`, and `DefaultEnvironment::init_with_envref`), which panics on wasm because there
is no tokio runtime there.

- Stock `tokio` compiles to wasm (types resolve) but `tokio::spawn` panics at runtime.
- `tokio_with_wasm` (the intended drop-in) does **not** compile here: core's
  `#[async_trait] impl AssetManager` methods require `Send`, while `tokio_with_wasm`'s
  primitives are `!Send` → `E0277` "future cannot be sent between threads".

**To fix (either):**
- (A) Make `liquers-core`'s async-trait hierarchy `Send`-conditional — `#[async_trait(?Send)]`
  on wasm across `AssetManager` / `AsyncStore` / `AsyncRecipeProvider`, plus the `+ Send`
  future bounds in `EnvRef::{evaluate,apply_recipe,...}` — then adopt `tokio_with_wasm`.
- (B) Introduce an `Environment`-provided spawn/timer seam and route every core
  `tokio::spawn` / `tokio::time` through it (native = tokio, wasm = `spawn_local` + browser timer).

Either unblocks the `examples-web/ui_spec_demo` browser example and its Playwright e2e.

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

### Issue: WEBUI-QUERY-CONSOLE-ENTER-KEY-SUBMIT
Status: Open
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
Status: Open
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
Status: Open
Priority: P2 (Medium)
Source: PR #10 review (chatgpt-codex-connector, 2026-07-22) — `liquers-lib/src/ui/web/app.rs:165`

#### Problem
After the initial paint, the browser loop only re-renders while `AppRunner::needs_repaint()` reports
active evaluations or monitoring. A web action that mutates `AppState` synchronously and leaves no
pending asset (e.g. `lui/remove`, `activate`, or a `SubmitQuery` that resolves inline) is processed by
`runner.run`, but `needs_repaint()` is false immediately afterward, so the DOM stays stale until some
unrelated async asset update occurs.

#### Fix direction
Track whether messages/state changed during processing and force a repaint after processing them
(independent of `needs_repaint()`).

#### Note (async-wasm-refactor interaction)
With `ImmediateAssetManager`, `SubmitQuery` now resolves **inline** (synchronously, no pending async
asset), which makes this stale-DOM window more likely to be hit in the browser — so this is worth
addressing alongside webui runtime work.

#### Verification
Perform a synchronous mutation (e.g. `lui/remove`) with no pending asset; assert the DOM updates
without waiting for an unrelated async event.
