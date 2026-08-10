---
id: LIQUERS-WEB-STORE
kind: design
title: Browser stores for liquers-web
status: complete
area: [web, store/config, core/store]
gh_pr: []
issues: [WEB-NATIVE-IO-TIER2, LANGUAGE-GUIDE-STORE-SCOPE-INCOMPLETE, WORKSPACE-SERDE-DERIVE-UNDECLARED, CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON, WEB-LIQUERSERROR-NOT-CONSTRUCTIBLE, STORE-FILESTORE-PATH-TRAVERSAL, STORE-COMMAND-NAMESPACE-MISSING, CORE-IMMEDIATE-MANAGER-KEYED-RECURSION, LIB-RECIPE-PROVIDER-PANIC]
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
- [x] Phase 4: Implementation Plan (approved 2026-08-09 — all four phases approved)
- [x] Implementation Complete — **M1-M6 done ✅** (Steps 1-23)

## Implementation status

| Milestone | Scope | Result |
|---|---|---|
| M1 | `liquers-store`: `opendal` feature, `StoreFactory` seam | ✅ 4 factory tests; 10 build configurations green; no regression |
| M2 | Envelope codec, key guard | ✅ 9 tests green under Node; 106 wasm tests total, no regression |
| M3 | `FetchStore` | ✅ 7 pure tests under Node; store compiles and is wired |
| M4 | `LocalStorageStore` | ✅ 10 tests green in Chromium |
| M5 | `JsStore`, factory, environment wiring | ✅ 16 tests green under Node |
| M6 | e2e, stubs, documentation | ✅ 6 e2e green, 5 `fixme` (blocked by a core defect); stubs green |

**M1 EXECUTED ✅.** `opendal` is optional and in `default`; `opendal_store` and the OpenDAL
dispatch arm are gated; `create_filesystem_store` follows `AsyncFileStore`'s existing wasm32 gate;
`StoreFactory` and `StoreRouterBuilder::with_factory` added, consulted before the built-ins.
Gates: `cargo test -p liquers-store` 26 passed and 22 passed without `opendal`;
`bash scripts/check-build-matrix.sh` 10/10; `liquers-lib` 296+ green; `liquers-axum` 162 green.

**The milestone's own gate proved its point immediately.** `liquers-store` now compiles for
`wasm32-unknown-unknown` without OpenDAL, which is the dependency edge every later milestone
assumes — had it not, Phase 2 would have needed reworking before any browser code was written,
which is exactly why M1 was gated separately.

**A latent defect surfaced that had nothing to do with OpenDAL.** Removing OpenDAL from the graph
produced 13 errors in `config.rs`, a file the change never touched: `liquers-store` declared
`serde = "1.0.181"` without `features = ["derive"]` and imported the derive macros anyway,
compiling only because OpenDAL enabled `serde/derive` and Cargo unified it. Fixed here with a
comment recording why. `liquers-core`, `liquers-lib` and `liquers-axum` have the same undeclared
dependency and were left alone as out of scope — filed as `WORKSPACE-SERDE-DERIVE-UNDECLARED`.
The lesson generalises: a crate that survives only through feature unification is one
optional-dependency change away from an afternoon of confusing errors in an unrelated file.

`factory02` is worth keeping in mind during M5: it asserts that a factory claiming `http` beats
the OpenDAL built-in, using a call counter rather than "the router built successfully", because
the latter cannot tell which code path ran.

**M2 EXECUTED ✅.** `liquers-web/src/store/` created with `encoding.rs` (the versioned `1u`/`1b`
envelope) and `key_guard.rs`, plus the `liquers-store` and `base64` dependencies and the four new
web-sys features. Gates: 9 new tests green under Node, full `liquers-web` wasm suite 106 tests
green with no regression.

**Group 1 delivers 9 of its 12 tests here, not 12.** The other three are the `STORE05` cells for
`LocalStorageStore`, `FetchStore` and `JsStore` — they assert that each *store* calls the guard,
which cannot be written until the stores exist, so they land with M3/M4/M5. The guard's own
behaviour is fully covered now.

Two small additions to the plan, both worth keeping:
- **`keyguard04`** — the negative half. Without it `check_key` could refuse everything and the
  three refusal tests would still pass.
