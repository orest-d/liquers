---
id: LIQUERS-WEB-STORE
kind: design
title: Browser stores for liquers-web
status: in_review
phase: implementation
area: [web, store/config, core/store]
gh_pr: []
issues: [WEB-NATIVE-IO-TIER2, LANGUAGE-GUIDE-STORE-SCOPE-INCOMPLETE]
created: 2026-08-09
superseded_by:
---
# Browser stores for liquers-web Design Tracking

**Created:** 2026-08-09

Implements the `STORE` feature of `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` for `liquers-web`,
which `specs/design/liquers-web/` explicitly deferred.

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (approved)
- [x] Phase 3: Examples & Testing (approved)
- [x] Phase 4: Implementation Plan (awaiting approval)
- [ ] Implementation Complete

## Notes

**Phase 1 scope:** four stores — `LocalStorageStore` (full `AsyncStore`), `FetchStore` (read-only
HTTP, `url_prefix` + key minus `key_prefix`, metadata inferred from extension and response media
type), `JsStore` (a JavaScript object adapted to `AsyncStore`), and the existing `AsyncStoreRouter`
driven by a `liquers_store::config`-shaped declaration.

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

**User decisions closing all Phase 1 open questions.** `JsStore` added as a fourth store, so the
design covers both readings of `STORE` — the guide's literal one (a store written in the language)
and the requested one (stores the integration provides) (1). The config **stays in
`liquers-store`**, which gains an `opendal` feature so `liquers-web` can depend on it with
`default-features = false`; nothing moves to `liquers-core` (2). localStorage stores UTF-8-clean
bytes directly and everything else base64, with a configurable byte quota, unlimited by default,
whose breach is `Error::key_write_error` (3). Directories are backed by an explicit index following
`AsyncMemoryStore`, not derived by prefix scanning, so `makedir` and `removedir` are meaningful (4).
`FetchStore` listing comes from a configured known-key set, with page crawling as future work (5).
Keys with `..` or empty segments are refused with `key_not_supported` — that is `STORE05` — and the
separate, accepted limitation is that not every URL is representable as a `Key`, with a key→URL
mapping held in reserve (6). No `${VAR}` expansion on wasm; the syntax stays unclaimed for
JavaScript-supplied variables later (7).

**A guide gap came out of question 1.** §5 `STORE` asks only about adapting a *language value* to
`AsyncStore`. It asks nothing about stores the *integration itself* provides, nor about store
configuration and composition — which together are most of this design, and which none of
`STORE01`–`STORE07` exercises. Filed as `LANGUAGE-GUIDE-STORE-SCOPE-INCOMPLETE`, deliberately not
fixed here: amending the guide from inside the first design that trips over it would make the
design its own conformance definition.

**On question 3, the reliability requirement drove the mechanism.** The encoding is *recorded* in a
versioned envelope at write time, never inferred at read time, and it is *chosen by a check*
(`from_utf8` succeeding is a proof) rather than by a `Metadata` hint (`type_name` or media type is
a guess, and a wrong guess corrupts silently — `text/plain` may hold invalid UTF-8). The check
costs a linear scan of bytes already being copied, so the hint would not even be faster.
Losslessness of valid UTF-8 through the Rust `String` → JS `DOMString` → back path is pinned by a
Phase 3 round-trip corpus rather than asserted.

**Phase 1 has no open questions remaining.** One question *emerged* from question 2 and belongs to
Phase 2: `store_builder::create_store` is a closed `match` on type strings, so `liquers-web` cannot
add `localstorage` / `http` / `js` to it from outside. Either a factory-registration seam in
`liquers-store` — which would also let one configuration document select an OpenDAL `http` store
natively and a `fetch` store in the browser — or a separate `liquers-web` builder that delegates.

**Verified while closing question 2:** `liquers-store/src/config.rs` imports only `std`, `serde` and
`liquers_core`, so it needs no gating at all; the OpenDAL coupling is confined to
`opendal_store.rs` and one branch of `store_builder.rs`. `create_filesystem_store` needs the same
target gate `AsyncFileStore` already carries (`liquers-core/src/store.rs:816`). No consumer is
affected: `liquers-axum` takes default features and `liquers-lib` uses `liquers-store` only as a
dev-dependency.

**Phase 2 outcome.** No change to `AsyncStore`, `AsyncStoreRouter`, `Key`, `Metadata` or the query
language. Two additive changes land outside `liquers-web`: `liquers-store` gains an `opendal`
feature, and its builder gains a `StoreFactory` seam consulted *before* the built-in types — which
`liquers-web` needs because `http` is already a built-in OpenDAL type and the browser must override
it, and which is what makes one configuration document mean the same thing on both targets.

Decisions worth carrying forward:
- **The `localStorage` index is derived by scanning, never persisted separately.** A second source
  of truth can disagree with the entries after a partial write. Empty directories are the one thing
  not derivable, so they get marker entries — which is also what makes `makedir` survive a reload.
- **The data envelope is a two-character prefix (`1u` / `1b`), not JSON.** JSON would escape the
  payload a second time and cost a parse on every read, for two fields that will not grow.
- **Metadata precedence is extension first, response `Content-Type` second** — the inverse of the
  usual web rule, because Liquers dispatches deserialization on `data_format`, and a static server
  labelling everything `text/plain` would break every command downstream of a fetched asset.
