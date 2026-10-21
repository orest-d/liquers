# Phase 2: Solution & Architecture — Structured Error Context

Revised 2026-10-20: the 2026-09-03 blockers are carried by Phase 1's recommended answers; line
references became symbols.

## Overview

`ErrorPayload` gains `context: Vec<ErrorContextFrame>`, innermost first. The flat fields (`query`,
`key`, `position`, `command_key`) describe where the error was raised; boundaries fill a flat field
only when it is empty and always append a frame. Four producers append frames: the asset run-failure
path, link materialization, action execution, and three store constructors. Consumers (metadata
log, HTTP, Python, JavaScript, the egui widget) read them.

Rejected: flat fields only (cannot hold owner and inner key at once); a map of named contexts (no
order, ad-hoc names); a flat `Vec<QueryOrKey>` (loses which position belongs to which query); markup
in `message` (fragile, not data); a recursive cause chain (serde depth, `PartialEq`, binding
bridges — and frames already give the path without owning arbitrary errors).

## Known-Issue Preflight

| Issue | Status | Priority | Impact | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `MISSING-RESOURCE-REPORTED-AS-GENERAL-ERROR` | draft | P2 | its error is built with `with_key` today | no | no | `key-without-recipe-answer` sets `key` directly |
| `CORE-ERROR-STORE-NAME-NOT-STRUCTURED` | rejected | P2 | its resolution defers a structured store reference to this design | no | no | `Resource` frame carries `store` |
| `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` | accepted | P3 | Python→Rust conversion must keep `context` | no | no | `error_from_pyerr` reads it (step 7) |
| `ASSETS-IMPROVEMENTS` | accepted | P2 | its persistence warnings need asset context | no | no | the `Asset` frame serves it |
| `CORE-METADATA-TRACEBACK-SUPPORT` | closed | — | `traceback` stays a separate field | — | no | none |

## Interfaces: Data Structures, Trait Implementations, Function Signatures

`liquers-core/src/error.rs` (sync; owned `String`s, because errors are serialized and cross
threads and language boundaries):

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorContextRole { Asset, Link, Action, Resource }

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ErrorContextFrame {
    pub role: ErrorContextRole,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Position::is_unknown")] pub position: Position,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub store: Option<String>,
}

// ErrorPayload:
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub context: Vec<ErrorContextFrame>,

pub const MAX_ERROR_CONTEXT_FRAMES: usize = 16;

impl Error {
    pub fn with_key(self, key: &Key) -> Self;                       // now sets `key`
    pub fn with_context(self, frame: ErrorContextFrame) -> Self;    // dedup + bound
    pub fn with_asset_context(self, key: Option<&Key>, query: Option<&Query>) -> Self;
    pub fn with_link_context(self, query: &Query, position: Option<&Position>) -> Self;
    pub fn with_action_context(self, command: &CommandKey, position: &Position) -> Self;
}
impl ErrorContextFrame {
    pub fn asset(key: Option<&Key>, query: Option<&Query>) -> Self;
    pub fn resource(key: &Key, store_name: &str) -> Self;
}
```

`Position::is_unknown` exists (`query.rs`). `command` is the `CommandKey`'s `Display` form, because
`command_key` itself is `#[serde(skip)]`. `with_asset_context` also sets the flat `key` (keyed) or
`query` (query asset) when that field is empty; `with_link_context` sets flat `query` and
`position` only when both are empty. `with_context` drops a frame equal to the last one; past the
bound it removes index 1, keeping the innermost.

No new trait or trait implementation; derives only. No commands (`register_command!` untouched).

## Integration Points

`liquers-core`:
- `error.rs` — the items above; `key_not_found` sets `key`; `status_conflict` sets `key` only;
  `dependency_version_mismatch` / `dependency_cycle` set `key` for a store key, else `query =
  dep_key.as_str()`; `key_read_error`, `key_write_error`, `key_not_supported` append
  `ErrorContextFrame::resource`. `ErrorType::Cancelled` doc: root cause in `key` or `query`.
- `assets.rs` — `AssetRef::attribute_run_error`: for every error, append the asset frame
  (`AssetData::key`, else its query, else the recipe query — the order `identify_in_error` uses);
  for a cancellation keep today's attribution, testing `query.is_none() && key.is_none()`.
  `cancelled_dependency_cause`: root = `key`, else `query`, else message.