- **`keyguard05`** — the empty key must be *accepted*. It is the store root, which
  `AsyncStore::keys` reaches through `key_prefix()`, so refusing empty keys wholesale would break
  top-level listing. The guard rejects empty *segments*, not the empty key, and that distinction
  now has a test.

**`encoding02` exists because `encoding01` alone is not enough.** If the encoding selector
inverted and everything took the base64 path, the round trip would still succeed and the
recorded-encoding contract would be silently gone. `encoding02` asserts the tag actually records
the path taken.

**Environment note:** the Node loop needs `wasm-bindgen-test-runner` at *exactly* the crate's
`wasm-bindgen` version (0.2.127 here). It was absent in this container and a mismatch fails at
bindgen time rather than at compile time, which is the late, obscure failure the guide's harness
question 3 warns about.

**M3 EXECUTED ✅.** `FetchStore` with `key_to_url`, `infer_metadata`, `media_type_of` and
`directory_index` as free functions, plus the `AsyncStore` impl. Gates: 12 tests in
`store_pure_STORE` (7 new), Node loop green.

**M4 EXECUTED ✅.** `LocalStorageStore` — `{ns}/d|m|D/{key}` layout, index rebuilt by scanning,
quota accounting, full contract. Gates: **10 tests green in Chromium**, Node loop 113 tests green.

**M4's quota test found a real bug, which is why it was worth running rather than assuming.** The
first run failed: `set` writes *two* entries and the budget was checked per entry, so the metadata
entry could land while the data entry was refused — leaving an orphan metadata entry occupying
quota for a key with no value. `contains` still reported false, so nothing but the byte accounting
could have caught it. Fixed by budgeting **all of an operation's entries together**
(`ensure_budget` / `put_all`), and the test now asserts `used_bytes` is *exactly* unchanged after a
refusal rather than merely not smaller.

The same run showed the test's own quota was unrealistic: metadata dominates the budget for small
values — a `MetadataRecord` serializes to several hundred bytes whatever the payload — so a
200-byte quota cannot hold one entry. That is honest accounting, not a defect, and is now
documented on `ensure_budget`.

**Two harness facts that changed the repository, not just this design:**

