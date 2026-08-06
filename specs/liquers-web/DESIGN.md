# liquers-web Design Tracking

**Created:** 2026-08-06

**Status:** In Progress

## Phase Status

- [x] Phase 1: High-Level Design (drafted; awaiting user approval)
- [ ] Phase 2: Solution & Architecture
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
next, npm later (7). Only question 2 (opaque `JsValue` ownership and identity) is open.

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

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
