# liquers-web Design Tracking

**Created:** 2026-08-06

**Status:** In Progress

## Phase Status

- [x] Phase 1: High-Level Design (all 7 questions decided)
- [x] Phase 2: Solution & Architecture (reviewed; Option Y decided, awaiting approval)
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Notes

**Phase 1 scope:** the LANGUAGE-INTEGRATION_GUIDE "Browser JavaScript" profile —
`OBJECT ERROR RUNTIME VALUE ENVIRON EVAL COMMAND` + `ASYNCQ`, plus `ASYNCCMD` (promoted by
user decision: server-fetching commands are a primary case) and minimal `STUBS`/`PACKAGE`
so the artifact is loadable. `STORE`, `RECIPE`, `UIUSE`, `UIDEF` deferred;
`MODULE`, `POLYGLOT`, `WEBSERV`, `WEBAPI` are `NA` for this milestone.

**User decisions closing Phase 1 open questions 1 and 3-7:** relax `ValueExtension` and reuse
`CombinedValue` (1); `name` + JS function required, everything else defaulted, argument specs
inferred from the function only where honestly possible (3); global singleton first plus
explicit instances (4); re-entrant evaluation runs inline, tradeoffs accepted (5); Promises
supported from the start (6); trunk first, CDN/plain-page loadable, single-file wasm wanted
next, npm later (7).

**Question 2 (opaque `JsValue`) closed — all Phase 1 questions resolved.** Structural conversion
by default, opaque retention opt-in, `JsValue` held directly (registry-plus-ID reserved for
callables under `COMMAND`). Liquers guarantees query determinism, **not** `roundtrip(obj) === obj`,
so direct retention is an optimization and structural conversion is a legitimate fallback.
Follows from that: `===` may hold incidentally but is not promised; opaque values are immutable
by discipline rather than enforcement (the Python implementation allowed the same and it caused
fewer problems than expected — the browser deliberately trades guarantees for flexibility, and
`liquers-axum`/backend must not inherit that posture); mutable state belongs in the language
runtime (`window`, closures, IndexedDB via `web-sys`) and such commands should be `volatile`
since that state is invisible to dependency tracking; retention is session-and-realm-scoped,
converting or erroring at the boundary; serialization fails cleanly, which the core already
absorbs (`assets.rs:2994`/`:3016`).

Opaque retention is also the **fast path** — structural conversion is O(size) boundary crossings
(one `Reflect` call per property, UTF-16→UTF-8 per string) versus O(1) for a heap-table slot — so
the opt-in must be ergonomic. It still does not flip the default, because primitives must convert
or `identifier`/`type_name`/media type/`as_bytes` and every Rust command break. Magnitude is a
**Phase 3 measurement**, not an assumption. Wasm *size* is not a factor here (the conversion layer
exists either way); size belongs to the packaging milestone.

**Phase 1 findings:**
- The wasm foundation already exists: `MaybeSend`/`MaybeSync`, `BoxFuture`, and
  `ImmediateAssetManager` were delivered by `specs/async-wasm-refactor` (complete), and
  `liquers-core` + `liquers-lib` already compile to `wasm32-unknown-unknown` with a Playwright
  e2e proof (`liquers-lib/examples-web/ui_spec_demo`).
- **Resolved (user decision):** `liquers_lib::value::extended::ValueExtension` still requires
  `Send + Sync + 'static` (`liquers-lib/src/value/extended.rs:12`), which an opaque `JsValue`
  cannot satisfy. It will be **relaxed to `MaybeSend + MaybeSync + 'static`**, matching
  `ValueInterface` (`liquers-core/src/value.rs:49-50`), and `liquers-web` reuses `CombinedValue`
  with a `JsExt` extension. One implementor (`ExtValue`), all bounds local to `extended.rs`,
  native behaviour unchanged.
- `liquers-wf` was designed but never implemented (not a workspace member); `liquers-web`
  supersedes it.

**Phase 2 outcome.** No new `Environment` and no new `CommandExecutor` are needed —
`DefaultEnvironment` is already generic over the value type and cfg-selects `ImmediateAssetManager`
on wasm, and the executor closure aliases already drop `Send`/`Sync` there, so a JS command is an
ordinary async command whose closure owns a `js_sys::Function`.

**Option Y decided:** the opaque JS value is a cfg-gated `ExtValue::Js` variant in `liquers-lib`
(14 match sites to extend), so `liquers-web` reuses `liquers_lib::value::Value` and the existing
wasm-viable command library. Precedent: `liquers-py` already carries `Py { value: Py<PyAny> }`.
**Extensibility requirement (user):** the bridge, command adapter and Promise bridge stay generic
over `E`/`V` behind a `JsValueBridge` trait, with the concrete type appearing only in the exported
`#[wasm_bindgen]` wrappers, so a downstream crate can supply its own value type and environment.
Most third-party JS types need no custom value type at all — the opaque path covers them.

**Argument inference rejected on evidence.** Measured with `node`: `fn.length` collapses at the
first default/rest parameter, destructured parameters have no name, `bind` yields `[native code]`,
and no parameter-name reflection API exists. Explicit `arguments` declaration is required whenever a
command takes parameters; `fn.length` serves only as a one-directional check for a missing
declaration. Consequence: no JavaScript parser is linked into the wasm artifact.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
