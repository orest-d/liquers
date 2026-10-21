---
id: HTTP-REQUEST-BODY-LIMIT
kind: design
title: A configurable, documented request-body limit for the web API
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [axum, docs]
issues: [ASSETS-IMPROVEMENTS]
created: 2026-10-20
---
# A configurable, documented request-body limit for the web API

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. Re-scoped from `ASSETS-IMPROVEMENTS` (persistence, eviction, upload limits): the
persistence part is done and the eviction part belongs to `CORE-ASSET-GC`; see Phase 1 Purpose.

## Phase 1: High-Level Design

### Purpose

Every route of `liquers-axum` that takes a body (`Bytes`, `Json`, `Multipart`) is limited by axum's
built-in default of 2 MiB (`axum-core` `DEFAULT_LIMIT = 2_097_152`), applied silently: no builder
can change it, no document mentions it, and a larger upload — a 3 MB CSV through
`PUT /api/store/data/...` or `POST .../assets/key/data/...` — fails with a plain-text 413. This
design makes the limit a stated, per-router builder option (`with_body_limit` /
`without_body_limit` on the Store, Assets and Query API builders), documents it with the 413 it
produces, and corrects `ASSET_SET_OPERATION.md`, which still promises LRU eviction and a future
upload limit.

What else `ASSETS-IMPROVEMENTS` asked for, at HEAD (commit 18732dd):

- **A, A1 persistence failures** — done: `PersistenceStatus` (`None`, `Persisted`, `NonSerializable`,
  `NotPersisted`, `assets.rs:410`), recorded for every save path by `persist_with_status_tracking`
  / `record_persistence_result`, with a metadata warning carrying the full error
  (`AssetData::set_persistence_status`); `set_state` stores a value with no byte form
  metadata-only and `get` still returns it (`STORE_SEMANTICS.md` §2). The "no store key" case is
  obsolete: an unkeyed asset is never written by design (`ASSETS.md` §Key ownership), so it is not
  a persistence failure and gets no warning.
- **B sticky eviction** — there is no LRU or memory-pressure eviction to protect against: the maps
  evict only expired, volatile and stale-terminal entries. A future size-bounded cache is
  `CORE-ASSET-GC`, which must keep `Source` (and decide `Override`) out of collection. One related
  defect found: `TO-OVERRIDE-KEEPS-THE-PENDING-EXPIRATION-DEADLINE`.
- **C upload limit** — this design.

### Problem Example

```text
$ head -c 3000000 /dev/urandom > big.bin
$ curl -X PUT --data-binary @big.bin http://localhost:3000/liquer/api/store/data/data/big.bin
HTTP/1.1 413 Payload Too Large
Failed to buffer the request body: length limit exceeded
```

Today nothing in `StoreApiBuilder`, `AssetsApiBuilder` or `QueryApiBuilder` can raise or lower the
limit, and `WEB_API_SPECIFICATION.md` lists no 413. A deployer must know axum's internals and add
`.layer(DefaultBodyLimit::max(n))` to the router. Should be:
`StoreApiBuilder::new("/liquer/api/store").with_body_limit(64 * 1024 * 1024).build()` accepts the
upload, `with_body_limit(1024)` refuses it with 413 and writes nothing, and both are documented.

### Scope and Acceptance Criteria

- **AC-1** Documented default
  WHEN a builder is built without a body-limit option and a request body exceeds 2 MiB
  THEN the route answers 413 and the store is unchanged, as at HEAD, and `WEB_API_SPECIFICATION.md` states the default and the 413
- **AC-2** Raised limit
  WHEN `with_body_limit(n)` is set and a body of more than 2 MiB but at most `n` bytes is sent to a writing route
  THEN the write succeeds as for a small body
- **AC-3** Lowered limit refuses before anything is written
  WHEN `with_body_limit(n)` is set and a body of more than `n` bytes is sent
  THEN the route answers 413 and neither the store nor the asset manager holds the key
- **AC-4** No limit
  WHEN `without_body_limit()` is set
  THEN a body larger than 2 MiB is accepted
- **AC-5** Per router
  WHEN two routers built with different limits are merged into one application
  THEN each route keeps its own router's limit
- **AC-6** Records corrected
  WHEN `ASSET_SET_OPERATION.md` is read
  THEN it no longer claims LRU eviction or a planned core upload limit, and points to the web API limit and `CORE-ASSET-GC`

Non-goals: a size limit inside `liquers-core` (`set_binary`, Python, wasm callers); streaming
uploads (`VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`); WebSocket limits (already
`WebSocketLimits`); cache size limits (`CORE-ASSET-GC`); a JSON error body for the 413 (Q1).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds `pub` builder methods to `liquers-axum` (rule 4).
- **Leading issue:** the 413 response body (question 1).
- **Explanation:** axum's `DefaultBodyLimit` layer does all the work and every body extractor in the
  crate honours it, so the plan is small and certain. The proposals below change what a client sees
  and what a deployer configures, so they stay visible.
