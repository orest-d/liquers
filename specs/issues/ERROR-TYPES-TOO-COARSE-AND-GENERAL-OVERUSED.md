---
id: ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED
kind: issue
title: Error types are not clustered, and General stands in for hundreds of distinct failures
status: draft
priority: P2
complexity: L
area: [core/error, py, web, axum]
design:
created: 2026-10-25
github:
---

## Problem

`ErrorType` (`liquers-core/src/error.rs`) is a flat list of 25 variants that grew one need at a
time. About 735 call sites in library code build `Error::general_error` (`grep -rn general_error
liquers-*/src`), so `General` covers very different failures: a missing recipe, an unreadable stored
type (`AssetManager::get_any_status`), a stale dependency, a corrupted outside change. A caller
cannot tell these apart by type, Python cannot catch them separately, and HTTP maps all of them
to 500.

`key-without-recipe-answer` adds the first error categories (`ErrorCategory::Store`, `KeyedAsset`,
`NotFound`, a type belonging to any number of them, mirrored as Python base classes and a
JavaScript `categories` getter) and two keyed-asset types (`AssetNotFound`, `AssetNotSupported`).
Every other variant is left in no category.

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
