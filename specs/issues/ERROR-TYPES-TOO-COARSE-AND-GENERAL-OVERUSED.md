---
id: ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED
kind: issue
title: Error types are not clustered, and General stands in for hundreds of distinct failures
status: draft
priority: P2
complexity: L
area: [core/error, py, web, axum]
design: error-type-taxonomy
created: 2026-10-25
github:
---

## Problem

`ErrorType` (`liquers-core/src/error.rs`) is a flat list of 27 variants that grew one need at a
time. Library code (non-test `src/`, binaries included) builds a `General` error at about 636
sites: 625 `Error::general_error(…)` calls plus 11 `Error::from_error(ErrorType::General, …)`.
By crate (at cd6d5da):

| Crate | `general_error` | Crate | `general_error` |
|---|---|---|---|
| `liquers-lib` | 221 | `liquers-web` | 15 |
| `liquers-records` | 205 | `liquers-mcp` | 8 |
| `liquers-core` | 145 | `liquers-py` | 8 |
| `liquers-template` | 18 | `liquers-agent` 3, `liquers-store` 2 | 5 |

So `General` covers very different failures: a missing recipe (`recipes.rs`, three providers), a
dependency that produced no value (`assets.rs`), a column a command names that the frame lacks
(`polars/filtering.rs`), an argument out of its domain (`image/*.rs`, about 35 sites), a malformed
Arrow file (`records/formats/ipc.rs`, 42), an index out of range (`records/views.rs`), a failed
channel send (`assets.rs`, `context.rs`). A caller cannot tell these apart by type, Python cannot
catch them separately, and HTTP maps all of them to 500.

Two existing types are also overloaded or dead:

- `SerializationError` means both "this type cannot be written in this format" (a capability
  refusal, which `AssetRef::classify_persistence_error` turns into the non-error
  `PersistenceStatus::NonSerializable`) and "encoding or decoding failed" (a real failure). A
  genuine encode failure is therefore silently recorded as "not serializable".
- `CacheNotSupported` has no producer outside `error.rs` (`Error::cache_not_supported` is never
  called).

`key-without-recipe-answer` (PR #179) added the first error categories (`ErrorCategory::Store`,
`KeyedAsset`, `NotFound`; a type may be in several; mirrored as Python base classes and a
JavaScript `categories` getter). Every variant outside the store and keyed-asset misses is in no
category.

## Problem Example

```text
GET /liquer/q/-R/photo.png/-/ns-img/erode-0
```

Today: `erode` returns `Error::general_error("Erosion radius must be greater than 0")`; the answer
is HTTP 500 with `error.type` `General`, Python raises `GeneralError` (catchable only as
`LiquersError`), and JavaScript reports `errorType = "general"` with no categories. Should be: a
`ParameterError` (HTTP 400, Python `ParameterError`, a member of an "invalid input" category), the
same as an argument the parser rejects.

## Impact

Error handling in Rust, Python and JavaScript has to match on message text, or catch
`LiquersError` broadly, whenever the failure is `General`. HTTP clients get 500 for errors that are
the client's fault or are simply "not here". Nothing is broken, but each new feature that needs to
react to a specific failure has to add a type ad hoc, which is how the list became flat.

## Expected behaviour

Review every `ErrorType` and the `General` call sites, cluster them, and propose:

- new, more detailed types where a cluster of `General` sites shares a meaning. A good type is
  reusable across sites. A type that occurs at only one place is acceptable only when it is a
  clearly distinct error, and the review must say so;
- the category of each existing variant (a variant may be in several, as `KeyNotFound` is in
  `Store` and `NotFound`), and any further categories, keeping Python's generated classes and
  JavaScript's `categories` in step;
- whether `NotSupported` / `AssetNotSupported` / `KeyNotSupported` and `General` keep their
  current boundaries, including the degraded stored value (`AssetNotSupported` by decision of
  2026-10-25, explicitly open to this review);
- the HTTP status of each type in `liquers-axum/src/api_core/error.rs`.

## Discovery

Orest, 2026-10-25, deciding open question 4 of `specs/design/key-without-recipe-answer/`: the
degraded stored value's type should be settled there for now, but all errors, and `General`
especially, should be reviewed, clustered and refined under their own issue.

2026-10-26 (bulk design, compaction): counts re-measured at cd6d5da (the filed "25 variants" and
"about 735 sites" counted test modules); the `SerializationError` overload and the dead
`CacheNotSupported` added. Designed together with `LIBRARY-CODE-CALLS-ERROR-NEW-DIRECTLY`
(`overlap.md` T2 and T3) in `specs/design/error-type-taxonomy/`.