- **Absent optional `JsStore` methods error rather than take the trait default.** `contains` and
  `listdir` default to `false` and `[]`, so a half-written store would look like an empty one and
  `STORE03` would pass vacuously. `isSupported` is the exception, since a store answering "no" is
  invisible to the router.
- **The store joins the existing rebuild path** rather than getting a swappable indirection. A
  `SwappableStore` would preserve the asset cache, but assets computed against the old store are
  stale the moment it is replaced and there is no invalidation path for them — discarding is
  correct, so the rebuild is a feature.
- **`LocalStorageStore` never awaits**, because Web Storage is synchronous. That is load-bearing:
  it is why `RefCell` is sound, and why `STORE06` has a real assertion (no interleaving point
  exists, so last-write-wins is a fact rather than one of two acceptable outcomes).

**A `liquers-core` defect was found while specifying the key guard:** `..` is a valid
`ResourceName`, and `AsyncFileStore::key_to_path` is `path.push(key.to_string())`, so a key can
escape the store root — reachable from a query and therefore over `liquers-axum`. Filed as
`STORE-FILESTORE-PATH-TRAVERSAL` (P1). Not fixed here; the guard lives in `liquers-web` for now and
the issue proposes hoisting a shared version into `liquers_core::store`.

**Phase 2 question 8 closed by the user:** store-manipulation commands are out of scope. They
belong in `liquers-lib` so every target gets them rather than one host at a time, and folding them
in would have turned "the browser can have a store" into "queries can mutate stores". Filed as
`STORE-COMMAND-NAMESPACE-MISSING`. **Phase 2 has no open questions remaining.**

**Phase 3 outcome — 41 tests in five tiers.** Of the 21 prescribed cells (`STORE01`–`STORE07` ×
three stores), **19 are required and 2 are `NA`** — both `FetchStore`, both because it has no write
path, both with the reversing condition recorded (a `PUT`-capable variant makes `STORE04` and
`STORE06` required immediately). Four further tests, `STORE08`–`STORE11`, cover what the guide's
inventory does not reach; the IDs are the ones `LANGUAGE-GUIDE-STORE-SCOPE-INCOMPLETE` proposes and
are adopted provisionally, to be renamed if the guide lands different numbers.

**The harness analysis changed the architecture, which is what the phase is for.**
`localStorage` does not exist under Node and `web_sys::window()` returns `None` there, so the
`LocalStorageStore` contract tests must run in a browser. Rather than accept that for everything,
the design now pulls **pure functions** out of each store — `encode_envelope`/`decode_envelope`,
the key guard, URL construction, and `infer_metadata(key, content_type, content_length)` — so the
logic that can silently corrupt or misroute data is tested in the fast Node loop, and only plumbing
needs a browser. That is a requirement on the implementation, not a test-plan detail, and Phase 2
was amended to carry it.

**A Phase 2 correction came out of Phase 3.** Phase 2 specified acquiring `fetch` from
`web_sys::Window` with a `WorkerGlobalScope` fallback. Neither exists under Node, which would have
forced every `FetchStore` test into a browser for no reason. Corrected to `js_sys::global()` +
`Reflect` + `apply`, which works in a window, a worker *and* Node, is less code than two web-sys
types, and drops two web-sys features.

**Non-vacuous assertions were specified test by test**, per the guide's warning. The one worth
naming: `STORE06` does not accept "either the last write won or one of them did" — because
`LocalStorageStore` never awaits, there is no interleaving point, so the test asserts the value
equals the *last* write specifically and is one byte long. It becomes a tripwire the day someone
makes a storage call async.

**Phase 4 outcome — 23 steps in 6 milestones, 44 tests.** M1 (`liquers-store`: the `opendal`
feature and the `StoreFactory` seam) is the only milestone touching an existing crate and is
separately gated: if it cannot be made green, the design's foundation is wrong and nothing should
be built on it. Everything after it is new code in a crate excluded from `default-members`, so
rollback is a file deletion.

The ordering principle is that **the tests which catch silent data corruption run before the code
that needs a browser exists**: M2 delivers the envelope codec and the key guard as pure functions
with their full corpus, in the fast Node loop, before either store is written. M3 and M4 are
independent.

Four steps carry most of the risk, and the plan says so where an implementer will see it: Step 18
(a missed thread-local replay silently drops every `js` store on rebuild), Step 12 (the `RefCell`
borrow rule), Step 1 (the cfg cross-product), and Step 16 (a permissive default would make
`STORE03` pass vacuously).

**Step 1 is the only change that is not freely reversible**, since it alters a published crate's
feature set. Mitigated by putting `opendal` in `default`, so a consumer who does nothing sees no
change; within this workspace the set of consumers using `default-features = false` is empty.

**Partial delivery is a defined outcome:** if M5 is abandoned, M1-M4 still leave `liquers-store`
usable from wasm and two working stores, and the remainder becomes an issue rather than a
half-finished design (`DOCS_STRUCTURE_GUIDE.md` §5.6).

**Review corrected a test-count error** carried from Phase 3: the roll-up said 41 where its own
table summed to 44. Both documents now say 44 (4 native + 29 Node + 6 Chromium + 5 Playwright).

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
