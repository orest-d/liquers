# Phase 1: High-Level Design - Error Traceback in Metadata Log Entries

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Proposed resolution - traceback representation:** Store one optional UTF-8
  traceback string on `ErrorPayload` and copy it to the existing `LogEntry.traceback` field.
- **Explanation:** The additive string solution is implementable and backward compatible, but it
  establishes a public serialized error contract that later language bindings must follow.
- **Open questions:** **Proposed resolution - traceback representation:** Approve a rendered string
  for this S-sized change; structured frames remain with `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`.

## Problem and Evidence

`LogEntry::from_error` in `liquers-core/src/metadata.rs` preserves message, query and position, but
still has a TODO where traceback support should be wired. Metadata therefore records that an asset
failed without preserving structured failure context.

## Expected Behaviour and Acceptance Criteria

An error log entry carries an optional traceback string when the originating `Error` provides one.
Existing metadata without a traceback continues to deserialize, and errors without traceback keep
the current message-only behaviour.

## Affected Systems

Core error and value metadata are affected. Query, Store, Commands, Assets, bindings and UI only
consume the serialized metadata shape; no query syntax, command registration or store API changes
are intended.

## Scope and Non-Goals

Scope is the additive metadata/error field and conversion path. This design does not create a full
language exception hierarchy, source-chain renderer or UI traceback view.

## Compatibility, Assumptions and Questions

The field must be optional with serde defaults so persisted metadata remains compatible. The main
assumption is that a plain string traceback is sufficient for this `S` issue; richer language error
transport remains separate work.

## Documentation Assessment

Small maintenance may be needed in `specs/reference/PAYLOAD_GUIDE.md` or the error/reference area
if they claim the complete error metadata shape. No new guide is expected unless implementation
reveals a repeatable traceback integration workflow.

## Design Dependencies

- `overlaps` `plan-cwd-freeze`: that completed design deliberately kept error chaining outside its
  scope and confirms that recursive `Error` causes are not required here.

## Consolidated Findings

`LogEntry` already serializes `traceback: Option<String>`; the missing source is `ErrorPayload` and
the copy in `LogEntry::from_error`. Keep `Error` pointer-sized, default the new field for old JSON,
and add a builder rather than attempting to serialize `std::error::Error::source`. Validate the
flat error wire shape, old-data compatibility, conversion with and without traceback, and metadata
round-trip. Structured language frames remain separate.

## Review

The scope matches the issue, is additive, fits Liquers' metadata model, and has testable acceptance
criteria. No duplicate design folder or blocking unknown was found.