1. **`run_in_browser` in any one file makes the *whole* crate's Node loop demand a WebDriver.** This
   design added the crate's first such file, and `cargo test -p liquers-web --target
   wasm32-unknown-unknown` began failing. Fixed with a `browser-tests` feature — off by default,
   mirroring `debug-handles` — so only tests that cannot work otherwise pay that cost. Documented
   in `liquers-web/README.md`.
2. **The chromedriver in this container is 147; the available Chromium is 141,** and ChromeDriver
   refuses across major versions. The matching driver could not be downloaded (the environment's
   network policy blocks the host). The tests were nevertheless *run and verified* by the route the
   README now records: `NO_HEADLESS=1` makes the runner serve the suite at `127.0.0.1:8000`, and
   Playwright drives it over CDP with no WebDriver at all.

**M5 EXECUTED ✅.** `JsStore`, `WebStoreFactory`, `build_router`, the `Store` wasm class, and the
environment wiring (`configureStore`, `registerStoreObject`, `store()`). Gates: 16 tests green
under Node; full Node suite **129 tests**, no regression.

**The riskiest step was verified by breaking it on purpose.** Phase 4 flagged Step 18 — a missed
thread-local replay silently dropping every `js` store when a rebuild happens — as the most
dangerous in the milestone. `store_survives_a_rebuild` covers it, and rather than trust that, the
`apply_store` call was temporarily removed from `rebuild_with` and the test **did** fail, then
passed again once restored. A tripwire nobody has seen trip is not yet a tripwire.

The fix that makes it safe is structural rather than careful: `apply_store` is called from *every*
path that builds an environment for the singleton, so a future rebuild path cannot forget it by
omission — it would have to actively skip it. `reset_global` clears the store thread-locals too,
or one suite would leak a store into the next.

**Two `liquers-core` defects surfaced, both filed, neither fixed here:**
- `Metadata::get_media_type` returns JSON-*quoted* strings for `LegacyMetadata`
  (`"\"text/plain\""`), because it uses `Value::to_string()` where the record branch uses the
  value — `CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON`. Compounded by `Metadata::from_json`
  silently falling back to `LegacyMetadata` for any *partial* document, which is how a page's
  `{media_type: "text/plain"}` ends up there. `liquers-web` normalizes partial metadata at the
  boundary (`store::metadata_from_js_value`) so no page has to know, but the core bug affects
  every other consumer.
- `LiquersError` has no JavaScript constructor, so a page cannot raise a *typed* error —
  `WEB-LIQUERSERROR-NOT-CONSTRUCTIBLE`.

**A Phase 3 example was wrong and is corrected.** Example 2 showed a page throwing
`new liquers.LiquersError("key_not_found", …)`. It cannot: there is no constructor. The protocol
now says absence is `undefined`, which is both the path that works and the better design — a throw
means *failure*, and conflating it with absence would make a broken store look like an empty one.

**M6 EXECUTED ✅.** Playwright fixtures and store suite, TypeScript declarations for `Store`,
`LiquersStoreConfig` and `LiquersStoreObject`, and documentation. Gates: 6 e2e tests green,
`check-stubs.sh` all green (`class Store` included), build matrix 10/10, `liquers-core` 448,
`liquers-lib` 15 suites, `liquers-store` 26, `liquers-axum` green, Node suite 129, browser suite 10.

**M6 found the defect that decides what this design actually delivers.** The end-to-end tests hang
where direct store access succeeds, because **`-R/` keyed evaluation recurses forever under
`ImmediateAssetManager`** — `get` runs the asset inline, and `evaluate_recipe` calls `get` on its
*own* key to check identity, so the guard `asset.id() == self.id()` sits one line after the call
that never returns. In wasm the stack overflow kills the instance and the `Promise` never settles.
Filed as `CORE-IMMEDIATE-MANAGER-KEYED-RECURSION` (P1). Diagnosed from a Chromium stack trace, not
inferred.

So: **the four stores work and are verified; queries that reach them through the asset manager do
not.** That is the honest statement of what shipped, and it is recorded in `liquers-web/README.md`
under Known limitations. The five blocked e2e tests are kept as `fixme` rather than deleted — they
are the regression guard the issue asks for, and removing the marker will prove the fix.

**Two more defects on the same path, both found by running the tests rather than reading:**
- `DefaultEnvironment::get_recipe_provider` **panics** when none is configured, and every `-R/`
  query reaches it — so before this milestone a resource query aborted the wasm instance outright.
  Worked around by calling `with_default_recipe_provider()` in `new_environment()`; filed as
  `LIB-RECIPE-PROVIDER-PANIC`.
- **`AsyncStoreRouter::listdir` panicked whenever the key equalled a store's own prefix** —
  `listdir("data")` on a store mounted at `data`, the most ordinary call there is. `has_key_prefix`
  is true for equal keys, so `key_prefix[key.len()]` indexed one past the end. The comment directly
  above the line already stated the intent ("but smaller"); the code did not enforce it. **Fixed
  here** — one guard, both the sync and async routers, with two native regression tests — because
  it is a panic in library code sitting directly on the path this design delivers, and the fix is
  the comment's own words.

**Pre-existing, unrelated:** five tests in `liquers-core/tests/expiration_integration.rs` fail on
this tree *and* on a stashed baseline. Not caused by this work; not investigated here.

**The guide was reviewed against this design afterwards** (2026-08-09), and `STORE` now prescribes
the tests this design had adopted provisionally, under the same numbers — so `STORE08`–`STORE11`
need no renaming. It also added two the design did not anticipate, and both are already satisfied
here under local names, which is worth recording so the conformance claim stays checkable:

| Guide ID | Satisfied by |
|---|---|
| `STORE12` (integration types configurable; an override resolves to the integration's implementation) | `factory01`/`factory02`/`factory03` in `liquers-store/src/store_builder.rs`, `store11_configuration_routes_by_prefix` and `c12_unregistered_object_fails_with_its_name` in `tests/store_js_STORE.rs` |
| `STORE13` (an unavailable type names the feature or target) | `factory04_gated_type_names_the_feature` |

The guide's `STORE11` blueprint now also asserts listing at a store's own prefix and the overlap
order for nested prefixes. The first of those is the router panic this design fixed; the second is
**not** covered here and is the one genuine gap against the current guide — a router with `data`
and `data/scratch` configured in that order is untested.

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
