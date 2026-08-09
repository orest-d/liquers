---
id: LIQUERS-WEB-STORE
kind: design
title: Browser stores for liquers-web
status: in_review
phase: high-level
area: [web, store/config, core/store]
gh_pr: []
issues: [WEB-NATIVE-IO-TIER2]
created: 2026-08-09
superseded_by:
---
# Browser stores for liquers-web Design Tracking

**Created:** 2026-08-09

Implements the `STORE` feature of `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` for `liquers-web`,
which `specs/design/liquers-web/` explicitly deferred.

## Phase Status

- [x] Phase 1: High-Level Design (awaiting approval)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Notes

**Phase 1 scope:** three stores — `LocalStorageStore` (full `AsyncStore`), `FetchStore` (read-only
HTTP, `url_prefix` + key minus `key_prefix`, metadata inferred from extension and response media
type), and the existing `AsyncStoreRouter` driven by a `liquers_store::config`-shaped declaration.

**Phase 1 findings:**
- `WebEnvironment` is built on `NoAsyncStore` today, so every `-R/` query in the browser fails.
  This design is what makes resource queries work at all in a page.
- `AsyncStoreRouter` (`liquers-core/src/store.rs:1770`) is already `?Send` on wasm and needs no
  change — routing is "first store whose `key_prefix()` matches *and* whose `is_supported()`
  returns true". `is_supported` defaults to **false**, so both new stores must override it.
- `liquers-store` cannot be a dependency of `liquers-web`: it pulls OpenDAL. But its config module
  is pure serde over dependencies `liquers-core` already has, so the config types can be shared by
  moving rather than duplicating them.
- `http`/`https` are already OpenDAL store types (`liquers-store/src/config.rs:286`), so one
  configuration document can mean the same thing natively (OpenDAL `services-http`) and in the
  browser (`fetch`).
- `liquers_core::media_type::file_extension_to_media_type` already exists and is what
  `MetadataRecord` uses, so extension-based inference in the fetch store reuses it.

**Decision (fetch, not reqwest).** `reqwest`'s wasm backend wraps `web_sys::fetch` and adds `http`,
`bytes`, `tower-service`, `url`, `serde_urlencoded` for a store that only issues `GET`. The reuse
argument does not apply: the native read-only HTTP store already exists as OpenDAL's `http`
service, so a reqwest store would duplicate it rather than be reused. `web-sys` is already a
`liquers-web` dependency.

**Phase 1 open questions** (7) are listed in the phase document; the substantive one is Q1 —
whether `STORE` here also includes a JS-*implemented* store adapter, which is the guide's literal
reading of the feature.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
