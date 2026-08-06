# Issues and Open Problems

## Open

### Issue: QUEUED-MANAGER-STARTUP-READINESS
Status: Open
Priority: P1 (Medium-High)

#### Problem

Initialization of a queued asset manager has no observable completion boundary.

`Environment::to_ref` calls the synchronous
`Environment::init_with_envref` hook and then returns `EnvRef`. In the built-in
native queued environments, `init_with_envref`:

1. Installs the environment back-reference with `AssetManager::set_envref`.
2. Spawns `AssetManager::start` as a detached Tokio task.
3. Returns without waiting for `start` to finish.

`DefaultAssetManager::start` loads command metadata and implementation versions
into the dependency manager. A caller can begin evaluation as soon as `to_ref`
returns, while that loading task may still be in progress. There is no readiness
future, state query, or error result through which application code can determine
that startup has completed.

This is separate from construction of `DefaultAssetManager`: its job-queue and
expiration-monitor tasks are already spawned by the manager constructor. The
unobservable startup phase discussed here is the environment-dependent
initialization performed by `AssetManager::start`.

The race is especially relevant to dependency-version registration and cache
validation. The current API does not establish whether evaluation is allowed to
observe a partially initialized dependency manager.

#### Expected behavior

Environment initialization should provide one documented guarantee:

1. `Environment::to_ref` does not expose an environment until required manager
   startup has completed; or
2. Every evaluation entry point awaits an idempotent manager-startup barrier before
   reading startup-dependent state.

Startup failures should be returned to the caller rather than being confined to a
detached task. Multiple concurrent first evaluations must share one startup
operation.

`ImmediateAssetManager` already uses lazy, idempotent startup through its internal
`ensure_started` path. The queued and inline managers should expose equivalent
readiness semantics even if their execution models remain different.

#### Fix direction

Consider one of:

1. Make environment initialization asynchronous and fallible.
2. Add a fallible `ensure_started` operation to the `AssetManager` contract and
   invoke it from all public evaluation entry points.
3. Return an initialization handle from `Environment::to_ref` that must be awaited
   before evaluation.

Avoid relying on task scheduling order between the detached `start` task and the
first evaluation.

#### Verification

Add tests covering:

1. Evaluation immediately after `Environment::to_ref`.
2. A command whose metadata and implementation versions must be registered during
   startup.
3. Multiple concurrent first evaluations sharing one startup operation.
4. Startup failure propagation.
5. Equivalent readiness guarantees for `DefaultAssetManager` and
   `ImmediateAssetManager`.
6. Native queued execution and the Wasm-compatible inline path.

### Issue: VOLATILE-KEYED-RECIPE-SELF-DELEGATION
Status: Open
Priority: P1 (Medium-High)

#### Problem

Evaluating a keyed asset whose recipe is volatile fails with a spurious
`ErrorType::DependencyCycle` instead of producing a value.

`AssetManager::get` resolves a volatile key through `get_volatile_resource_asset`, which
builds a **fresh** `AssetRef` and deliberately does not insert it into the `assets` map
(`liquers-core/src/assets.rs`). `AssetRef::evaluate_recipe` then calls `manager.get(&key)`
to decide whether it owns the recipe, and compares asset ids: because the volatile path
mints a new asset on every call, the returned id never equals the caller's, so the branch
always takes the *delegation* path. The delegation records a dependency of the asset on
what is effectively itself, and `register_scheduled_dependency` correctly reports a cycle.

Non-volatile keyed recipes are unaffected: their assets are shared through the map, so the
id comparison succeeds and the asset evaluates its own recipe.

#### Reproduction

`liquers-core/tests/payload_inheritance.rs::test_volatile_keyed_recipe_cycles_preexisting_defect`
registers a command with `volatile: true`, stores a recipe using it, and evaluates
`-R/<key>`. No payload is involved. The test currently asserts the broken behaviour so that
a fix fails loudly.

#### Impact

Any keyed recipe using a volatile command is unusable. This also blocks the natural
evaluation-path test for the keyed-payload boundary, since `payload: required` implies
`volatile` — see PAYLOAD-NESTED-EVALUATION-INHERITANCE. That rejection is therefore verified
through recipe resolution and asset introspection instead.

#### Fix direction

The ownership test in `evaluate_recipe` should not rely on asset-id identity for volatile
keys, since that identity is not stable by design. Consider comparing keys, or having the
volatile path return the calling asset when one is already evaluating that key.

#### Verification

1. A keyed volatile recipe evaluates to its value rather than a cycle error.
2. Non-volatile keyed recipes are unchanged.
3. Invert `test_volatile_keyed_recipe_cycles_preexisting_defect` and re-enable the
   `evaluate()` path in `test_keyed_recipe_requiring_payload_is_rejected`.

