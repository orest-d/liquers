# Phase 1: High-Level Design - liquers-web

## Feature Name

liquers-web — browser/JavaScript integration of Liquers (wasm)

## Purpose

Provide a new workspace crate `liquers-web` that compiles to `wasm32-unknown-unknown` and exposes
Liquers to JavaScript in the browser through `wasm-bindgen`. It is the **language integration** for
browser JavaScript as defined in [LANGUAGE-INTEGRATION_GUIDE.md](../LANGUAGE-INTEGRATION_GUIDE.md):
a page can construct an environment, evaluate queries as Promises, and register its own commands
written in JavaScript. It is the browser-side counterpart of `liquers-axum`.

## Scope: selected features (initial phase)

The guide's **Browser JavaScript** profile is `OBJECT ERROR RUNTIME VALUE ENVIRON EVAL COMMAND` +
`ASYNCQ`. That set is the initial phase; delivery features are included because a wasm artifact is
useless unless it can be loaded from a page.

| Feature | Level | Initial phase |
|---|---|---|
| `OBJECT` `ERROR` `RUNTIME` `VALUE` `ENVIRON` `EVAL` `COMMAND` | Essential | **selected** |
| `ASYNCQ` | Profile | **selected** (browser has no synchronous option) |
| `ASYNCCMD` | Optional | **selected** — promoted into the initial phase by decision 6; a command that fetches from a server is a primary use case, and a Promise-returning `run` is the natural JS idiom |
| `STUBS` `PACKAGE` | Optional | **selected, minimal** — `wasm-bindgen` emits `.d.ts`; a `trunk` build and a runnable quick-start page (decision 7) |
| `STORE` `RECIPE` `UIUSE` `UIDEF` | Optional | deferred, Phase 2 must not preclude them |
| `MODULE` `POLYGLOT` `WEBSERV` `WEBAPI` | Optional | `NA` for this milestone (`WEBSERV`/`WEBAPI` are server-side; reasons recorded in Phase 2) |

## Core Interactions

### Query System
No change to the language. Query/Key strings are always routed through `liquers_core::parse` —
never reparsed in JavaScript — and exposed as `Query`/`Key` wrapper classes with parse/encode.

### Store System
Initial phase uses the existing in-memory async store; `STORE` (a JS-defined store, and
localStorage/IndexedDB backends) is deferred. Phase 2 must keep the environment's store injectable.

### Command System
Adds a JS command backend: a `CommandExecutor` adapter plus a callable registry, so a JS object
carrying metadata and a `run` function becomes a real command with real `CommandMetadata`, planned
and executed like a Rust command. Rust and JS commands coexist in one registry. `run` may return a
value or a Promise (decisions 3 and 6).

### Asset System
Reuses `AssetManager`/`AssetRef` unchanged — on wasm this is the `ImmediateAssetManager` selected by
the completed [async-wasm-refactor](../async-wasm-refactor/DESIGN.md). Evaluation returns a Promise.

### Value Types
Needs a browser *value type* that can hold an **opaque `JsValue`** alongside ordinary Liquers values,
plus a bidirectional structural bridge (null/bool/number/string/`Uint8Array`/array/object). Reuses
`CombinedValue<SimpleValue, JsExt>` rather than a new enum, after relaxing `ValueExtension` — see
decision 1. Structural conversion is the default and opaque retention is opt-in; identity is not
guaranteed, so structural conversion is always an available fallback — see decision 2.

### Web/API
None. The public surface is `#[wasm_bindgen]` classes and functions, not HTTP routes.

### UI
None in the initial phase. The `webui` DOM backend already in `liquers-lib` is the intended target of
a later `UIUSE`/`UIDEF` milestone.

## Crate Placement

New workspace crate **liquers-web**, at the end of the dependency flow parallel to `liquers-axum`,
depending on `liquers-core` and (for `CombinedValue` and later `webui`) `liquers-lib` with default
features off. Named `liquers-web` per the request; it supersedes the never-implemented `liquers-wf`
sketch in [`specs/liquers-wf`](../liquers-wf/phase1-high-level-design.md), whose open questions 1, 2
and 4 are now largely answered by the async-wasm-refactor.

## Decisions