- **Open questions:**
  1. **Open design - 413 body:** keep axum's own plain-text 413, documented beside the 404/405
     axum already answers with an empty body (recommended: no core change), or answer with the
     §3.2 JSON error body, which needs a new `ErrorType` (for example `PayloadTooLarge`) and arms in
     the exhaustive matches of `liquers-axum`, `liquers-py` and `liquers-web` (size `L`).
  2. **Proposed resolution - default:** keep axum's 2 MiB when no option is set (recommended:
     no behaviour change, safe for an exposed server; the examples can raise it), or raise the
     default for a local data tool.
  3. **Proposed resolution - no core limit:** the original "configurable `max_binary_size` for
     `set()`" is dropped (recommended: in-process callers already hold the bytes, so a core check
     protects nothing; HTTP is the untrusted boundary).

### Design Dependencies

- `CORE-ASSET-GC` (issue, no design) — **overlaps**: owns cache size and eviction; inherits the
  requirement that non-derivable `Source` values are never collected (item B of the source).
- `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` (issue) — **overlaps**: streaming uploads
  remove the memory reason for a limit, not the policy; its §5 notes no limit is configured. No
  order.
- `AXUM-WEBSOCKET-HARDENING` (issue, design `axum-assets-endpoints`) — **overlaps**: other
  server-hardening limits; independent change sites.
- `TO-OVERRIDE-KEEPS-THE-PENDING-EXPIRATION-DEADLINE` (filed with this design) — found while
  checking item B; independent.

### Consolidated Findings

- Every body-taking handler uses an extractor that reads `DefaultBodyLimit`: `Bytes`
  (`store/handlers.rs` `put_data_handler`, `put_entry_handler`; `assets/key_handlers.rs`
  `key_post_data_handler`, `key_post_entry_handler`, `key_description_handler`;
  `assets/query_handlers.rs` `q_post_data_handler`, `q_post_entry_handler`, `q_submit_handler`;
  `query/handlers.rs` `post_query_handler`), `Json` (`store/handlers.rs` `put_metadata_handler`) and `Multipart` (`upload_handler`). One layer per router covers all.
- The layer must be added in `build()` before the router is returned, so it applies to that
  router's routes only and survives `merge` (AC-5).
- `RecipesApiBuilder` has no body-taking route and gets no option.
- No Liquers query, command or registry change.

## Phase 2: Architecture

### Solution

Each of `StoreApiBuilder`, `AssetsApiBuilder` and `QueryApiBuilder` gains a private field
`body_limit: Option<DefaultBodyLimit>` (`axum::extract::DefaultBodyLimit` is `Copy`), `None` by
default, and two methods:

```rust
/// Largest request body a route of this router accepts, in bytes; a larger one is answered 413
/// before the handler runs. Default: axum's 2 MiB.
pub fn with_body_limit(mut self, bytes: usize) -> Self   // Some(DefaultBodyLimit::max(bytes))
/// Accept request bodies of any size.
pub fn without_body_limit(mut self) -> Self              // Some(DefaultBodyLimit::disable())
```

`build()` ends with `if let Some(limit) = self.body_limit { router = router.layer(limit) }`.
Under Q1's recommendation the rejection stays axum's (`BytesRejection` /
`JsonRejection` / `MultipartRejection` with status 413). A public constant
`DEFAULT_BODY_LIMIT: usize = 2 * 1024 * 1024` in `liquers-axum/src/lib.rs` documents the default
(not applied; axum applies it).

Rejected: `tower_http::limit::RequestBodyLimitLayer` (needs the `limit` feature, a new dependency
feature, and also rejects bodies the handler never reads); a `Content-Length` check in each handler
(misses chunked bodies); a core `max_binary_size` (Q3); a field on the existing `QueryApiConfig`
`Extension` (only the Query API has one).

Known-issue preflight: `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` (none added here);
`CORE-SESSION-AND-KEY-ACL` (a limit is not authorization). Nothing blocks.

### Changes

- `liquers-axum/src/store/builder.rs`, `assets/builder.rs`, `query/builder.rs` — field, two
  methods, `build()` layer; synchronous builder code.
- `liquers-axum/src/lib.rs` — `pub const DEFAULT_BODY_LIMIT`.
- `liquers-axum/examples/basic_server.rs` — `with_body_limit` shown on the Store API.
- `specs/reference/WEB_API_SPECIFICATION.md` — §3.1: axum answers **413** (plain text) for a body
  over the router's limit, before the handler; §5.6: rows `with_body_limit(n)` (2 MiB) and
  `without_body_limit()`; the same two options named in §4 (Store API) and §7 (Query API); §8: a
  line on choosing a limit; History row, `reviewed:`.
