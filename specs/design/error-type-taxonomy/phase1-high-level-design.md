# Phase 1: High-Level Design — Error type taxonomy

## Purpose

Give every library error a type that says what went wrong, and group the types into overlapping
categories a caller can catch at once, so Rust, Python, JavaScript and HTTP clients react to a
failure without reading its message. Today `General` stands in for about 636 distinct library
failures and every one of them is an HTTP 500. This serves VISION's "server / data service" and
"embedded in Python" uses: a client must tell "you asked wrong" from "not here" from "we broke".

## Problem Example

```text
GET /liquer/q/-R/photo.png/-/ns-img/erode-0
GET /liquer/q/-R/data/x.csv/-/ns-rec/select_columns-nosuch
```

Today both fail with `General` (`image/morphology.rs` "Erosion radius must be greater than 0";
`records/views.rs` "RecordView::select_columns: no field named 'nosuch'"): HTTP 500, Python
`GeneralError` (only `LiquersError` catches it), JavaScript `"general"` with no categories.
Should be: `ParameterError` (400) and `ItemNotFound` (422); in Python both are `InvalidInputError`
(a `ValueError`), the second also a `NotFoundError` (a `LookupError`).

## Scope and Acceptance Criteria

- **AC-1** Argument out of its domain
  WHEN a command rejects an argument value (`ns-img/erode-0`, a negative sigma, an unknown orient)
  THEN the error is `ParameterError`, HTTP 400, in category `InvalidInput`
- **AC-2** No `General` from library code
  WHEN the library crates (`src/`, outside test modules) are checked by `scripts/check_error_construction.py`
  THEN no call to `Error::general_error` or `Error::from_error(ErrorType::General, …)` remains, and a new one fails the check and CI
- **AC-3** `Error::new` unreachable outside `error.rs`
  WHEN code outside `liquers-core/src/error.rs` calls `Error::new`
  THEN it does not compile; the 19 former sites use typed constructors and keep their type
- **AC-4** Category graph
  WHEN `ErrorType::categories()` is asked for any type
  THEN it answers the Phase 2 table: eight categories (`Store`, `KeyedAsset`, `NotFound`, `InvalidInput`, `Unsupported`, `Conflict`, `Dependency`, `Data`); Python has one base class per category and JavaScript reports the same names in `categories`
- **AC-5** HTTP status per type
  WHEN an error of each type reaches `liquers-axum`
  THEN the status is the one in the Phase 2 table, and `parse_error_type` round-trips every type
- **AC-6** Capability refusal is not a failure; an encode failure is
  WHEN a value's type cannot be written in the requested format
  THEN the error is `FormatNotSupported` and persistence records `NonSerializable`;
  WHEN encoding a value fails (`write_table` with a non-finite float in JSON)
  THEN it is `SerializationError` and persistence records `NotPersisted`
- **AC-7** Missing item inside a value
  WHEN an operation names a column, field, section, collection, map key or UI node the value lacks
  THEN the error is `ItemNotFound` (422), in `NotFound` and `InvalidInput`
- **AC-8** Recipe provider miss
  WHEN no recipe provider has a recipe for a key
  THEN the provider answers `RecipeNotFound`; an evaluation of that key still answers `AssetNotFound` (`key-without-recipe-answer` AC-1 unchanged)
- **AC-9** Upstream failure
  WHEN a dependency an evaluation waited for ends without a value
  THEN the error is `DependencyFailed` (424), in category `Dependency`
- **AC-10** Dead type removed
  WHEN the bindings list error types
  THEN `CacheNotSupported` is absent and `NotSupported` covers it
- **AC-11** Data errors are typed by layer
  WHEN a table file is truncated or malformed
  THEN `SerializationError`; WHEN a well-formed value contradicts its declared schema or invariants (a non-nullable column with nulls, a column of the wrong type)
  THEN `InvalidData` (422)

Non-goals: error payload fields (`error-with-key-field`), structured tracebacks
(`MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING`), language exception class and stack
(`LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`), message wording, errors raised by user commands.
No wire or serde backward compatibility (maintainer, 2026-10): a persisted `error_data` naming a
removed variant fails to read, as for any unknown variant.

## Core Interactions

- Error (`liquers-core`): new variants and categories, typed constructors, `Error::new` private.
- Assets: persistence classification of `FormatNotSupported` versus `SerializationError`.
- Every library crate: about 636 `General` sites and 19 `Error::new` sites re-typed.
- Web/API: HTTP status table; bindings: Python classes and JavaScript names follow the enum.
- Commands: bodies of `version: auto` commands change, so `specs/command_registry.yaml` is
  regenerated (no signature changes).

## Crate Placement

The vocabulary lives in `liquers-core/src/error.rs` (every crate already depends on it). The
re-typing happens where each error is raised; bindings and `liquers-axum` only map.

## Documentation Intent

- Reference: `WEB_API_SPECIFICATION.md` §3.1; `PYTHON_INTEGRATION.md` error classes;
  `PROJECT_OVERVIEW.md` error handling; `api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` persistence.
- Guide: `LANGUAGE-INTEGRATION_GUIDE.md` (categories); `PYTHON_GUIDE.md` (catching); `CLAUDE.md`
  Error Handling (the classification rule and banned constructors).
- Other documents: `liquers-axum/README.md`, `liquers-py/README.md` if they list types.
- Documents to update: both source issues (closed by the final stage).

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — complexity `L`; new enum variants and a removed one, new
  `pub` constructors, a private `Error::new`, binding and HTTP changes (rules 1, 3, 4, 6).