1. **Value type composition — decided.** `liquers_lib::value::extended::ValueExtension`
   (`liquers-lib/src/value/extended.rs:12`) currently requires `Send + Sync + 'static`, which an
   opaque `JsValue` cannot satisfy. **Relax it to `MaybeSend + MaybeSync + 'static`** and reuse
   `CombinedValue`; do not define a competing standalone value type. `liquers-web` then supplies
   `JsExt: ValueExtension` and uses `WebValue = CombinedValue<SimpleValue, JsExt>`.

   The change is a one-line supertrait edit with a verified-small blast radius:
   - `liquers_core::value::ValueInterface` already uses exactly these markers
     (`liquers-core/src/value.rs:49-50`), so this is the async-wasm-refactor's own convention
     finishing a spot it missed — not a new relaxation of the model.
   - `ValueExtension` has exactly **one** implementor today, `ExtValue`
     (`liquers-lib/src/value/mod.rs:113`), and it satisfies the weaker bound trivially.
   - Every use site writes the bound as `Ext: ValueExtension` and lives inside `extended.rs`, so no
     call site or generic parameter changes.
   - The `ValueInterface for CombinedValue<B, E>` impl (`extended.rs:75`) still forces
     `E: Send + Sync` on native through `MaybeSend`/`MaybeSync`, so native multi-threaded behaviour
     is unchanged; only `wasm32` gains the vacuous bound.

   Executed as a Phase 4 step, ahead of the value-bridge work that depends on it.

   **Corrected during M1 execution — this estimate was low.** The bounds *are* local to
   `extended.rs`, but making `Value` non-`Send` on `wasm32` propagates to anything storing a
   `Value` under a hard bound. Two further `liquers-lib` traits had to be relaxed the same way:
   `ui::element::UIElement` (three implementors hold a `Value` behind an `RwLock`) and then
   `ui::app_state::AppState` (stores `dyn UIElement` handles). The chain is
   `ValueExtension → UIElement → AppState` and stops there. Nothing on native could have shown
   this; only the `wasm32` build configuration did. See Phase 2, "Why the bound works across
   languages", and Phase 4's M1 execution record.

   **Naming note:** `JsExt` below is Phase 1 vocabulary. Phase 2 superseded it — the opaque value
   is carried by an ungated, language-neutral `ExtValue::Foreign { Arc<dyn ForeignValue> }`, with
   `JsOpaque` implementing `ForeignValue` in `liquers-web`. `CombinedValue` is still reused, via
   `liquers_lib::value::Value`, so the substance of this decision is unchanged.

2. **Opaque value ownership — decided.** Structural conversion is the default; a *language value*
   becomes opaque only through an **explicit opt-in**, and an opaque value retains the `JsValue`
   **directly** (`JsExt::Opaque(JsValue)`), not through a registry of IDs. Callables are the
   exception and keep the guide's registry-plus-stable-ID pattern, owned by `COMMAND`.

   **Liquers does not guarantee `roundtrip(obj) === obj`.** The framework guarantee is that the same
   query evaluates to the same value when it is neither volatile nor expired — a statement about
   query determinism, not about preserving a JavaScript object graph. Direct retention is therefore
   an *optimization* (avoid a lossy copy; keep a live object usable inside the session), not a
   contract, which is what makes **structural conversion a legitimate fallback** rather than a
   broken promise.

   Consequences Phase 2 must carry through:
   - **Identity may hold in practice and must not be relied upon.** A cached non-volatile query
     returns the identical JS object from the in-memory asset map on re-evaluation, so `===` will
     often be observably true. It is documented as incidental, not promised (`VALUE10`).
   - **Opaque values are immutable by discipline, not by enforcement.** A retained `JsValue` is
     mutable and shared, so page code or a later command can mutate it *after* the asset is cached
     and retroactively change that asset's value. Structural conversion is immune (a copy is a
     snapshot); direct retention is not. This looseness is **accepted deliberately**: the older
     Python implementation permitted the same thing, discipline proved sufficient, and it caused
     fewer problems in practice than expected. For the browser — simpler, often frontend
     applications — flexibility and simplicity outrank strong guarantees. The native/backend side
     (`liquers-axum`) has the opposite tradeoff and should not inherit this posture.
   - **Mutable state belongs in the language runtime, not in a liquers value.** A JS command can
     close over a module-scope variable, `window`, a DOM node, or IndexedDB through `web-sys`, which
     keeps mutation explicit instead of hidden inside a value. Liquers values therefore do not need
     to be mutable, which is what makes the convention above cheap. Consequence: such state is
     invisible to dependency tracking, so a command reading it should be declared `volatile` or its
     assets are cached against state Liquers cannot see.
   - **Opaque retention is session-and-realm-scoped.** Anything leaving that scope — persistence
     through a store, a worker or second realm, a codec boundary — structurally converts if it can
     and raises a typed error if it cannot.
   - **Lifetime is automatic.** `JsValue` clone/drop is a refcount on the wasm-bindgen heap table,
     so no hand-rolled refcounting in `Clone`/`Drop` and no ambient registry. Note the cost: while
     an opaque value sits in `asset_data.data`, it pins whatever it references (a DOM subtree, a
     large `ArrayBuffer`) for the cache's lifetime. The explicit opt-in keeps that from happening
     by accident.
   - **Serialization fails cleanly by default** (`VALUE06`). The core already absorbs this:
     `assets.rs:2994` falls back to `Version::from_time_now()` and `assets.rs:3016` to
     `store.set_metadata(...)`, so a non-serializable value degrades instead of breaking
     evaluation — at the cost of a time-based version, which makes such assets look freshly changed
     to dependency tracking. Degrading to structural conversion at the serialization boundary is
     **opt-in only**: a class instance silently returning as a plain object after a cache eviction
     is a bad debugging experience, and silent inference is ruled out by decision 3.
   - **Opaque retention is also the fast path, which is why the opt-in must be ergonomic.**
     Structural conversion of a compound object costs roughly one boundary crossing per property
     (each `Reflect` access is a call) plus a UTF-16→UTF-8 re-encode per string, so it is O(size);
     opaque retention is one heap-table slot, O(1). For a value that passes *through* Rust untouched
     — the common JS-command-to-JS-command frontend pipeline — conversion is pure overhead.

     This does **not** flip the default. Primitives must convert or the framework stops working: an
     opaque string has no `identifier`, `type_name`, media type or filename, fails `as_bytes` so it
     never persists, and no Rust command can consume it. And any Rust command that does inspect the
     value pays the conversion anyway, only later and possibly repeatedly. The magnitude of the
     win is a *hypothesis to be measured* in Phase 3 (a benchmark crossing the boundary with a large
     object, both ways), not an assumption to design on.

     Wasm **size** is not a factor in this choice: the conversion layer exists either way, so opaque
     support adds an enum variant rather than a subsystem. The size levers are `liquers-lib` default
     features, panic/formatting machinery, and `wasm-opt` — a packaging-milestone concern
     (decision 7).

