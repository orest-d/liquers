# Phase 2: Solution & Architecture — Accept negotiation on query routes

## Overview

A new module `liquers-axum/src/api_core/negotiate.rs` parses `Accept` and decides, for a finished
asset, one of: serve the cached bytes as today, serialize the same value in another format, convert
it by an automatic edge and serialize, or answer `406`. The Query API's `serve_query` and the
Assets API's `q/data` handlers take the request headers and call it before serializing. Nothing
changes in `liquers-core`; nothing changes on key, entry or store routes.

Rejected:
- Rewriting the query (appending a filename) and re-evaluating — a second asset per format and a
  second evaluation, and a filename does not reach a conversion.
- Negotiating inside `AssetRef::get_binary` — core would learn HTTP media ranges, and every
  caller of the method would change.
- A reverse table media type → format in `media_type.rs` — the answer depends on the value's type
  (`application/json` is `json` for an `Array`, a manifest for a `RecordSource`), so it is derived
  per value from its `TypeInfo`.
- A core `ErrorType::NotAcceptable` (Phase 1 question 3).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `VALUE-CONVERSION-CAPABILITY` (`value-conversion-registry`) | draft | P2 | provides the registry for AC-3/AC-4 | for steps 5-6 | no (steps 1-4) | `requires` |
| `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` (`value-serialization-streaming`) | draft | P2 | rewrites the same two serving paths | no | no | rebase second |
| `RECORD-SOURCE-ROWS-CANNOT-BE-STREAMED` (`record-source-row-streaming`) | draft | P3 | same paths; a `RecordSource` could later negotiate CSV rows | no | no | rebase second |
| `ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED` (`error-type-taxonomy`) | draft | P2 | would place a `NotAcceptable` category if one were added | no | no | none (question 3) |

## Interfaces

