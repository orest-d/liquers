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
`OBJECT ERROR RUNTIME VALUE ENVIRON EVAL COMMAND` + `ASYNCQ`, plus minimal `STUBS`/`PACKAGE`
so the artifact is loadable. `STORE`, `RECIPE`, `ASYNCCMD`, `UIUSE`, `UIDEF` deferred;
`MODULE`, `POLYGLOT`, `WEBSERV`, `WEBAPI` are `NA` for this milestone.

**Phase 1 findings:**
- The wasm foundation already exists: `MaybeSend`/`MaybeSync`, `BoxFuture`, and
  `ImmediateAssetManager` were delivered by `specs/async-wasm-refactor` (complete), and
  `liquers-core` + `liquers-lib` already compile to `wasm32-unknown-unknown` with a Playwright
  e2e proof (`liquers-lib/examples-web/ui_spec_demo`).
- **Blocker candidate:** `liquers_lib::value::extended::ValueExtension` still requires
  `Send + Sync + 'static` (`liquers-lib/src/value/extended.rs:12`), which an opaque `JsValue`
  cannot satisfy. Either relax it to `MaybeSend`/`MaybeSync` or define a standalone value type
  in `liquers-web` — Phase 2 decision (open question 1).
- `liquers-wf` was designed but never implemented (not a workspace member); `liquers-web`
  supersedes it.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