- **Leading issue:** Q1, the set of new types.
- **Explanation:** the inventory resolves every site into the Phase 2 rule table under the
  recommended answers, and each stage is a mechanical migration checked by the compiler and a CI
  script. The type, category and status vocabularies are user-visible, so they stay decisions.
- **Open questions:**
  1. **Open design - new types:** add `ItemNotFound`, `OutOfRange`, `InvalidData`,
     `FormatNotSupported`, `RecipeNotFound`, `DependencyFailed`, `AlreadyExists`, `PayloadMissing`
     (recommended; each covers 4-100 sites, except `RecipeNotFound` (6) and `DependencyFailed` (4),
     kept because a caller must tell them from `AssetNotFound` and `ExecutionError`). Smaller
     alternative: fold `OutOfRange` into `ParameterError` and `InvalidData` into
     `SerializationError` — then a corrupt Arrow file and a column a query mistyped share a type.
  2. **Open design - categories:** add `InvalidInput`, `Unsupported`, `Conflict`, `Dependency`,
     `Data` (recommended). Python extra bases: `InvalidInputError` is a `ValueError` (as
     `NotFoundError` is a `LookupError`); no others.
  3. **Open design - persistence:** `FormatNotSupported` becomes the only `NonSerializable` cause;
     a `SerializationError` while persisting is `NotPersisted` and logged (recommended; today every
     `SerializationError` is silently `NonSerializable`).
  4. **Open design - HTTP:** 422 for `ItemNotFound`, `OutOfRange`, `InvalidData`; 415
     `FormatNotSupported`; 404 `RecipeNotFound`; 424 `DependencyFailed`; 409 `AlreadyExists`; 400
     `PayloadMissing` (recommended). Unchanged for existing types.
  5. **Open design - `General`:** keep the variant and `general_error` for code outside the
     workspace and for foreign errors; refuse both in workspace library code by a CI check (recommended),
     rather than removing `General`.
  6. **Proposed resolution - `Error::new`:** module-private (recommended): the compiler enforces
     the rule; `from_error` stays for wrapping external errors.
  7. **Proposed resolution - removals:** remove `CacheNotSupported` (no producer); keep
     `AssetNotSupported` for the degraded stored value (now also in `Unsupported`); keep
     `CommandAlreadyRegistered` beside `AlreadyExists` (a registry clash, mapped by bindings).

## Design Dependencies

- `key-without-recipe-answer` — **requires** (complete; E4, not extended): defined
  `ErrorCategory` and the binding mirrors this design extends; AC-8 keeps its AC-1.
- `library-no-panic-lint` — **overlaps**: another library-code convention guard. This design's
  guard is a script, not clippy, so neither orders the other. `BUILD-MATRIX-CI-SKIPS-NEWER-CRATES`
  (issue) limits it the same way: until fixed, a change confined to `liquers-template`, `-mcp` or
  `-agent` does not run the check in CI.
- `access-policy-at-entry-points` (`AccessDenied`), `store-conditional-write` (possible
  `WriteConflict`) — **overlaps**: whichever lands second places its variant in the category and
  status tables. No order.
- `error-with-key-field` — **overlaps**: same files (`error.rs`, binding error tables), different
  fields; textual conflicts only.
- `core-value-json-identifier-dispatch`, `value-serialization-writer`,
  `value-serialization-streaming`, `polars-dataframe-ipc` — **overlaps**: new serializer arms
  should use `FormatNotSupported` / `Error::serialization_error`; if they land first, Stage 1
  re-types their arms too.
- `environment-orderly-shutdown` — **overlaps**: uses `NotAvailable` for a shut-down environment,
  consistent with the rule table.
- Issues `MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING`, `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`,
  `PY-METADATA-METHODS-RAISE-PLAIN-EXCEPTION` — **overlaps**: same binding modules, independent.

## Consolidated Findings

- Two sources, merged 2026-10-26 (`overlap.md` T2 + T3): the 19 `Error::new` sites are
  `SerializationError` (18) and `ArgumentMissing` (1); which typed constructor replaces them is this
  design's split of `SerializationError`, and its Stage 1 rewrites the same arms. E1 does not apply:
  the issue spans `liquers-core` and `liquers-lib` (auto-fix rule 6).
- `SerializationError` is overloaded and drives behaviour: `AssetRef::classify_persistence_error`
  maps all of it to `NonSerializable` (documented in `DOC_03` as "serialization is unsupported for
  the value representation"), and `liquers-web` `is_serialization_refusal` tests it. The split must
  re-type every existing `SerializationError` site, not only `General` ones (about 70 producers).
- Code branches on type in few places (`KeyNotFound` ×20, `KeyNotSupported` ×2, `StatusConflict`,
  `SerializationError`, `NotFound` category); none branches on `General`, so re-typing changes no
  control flow except persistence (Q3).
- 51 of the 100 `#[command_version]` command bodies in `liquers-lib` hold `General` sites (66;
  image, polars, ui, lib commands): Stage 4 changes their `impl_version` and regenerates
  `specs/command_registry.yaml` (`committed_registry_impl_versions_are_fresh`). Helpers they call
  do not count.
- Adding a variant touches eight places (`error.rs` enum and `categories`, `assets.rs`
  persistence match, axum status and parser, py enum and both conversions, web names both ways,
  and the variant lists in `objects_OBJECT.rs` and `keyabs15b`); a `pub const ErrorType::ALL`
  removes the hand-kept lists.
- Order: Stage 1 (vocabulary, bindings, `Error::new`, the `SerializationError` split) first;
  Stages 2-5 per crate in any order; Stage 6 (the guard) last. Each stage is one PR.