```rust
// liquers-axum/src/api_core/negotiate.rs (new; `pub mod negotiate` in api_core/mod.rs)

/// One media range of an `Accept` header, lowercased, parameters other than `q` dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaRange { pub kind: String, pub subtype: String, pub q: f32 }   // "*" for wildcards

/// Ranges with `q > 0`, highest `q` first, then more specific first, then header order.
/// A malformed element is skipped, as RFC 9110 permits; an absent or empty header gives `[]`.
pub fn parse_accept(headers: &HeaderMap) -> Vec<MediaRange>;

impl MediaRange {
    pub fn matches(&self, media_type: &str) -> bool;   // `*/*`, `text/*`, exact; parameters ignored
}

/// What to answer for a finished value.
#[derive(Debug, Clone, PartialEq)]
pub enum Negotiated {
    /// Today's answer: the asset's cached bytes and its own media type.
    AsIs,
    /// The same value written in `data_format` (whose media type is `media_type`).
    Serialize { data_format: String, media_type: String },
    /// The value converted to `target` by an automatic edge, then written in `data_format`.
    Convert { target: String, data_format: String, media_type: String },
    /// Nothing acceptable; the media types this value can be answered in.
    NotAcceptable { producible: Vec<String> },
}

/// Pure decision; no serialization. `explicit` is true when the query ends in a filename.
pub fn negotiate<E: Environment>(
    env: &EnvRef<E>, ranges: &[MediaRange], value: &E::Value, metadata: &Metadata, explicit: bool,
) -> Negotiated;

/// Serves a finished asset under the decision: the shared body of both routes.
pub(crate) async fn negotiated_bytes<E: Environment>(
    env: &EnvRef<E>, asset: &AssetRef<E>, headers: &HeaderMap, explicit: bool,
) -> Response;
```

Decision (`negotiate`):

1. `ranges` empty, or `explicit` → `AsIs`.
2. `own = metadata.get_media_type()`; any range matches `own` → `AsIs` (question 1).
3. For each range in order:
   a. *Direct:* for each `f` in `value.type_info().supported_data_formats`, in declared order,
      `media(f) = file_extension_to_media_type(f.split(':').next())`; the first `f` with
      `range.matches(media(f))` and `media(f) != "application/octet-stream"` (unless the range is
      exactly that type) → `Serialize`.
   b. *Conversion* (after step 5 of Phase 4): for each `(target, class)` of
      `registry.targets_from(&value.identifier())` with `class.is_automatic()`, in the registry's
      order, and each format of `env.get_type_registry().get(target)` as in (a) → `Convert`.
4. Otherwise → `NotAcceptable` with the deduplicated media types of the own type's formats and of
   every automatic target's formats, sorted.

Serving (`negotiated_bytes`):

- Wait for the state with `asset.get().await` only when the header is non-empty; with no header,
  call `asset_bytes` exactly as today.
- `AsIs` → `asset.get_binary()` as today. `Serialize` → `state.data.as_bytes(&data_format)`.
  `Convert` → `registry.convert(&ConversionInput { value: &*state.data, data_format:
  metadata.declared_data_format().as_deref() }, &target, ConversionPolicy::Automatic)?` then
  `.as_bytes(&data_format)`.
- The response metadata is a clone with `data_format` and the type identifier set to the answer
  (and the declared media type cleared), so `BinaryResponse` emits the negotiated
  `Content-Type`; the asset itself is never written.
- Every answer, error or not, gets `Vary: Accept`.

## Integration Points

- `liquers-axum/src/api_core/negotiate.rs` (new), `api_core/mod.rs`.
- `liquers-axum/src/query/handlers.rs`: `get_query_handler` / `post_query_handler` extract
  `HeaderMap`; `serve_query` takes it and, on success, calls `negotiated_bytes` inside the
  existing timeout instead of `get_binary`; `explicit = query.filename().is_some()`.
- `liquers-axum/src/assets/query_handlers.rs`: `q_get_data_handler` / `q_post_data_handler`
  extract `HeaderMap` and call `negotiated_bytes`; `key_handlers.rs` keeps `asset_bytes`.
- `liquers-axum/src/assets/common.rs`: `asset_bytes` unchanged; `negotiated_bytes` delegates to
  it for the no-header path.
- Reads: `liquers_core::media_type::file_extension_to_media_type`, `TypeInfo::supported_data_formats`,
  `ValueInterface::{type_info, identifier, as_bytes}`, `Metadata::{get_media_type,
  declared_data_format}`, `Environment::get_type_registry`; after the registry lands,
  `Environment::get_conversion_registry`, `ConversionRegistry::{targets_from, convert}`.

## Error Handling

- `406`: built in `negotiate.rs` as `ApiResponse::error(ErrorDetail { error_type
  (serialized as `type`): "NotAcceptable", message: "cannot answer in <ranges>; available: <producible>", query, … })`
  with `StatusCode::NOT_ACCEPTABLE`; `x-liquers-managed` is still added by the callers.
- A serialization or conversion failure after a positive decision is the error's own type
  (`SerializationError` / `ConversionError` → `422` through the existing mapping), not a fallback
  to another format: the client asked for this one.
- An evaluation failure is answered exactly as today (the decision is never reached).
- No `unwrap`; `parse_accept` skips malformed elements; a `q` outside `0..=1` is clamped.

## Relevant Commands

None added or changed; `specs/command_registry.yaml` unchanged.

## Documentation Architecture

| Path | Change |
|---|---|
| `specs/reference/WEB_API_SPECIFICATION.md` | §2.3: the negotiation rules, `Vary`, the explicit-filename rule; §3.1: `406`; §7: Query API headers; History, `reviewed:` |
| `specs/guides/WEB_API_GUIDE.md` | "Asking for a format": header versus filename, with the curl examples of Phase 3; History, `reviewed:` |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `api_core/negotiate.rs` (new), `api_core/mod.rs`, `query/handlers.rs`, `assets/query_handlers.rs`, `assets/common.rs`; tests |
| Crates and workflows affected | `liquers-axum`; every Query API and `q/data` request carrying an `Accept` header |
| Existing tests likely to change | none: no current test sends a non-envelope `Accept` to these routes (re-checked in step 1) |
| New validation | Phase 3 tests; `cargo test -p liquers-axum` |
| Compatibility | AC-2 keeps every request whose own type is acceptable unchanged; a client sending a narrow `Accept` that does not include the own type gets a different format or `406` where it got the default before — the feature, but observable; documented |
| Performance | Header parsing per request; a negotiated answer serializes once more (not cached in the asset); conversion is synchronous on the request task (as `as_bytes` already is) |
| Security | No new input reaches evaluation; the header only selects among registered formats and edges |
| Concurrency | None new: the decision reads a finished state |
| Recovery | Revert the two call sites to `get_binary` / `asset_bytes`; the module is unused alone |
| Certainty | High for steps 1-4; medium for 5-6 (registry interface as designed, re-checked when it lands) |
