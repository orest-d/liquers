# Phase 2: Solution & Architecture — Error type taxonomy

## Overview

Extend `ErrorType` with eight types and `ErrorCategory` with five categories, remove the dead
`CacheNotSupported`, give every type a typed constructor, make `Error::new` private to `error.rs`,
and split `SerializationError` into an encode/decode failure and a capability refusal
(`FormatNotSupported`). Then re-type every library `General` site by one ordered rule, crate by
crate, and close the door with a CI script. Counts are at cd6d5da, non-test `src/` (binaries
included); "≈" marks a count from message clustering, settled site by site during migration.

Rejected: removing `General` (foreign code and external crates need a catch-all; Q5); one type
per subsystem (`RecordsError`, `ImageError` — a caller wants "what went wrong", which the
categories and types already give, not "which crate"); error codes or a numeric hierarchy (the
category graph already allows overlap and is mirrored by the bindings); clippy
`disallowed-methods` as the guard (cannot see `from_error(ErrorType::General, …)`, and needs the
clippy CI run of `library-no-panic-lint`).

## Known-Issue Preflight

| Issue | Status | Impact | Fix first? | Action |
|---|---|---|---|---|
| `ERROR-WITH-KEY-SETS-QUERY-FIELD` (`error-with-key-field`) | draft | `with_key` writes `query`; constructors here set `key` directly, as `asset_not_found` does | No | None; textual conflicts in `error.rs` only |
| `BUILD-MATRIX-CI-SKIPS-NEWER-CRATES` | draft | the guard's CI step skips template/mcp/agent-only changes | No | Noted in Stage 6 |
| `PY-METADATA-METHODS-RAISE-PLAIN-EXCEPTION` | draft | py metadata methods bypass `LiquersError` | No | Independent |

## Interfaces

`liquers-core/src/error.rs`:

```rust
pub enum ErrorType {
    // existing, minus CacheNotSupported; plus:
    ItemNotFound, OutOfRange, InvalidData, FormatNotSupported,
    RecipeNotFound, DependencyFailed, AlreadyExists, PayloadMissing,
}
pub enum ErrorCategory { Store, KeyedAsset, NotFound, InvalidInput, Unsupported, Conflict, Dependency, Data }
impl ErrorCategory { pub const ALL: [ErrorCategory; 8]; }
impl ErrorType { pub const ALL: [ErrorType; 34]; }      // declaration order; guards the binding lists

impl Error {
    fn new(error_type: ErrorType, message: String) -> Self;   // was pub
    pub fn parameter_error(message: String) -> Self;
    pub fn argument_missing(message: String) -> Self;          // plan.rs ParameterValue::to_result
    pub fn serialization_error(message: String) -> Self;
    pub fn format_not_supported(message: String) -> Self;
    pub fn item_not_found(message: String) -> Self;
    pub fn out_of_range(message: String) -> Self;
    pub fn invalid_data(message: String) -> Self;
    pub fn recipe_not_found(key: &Key) -> Self;                // sets `key`; "No recipe found for key …"
    pub fn dependency_failed(dep_key: &DependencyKey, message: impl Into<String>) -> Self; // like dependency_cycle
    pub fn already_exists(message: String) -> Self;
    pub fn payload_missing(message: String) -> Self;
    pub fn not_available(message: String) -> Self;             // was (), no callers outside error.rs
    // removed: cache_not_supported()
}
```

`from_error` stays for wrapping an external error under a chosen type; `general_error` stays
`pub` for code outside the workspace (Q5).

Types, categories and HTTP status (changes in bold; Python extra base in brackets):

| Type | Categories | HTTP |
|---|---|---|
| `ArgumentMissing`, `TooManyParameters`, `ParameterError`, `ParseError`, `ActionNotRegistered`, `UnknownCommand` | InvalidInput | 400 |
| `KeyNotAbsolute` | Store, InvalidInput | 400 |
| **`PayloadMissing`** | InvalidInput | **400** |
| **`ItemNotFound`** | NotFound, InvalidInput | **422** |
| **`OutOfRange`** | InvalidInput | **422** |
| `ConversionError` | InvalidInput, Data | 422 |
| `SerializationError` (narrowed: encoding or decoding failed) | Data | 422 |
| **`InvalidData`** | Data | **422** |
| **`FormatNotSupported`** | Unsupported, Data | **415** |
| `NotSupported` (absorbs `CacheNotSupported`) | Unsupported | 501 |
| `KeyNotSupported` | Store, Unsupported | 404 |
| `AssetNotSupported` | KeyedAsset, Unsupported | 404 |
| `KeyNotFound` | Store, NotFound | 404 |
| `AssetNotFound` | KeyedAsset, NotFound | 404 |
| **`RecipeNotFound`** | NotFound | **404** |
| `KeyReadError`, `KeyWriteError` | Store | 500 |
| `StatusConflict`, `CommandAlreadyRegistered` | Conflict | 409 |
| **`AlreadyExists`** | Conflict | **409** |
| `DependencyVersionMismatch` | Dependency, Conflict | 409 |
| `DependencyCycle` | Dependency | 409 |
| **`DependencyFailed`** | Dependency | **424** |
| `NotAvailable` | — | 404 |
| `General`, `ExecutionError`, `UnexpectedError`, `ConfigurationError` | — | 500 |
| `Cancelled` | — | 499 |