### Issue: ASSET-EXPIRED-CACHED-BINARY-READ
Status: Open
Priority: P0 (High)

#### Problem

Normal asset reads are intended not to expose an expired value. `AssetData::poll_state`
returns `None` for `Status::Expired`, and stale-value recovery is explicit through
the any-status APIs.

Cached binary reads do not apply the same rule:

- `AssetData::poll_binary` returns cached binary data without checking status.
- `AssetRef::poll_binary` delegates directly to it.
- `AssetRef::get_binary` calls `poll_binary` before the expiration-aware `get`
  path.

Consequently, an asset whose status is `Expired` can still return stale bytes
through the normal `poll_binary` or `get_binary` API when a binary representation
is cached. This bypasses the normal expiration contract and is a bug.

#### Expected behavior

1. Normal binary reads do not return data for `Status::Expired`.
2. Binary and state reads follow the same expiration policy.
3. Access to retained expired data remains possible only through an explicit
   recovery API.
4. The behavior is consistent for both cached binary data and binary data produced
   by serializing an in-memory value.

#### Verification

Add tests that create an asset with both value and cached binary data, expire it,
and verify:

1. `poll_binary` returns `None`.
2. `try_poll_binary` returns `None`.
3. `get_binary` does not return the expired cached bytes.
4. Explicit any-status recovery still exposes retained expired state.
5. `Ready`, `Source`, `Override`, and `Volatile` binary behavior remains unchanged.

### Issue: QUERY-ACTION-PARAMETER-LINK-PARSER
Status: Resolved (2026-08-06)
Priority: P0 (High)

Design: `specs/query-link-parser/`. Implementation: `liquers-core/src/parse.rs`.

#### Problem

The query language defines `~X~<query>~E` as the textual representation of an
action-parameter link: the text between `~X~` and `~E` is an embedded Liquers query
and the parsed value must be `ActionParameter::Link`.

`liquers-core::query` already represents and encodes this form:

- `ActionParameter::Link(Query, Position)` is part of the public semantic model.
- `ActionParameter::encode()` emits `~X~<query>~E`.
- Plan construction accepts `ActionParameter::Link` and resolves it as a linked
  query parameter.

However, `liquers_core::parse` has no corresponding parser production. Consequently,
valid link syntax such as `action-~X~hello~E` is rejected, and an encoded
`ActionParameter::Link` cannot round-trip through `parse_query`.

This omission is a parser bug. Action-parameter links and the
`~X~<query>~E` syntax are supported language features, not reserved or
programmatic-only features.

#### Expected behavior

1. The action-parameter parser recognizes `~X~<query>~E`.
2. `<query>` is parsed using the authoritative Liquers query grammar.
3. The result is `ActionParameter::Link(parsed_query, position)`.
4. Encoding and reparsing an action containing `ActionParameter::Link` preserves
   the link and embedded query semantics.
5. Link parameters work at every action-parameter position, including between
   string parameters.
6. Malformed or unterminated link syntax returns `ErrorType::ParseError` with a
   useful source position.

#### Verification

Add parser and round-trip tests covering:

1. A single link: `action-~X~hello~E`
2. A link between strings: `action-before-~X~hello~E-after`
3. An embedded multi-segment query
4. An embedded query containing encoded parameter entities
5. Malformed and missing `~E` delimiters
6. Source positions for the link and embedded query

The existing encoder tests in `liquers-core/src/query.rs` establish the intended
serialized form. The current rejection test in `liquers-core/src/parse.rs` records
the bug and should be replaced by successful parsing and round-trip assertions when
the parser is fixed.

#### Resolution

All six expected behaviors are implemented and covered:

| # | Expected behavior | Test |
|---|---|---|
| 1 | parser recognizes `~X~<query>~E` | `link_tests::a1`, `documented_query_language_contract` |
| 2 | embedded query uses the authoritative grammar | `link_tests::b3`, `b4` (15-entry canonical corpus) |
| 3 | result is `ActionParameter::Link(query, position)` | `link_tests::a1`, `a8` |
| 4 | encode/reparse preserves link and embedded semantics | `link_tests::b1`, `b2`, `b3` |
| 5 | links work at every parameter position | `link_tests::a2`, `a3`, `a11` |
| 6 | malformed link → `ParseError` with a useful position | `link_tests::c1`, `c3`-`c6`, `c10` |

The rejection test at `parse.rs` was replaced, not deleted, as required.

Two behaviors were added beyond the issue's scope because the fix created them:

1. **The resource/transform shorthand is rejected inside a link.** `~X~a/b/-/c~E`
   would otherwise mean something different from `parse_query("a/b/-/c")`, because the
   `eof`-gated query forms cannot match before a `~E`. Rejecting removes the ambiguity
   instead of resolving it silently. The shorthand is also now documented as
   discouraged generally.
