# PYTHON-BASIC-OBJECTS

Status: Draft

## Summary
Define a first Python-wrapper feature for **basic Liquers core objects** that are:
1. non-generic
2. synchronous (no async functionality)
3. testable as standalone wrappers

The feature introduces wrappers for objects such as `Query`, `Key`, and `CommandMetadata`, and defines how trait coverage is organized by `liquers-core` module.

## Resolved Decisions
1. Feature documents live under `specs/FEATURES` (current repository layout).
2. Wrapper modules follow strict parity with `liquers-core`, with one exception: `query` and `parse` are merged for ergonomic query construction.
3. `Store` and `BinCache` are out of scope (obsolete; planned phase-out). `Session` is out of scope for this feature.
4. Trait wrappers in this feature target Rust implementations only (no Python-defined implementations).

## Goals
1. Provide thin, predictable Python wrappers over selected `liquers-core` types.
2. Keep Python module structure aligned with `liquers-core` module structure.
3. Make wrappers deterministic and easy to test (Rust unit tests + Python tests).
4. Defer async and generic services to later wrapper features.

## Scope
In scope:
1. Non-generic structs/enums from `liquers-core` that do not require async services.
2. Non-generic traits without async behavior, where wrapper behavior is clear.
3. Roundtrip conversion APIs: Rust type <-> Python wrapper.
4. Basic object behavior: construction, parse/encode, equality, repr/str, serialization-friendly access.

Out of scope:
1. Async traits/services (`AsyncStore`, `AsyncRecipeProvider`, async `AssetManager` API paths).
2. Generic traits/structures (`State<V>`, `EnvRef<E>`, `CommandRegistry<E>`, `Cache<V>`, etc.).
3. Python command execution bridge (`pycall`, async callable execution).
4. Full value-extension interoperability (`PythonValueExtension<E>`) and runtime/service wrappers.

## Python Module Structure
Python wrapper modules mirror `liquers-core` modules for wrapped objects.
Exception: `query` also exposes parse helpers, so users can construct queries ergonomically in one place.

Proposed module mapping (this feature):
1. `liquers_py.query` (includes parse helpers)
2. `liquers_py.command_metadata`
3. `liquers_py.metadata`
4. `liquers_py.error`
5. `liquers_py.expiration`
6. `liquers_py.dependencies`
7. `liquers_py.plan`
8. `liquers_py.recipes` (basic recipe objects only)
9. `liquers_py.parse` is not exposed as a separate user-facing module in this feature.

Future extension:
1. Thin ergonomic layers may be added later (aliases/helper modules) without changing the strict-parity core wrappers.

## Wrapper Contract (Testability-First)
Each basic wrapper class should follow the same contract:
1. Hold exactly one inner Rust value (newtype wrapper pattern).
2. Provide deterministic constructors and conversion helpers (`from_*`, `to_*`, `encode()`).
3. Implement stable object behavior (`__repr__`, `__str__`, `__richcmp__`, `__hash__` where supported).
4. Avoid hidden global state or runtime handles in these wrappers.
5. Expose errors as typed Liquers Python exceptions (not generic `PyException`).

## Initial Wrapper Object Set
Minimum required wrappers in this feature:
1. `query`: `Position`, `ActionParameter`, `ActionRequest`, `ResourceName`, `SegmentHeader`, `TransformQuerySegment`, `ResourceQuerySegment`, `QuerySegment`, `QuerySource`, `Key`, `Query`
2. `command_metadata`: `CommandKey`, `CommandMetadata`, `CommandMetadataRegistry`, `ArgumentInfo`, `ArgumentType`, `EnumArgument`, `EnumArgumentType`, `CommandDefinition`, `CommandPreset`
3. `metadata`: `Version`, `DependencyKey`, `DependencyRecord`, `Status`, `LogEntryKind`, `LogEntry`, `ProgressEntry`, `AssetInfo`, `MetadataRecord`, `Metadata`
4. `error`: `ErrorType`, `Error`
5. `expiration`: `Expires`, `ExpirationTime`
6. `dependencies`: `DependencyRelation`, `PlanDependency`
7. `plan`: `Step`, `ParameterValue`, `ResolvedParameterValues`, `Plan`
8. `recipes`: `Recipe`, `RecipeList`

## Trait Overview By Module
Only traits that are relevant to wrapper planning are listed.

### query
1. `QueryRenderStyle` -> Out of scope in this feature
2. `QueryRenderer` -> Out of scope in this feature
3. `TryToQuery` -> In scope as conversion behavior (`str`/`Query`/`Key` parsing helpers)

### command_metadata
1. No trait definitions in module (object wrappers only)

### metadata
1. No trait definitions in module (object wrappers only)

### error
1. No trait definitions in module (object wrappers only)

### expiration
1. No trait definitions in module (object wrappers only)

### dependencies
1. No trait definitions in module (object wrappers only)

### plan
1. No trait definitions in module (object wrappers only)

### recipes
1. `AsyncRecipeProvider<E>` -> Out of scope (generic + async)

### store
1. `Store` -> Out of scope in this feature (obsolete, planned phase-out)
2. `AsyncStore` -> Out of scope (async)

### context
1. `Session` -> Out of scope in this feature
2. `Environment` -> Out of scope (generic + async futures)

### cache
1. `BinCache` -> Out of scope in this feature (obsolete, planned phase-out)
2. `Cache<V>` -> Out of scope (generic)

### commands
1. `PayloadType` -> Out of scope (marker trait, tied to command runtime)
2. `ExtractFromPayload<P>` -> Out of scope (generic)
3. `InjectedFromContext<E>` -> Out of scope (generic)
4. `CommandExecutor<E>` -> Out of scope (generic service/runtime)

### value
1. `ValueInterface` -> Out of scope in this feature (broad value/runtime contract; handled in value-wrapper feature)
2. `DefaultValueSerializer` -> Out of scope in this feature

### assets
1. `AssetManager<E>` -> Out of scope (generic + async)

## Testing Strategy
Rust-side tests (in `liquers-py`):
1. Wrapper roundtrip: core type -> wrapper -> core type (no data loss).
2. Parse/encode idempotency for query/key wrappers.
3. Equality/hash consistency with underlying Rust behavior.
4. Error conversion for invalid constructors/parsers.

Python-side tests (`pytest`):
1. Construction and conversion tests for `Query`, `Key`, `CommandMetadata`.
2. `str`/`repr`/`==` behavior tests.
3. Invalid parse/error-path tests produce typed Liquers exceptions.
4. Module-layout tests: wrappers are importable from module names matching core modules.

Acceptance criteria for this feature:
1. `Query`, `Key`, and `CommandMetadata` wrappers are complete and tested.
2. The initial wrapper object set has at least constructor + encode/serialize path + equality behavior.
3. No async/generic service wrappers are introduced in this feature.
4. Wrapper module names follow `liquers-core` module naming, with the `query`+`parse` merge exception.

## Open Questions / Ambiguities
1. None currently.