Python: `InvalidInputError` gains `ValueError` (`_CATEGORY_EXTRA_BASES`), beside `NotFoundError`
(`LookupError`); `ItemNotFoundError` derives from both, which CPython allows (same base layout).
JavaScript names: `invalid_input`, `unsupported`, `conflict`, `dependency`, `data`; types
`item_not_found`, `out_of_range`, `invalid_data`, `format_not_supported`, `recipe_not_found`,
`dependency_failed`, `already_exists`, `payload_missing`.

## Integration Points

**The classification rule.** Each `General` site takes the first type whose question it answers:

1. A name or handle looked up inside a value or structure is absent (column, field, section,
   collection, map key, UI node, MCP tool) → `ItemNotFound`. A store key → `KeyNotFound`; a recipe
   → `RecipeNotFound`; a command → `ActionNotRegistered`.
2. An index, offset, length or count is outside its bounds → `OutOfRange`.
3. Text the caller or a configuration wrote does not parse (expiration spec, shortcut, regex,
   YAML or JSON given as an argument, a command spec) → `ParseError`.
4. An argument value is outside its domain, or arguments are missing, empty or inconsistent →
   `ParameterError`; text that does not convert to the column's type → `ConversionError`.
5. Bytes ↔ value conversion failed → `SerializationError`; the type and format do not go
   together at all → `FormatNotSupported`.
6. A value's content contradicts its declared schema or its own invariants (column buffers,
   batch shape, manifest, metadata record, a file's fields against the declared schema) →
   `InvalidData`.
7. The operation never applies to this type, variant or build (legacy metadata, ordering a
   binary column, `draw_text`) → `NotSupported`.
8. The asset's status forbids it now → `StatusConflict`; the thing to create exists →
   `AlreadyExists`.
9. A required payload is absent → `PayloadMissing`.
10. A dependency ended without a value → `DependencyFailed`; a cycle → `DependencyCycle`.
11. A builder, store factory, binary flag or config file is unusable → `ConfigurationError`.
12. The environment, a context or a browser service is gone or inaccessible → `NotAvailable`.
13. A third-party operation failed while a command ran (polars compute, SVG render, MCP server) →
    `ExecutionError` (an encoder failure is rule 5).
14. Cannot happen in a correct program (channel closed, task join, lost frame, caught panic,
    inconsistent plan) → `UnexpectedError`.

**Migration map** (dominant targets per area; `General` sites):

| Area | Sites | Targets |
|---|---|---|
| core `assets.rs` | 36 | UnexpectedError ≈11, StatusConflict ≈6, InvalidData ≈8, DependencyFailed 4, ParameterError 3, NotSupported, NotAvailable |
| core `metadata.rs` | 34 | NotSupported ≈28 (legacy metadata), SerializationError 4, UnexpectedError 2 |
| core `expiration.rs`, `parse.rs`, `validate/`, bin | 33 | ParseError ≈22, AlreadyExists 2, ConfigurationError ≈5, SerializationError ≈4 |
| core `commands.rs`, `context.rs`, `interpreter.rs`, `plan.rs` | 26 | UnexpectedError ≈14, PayloadMissing ≈5, ParameterError 3, DependencyCycle 1, NotSupported 1 |
| core `recipes.rs`, builders, configs, rest | 23 | RecipeNotFound 5, ConfigurationError ≈6, SerializationError 4, InvalidData 2, … |
| records `formats/*` | 88 | SerializationError ≈60, InvalidData ≈20, FormatNotSupported 2, OutOfRange 3 |
| records `column`, `views`, `batch`, `mutable`, `buffer`, `schema` | 86 | OutOfRange ≈40, InvalidData ≈35, NotSupported 4, ParameterError 3, ItemNotFound 1 |
| records `manifest`, `sources`, `provider`, `search/*` | 33 | InvalidData ≈15, ItemNotFound 3, ParseError 2, AlreadyExists 2, RecipeNotFound 1, … |
| lib `polars/*` | 49 | ExecutionError ≈19, ConversionError ≈12, ParseError ≈6, ItemNotFound 3, … |
| lib `image/*` | 45 | ParameterError ≈32, FormatNotSupported 3, SerializationError 3, ExecutionError 3, NotSupported 1 |
| lib `ui/*`, `egui` | 56 | ItemNotFound ≈35, ParseError 7, ParameterError ≈5, PayloadMissing 3, … |
| lib `mem/*`, `records/*`, `search/*`, rest | 73 | ParameterError, ItemNotFound, AlreadyExists, InvalidData, OutOfRange, ParseError |
| template, web, py, mcp, agent, store | 54 | ItemNotFound ≈10, ConfigurationError ≈11, NotAvailable ≈6, ParameterError ≈8, … |

