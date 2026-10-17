---
id: AXUM-QUERY-API-BASE-SLASH
kind: design
title: The Query API route separates its base from the query with a slash
gh_pr: [147]
form: compact
phase: implementation
readiness: ready
autofix: eligible
area: [axum]
issues: [AXUM-QUERY-API-QUERY-KEEPS-LEADING-SLASH, AXUM-API-BASE-WITH-TRAILING-SLASH-DOUBLES-THE-SEPARATOR]
merged: 2026-10-17
created: 2026-10-17
---
# The Query API route separates its base from the query with a slash

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-17 backlog compaction. Phases 1-4
are written but not approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

`QueryApiBuilder::build` (`liquers-axum/src/query/builder.rs`) registers
`format!("{}{{*query}}", self.base_path)`. With the conventional base `/liquer/q` (no trailing
slash: `examples/basic_server.rs`, `assets_recipes_basic.rs`, `error_handling.rs`, and
`WEB_API_SPECIFICATION.md` §7 and its Appendix A) the wildcard captures the separating `/`, so the
handler parses `/make_text` instead of `make_text`. The query keeps a leading slash it was never
given, gets a different identity from the same query through the Assets API (`{b}/q/data/{*query}`
captures after a literal `/`), and error envelopes show it. Tests and `examples/websocket_client.rs`
pass `"/q/"` and never see it.

### Problem Example

```bash
# server built with QueryApiBuilder::new("/liquer/q")
curl http://localhost:3000/liquer/q/no_such_command
```

Today the handler (`serve_query`, `liquers-axum/src/query/handlers.rs`) calls
`parse_query("/no_such_command")`, so the result is cached under, and errors report, the query
`/no_such_command`; `{assets}/q/info/no_such_command` does not find an asset the Query API computed.
It should evaluate `no_such_command`, exactly as `QueryApiBuilder::new("/liquer/q/")` does today.

### Scope and Acceptance Criteria

- **AC-1** No leading slash from a base without one
  - WHEN the Query API is built with `QueryApiBuilder::new("/q")` and serves `GET /q/<query>`
  - THEN the evaluated query is `<query>` (its `encode()` has no leading `/`)
- **AC-2** Both base spellings are the same route
  - WHEN it is built with `"/q"` and, separately, with `"/q/"`
  - THEN `GET /q/make_text` and `POST /q/make_text` answer the same value with status 200
- **AC-3** The base is a whole path segment
  - WHEN it is built with `"/q"` and serves `GET /qmake_text`
  - THEN the answer is 404 (no route), not an evaluation of `make_text`
- **AC-4** A root base still works
  - WHEN it is built with `""` or `"/"`
  - THEN `GET /make_text` evaluates `make_text`

- **AC-5** Every builder accepts both base spellings
  - WHEN `StoreApiBuilder`, `RecipesApiBuilder` or `AssetsApiBuilder` is built with `"/api/x"` and,
    separately, with `"/api/x/"`
  - THEN both register the same routes, so `GET /api/x/data/<key>` (and the Assets `ws` route)
    answers alike and no `//` route exists

Non-goal: an explicitly doubled slash in the URL (`GET /q//x`), which keeps meaning the query `/x`.

**Merged source (2026-10-17):** `AXUM-API-BASE-WITH-TRAILING-SLASH-DOUBLES-THE-SEPARATOR` — the
other builders' half of the same contract (T2, shared contract: how a builder's base joins its
routes). Still one crate and a bug fix, so the automatic-fix label is unchanged (E1 does not apply).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix in the four route builders of `liquers-axum` (one private helper)
  restoring the documented behaviour (§7: "the query is whatever follows the base path"), no `pub`
  signature change, tests in `liquers-axum/tests/`; the reference wording is corrected alongside.
  Close call on rule 4: the route pattern changes for a base without a trailing slash, but only by
  no longer matching `/liquer/qX` and no longer prepending `/`, which no documented use relies on.
- **Leading issue:** None
- **Explanation:** The issue offered "normalise" or "document and enforce a trailing `/`"; the tree
  uses both spellings (examples `"/liquer/q"`, tests `"/q/"`), and normalising keeps all of them
  working, so enforcement is rejected on evidence rather than left as a choice.
- **Open questions:** None.

### Design Dependencies

- `query-leading-slash-field` (complete, `QUERY-ABSOLUTE-FIELD-NAME-AMBIGUOUS`) — **overlaps**: it
  gave a leading slash a meaning in the query language, which is why one must not appear by
  accident. No ordering.
- `axum-query-named-arguments` (where this was discovered) — **overlaps**, no shared change site.

### Consolidated Findings

