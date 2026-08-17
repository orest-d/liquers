# Phase 1: High-Level Design — Store Key Guard

Resolves `specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md` (P0).

## Feature Name

Store Key Guard — a shared key-shape check refusing `..`, `.` and empty segments in every store.

## Purpose

A key containing `..` reaches the file stores unmodified and `PathBuf::push` resolves it, so a
query such as `-R/../../etc/passwd` reads and writes outside the store root with the server's
privileges. This design puts one refusal rule in `liquers_core::store`, applies it at the point
every backend must pass through, and makes the same rule the default for backends not yet written.

## Core Interactions

### Query System

None. `..` stays a legal `ResourceName` and `Key::to_absolute` keeps consuming it during CWD
resolution — the guard sits after that, at the store boundary. Refusing at parse time would break
relative resolution and is rejected (issue option 3).

### Store System

The whole change. `Store` and `AsyncStore` gain the guard as an overridable default; `FileStore`,
`AsyncFileStore`, `AsyncMemoryStore`, `MemoryStore` and both OpenDAL stores adopt it, and
`liquers-web`'s existing private `check_key` (`liquers-web/src/store/key_guard.rs`) collapses onto
it. `is_supported` is *not* sufficient on its own: only the routers consult it, so a store used
directly skips it entirely. The guard must also sit on the path-building choke point.

### Command System / Asset System / Value Types

None. Refusal surfaces as the existing `Error::key_not_supported`, already handled everywhere a
store error is.

### Web/API

`liquers-axum` store and recipe handlers pass parsed keys straight to the store, so the traversal is
reachable over HTTP today and stops being so with no handler change. Response shape is unchanged —
`key_not_supported` already maps to an error response.

## Crate Placement

`liquers-core/src/store.rs` (rule + trait defaults), `liquers-store/src/opendal_store.rs` (adopt),
`liquers-web/src/store/key_guard.rs` (delegate or delete). No new dependencies, no dependency-flow
change.

## Documentation Intent

**Reference:** New `specs/reference/STORE_KEY_RULES.md`. There is no reference describing what a
`Key` may contain at the store boundary — `STORE_CONFIG_FSD.md` covers configuration only — and
backend authors need that rule stated where they will find it, not inside a design folder.

**Guide:** Neither. Writing a backend is covered by the trait docs plus the new reference; revisit
if Phase 3 shows the adoption steps need a narrative.

**Other documents to create:** None.

**Specific documents to update:** `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (`STORE05` gains the
`.`/empty-segment cases and the direct-call requirement); `specs/reference/PROJECT_OVERVIEW.md`
(one line in §5 Storage pointing at the new reference); `specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md`
(link the design, close in Phase 5); `specs/README.md` and `specs/index.csv`.

Audience: backend authors and reviewers. They should learn which key shapes a store must refuse and
where the check belongs without reading this folder.

## Open Questions

1. Where is the guard enforced, given `is_supported` gates only routing? Candidates: make
   `key_to_path` fallible (one choke point per file store, ~50 mechanical call sites), or guard each
   fallible trait method. Decide in Phase 2.
2. Does the guard belong in the trait's *default method bodies* so an unmodified third-party backend
   inherits it, or must each backend opt in? Trait defaults are overridable, so this is about the
   default posture, not a guarantee.
3. Is `AsyncMemoryStore` (exact-match map, no traversal risk) refused too? Uniform refusal avoids a
   key that one store accepts and another rejects; Phase 2 decides.
4. Does anything in-tree legitimately hand a store a key with a `.`, `..` or empty segment
   (recipes, `listdir` round-trips, `Key::join("")`)? Must be answered before the guard lands.
5. Should the router report the refusal distinctly, rather than as "no store matched"?

## References

- `specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md` — the issue (P0)
- `specs/issues/STORE-OPENDAL-SLASH-HANDLING.md` — adjacent, different cause; not fixed here
- `specs/design/liquers-web-store/phase2-architecture.md` §"Key guard (`STORE05`)" — the precedent
- `liquers-web/src/store/key_guard.rs` — the existing implementation to hoist
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` — `STORE05` conformance cell