**`SerializationError` split.** Every existing `SerializationError` producer (≈70, among them the
19 `Error::new` sites) is re-read: "serialization to F not supported by T", "unsupported format",
"unsupported type identifier", an opaque foreign value refusing bytes (`liquers-web`
`default_value.rs`, `liquers-py` value bridge) → `FormatNotSupported`; a serde, encoder or parser
failure → `serialization_error`.

**Places every variant change touches:** `error.rs` (enum, `categories`, `ALL`, tests);
`assets.rs` `classify_persistence_error`; `liquers-axum/src/api_core/error.rs`
(`error_to_status_code`, `parse_error_type`, `keyabs15b` over `ErrorType::ALL`);
`liquers-py/src/error.rs` (pyclass enum, both `From` impls, `category_name`) and
`python/liquers/_errors.py` (`_CATEGORY_EXTRA_BASES`); `liquers-web/src/error.rs` (both name
tables, `error_category_name`) and `tests/objects_OBJECT.rs` (`error_types!` list).

## Error Handling

`classify_persistence_error`: `FormatNotSupported` → `NonSerializable`; every other type,
`SerializationError` included, → `NotPersisted` (Q3). `liquers-web` `is_serialization_refusal`
tests `FormatNotSupported`. `liquers-py` `PyObjectValue::as_bytes`: pickle off or a non-pickle
format → `FormatNotSupported`; a `pickle.PicklingError` or `TypeError` from `pickle.dumps` (this
object has no byte form, such as a lambda) → `FormatNotSupported` too, so an unpicklable result
stays a silent `NonSerializable` as today; any other exception → `SerializationError`. No other control flow branches on a re-typed type (`General` is never
matched; `KeyNotFound`, `KeyNotSupported`, `StatusConflict` keep their producers).

## Relevant Commands

No signature changes. 51 `#[command_version]` bodies in `liquers-lib` change, so Stage 4
regenerates `specs/command_registry.yaml` (`export-command-registry`) with a CHANGELOG line.

## Documentation Architecture

`WEB_API_SPECIFICATION.md` §3.1 status table; `PYTHON_INTEGRATION.md` and `PYTHON_GUIDE.md`
category classes; `LANGUAGE-INTEGRATION_GUIDE.md` category list; `PROJECT_OVERVIEW.md` error
handling paragraph; `DOC_03` persistence statuses; `CLAUDE.md` Error Handling gains the
classification rule's short form and "never `general_error` in workspace code". Each reference or
guide change adds a History row (§9.2).

## Risks

| Risk | Where | Mitigation | Certainty |
|---|---|---|---|
| Missed exhaustive match or name table | 5 crates | `ErrorType::ALL` drives axum, web and py tests; build matrix; py and web loops | High |
| Tests pinned on `General` or messages | ≈19 test files, ≈130 in-module asserts | each stage updates the asserts of the sites it re-types; messages unchanged | High |
| Persistence behaviour change (Q3) | `assets.rs` | AC-6 tests both directions; an encode failure becomes a logged `NotPersisted` | Medium |
| Wrong type chosen at a site | all crates | the ordered rule; review per stage; a wrong type is a one-line fix | Medium |
| Registry impl versions | `liquers-lib` | regenerate in Stage 4; `registry_export` fails otherwise | High |
| HTTP clients relying on 500 | axum users | none needed (no compatibility promise); documented in §3.1 | High |
| Persisted `error_data` with `CacheNotSupported` | stores | no producer ever wrote it | High |
| Overlapping designs add variants concurrently | `error.rs`, bindings | rebase; the second places its variant in the tables | Medium |

Recovery: each stage is one PR and reverts alone; Stage 1 is the only one others depend on.