One-line route change plus tests and a §7 wording fix. `"/q/"` callers see no change. The timeout
envelope (`serve_query`'s `query: Some(query.encode())`) is the deterministic place to observe the
evaluated query, so AC-1 is proved through it.

## Phase 2: Architecture

### Solution

In `QueryApiBuilder::build`, register the route as
`format!("{}/{{*query}}", self.base_path.trim_end_matches('/'))`. `"/liquer/q"` and `"/liquer/q/"`
both become `/liquer/q/{*query}`; `""` and `"/"` become `/{*query}`. Keep `base_path` stored as given
(the unit tests `test_query_api_builder_creation` and `..._with_custom_path` assert it).

Rejected: stripping one leading `/` in the handler (would also strip a deliberate `GET /q//x`, and
the router would still match `/qX`); requiring a trailing `/` (breaks the three examples and the
spec's own snippet); normalising in `new` (changes the stored value the unit tests read, for no
gain).

Known-issue preflight: no open issue or design edits `QueryApiBuilder::build`.

### Changes

- `liquers-axum/src/query/builder.rs` `QueryApiBuilder::build` — the route string; doc comment on
  `new` says either spelling is accepted.
- `specs/reference/WEB_API_SPECIFICATION.md` §7 and Appendix A — `{base}{*query}` becomes
  `{base}/{*query}`, with a sentence that a trailing `/` on the base is ignored; `## History` row and
  `reviewed:` bump.
- `liquers-axum/src/store/builder.rs` `StoreApiBuilder::build`, `recipes/builder.rs`
  `RecipesApiBuilder::build`, `assets/builder.rs` (the `ws` path) — join through one private helper
  `fn join_base(base: &str, rest: &str) -> String` (`base.trim_end_matches('/')` + `/` + `rest`),
  which `QueryApiBuilder::build` uses too.
- No signature, command, store or binding change.

### Risks

| Risk | Note |
|---|---|
| Files | `builder.rs`, `tests/query_api_routes.rs`, `WEB_API_SPECIFICATION.md` |
| Existing tests | `qar01`-`qar08` use `"/q"`/`"/custom/q"` and `/q/<query>` URLs; they keep passing (the query just loses its leading `/`). `argument_overrides_http.rs` uses `"/q/"`: unchanged |
| Compatibility | A client relying on `/liquer/qX` matching, or on cached assets keyed `/X`, changes; neither is documented. Cached assets are in-memory, so nothing persisted is orphaned |
| axum | `/{*query}` at the router root is valid in axum 0.8 (matchit); `/q/` with an empty capture stays 404, as today for `"/q/"` |
| Certainty | High; recovery is reverting one line |

## Phase 3: Examples and Tests

### Examples

The Problem Example. Normal use: `QueryApiBuilder::new("/liquer/q")` serves
`GET /liquer/q/make_text/upper` as the query `make_text/upper` (`liquers-validate` encodes it
without a leading `/`).

### Tests

In `liquers-axum/tests/query_api_routes.rs`, beside `qar01`-`qar08`:

- `qar09_base_without_slash_evaluates_query_without_leading_slash` — `"/q"`, the blocking `slow`
  command of `qar05` with a 100 ms timeout; assert `json["error"]["query"] == "slow"` — AC-1
- `qar10_base_with_and_without_trailing_slash_are_the_same_route` — `"/q"` and `"/q/"`, GET and POST
  `/q/make_text`, both 200 with body `generated` — AC-2
- `qar11_base_is_a_whole_segment` — `"/q"`, `GET /qmake_text` is 404 — AC-3
- `qar12_root_base` — `""` and `"/"`, `GET /make_text` is 200 — AC-4

- `base_with_trailing_slash_registers_the_same_store_routes` (in `liquers-axum/tests/`, beside the
  store API route tests) — `StoreApiBuilder` with `"/api/store"` and `"/api/store/"`, `GET
  /api/store/data/a.txt` is 200 for both and `GET /api/store//data/a.txt` is 404; the same pair for
  `RecipesApiBuilder` `listdir` — AC-5

Run: `cargo test -p liquers-axum --test query_api_routes`, then
`cargo test -p liquers-axum --lib` (builder unit tests).

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-axum/tests/query_api_routes.rs` — add `qar09` and `qar11` first and see them fail
  against HEAD — `cargo test -p liquers-axum --test query_api_routes qar09` and `… qar11` (Cargo takes one filter per run)
- [x] 2. `liquers-axum/src/query/builder.rs` `QueryApiBuilder::build` — trim the base's trailing `/`
  and insert one; update the `new` doc comment — `cargo test -p liquers-axum --test query_api_routes`
- [x] 3. Add `qar10`, `qar12` — same command, plus `cargo test -p liquers-axum --lib`
- [x] 3a. Add `join_base` and route the Store, Recipes and Assets builders through it; add
  `base_with_trailing_slash_registers_the_same_store_routes` — `cargo test -p liquers-axum --lib --tests`
- [x] 4. `specs/reference/WEB_API_SPECIFICATION.md` §7, Appendix A — route shape and trailing-slash
  sentence; History row, `reviewed:` bump — `python3 scripts/docs_index.py --check`
- [x] 5. Close `AXUM-QUERY-API-QUERY-KEEPS-LEADING-SLASH` and
  `AXUM-API-BASE-WITH-TRAILING-SLASH-DOUBLES-THE-SEPARATOR`, each with a resolution note; regenerate the
  index — `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-axum --lib --tests`; `cargo build -p liquers-axum --examples` (the examples
still compile; `basic_server` now serves `/liquer/q/<query>` without the slash). Final diff review:
one helper and four call sites, tests, the spec.
