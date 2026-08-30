# Phase 2: Solution and Architecture - Error Traceback in Metadata Log Entries

## Overview

Use the existing `LogEntry.traceback: Option<String>` serialization slot and add the missing source
field on `ErrorPayload` only if code inspection confirms no existing error-level traceback storage.
`LogEntry::from_error` copies that optional value; no public metadata field is removed or renamed.

## Known-Issue Preflight

| Issue | Status | Priority | Relevance and action | Blocking? |
|---|---|---|---|---|
| `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` | accepted | P3 | Same long-term area, but broader language exception transport is outside this `S` repair. Keep this design string-only and do not block on it. | no |
| `CORE-ERROR-PAYLOAD-SIZE` | draft | P2 | `Error` is boxed at HEAD; adding an optional field to `ErrorPayload` should preserve the one-pointer `Error` size. Add a size regression if that issue has tests. | no |

## Files and Symbols

Likely source file: `liquers-core/src/error.rs`, extending `ErrorPayload` with
`traceback: Option<String>` plus builder/access path such as `with_traceback`. Likely conversion
file: `liquers-core/src/metadata.rs`, updating `LogEntry::from_error`. Tests should live near the
existing metadata or error tests in `liquers-core`.

## Data, Ownership, Serialization and Errors

The traceback is owned `String` inside `Option` and derives with `Serialize`/`Deserialize`.
Use `#[serde(default)]` if the field is added to `ErrorPayload` so old serialized errors remain
readable. No new error type is introduced; constructors remain `Error` typed constructors.

## Sync, Async and API Effects

The conversion is synchronous and CPU-only. The API effect is additive: serialized errors may gain
`traceback`, and log entries may start populating their existing `traceback` key.

## Alternatives

Rejected: serializing `std::error::Error::source()` directly in `LogEntry::from_error`; the current
`Error` type does not preserve a source object through serde and command boundaries. Rejected:
structured frame arrays; useful later, too large for this issue.

## Risk Assessment

| Assessment | Record |
|---|---|
| Files | 2 source files (`error.rs`, `metadata.rs`), 1 focused test location, issue/design/index specs. |
| Impact area | Error serialization and metadata logs consumed by assets, UI and bindings. |
| Module/crate reach | Confined to `liquers-core`. |
| Existing-test breakage | Low; serde snapshot tests, if any, may need expected optional field handling. |
| New validation | Unit test: error with traceback becomes `LogEntry.traceback`; old JSON without it deserializes. |
| Behavioural risk | Compatibility low due optional/default field; no persistence migration; no concurrency/performance/security concern. |
| Recovery | Revert the field and `from_error` copy; old metadata remains readable either way. |
| Certainty | High that `LogEntry` already has the target slot; moderate on exact builder naming until implementation. |

## Rust Review

Ownership is simple owned data, no new trait bounds, no panic path, no async work, and no crate
dependency change. The design follows the typed `Error` convention and avoids a default enum match.
