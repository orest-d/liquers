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

#### Concrete reproduction
In `liquers-lib/examples-web/ui_spec_demo`, click **Add Dashboard**. The button dispatches
`dashboard/q/ns-lui/add-child` against the enclosing dashboard handle. `AppRunner` receives the
`SubmitQuery`, evaluates it inline, and `lui/add-child` successfully inserts a `StateViewElement`
containing `DASHBOARD_YAML` beneath the dashboard in `AppState`. Because the inline evaluation has
already completed, neither the evaluating nor monitoring collections are populated, so
`needs_repaint()` returns false and `mount_web` does not rebuild the DOM. The inserted YAML is
therefore present in application state but remains invisible in the browser.

The action serialization, delegated click handler, query registration, and `add-child` insertion are
all working; the defect is specifically the missing DOM invalidation after the synchronous state
mutation.

#### Fix direction
Track whether messages/state changed during processing and force a repaint after processing them
(independent of `needs_repaint()`).

Prefer a one-shot dirty/repaint signal from `AppRunner::run` over unconditionally rebuilding the DOM
on every 16 ms timer tick.

#### Note (async-wasm-refactor interaction)
With `ImmediateAssetManager`, `SubmitQuery` now resolves **inline** (synchronously, no pending async
asset), which makes this stale-DOM window more likely to be hit in the browser — so this is worth
addressing alongside webui runtime work.

#### Verification
1. Playwright: open `examples-web/ui_spec_demo`, click **Add Dashboard**, and assert that
   `DASHBOARD_YAML` appears without waiting for an unrelated async event.
2. Perform another synchronous mutation (e.g. `lui/remove`) with no pending asset and assert that
   the DOM updates.

If the demo is changed to expect a nested interactive dashboard rather than the literal YAML text,
use `dashboard/ns-lui/ui_spec/q/add-child`; that conversion is separate from this repaint defect.

### Issue: WEBUI-WASM-SIZE-IMAGE-CODEC-FEATURE-GATING
Status: Open
Priority: P2 (Medium)
Source: `ui_spec_demo_web` Wasm size investigation (2026-07-25)

#### Problem
The `ui_spec_demo_web` module is much larger than expected for a small UI demo. The current Trunk
debug output is 24.26 MB raw (4.02 MB gzip / 2.41 MB Brotli). A Cargo release build reduces this to
8.30 MB raw (2.08 MB gzip / 1.29 MB Brotli), showing that debug metadata and lack of release
optimization are significant, but the optimized executable code is still large.

Approximate symbol-level attribution of the 5.33 MB release code section:

| Contributor | Code size | Share |
|---|---:|---:|
| AVIF / rav1e image codec | 1.88 MB | 35.3% |
| Liquers core evaluation engine | 1.22 MB | 22.9% |
| Liquers UI and value implementation | 0.73 MB | 13.7% |
| Other image codecs | 0.58 MB | 10.9% |
| Rust standard library and other dependencies | 0.51 MB | 9.6% |
| Serde / YAML / JSON | 0.18 MB | 3.4% |
| Markdown | 0.16 MB | 3.0% |
| Browser bindings and async runtime | 0.04 MB | 0.8% |

Image handling therefore contributes approximately 46% of the release code, with AVIF/rav1e alone
responsible for approximately 35%.

Although `ui_spec_demo_web` uses `liquers-lib` with `default-features = false` and only the `webui`
feature, `image`, `resvg`, `usvg`, and `tiny-skia` are unconditional dependencies. `ExtValue::Image`
and its general serialization/deserialization paths are also unconditional. The `image` crate's
default codec set makes AVIF/rav1e, TIFF, EXR, WebP, GIF, JPEG, PNG, and other codec code reachable
from the generic image value and web rendering paths.

The build configuration adds avoidable overhead as well: the demo is normally built in debug mode,
`index.html` explicitly sets `data-wasm-opt="0"`, and no size-oriented release profile is defined.
These build issues amplify the result but do not explain the large optimized code section.

#### Proposed feature model
Split image functionality into two optional tiers:

1. **Partial image support without AVIF**
   - Keep the existing `image-support` feature name, or introduce a clearly named
     `image-support-basic` feature.
   - Depend on `image` with `default-features = false`.
   - Enable only the codecs required by the supported baseline, initially PNG and, if required,
     JPEG/GIF/WebP.
   - Gate `ExtValue::Image`, image serialization, image web rendering, raster commands, and related
     dependencies consistently behind this feature.
   - Do not enable AVIF, `ravif`, `rav1e`, or `av-scenechange`.

2. **Full image support with AVIF**
   - Introduce `image-support-full` or `image-support-avif`.
   - Make it depend on partial image support and additionally enable the `image` AVIF feature.
   - Preserve the current broad codec behavior for native/full installations that require it.

The default feature set may continue to select full image support for compatibility, but
`webui` alone must not implicitly enable either image tier. Consumers should be able to choose:

```toml
# Web UI with no image values or codecs
liquers-lib = { default-features = false, features = ["webui"] }

# Web UI with baseline image support but no AVIF/rav1e
liquers-lib = { default-features = false, features = ["webui", "image-support"] }

# Full image support including AVIF
liquers-lib = { default-features = false, features = ["webui", "image-support-full"] }
```

If web image rendering needs PNG encoding whenever image values are enabled, PNG should belong to
the partial tier; the full multi-codec serializer should not be required merely to render a PNG data
URL.

#### Additional size reductions
1. Build production artifacts with `trunk build --release`.
2. Enable `wasm-opt` instead of `data-wasm-opt="0"`.
3. Strip function-name/debug sections from production Wasm.
4. Add a size-oriented release profile (`opt-level = "z"`, LTO, one codegen unit, and stripped
   symbols).
5. Consider separate optional features for Markdown and QueryConsole widgets, and allow examples to
   register only the LUI commands they use.

#### Compatibility considerations
1. Feature-gate all `ExtValue::Image` match arms explicitly so no-image builds remain exhaustive.
2. Check public APIs that currently expose `image::DynamicImage`; they may need matching feature
   gates.
3. Ensure native default builds retain their current image behavior unless a deliberate breaking
   change is approved.
4. Avoid making SVG support depend accidentally on the AVIF/full raster tier.
5. Update serialization errors so disabled codecs report that the relevant feature is unavailable.

#### Verification
1. Build `ui_spec_demo_web` in release mode with `webui` only and confirm that `image`, `ravif`,
   `rav1e`, and `av-scenechange` are absent from the dependency/symbol graph.
2. Build with partial image support and verify PNG web rendering and configured baseline codecs.
3. Confirm that partial support contains no AVIF/rav1e symbols.
4. Build with full image support and round-trip AVIF plus all currently supported formats.
5. Record raw, gzip, and Brotli Wasm sizes for all three configurations and enforce an agreed size
   budget in CI.
