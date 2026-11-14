# Phase 5: Documentation - axum-server-assembly

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (no PR review comments yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #264)

## Implementation Summary

liquers-axum has one standard server for every host (`liquers-axum/src/server/`):
`ServerConfig` (the `server` section: `host`, `port`, `prefix`, `apis`, `read_only`, `web`; unknown
keys refused; defaults `127.0.0.1:8080`, `/liquer`, all APIs, writes enabled, web on),
`LiquersServerBuilder` (`from_config` or `new`, switches, `router()`, `bind(source)`), the
`EnvSource` trait (a fixed `EnvRef`, or a host's current build; the router is cached per `EnvRef`;
a failing source answers 503), and `LiquersServer` with a two-stage `shutdown(deadline)` on its own
hyper-util accept loop (graceful watcher, `JoinSet`, `CancellationToken`, `TaskTracker`). The web
folder and index redirect are `liquers-axum/src/web/`: stored files only, folder `index.html`,
308/307 redirects, content type, `nosniff`, `no-cache`, ETag/304, Last-Modified. WebSockets get
`Close(1001, "server shutting down")` on stop. `examples/basic_server.rs` uses the builder.

It conforms to Phases 1–4 with one deviation in mechanism, not behaviour: Phase 2 planned an
`Outgoing` enum on the WebSocket writer's channel for the Close frame; the implementation gives
`writer_task` a separate `oneshot` close channel, which needs no change to the existing message
type. The four WebSocket handlers take `Option<Extension<ServerStop>>`, so the API builders still
work alone (Phase 2 was corrected to say so).

## Documentation Delivered

### New Reference Documents
None; the server is §8 of `WEB_API_SPECIFICATION.md`, as planned.

### New Guide Documents
None, as planned.

### Existing Documents Reviewed or Updated
`specs/reference/WEB_API_SPECIFICATION.md` (§8 rewritten: §8.1 assembly and `ServerConfig`, §8.2
stopping, §8.3 web folder, §8.4 index page, §8.5 composing by hand; §9 the security-sensitive
settings and the read-only web mount; Appendix B), `specs/guides/WEB_API_GUIDE.md` (§1, §9),
`liquers-axum/README.md` (quick start with the builder). History rows and `reviewed: 2026-11-09`
on the two `specs/` documents.

### Module Docs
`src/server/mod.rs`, `src/web/mod.rs`, `src/lib.rs`, `src/assets/websocket.rs`:
`check_api_docs.py` 0 warnings, 0 errors; their examples run as doctests.

### User-Guide Chapters
`docs/guides/{rust,web,python}/12-serving.md` are now `draft`, with the server's settings, web
folder and **Security** section in one shared fragment, `docs/guides/_shared/serving.md` (host and
DNS rebinding, `read_only`, `web` and stored pages, the read-only `web` mount), as Decision 3 and
`SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED` ask. The WebSocket sections stay `TBD:`.

### Links and Capability Map
`specs/README.md` keeps its design row until `status: complete`.

## Issues Filed

None new. Closed: `AXUM-CANNOT-SERVE-STATIC-WEB-CONTENT`, `AXUM-HAS-NO-SERVER-WITH-GRACEFUL-SHUTDOWN`.
Still open, by decision: `WEB-FOLDER-SERVES-STORED-FILES-ONLY`, `INDEX-PAGE-IS-A-FIXED-REDIRECT`;
`SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED` records what is documented and what is not.

## Important Learning

A filesystem mount keeps its prefix in the path: a mount at `web` with `path: ./site` serves
`./site/web/…`. The docs say so where they show the read-only web mount. A port check after a
server closed connections must allow `TIME_WAIT` (`SO_REUSEADDR`), or it reports a stopped server
as still bound.

## Conformance and Remaining Work

Implemented as approved, with the WebSocket close mechanism above. Remaining: the open issues
named above.

## Validation

`cargo test -p liquers-axum` (all suites, `server_web`, `server_assembly`, `server_shutdown` five
times each, `server_config_document`) and its doctests; `check_api_docs.py`; `cargo run -p
liquers-docs -- check`; `python3 scripts/docs_index.py --check`.
