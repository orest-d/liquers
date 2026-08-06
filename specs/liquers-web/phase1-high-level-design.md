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
decision 1.

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

   Executed as a Phase 4 step, ahead of the `JsExt` work that depends on it.

3. **Command declaration — decided.** Meaningful defaults wherever possible. **Required:** the
   command `name` and the JS function. **Everything else is defaulted**, so the minimal declaration
   stays one line (`COMMAND09`). Argument specifications are required *unless* they can be inferred
   from the JS function.

   Phase 2 must resolve how far inference can honestly go, because JavaScript carries no type
   information: `Function.prototype.length` yields arity only, and parameter *names* are available
   only by parsing `Function.prototype.toString()`, which a minifier or bundler is free to mangle.
   Inferred arguments therefore get a permissive argument type, and the guide's rule stands — do not
   infer *silently*: inference results must be visible through metadata inspection after
   registration (`COMMAND05`), and an explicit `arguments` array always wins.

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

2. **Opaque value ownership.** How is a retained `JsValue` kept alive, compared, and reported when
   serialization is attempted (`VALUE06`)? Identity retention promised or not? → Phase 2.

## References

- [LANGUAGE-INTEGRATION_GUIDE.md](../LANGUAGE-INTEGRATION_GUIDE.md) — §4 profiles, §5 feature
  contracts, §6 Browser JavaScript, §7 checklist, Appendix A reference tests
- [`specs/liquers-wf/phase1-high-level-design.md`](../liquers-wf/phase1-high-level-design.md) — older sketch
- [`specs/async-wasm-refactor/DESIGN.md`](../async-wasm-refactor/DESIGN.md) — `MaybeSend`/`MaybeSync`,
  `BoxFuture`, `ImmediateAssetManager`; the wasm foundation this crate stands on
- [`specs/webui/`](../webui/), `liquers-lib/src/ui/web/` — DOM backend for a later UI milestone
- `liquers-core/src/maybe_send.rs`, `liquers-lib/src/value/extended.rs`, `liquers-lib/examples-web/`