2. **Nesting and total link count are bounded** (`MAX_LINK_DEPTH = 8`,
   `MAX_LINK_MARKERS = 64`). See the follow-up below.

#### Follow-up: QUERY-LINK-EXPONENTIAL-BACKTRACKING

Discovered while implementing this issue, and the reason a depth bound exists.

**Parsing is exponential in link nesting depth.** In
`transform_segment_without_header`, `action_requests` parses an action in full —
recursing through any nested link — and then discards that work when the required `/`
separator does not follow; `filename_or_action` immediately parses the same action
again. Two full sub-parses per level gives `T(n) = 2·T(n-1)`.

Measured on a debug build: depth 10 ≈ 32 ms, 14 ≈ 0.54 s, 16 ≈ 2.3 s, 17 ≈ 4.3 s,
doubling per level. A ~200-byte query nested 64 deep never finishes, so an unbounded
depth limit would itself be a denial-of-service vector on any parser reachable from
HTTP input.

`MAX_LINK_DEPTH = 8` (≈10 ms worst case) contains it. Removing the bound requires
restructuring the double-parse in `transform_segment_without_header`, which is a
change to the core query grammar and was out of scope here. The exponential behavior
predates links — links are simply the first construct that makes it reachable through
recursion.

#### Follow-up: QUERY-TEMPLATE-SHORTHAND-AMBIGUITY

The same shorthand ambiguity this issue fixes for links exists in `$...$` template
expansions and is **not** diagnosed. `template_expand_query` calls `query_parser` with
a trailing `$`, so the `eof`-gated forms cannot match there either:

```
parse_query("data/report/-/to_text")             -> -R/data/report/-/to_text  (resource read)
parse_simple_template("$data/report/-/to_text$") -> data/report/-/to_text     (three commands)
```

Pre-existing, unrelated to links, and left alone deliberately. The same detector would
apply with `peek(tag("$"))` in place of `peek(tag("~E"))`. Documented as a caveat in
`parse.rs` and doc-02 in the meantime.

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

## Resolved

### Issue: PAYLOAD-NESTED-EVALUATION-INHERITANCE
Status: Resolved
Priority: P0 (High)

#### Resolution

Inheritance is implemented, opt-in per command. A command that reads the payload declares
`payload: required` in `register_command!`; the requirement propagates to
`Plan::payload_required`, and `Context::evaluate`, `Context::get_dependency_state` and
`Context::apply` forward the parent's payload to such a nested evaluation, which then runs
inline rather than through the job queue.

Design record: `specs/payload-nested-evaluation-inheritance/` (phases 1-4).

Rules adopted:

1. **Requiring a payload implies `volatile`.** Set at registration, so all existing
   volatility propagation applies unchanged. A payload-evaluated asset is fresh per
   evaluation, never cached, shared, or persisted.
2. **A payload asset may have dependencies, but may never be one.** A payload is not part of
   the dependency key, so no graph edge may point at such an asset. Its own dependency
   records are still written.
3. **Cycles are detected along the evaluation path**, since neither end of a
   payload-to-payload chain is a graph node. The path travels on `AssetData` and is re-seeded
   into each nested context.
4. **Keys are a payload boundary.** A key names one shared global asset while a payload is
   per-evaluation, so a keyed recipe requiring a payload is rejected when its plan is built,
   and a requirement never propagates through a keyed step.
5. **`Optional` is deliberately not implemented.** It would re-open the otherwise unreachable
   "not volatile but uses payload" state. Adding it is intentionally a breaking change for
   exhaustive matches on `PayloadRequirement`.

#### Known limitations

- **Declaration is manual and not compiler-visible.** A command that reads the payload but
  omits `payload: required` keeps the previous behaviour: it works at top level and silently
  receives no payload when nested. Pinned by
  `test_unannotated_payload_command_is_payload_free_when_nested`.
- **Commands with dynamically constructed nested queries** are invisible to plan analysis and
  fail at execution rather than at plan time. Accepted.
- The keyed-recipe rejection is verified through recipe resolution and asset introspection
  rather than through `evaluate("-R/<key>")`, because of
  VOLATILE-KEYED-RECIPE-SELF-DELEGATION above.

#### Verification

`liquers-core/tests/payload_inheritance.rs` (11 tests) and
`liquers-core/tests/injection.rs::test_payload_inherited_in_nested_evaluation`, plus unit
tests in `command_metadata.rs`, `metadata.rs`, `plan.rs` and
`tests/volatility_integration.rs`.

Not run in the implementing environment: `cargo check --target wasm32-unknown-unknown`
(target not installed). The inline path is covered natively via
`ImmediateEnvironmentWithPayload`.
