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
resolving them, and relative queries keep working exactly where they work now. One new predicate on
`Key` states the store-level rule. Refusal at parse time is rejected — it would break CWD
resolution, which is the legitimate use of `..`.

### Store System

The whole change. `Store` and `AsyncStore` state the precondition and refuse a relative key in
every fallible method, not only in `is_supported`: `is_supported` is consulted **only** by the two
routers, so a directly-held store would otherwise skip the check entirely. `FileStore` and
`AsyncFileStore` additionally get the check at their path-building choke point, so the filesystem
cannot be reached without passing it. `liquers-web`'s private `check_key`
(`liquers-web/src/store/key_guard.rs`) collapses onto the shared rule.

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

**Reference:** New `specs/reference/STORE_KEY_RULES.md`. Nothing currently states what a key may
contain at the store boundary — `STORE_CONFIG_FSD.md` is about configuration — and a backend author
needs the rule where they will find it, not in a design folder.

**Guide:** Neither. The trait docs plus the reference cover writing a backend; revisit if Phase 3
shows adoption needs a narrative.

**Other documents to create:** None.

**Specific documents to update:** `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (`STORE05` gains the
relative-key case and the direct-call requirement); `specs/reference/PROJECT_OVERVIEW.md` (§5
Storage points at the new reference); `specs/reference/WEB_API_SPECIFICATION.md` (new error type in
the status-code table); `specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md` (link, then close in Phase
5); `specs/README.md`; `specs/index.csv`.

Audience: backend authors and reviewers, who should learn the precondition and where it is enforced
without reading this folder.

## Open Questions

1. **"Relative" needs a definition that catches `a/../../etc`.** The existing predicate,
   `CwdCursor::is_relative` (`query.rs:2187`), tests only the **first** segment, because at query
   level relative means "needs a CWD to resolve". `a/../../etc/passwd` passes that test and is
   normalized by nothing. The store rule must be *any* segment. Widening the existing predicate is
   not safe — it would send `a/../b` through `to_absolute` inside `CwdCursor::resolve_key` and make
   `Context::evaluate` reject it with a message about link arguments that does not fit. So: two
   predicates with distinct names, and Phase 2 picks them.
2. **The word "absolute" is already taken.** `Query::absolute` means "the text had a leading `/`",
   documented as independent of `.`/`..` resolution and as having no semantic meaning
   (`query.rs:67`, `:2148`). A `Key::is_absolute()` meaning "carries no relative segment" would
   read as the same concept and is not. Phase 2 decides naming, and whether `Query::absolute` is
   worth renaming or documenting against.
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

## References

- `specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md` — the issue (P0)
- `specs/issues/STORE-OPENDAL-SLASH-HANDLING.md` — adjacent, different cause; not fixed here
- `specs/design/liquers-web-store/phase2-architecture.md` §"Key guard (`STORE05`)" — the precedent
- `liquers-web/src/store/key_guard.rs` — the existing implementation to hoist
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` — `STORE05` conformance cell