3. **Command declaration — decided.** Meaningful defaults wherever possible. **Required:** the
   command `name` and the JS function. **Everything else is defaulted**, so the minimal declaration
   stays one line (`COMMAND09`). Argument specifications are required *unless* they can be inferred
   from the JS function.

   **Resolved in Phase 2: both paths, with inference restricted to a verified-safe subset.**
   Explicit `arguments` is the reliable path and always wins. When absent, a regex parse of
   `Function.prototype.toString()` infers names — but *only* when every parameter is a plain
   identifier and the token count equals `fn.length`; defaults, rest, destructuring, and
   bound/native functions are refused with a specific `ParameterError` rather than mangled. The one
   undetectable case is minification, which yields correct arity and wrong names; since Liquers
   binds arguments **positionally**, that degrades labels and documentation rather than behaviour,
   and a heuristic `console.warn` surfaces it. See Phase 2, "Argument declaration, and simple
   inference over a verified-safe subset".

4. **Environment lifecycle — decided.** Support **both a global singleton and explicit instances**,
   with the singleton as the first priority and the documented default path. Explicit instances
   exist for embedders and for test isolation (`ENVIRON05`).

5. **Reentrancy — decided.** A JS command that calls `evaluate` **evaluates immediately/inline**.
   No heavy long-running background computation is expected in a browser, so the tradeoffs are
   accepted. Phase 2 must still show the inline path cannot self-deadlock: `ImmediateAssetManager`
   guards its maps with `std::sync::Mutex`, and on a single-threaded event loop a guard held across
   an `await` that re-enters the manager would deadlock rather than block. Decision 6 helps here —
   an async JS command yields to the event loop instead of blocking it (`RUNTIME04`).

6. **Async commands — decided.** Promises are supported **from the start**; `ASYNCCMD` joins the
   initial phase (see the feature matrix). Many commands are expected to fetch data from a server,
   so async is a primary case rather than an extension. Sync and async commands must remain
   distinguishable in metadata (`ASYNCCMD06`).

7. **Packaging — decided.** Start with **trunk**. The artifact must be usable by including the wasm
   library from a CDN or a plain website — i.e. a plain `<script type="module">` page, not only a
   bundler. **Embedded/single-file wasm** (everything inlined into one HTML file) is a wanted second
   delivery form; `npm`/`wasm-pack` and other packaging come later. For the initial phase trunk is
   sufficient.

   Reference for the single-file target: [`orest-d/gymlog`](https://github.com/orest-d/gymlog).
   Its build mechanism was not established during Phase 1 — the repository appears to be
   Dioxus-based rather than trunk-based, so its approach must be examined directly at the packaging
   milestone rather than assumed to transfer.

## Open Questions

All seven Phase 1 open questions are resolved above. Questions carried into Phase 2 as design work
rather than as unknowns:

- The honest limit of argument inference from a JS function, given no types, minifier-mangled
  parameter names, and the requirement that inference stay visible (decision 3).
- A demonstration that inline re-entrant evaluation cannot self-deadlock against
  `ImmediateAssetManager`'s `std::sync::Mutex` on a single-threaded event loop (decision 5).
- The exact opt-in surface and typed error for opaque values leaving session/realm scope
  (decision 2).

## References

- [LANGUAGE-INTEGRATION_GUIDE.md](../LANGUAGE-INTEGRATION_GUIDE.md) — §4 profiles, §5 feature
  contracts, §6 Browser JavaScript, §7 checklist, Appendix A reference tests
- [`specs/liquers-wf/phase1-high-level-design.md`](../liquers-wf/phase1-high-level-design.md) — older sketch
- [`specs/async-wasm-refactor/DESIGN.md`](../async-wasm-refactor/DESIGN.md) — `MaybeSend`/`MaybeSync`,
  `BoxFuture`, `ImmediateAssetManager`; the wasm foundation this crate stands on
- [`specs/webui/`](../webui/), `liquers-lib/src/ui/web/` — DOM backend for a later UI milestone
- `liquers-core/src/maybe_send.rs`, `liquers-lib/src/value/extended.rs`, `liquers-lib/examples-web/`
