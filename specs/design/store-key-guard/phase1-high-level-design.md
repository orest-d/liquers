# Phase 1: High-Level Design — Absolute Store Keys

Resolves `specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md` (P0).

## Feature Name

Absolute Store Keys — a store refuses any key carrying a relative segment, with a dedicated error.

## Purpose

Relative keys are a plan-level feature: `.` and `..` are resolved against a current working
directory while a plan is built, and a store is never the thing that resolves them. Today nothing
enforces that boundary, so `-R/../../etc/passwd` reaches `AsyncFileStore`, `PathBuf::push` resolves
it, and the query API reads and writes outside the store root. The rule this design adds is one
sentence: **a key given to a store must be absolute**, and a store that is handed a relative one
refuses it by name rather than acting on it.

## Core Interactions

### Query System

No change to the language. `.` and `..` stay legal `ResourceName`s, `Key::to_absolute` keeps
resolving them, and relative queries keep working exactly where they work now. Refusal at parse
time is rejected — it would break CWD resolution, which is the legitimate use of `..`.

`Key` gains the rule as its own API, so no store has to restate it:

- `Key::is_relative(&self) -> bool` — true when **any** segment is `.` or `..`.
- `Key::as_absolute(&self) -> Result<&Key, Error>` — the checked accessor stores call.
- `Key::try_into_absolute(self) -> Result<Key, Error>` — the consuming convenience, one line over
  the above, for call sites that already own the key.

Both fallible forms return the key unchanged or the new error; neither resolves anything.

### Store System

The whole change. `Store` and `AsyncStore` state the precondition and every store calls
`key.as_absolute()?` before the key is used, in every fallible method — not only in `is_supported`,
which is consulted **only** by the two routers, so a directly-held store would otherwise skip the
check entirely. `FileStore` and `AsyncFileStore` additionally get the check at their path-building
choke point, so the filesystem cannot be reached without passing it. `liquers-web`'s private
`check_key` (`liquers-web/src/store/key_guard.rs`) collapses onto the shared rule.

### Error System

A dedicated error, so a traversal attempt is distinguishable from "this store does not serve this
prefix" in tests, logs and HTTP status. New `ErrorType` variant plus an `Error` constructor in
`liquers-core/src/error.rs`. Adding a variant forces a compile error at every match site — that is
the intent of the no-default-arm convention — costing edits in `liquers-axum` (status mapping),
`liquers-core/src/assets.rs` (persistence classification), `liquers-py` and `liquers-web` (name
tables).

### Command System / Asset System / Value Types

None.

### Web/API

`liquers-axum` handlers pass parsed keys straight to the store, so the traversal stops being
reachable over HTTP with no handler change. The new error maps to `400 Bad Request` — the caller
supplied an address that is not a store address — rather than the `404` that `KeyNotSupported` gets.

## Crate Placement

`liquers-core` (`query.rs` predicate, `error.rs` error, `store.rs` rule and stores),
`liquers-store` (OpenDAL stores adopt), `liquers-web` (delegate), `liquers-axum` and `liquers-py`
(match arms). No new dependencies, no dependency-flow change.

## Documentation Intent

**Primary home is rustdoc, not `specs/`.** The precondition is part of the store contract, so it is
documented where a backend author reads the contract: the `liquers-core::store` module docs, the
`Store` and `AsyncStore` trait docs, each guarded method, and the three `Key` methods. The `Key`
docs must say explicitly that `to_absolute(cwd)` *resolves* while `as_absolute` only *asserts* —
they are one word apart and do opposite things.

**Reference:** No new `specs/reference/` document. `specs/reference/api/DOC_07_STORES_PERSISTENCE.md`
is the reference that would carry this rule, and it does not exist yet — DOC-07 "Stores and
persistence" is P1 / *Not started* in the `API_DOCS_GAP_ANALYSIS.md` progress tracker. Writing a
one-rule reference now would pre-empt it and create a second place to keep current, so instead this
design records the requirement in the gap analysis for whoever writes DOC-07.

**Guide:** Neither. Trait rustdoc plus `STORE05` cover writing a backend.

**Other documents to create:** None.

**Specific documents to update:** `specs/reference/api/API_DOCS_GAP_ANALYSIS.md` (§7 *Stores and
persistence* gains the absolute-key rule as required DOC-07 content — done now, ahead of the rest of
this design, since it records a documentation requirement independent of how the fix lands);
`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (`STORE05` gains the relative-key case and the
direct-call requirement); `specs/reference/PROJECT_OVERVIEW.md` (§5 Storage states the
precondition); `specs/reference/WEB_API_SPECIFICATION.md` (new error type in the status-code table);
`specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md` (link, then close in Phase 5); `specs/README.md`;
`specs/index.csv`.

Audience: backend authors and reviewers, who should learn the precondition and where it is enforced
from the API docs alone, without reading this folder.

## Open Questions

1. **Two `is_relative`s will exist, meaning different things.** `Key::is_relative` is *any* segment;
   the existing `CwdCursor::is_relative` (`query.rs:2187`) is the **first** segment only, because at
   query level relative means "needs a CWD to resolve". Both are correct for their jobs — widening
   the cursor's is not safe, as it would send `a/../b` through `to_absolute` inside
   `CwdCursor::resolve_key` and make `Context::evaluate` reject it with a message about link
   arguments that does not fit. Two methods with one name is still a trap, so Phase 2 should rename
   the cursor's to what it actually tests (`needs_cwd`, `starts_relative`). It is `pub(crate)` with
   three call sites, so the rename is free.
2. **`Query::absolute` is a third meaning of the word.** It means "the text had a leading `/`",
   documented as independent of `.`/`..` resolution and as having no semantic meaning
   (`query.rs:67`, `:2148`). Nothing here changes it, but `as_absolute` on `Key` and `absolute` on
   `Query` must not be read as the same concept — Phase 2 decides whether that is a doc note or a
   rename.
3. **Empty segments are a different wrong.** `liquers-web`'s guard refuses `""` alongside `.` and
   `..`; an empty segment is malformed, not relative, so it does not belong under the new error.
   Recommendation: keep refusing it with plain `key_not_supported`, so collapsing the web guard onto
   the shared rule loses nothing. Confirm in Phase 2.
4. **`AsyncMemoryStore` and the routers.** A map-backed store cannot traverse, but a key that one
   store refuses and another serves is worse than a uniform rule. Recommendation: uniform refusal.
   Also: should the router report the refusal itself rather than "no store matched"?
5. **Nothing in-tree may hand a store a relative key.** Preliminary check says every dot-segment key
   found is pre-store — CWD resolution in `context.rs` and `interpreter.rs`, resolved by
   `resolve_key_from_cwd` before any store call. Phase 2 confirms it properly, including recipes and
   `listdir` round-trips.
6. **Is a borrowing check enough, or should the guarantee be typed?** An `AbsoluteKey` newtype that
   stores accept instead of `&Key` would make forgetting the call impossible rather than merely
   visible. Rejected for now — it changes every store signature and both routers — but Phase 2
   should say so explicitly rather than leave it unconsidered.

## References

- `specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md` — the issue (P0)
- `specs/issues/STORE-OPENDAL-SLASH-HANDLING.md` — adjacent, different cause; not fixed here
- `specs/design/liquers-web-store/phase2-architecture.md` §"Key guard (`STORE05`)" — the precedent
- `liquers-web/src/store/key_guard.rs` — the existing implementation to hoist
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` — `STORE05` conformance cell