- `interpreter.rs` — `materialize_link_json`'s `attach_context` and
  `attach_parameter_link_context` call `with_link_context` (default/override links have no
  position); action execution keeps `with_command_key(..).with_position(..)` when `command_key` is
  empty, and otherwise calls `with_action_context` instead of overwriting `position`.
- `metadata.rs` — `LogEntry::from_error`: no `query` but a `key` → `with_query(Query::from(&key))`.

Consumers:
- `liquers-axum/src/api_core/response.rs` `ErrorDetail.context: Vec<ErrorContextFrame>`
  (`skip_serializing_if = "Vec::is_empty"`), filled in `error.rs` `error_to_detail`.
- `liquers-py/src/error.rs` `error_to_pyerr` passes `context` as a list of dicts; `_errors.py`
  `_from_native(…, context)` and `LiquersError(…, context=None)`; `error_from_pyerr` reads it back.
- `liquers-web/src/error.rs` `LiquersError.context` getter (`serde_wasm_bindgen::to_value`, already
  a dependency); `.d.ts` stub updated (`scripts/check-stubs.sh`).
- `liquers-lib/src/egui/widgets.rs` error display: falls back to `key` when `query` is empty and
  lists frames, one line each.

## Error Handling

No new `ErrorType`, no new constructor beyond the builders. Enrichment never changes `error_type`
or `message`. `Display` is unchanged (message, command, position): frames are data for consumers.

## Sync vs Async

All builders are synchronous and consume `self`, like the existing ones; call sites already sit in
`map_err` on async paths. No lock, no new `await`.

## Relevant Commands

None added or changed. Namespaces involved: none (the test command `use_link` is registered by the
interpreter's own tests).

## Documentation Architecture

| Path | Kind / audience / area | Change |
|---|---|---|
| `specs/reference/ERROR_CONTEXT.md` | new reference; internal and integrators; `core/error` | flat-field rule, frames, roles, order, bound, serde, binding shapes; History |
| `specs/reference/PROJECT_OVERVIEW.md` | reference | "Query/Key context preserved in errors" → link to the new reference |
| `specs/reference/ASSETS.md`, `ASSET_LIFECYCLE.md` | reference; `core/assets` | the asset frame on failure; cancellation root cause in `key` |
| `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` | API doc | cancellation sentence (`query` → `key` or `query`) |
| `specs/reference/WEB_API_SPECIFICATION.md` | reference; `axum` | `ErrorDetail.context` |
| `specs/reference/PYTHON_INTEGRATION.md` | reference; `py` | `LiquersError.context` |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide; `web`, `py` | §ERROR: map `context` both ways |
| `specs/README.md` | capability map | link the reference under "Values, metadata and errors" |

`affects_docs`: the new reference, `PROJECT_OVERVIEW.md`, `ASSETS.md`, `ASSET_LIFECYCLE.md`,
`WEB_API_SPECIFICATION.md`, `PYTHON_INTEGRATION.md`, `api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`,
`LANGUAGE-INTEGRATION_GUIDE.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | core `error.rs`, `assets.rs`, `interpreter.rs`, `metadata.rs`; axum `api_core/{error,response}.rs`; py `src/error.rs`, `python/liquers/_errors.py`; web `src/error.rs` + stubs; lib `egui/widgets.rs` |
| Crates and workflows affected | every failing evaluation; metadata `error_data`; HTTP error bodies; Python and JavaScript exceptions |
| Existing tests likely to change | `asset_cancellation.rs` (six `query` asserts), `assets.rs` cancellation cascade test, `plan.rs` `walk_detects_recipe_cycle`, py `test_ERROR02` (its comment), axum response snapshot tests if any compare whole bodies |
| New validation | builder unit tests, nested-path tests through the interpreter, serde round trip, binding tests |
| Compatibility / data | serialized errors gain `context` and lose key text in `query`; waived by the maintainer; old `error_data` still deserializes (`default`) |
| Concurrency / performance / security | error paths only; ≤ 16 small frames; no HTML rendering, so no escaping concern here |
| Recovery | step-by-step; producers can be reverted while the field stays (it is then simply empty) |
| Certainty | high for the mechanism; the six Phase 1 choices are the open part |