- `specs/guides/WEB_API_GUIDE.md` — beside the `QueryApiBuilder::with_timeout` mention (its only
  server-option sentence), one sentence on the body limit and the 413; History row, `reviewed:`.
- `specs/reference/ASSET_SET_OPERATION.md` — §Non-Serializable Data "Eviction Handling": there is
  no LRU eviction; a value is lost only when its entry leaves the map (expiry of an `Override`,
  removal) or the process ends; cache bounds are `CORE-ASSET-GC`. §Future Enhancements 2 and
  §Related Issues #4-#6: resolved (persistence status), `CORE-ASSET-GC`, and the web API limit;
  History row, `reviewed:`.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | three builders, `lib.rs`, one example, three documents |
| Crates and workflows affected | `liquers-axum` only; deployers who merge routers |
| Existing tests likely to change | none: the default is unchanged (Q2) |
| New validation | `body_limit_*` tests in three test files |
| Compatibility | additive API; default behaviour identical. Under Q2's alternative, servers would start accepting larger bodies |
| Data / concurrency | a refused body never reaches a handler, so no partial write (AC-3) |
| Performance / security | a raised or disabled limit lets a client make the server buffer that much (documented); the default stays safe |
| Recovery | revert; the default path is untouched |
| Certainty | high for the mechanism (axum-core 0.5.6 `with_limited_body`); Q1-Q3 are policy |

## Phase 3: Examples and Tests

### Examples

```rust
let app = axum::Router::new()
    .merge(StoreApiBuilder::new("/liquer/api/store").with_body_limit(64 << 20).build())
    .merge(AssetsApiBuilder::new("/liquer/api/assets").build())   // 2 MiB
    .with_state(envref);
```

### Tests

Bodies are `vec![b'x'; n]`; sizes are 3 MiB (over the default) and 1 KiB / 2 KiB.

`liquers-axum/tests/store_api_routes.rs`:

- `body_limit_default_rejects_over_two_mib` — `PUT data/big.bin` with 3 MiB → 413; the store does not contain the key: AC-1
- `body_limit_raised_accepts_large_put` — `with_body_limit(4 << 20)`, 3 MiB → 200, `get_bytes` returns it; also `PUT entry`: AC-2
- `body_limit_lowered_rejects_before_write` — `with_body_limit(1024)`, 2 KiB `PUT data`, `PUT metadata` (JSON) and multipart `upload` → 413 each; store unchanged: AC-3
- `body_limit_disabled_accepts_large_put` — `without_body_limit()`, 3 MiB → 200: AC-4

`liquers-axum/tests/assets_api_endpoints.rs`:

- `body_limit_assets_key_data_lowered` — `with_body_limit(1024)`, 2 KiB `POST key/data/...` → 413; `AssetManager::get_any_status` finds nothing and the store does not contain the key; 512 B → 200: AC-2, AC-3
- `body_limit_is_per_router` — a store router with `with_body_limit(4 << 20)` merged with a default assets router: 3 MiB `PUT` store → 200, 3 MiB `POST key/data` → 413: AC-5

`liquers-axum/tests/query_api_routes.rs`:

- `body_limit_query_post_lowered` — `with_body_limit(16)`, `POST` with a 64-byte body → 413: AC-3

Documents: a review of `ASSET_SET_OPERATION.md` and `WEB_API_SPECIFICATION.md` against AC-6 and
AC-1 (no test; the History rows record it).

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-axum --test store_api_routes --test assets_api_endpoints --test query_api_routes`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-axum/src/lib.rs` `DEFAULT_BODY_LIMIT`; `store/builder.rs` field, `with_body_limit`, `without_body_limit`, layer in `build()`; store tests — `cargo test -p liquers-axum --test store_api_routes body_limit`. Rollback: revert.
- [ ] 2. `assets/builder.rs` the same; assets tests — `cargo test -p liquers-axum --test assets_api_endpoints body_limit`. Depends on 1 (constant).
- [ ] 3. `query/builder.rs` the same; query test — `cargo test -p liquers-axum --test query_api_routes body_limit`. Depends on 1.
- [ ] 4. `examples/basic_server.rs` — show the option — `cargo build -p liquers-axum --example basic_server`.
- [ ] 5. `WEB_API_SPECIFICATION.md` §3.1, §4, §5.6, §7, §8; `WEB_API_GUIDE.md`; `ASSET_SET_OPERATION.md` eviction and future sections; History rows and `reviewed:` bumps (AC-1, AC-6).
- [ ] 6. Records — `ASSETS-IMPROVEMENTS` `status: closed` with a resolution naming the tests; `python3 scripts/docs_index.py`, then `--check`.

### Validation

`cargo fmt -p liquers-axum -- --check`; `CARGO_INCREMENTAL=0 cargo test -p liquers-axum`;
`cargo clippy -p liquers-axum --no-deps -- -D clippy::unwrap_used -D clippy::expect_used`;
diff review: only `liquers-axum`, the three documents, the issue and the generated index.
